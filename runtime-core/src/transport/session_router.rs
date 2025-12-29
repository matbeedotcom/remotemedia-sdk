//! Session-level Async Router for Pipeline Streaming
//!
//! This module provides a graph-aware routing layer that processes data through
//! multi-node pipelines with proper topological ordering, fan-in/fan-out support,
//! and cycle detection.
//!
//! # Architecture
//!
//! The SessionRouter sits between transport implementations and the node execution
//! layer. It:
//!
//! 1. Builds a `PipelineGraph` at session creation (validates connections, detects cycles)
//! 2. Executes nodes in topological order
//! 3. Routes data between nodes based on manifest connections
//! 4. Sends outputs from terminal nodes (sinks) to the client
//!
//! # Usage
//!
//! Transport implementations should use `SessionRouter` instead of implementing
//! their own routing logic:
//!
//! ```ignore
//! use remotemedia_runtime_core::transport::SessionRouter;
//!
//! let router = SessionRouter::new(
//!     session_id,
//!     manifest,
//!     streaming_registry,
//!     output_tx,
//! )?;
//!
//! // Start the router (runs until shutdown)
//! let handle = router.start();
//!
//! // Feed data through the pipeline
//! router.send_input(input_data).await?;
//! ```

use crate::data::RuntimeData;
use crate::executor::PipelineGraph;
use crate::manifest::Manifest;
use crate::nodes::{StreamingNode, StreamingNodeRegistry};
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Data packet flowing through the pipeline
#[derive(Clone, Debug)]
pub struct DataPacket {
    /// The actual data
    pub data: RuntimeData,
    /// Source node ID (where this data came from)
    pub from_node: String,
    /// Target node ID (optional - for direct routing from client to specific node)
    pub to_node: Option<String>,
    /// Session ID
    pub session_id: String,
    /// Sequence number for ordering
    pub sequence: u64,
    /// Sub-sequence for streaming outputs (multiple outputs per input)
    pub sub_sequence: u64,
}

/// Session-persistent router that processes data through the pipeline graph
///
/// This router:
/// - Validates the pipeline graph at creation (cycle detection, missing nodes)
/// - Caches node instances for the session lifetime
/// - Executes nodes in topological order
/// - Handles fan-in (multiple inputs) and fan-out (multiple outputs)
/// - Only sends outputs from sink nodes to the client
pub struct SessionRouter {
    /// Session ID
    session_id: String,

    /// Pipeline manifest
    manifest: Arc<Manifest>,

    /// Pipeline graph (topological order, sources, sinks)
    graph: PipelineGraph,

    /// Registry for creating nodes
    registry: Arc<StreamingNodeRegistry>,

    /// Cached node instances (created once per session)
    cached_nodes: HashMap<String, Box<dyn StreamingNode>>,

    /// Channel to send outputs to client
    output_tx: mpsc::UnboundedSender<RuntimeData>,

    /// Channel to receive inputs from client
    input_rx: Option<mpsc::UnboundedReceiver<DataPacket>>,

    /// Channel to send inputs to router (held by external code)
    input_tx: mpsc::UnboundedSender<DataPacket>,

    /// Shutdown signal receiver
    shutdown_rx: Option<mpsc::Receiver<()>>,

    /// Shutdown signal sender (held externally)
    _shutdown_tx: mpsc::Sender<()>,
}

impl SessionRouter {
    /// Create a new session router
    ///
    /// # Arguments
    ///
    /// * `session_id` - Unique identifier for this session
    /// * `manifest` - Pipeline manifest defining nodes and connections
    /// * `registry` - Registry for creating streaming nodes
    /// * `output_tx` - Channel for sending outputs to the client
    ///
    /// # Returns
    ///
    /// * `Ok((router, shutdown_tx))` - Router and shutdown signal sender
    /// * `Err` - Graph validation failed (cycles, missing nodes, etc.)
    pub fn new(
        session_id: String,
        manifest: Arc<Manifest>,
        registry: Arc<StreamingNodeRegistry>,
        output_tx: mpsc::UnboundedSender<RuntimeData>,
    ) -> Result<(Self, mpsc::Sender<()>)> {
        // Build and validate the pipeline graph
        let graph = PipelineGraph::from_manifest(&manifest)?;
        tracing::info!(
            "Session {}: Built pipeline graph with {} nodes, execution_order: {:?}, sources: {:?}, sinks: {:?}",
            session_id,
            graph.node_count(),
            graph.execution_order,
            graph.sources,
            graph.sinks
        );

        // Create input/shutdown channels
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let shutdown_tx_clone = shutdown_tx.clone();

        let router = Self {
            session_id,
            manifest,
            graph,
            registry,
            cached_nodes: HashMap::new(),
            output_tx,
            input_rx: Some(input_rx),
            input_tx,
            shutdown_rx: Some(shutdown_rx),
            _shutdown_tx: shutdown_tx,
        };

        Ok((router, shutdown_tx_clone))
    }

    /// Get the input sender for feeding data to the router
    pub fn get_input_sender(&self) -> mpsc::UnboundedSender<DataPacket> {
        self.input_tx.clone()
    }

    /// Get the pipeline graph
    pub fn graph(&self) -> &PipelineGraph {
        &self.graph
    }

    /// Get the execution order
    pub fn execution_order(&self) -> &[String] {
        &self.graph.execution_order
    }

    /// Get the source nodes (nodes with no inputs)
    pub fn sources(&self) -> &[String] {
        &self.graph.sources
    }

    /// Get the sink nodes (nodes with no outputs)
    pub fn sinks(&self) -> &[String] {
        &self.graph.sinks
    }

    /// Initialize all nodes in the pipeline
    ///
    /// This pre-creates and caches all nodes before streaming starts,
    /// eliminating cold-start latency.
    pub async fn initialize_nodes(&mut self) -> Result<()> {
        tracing::info!(
            "Session {}: Initializing {} nodes",
            self.session_id,
            self.manifest.nodes.len()
        );

        for node_spec in &self.manifest.nodes {
            let node = self.registry.create_node(
                &node_spec.node_type,
                node_spec.id.clone(),
                &node_spec.params,
                Some(self.session_id.clone()),
            )?;

            // Initialize the node (load models, etc.)
            node.initialize().await?;

            self.cached_nodes.insert(node_spec.id.clone(), node);
            tracing::debug!(
                "Session {}: Initialized node '{}' (type: {})",
                self.session_id,
                node_spec.id,
                node_spec.node_type
            );
        }

        tracing::info!(
            "Session {}: All {} nodes initialized",
            self.session_id,
            self.cached_nodes.len()
        );
        Ok(())
    }

    /// Start the router - runs until shutdown signal
    pub fn start(mut self) -> JoinHandle<()> {
        let session_id = self.session_id.clone();
        tracing::info!("Session {}: Starting session router", session_id);

        tokio::spawn(async move {
            if let Err(e) = self.run().await {
                tracing::error!("Session {}: Router error: {}", session_id, e);
            }
            tracing::info!("Session {}: Router stopped", session_id);
        })
    }

    /// Main routing loop
    async fn run(&mut self) -> Result<()> {
        let mut input_rx = self
            .input_rx
            .take()
            .ok_or_else(|| crate::Error::Execution("Input channel already taken".to_string()))?;

        let mut shutdown_rx = self
            .shutdown_rx
            .take()
            .ok_or_else(|| crate::Error::Execution("Shutdown channel already taken".to_string()))?;

        tracing::info!(
            "Session {}: Router running, waiting for input...",
            self.session_id
        );

        loop {
            tokio::select! {
                Some(packet) = input_rx.recv() => {
                    tracing::debug!(
                        "Session {}: Received input packet (seq: {}, from: {})",
                        self.session_id,
                        packet.sequence,
                        packet.from_node
                    );

                    if let Err(e) = self.process_input(packet).await {
                        tracing::error!("Session {}: Processing error: {}", self.session_id, e);
                        // Continue processing other inputs
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Session {}: Shutdown signal received", self.session_id);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Process a single input through the pipeline graph
    async fn process_input(&mut self, packet: DataPacket) -> Result<()> {
        let start_time = std::time::Instant::now();

        // Track outputs from each node for routing to dependents
        let mut all_node_outputs: HashMap<String, Vec<RuntimeData>> = HashMap::new();

        // Determine which nodes receive the initial input
        if let Some(ref target_node) = packet.to_node {
            // Direct routing: send to specific node
            all_node_outputs.insert(target_node.clone(), vec![packet.data.clone()]);
        } else {
            // Default: send to all source nodes (nodes with no inputs)
            for source_id in &self.graph.sources {
                all_node_outputs.insert(source_id.clone(), vec![packet.data.clone()]);
            }
        }

        // Process nodes in topological order
        for node_id in &self.graph.execution_order {
            let node = match self.cached_nodes.get(node_id) {
                Some(n) => n,
                None => {
                    tracing::error!(
                        "Session {}: Node '{}' not found in cache",
                        self.session_id,
                        node_id
                    );
                    continue;
                }
            };

            // Collect inputs for this node
            let inputs: Vec<RuntimeData> = if all_node_outputs.contains_key(node_id) {
                // Source node or directly targeted - already has input
                all_node_outputs.get(node_id).cloned().unwrap_or_default()
            } else {
                // Collect inputs from predecessor nodes via connections (fan-in)
                let mut collected_inputs = Vec::new();
                for conn in self
                    .manifest
                    .connections
                    .iter()
                    .filter(|c| c.to == *node_id)
                {
                    if let Some(predecessor_outputs) = all_node_outputs.get(&conn.from) {
                        collected_inputs.extend(predecessor_outputs.clone());
                    }
                }
                collected_inputs
            };

            if inputs.is_empty() {
                tracing::warn!(
                    "Session {}: Node '{}' has no inputs, skipping",
                    self.session_id,
                    node_id
                );
                continue;
            }

            tracing::debug!(
                "Session {}: Processing node '{}' with {} input(s)",
                self.session_id,
                node_id,
                inputs.len()
            );

            // Collect outputs from this node using a shared buffer
            let node_outputs_ref = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

            // Process each input through the node
            for input in inputs {
                let node_outputs_clone = node_outputs_ref.clone();
                let callback = Box::new(move |output: RuntimeData| {
                    node_outputs_clone.lock().unwrap().push(output);
                    Ok(())
                });

                let session_id = self.session_id.clone();
                if let Err(e) = node
                    .process_streaming_async(input, Some(session_id.clone()), callback)
                    .await
                {
                    tracing::error!(
                        "Session {}: Node '{}' execution error: {}",
                        self.session_id,
                        node_id,
                        e
                    );
                    // Continue with other nodes - don't fail entire pipeline
                }
            }

            // Get collected outputs
            let node_outputs = node_outputs_ref.lock().unwrap().clone();
            tracing::debug!(
                "Session {}: Node '{}' produced {} output(s)",
                self.session_id,
                node_id,
                node_outputs.len()
            );

            // Store outputs for downstream nodes (fan-out happens automatically)
            all_node_outputs.insert(node_id.clone(), node_outputs);
        }

        // Send outputs from sink nodes (terminal nodes) to client
        for sink_id in &self.graph.sinks {
            if let Some(outputs) = all_node_outputs.get(sink_id) {
                for output in outputs {
                    tracing::debug!(
                        "Session {}: Sending output from sink '{}' to client",
                        self.session_id,
                        sink_id
                    );
                    if let Err(e) = self.output_tx.send(output.clone()) {
                        tracing::warn!(
                            "Session {}: Failed to send output from sink '{}': {}",
                            self.session_id,
                            sink_id,
                            e
                        );
                    }
                }
            }
        }

        let elapsed = start_time.elapsed();
        tracing::info!(
            "Session {}: Processed input through {} nodes in {:?}",
            self.session_id,
            self.graph.execution_order.len(),
            elapsed
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Connection, ManifestMetadata, NodeManifest};

    fn create_test_manifest(
        nodes: Vec<(&str, &str)>,
        connections: Vec<(&str, &str)>,
    ) -> Manifest {
        Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "test-pipeline".to_string(),
                ..Default::default()
            },
            nodes: nodes
                .into_iter()
                .map(|(id, node_type)| NodeManifest {
                    id: id.to_string(),
                    node_type: node_type.to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                })
                .collect(),
            connections: connections
                .into_iter()
                .map(|(from, to)| Connection {
                    from: from.to_string(),
                    to: to.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_session_router_graph_validation() {
        // Create a valid linear pipeline
        let manifest = create_test_manifest(
            vec![("A", "TestNode"), ("B", "TestNode"), ("C", "TestNode")],
            vec![("A", "B"), ("B", "C")],
        );

        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) = mpsc::unbounded_channel();

        let result = SessionRouter::new(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
        );

        assert!(result.is_ok());
        let (router, _shutdown_tx) = result.unwrap();

        assert_eq!(router.execution_order(), &["A", "B", "C"]);
        assert_eq!(router.sources(), &["A"]);
        assert_eq!(router.sinks(), &["C"]);
    }

    #[test]
    fn test_session_router_cycle_detection() {
        // Create a cyclic pipeline (should fail)
        let manifest = create_test_manifest(
            vec![("A", "TestNode"), ("B", "TestNode"), ("C", "TestNode")],
            vec![("A", "B"), ("B", "C"), ("C", "A")], // Cycle!
        );

        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) = mpsc::unbounded_channel();

        let result = SessionRouter::new(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
        );

        assert!(result.is_err());
        let error = result.err().unwrap().to_string();
        assert!(
            error.contains("cycle"),
            "Error should mention cycle: {}",
            error
        );
    }

    #[test]
    fn test_session_router_fan_out() {
        // A -> B, A -> C (fan-out)
        let manifest = create_test_manifest(
            vec![("A", "TestNode"), ("B", "TestNode"), ("C", "TestNode")],
            vec![("A", "B"), ("A", "C")],
        );

        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) = mpsc::unbounded_channel();

        let (router, _shutdown_tx) = SessionRouter::new(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
        )
        .unwrap();

        assert_eq!(router.sources(), &["A"]);
        assert_eq!(router.sinks().len(), 2);
        assert!(router.sinks().contains(&"B".to_string()));
        assert!(router.sinks().contains(&"C".to_string()));
    }

    #[test]
    fn test_session_router_fan_in() {
        // A -> C, B -> C (fan-in)
        let manifest = create_test_manifest(
            vec![("A", "TestNode"), ("B", "TestNode"), ("C", "TestNode")],
            vec![("A", "C"), ("B", "C")],
        );

        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) = mpsc::unbounded_channel();

        let (router, _shutdown_tx) = SessionRouter::new(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
        )
        .unwrap();

        assert_eq!(router.sources().len(), 2);
        assert!(router.sources().contains(&"A".to_string()));
        assert!(router.sources().contains(&"B".to_string()));
        assert_eq!(router.sinks(), &["C"]);
    }

    #[test]
    fn test_session_router_diamond() {
        // A -> B, A -> C, B -> D, C -> D (diamond)
        let manifest = create_test_manifest(
            vec![
                ("A", "TestNode"),
                ("B", "TestNode"),
                ("C", "TestNode"),
                ("D", "TestNode"),
            ],
            vec![("A", "B"), ("A", "C"), ("B", "D"), ("C", "D")],
        );

        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) = mpsc::unbounded_channel();

        let (router, _shutdown_tx) = SessionRouter::new(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
        )
        .unwrap();

        assert_eq!(router.sources(), &["A"]);
        assert_eq!(router.sinks(), &["D"]);

        // Verify execution order respects dependencies
        let order = router.execution_order();
        let a_idx = order.iter().position(|x| x == "A").unwrap();
        let b_idx = order.iter().position(|x| x == "B").unwrap();
        let c_idx = order.iter().position(|x| x == "C").unwrap();
        let d_idx = order.iter().position(|x| x == "D").unwrap();

        assert!(a_idx < b_idx);
        assert!(a_idx < c_idx);
        assert!(b_idx < d_idx);
        assert!(c_idx < d_idx);
    }
}
