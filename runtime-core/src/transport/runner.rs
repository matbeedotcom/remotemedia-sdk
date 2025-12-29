//! Core pipeline execution engine exposed to transports

use crate::executor::PipelineGraph;
use crate::nodes::schema::collect_registered_configs;
use crate::nodes::streaming_node::StreamingNodeFactory;
use crate::transport::{StreamSessionHandle, TransportData};
use crate::validation::{validate_manifest, SchemaValidator, ValidationResult};
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Core pipeline execution engine exposed to transports
///
/// PipelineRunner is the primary entry point for transport implementations
/// to execute pipelines. It provides both unary and streaming execution modes.
///
/// # Design Philosophy
///
/// - **Opaque**: Internal implementation details are hidden from transports
/// - **Simple**: Clean API with minimal surface area
/// - **Efficient**: Reuses internal resources (executor, registries, etc.)
/// - **Thread-safe**: Can be shared across multiple async tasks
///
/// # Architecture
///
/// ```text
/// Transport → PipelineRunner → Executor → SessionRouter → Nodes
/// ```
///
/// PipelineRunner encapsulates:
/// - Executor instance
/// - Node registries (native + Python)
/// - Multiprocess executor (for Python nodes)
/// - Session management
///
/// # Thread Safety
///
/// PipelineRunner is Arc-wrapped internally and clones are cheap.
/// All methods are async and thread-safe.
pub struct PipelineRunner {
    /// Internal state (hidden from transports)
    inner: Arc<PipelineRunnerInner>,
}

impl PipelineRunner {
    /// Create new pipeline runner
    ///
    /// Initializes all internal resources:
    /// - Node registries (native Rust nodes + Python multiprocess)
    /// - Executor instance
    /// - Multiprocess executor (if feature enabled)
    ///
    /// # Returns
    ///
    /// * `Ok(PipelineRunner)` - Ready to execute pipelines
    /// * `Err(Error)` - Initialization failed
    ///
    /// # Errors
    ///
    /// * `Error::Execution` - Failed to set up internal resources
    ///
    /// # Examples
    ///
    /// ```
    /// use remotemedia_runtime_core::transport::PipelineRunner;
    ///
    /// let runner = PipelineRunner::new().unwrap();
    /// // Runner is ready to use
    /// ```
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: Arc::new(PipelineRunnerInner::new()?),
        })
    }

    /// Execute pipeline with unary semantics
    ///
    /// Processes a single input through the pipeline and returns the result.
    /// Suitable for batch processing or simple request/response scenarios.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Pipeline configuration (nodes, connections, params)
    /// * `input` - Input data wrapped in TransportData
    ///
    /// # Returns
    ///
    /// * `Ok(TransportData)` - Pipeline output
    /// * `Err(Error)` - Execution failed
    ///
    /// # Errors
    ///
    /// * `Error::InvalidManifest` - Manifest validation failed
    /// * `Error::Execution` - Node execution failed
    /// * `Error::InvalidData` - Input data incompatible with pipeline
    pub async fn execute_unary(
        &self,
        manifest: Arc<crate::manifest::Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        self.inner.execute_unary(manifest, input).await
    }

    /// Create streaming session
    ///
    /// Establishes a persistent session for bidirectional streaming.
    /// The session maintains state across multiple inputs/outputs.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Pipeline configuration (shared across session)
    ///
    /// # Returns
    ///
    /// * `Ok(StreamSessionHandle)` - Session ready for I/O
    /// * `Err(Error)` - Session creation failed
    ///
    /// # Errors
    ///
    /// * `Error::InvalidManifest` - Manifest validation failed
    /// * `Error::Execution` - Resource limit or initialization failed
    pub async fn create_stream_session(
        &self,
        manifest: Arc<crate::manifest::Manifest>,
    ) -> Result<StreamSessionHandle> {
        self.inner.create_stream_session(manifest).await
    }

    /// Get a reference to the internal executor
    ///
    /// This is exposed for transports that need direct access to the executor,
    /// such as for use with SessionRouter.
    ///
    /// # Returns
    ///
    /// Arc-wrapped Executor instance
    pub fn executor(&self) -> Arc<crate::executor::Executor> {
        Arc::clone(&self.inner.executor)
    }

    /// Create a streaming node registry
    ///
    /// Returns a new registry with all default streaming nodes registered.
    /// This is exposed for transports that need to create SessionRouter instances.
    ///
    /// # Returns
    ///
    /// Arc-wrapped StreamingNodeRegistry with default nodes
    pub fn create_streaming_registry(
        &self,
    ) -> Arc<crate::nodes::streaming_node::StreamingNodeRegistry> {
        Arc::new(crate::nodes::streaming_registry::create_default_streaming_registry())
    }

    /// Validate manifest parameters without executing
    ///
    /// Call this to validate a manifest before execution. Validation is also
    /// performed automatically by execute_unary() and create_stream_session().
    ///
    /// # Arguments
    ///
    /// * `manifest` - Manifest to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Validation passed (may have warnings logged)
    /// * `Err(Error::Validation)` - Validation failed with errors
    pub fn validate(&self, manifest: &crate::manifest::Manifest) -> Result<()> {
        self.inner.validate(manifest)
    }

    /// Register a custom streaming node factory
    ///
    /// Use this to add application-specific nodes (e.g., MicInput, SpeakerOutput)
    /// that will be available in streaming pipelines.
    ///
    /// # Arguments
    ///
    /// * `factory` - The streaming node factory to register
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use remotemedia_runtime_core::transport::PipelineRunner;
    /// 
    /// let runner = PipelineRunner::new().unwrap();
    /// runner.register_streaming_factory(Arc::new(MyCustomNodeFactory)).await;
    /// ```
    pub async fn register_streaming_factory(&self, factory: Arc<dyn StreamingNodeFactory>) {
        self.inner.add_streaming_factory(factory).await;
    }

    /// Register multiple streaming node factories at once
    ///
    /// Convenience method for registering multiple factories.
    ///
    /// # Arguments
    ///
    /// * `factories` - Iterator of streaming node factories to register
    pub async fn register_streaming_factories(
        &self,
        factories: impl IntoIterator<Item = Arc<dyn StreamingNodeFactory>>,
    ) {
        for factory in factories {
            self.inner.add_streaming_factory(factory).await;
        }
    }
}

// Clone is cheap (Arc-wrapped)
impl Clone for PipelineRunner {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Internal implementation (opaque to transports)
struct PipelineRunnerInner {
    /// The actual executor (now in runtime-core)
    executor: Arc<crate::executor::Executor>,

    /// Session counter for generating unique IDs
    session_counter: Arc<std::sync::atomic::AtomicU64>,

    /// Pre-compiled schema validator for node parameter validation
    schema_validator: SchemaValidator,

    /// Custom streaming node factories (registered via add_streaming_factory)
    custom_factories: RwLock<Vec<Arc<dyn StreamingNodeFactory>>>,
}

impl PipelineRunnerInner {
    fn new() -> Result<Self> {
        // Initialize the real executor
        let executor = Arc::new(crate::executor::Executor::new());

        // Collect registered node schemas and create validator
        let schema_registry = collect_registered_configs();
        let schema_validator = SchemaValidator::from_registry(&schema_registry)?;

        tracing::info!(
            "PipelineRunner initialized with {} node schemas for validation",
            schema_validator.get_all_schemas().len()
        );

        Ok(Self {
            executor,
            session_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            schema_validator,
            custom_factories: RwLock::new(Vec::new()),
        })
    }

    /// Add a custom streaming node factory
    async fn add_streaming_factory(&self, factory: Arc<dyn StreamingNodeFactory>) {
        let mut factories = self.custom_factories.write().await;
        tracing::info!("Registered custom streaming node factory: {}", factory.node_type());
        factories.push(factory);
    }

    /// Create a streaming registry with default + custom factories
    async fn create_streaming_registry_with_custom(&self) -> crate::nodes::streaming_node::StreamingNodeRegistry {
        use crate::nodes::streaming_registry::create_default_streaming_registry;
        
        let mut registry = create_default_streaming_registry();
        
        // Add custom factories
        let custom = self.custom_factories.read().await;
        for factory in custom.iter() {
            registry.register(Arc::clone(factory));
        }
        
        registry
    }

    /// Validate manifest parameters
    fn validate(&self, manifest: &crate::manifest::Manifest) -> Result<()> {
        match validate_manifest(manifest, &self.schema_validator) {
            ValidationResult::Valid => Ok(()),
            ValidationResult::PartiallyValid { warnings } => {
                // Log warnings but proceed
                for warning in &warnings {
                    tracing::warn!("{}", warning);
                }
                Ok(())
            }
            ValidationResult::Invalid { errors } => {
                // Log errors and return validation error
                for error in &errors {
                    tracing::error!("{}", error);
                }
                Err(crate::Error::Validation(errors))
            }
        }
    }

    async fn execute_unary(
        &self,
        manifest: Arc<crate::manifest::Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        // Validate manifest parameters before execution
        self.validate(&manifest)?;

        // Find the first input node from manifest
        let first_node_id = manifest
            .nodes
            .first()
            .map(|n| n.id.as_str())
            .ok_or_else(|| crate::Error::InvalidManifest("No nodes in manifest".to_string()))?;

        // Create inputs HashMap for Executor
        let mut runtime_inputs = std::collections::HashMap::new();
        runtime_inputs.insert(first_node_id.to_string(), input.data);

        // Execute via real Executor
        let output_map = self
            .executor
            .execute_with_runtime_data(&manifest, runtime_inputs)
            .await?;

        // Extract output from last node
        let output_data = output_map
            .into_values()
            .next()
            .ok_or_else(|| crate::Error::Execution("No output from pipeline".to_string()))?;

        // Wrap in TransportData, preserve metadata
        let mut output = TransportData::new(output_data);
        if let Some(seq) = input.sequence {
            output = output.with_sequence(seq);
        }
        for (k, v) in &input.metadata {
            output = output.with_metadata(k.clone(), v.clone());
        }

        Ok(output)
    }

    async fn create_stream_session(
        &self,
        manifest: Arc<crate::manifest::Manifest>,
    ) -> Result<StreamSessionHandle> {
        // Validate manifest parameters before session creation
        self.validate(&manifest)?;

        // Build pipeline graph - validates connections, detects cycles, computes execution order
        let graph = PipelineGraph::from_manifest(&manifest)?;
        tracing::info!(
            "Built pipeline graph with {} nodes, execution_order: {:?}, sources: {:?}, sinks: {:?}",
            graph.node_count(),
            graph.execution_order,
            graph.sources,
            graph.sinks
        );

        // Generate unique session ID
        let session_num = self
            .session_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let session_id = format!("session_{}", session_num);

        // Create channels for communication
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<crate::data::RuntimeData>();
        let (output_tx, output_rx) = mpsc::unbounded_channel::<crate::data::RuntimeData>();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        // Create streaming registry with custom factories BEFORE spawning the task
        let streaming_registry = self.create_streaming_registry_with_custom().await;

        // Create streaming session router task
        let session_id_clone = session_id.clone();
        let manifest_clone = Arc::clone(&manifest);

        // Extract graph data for the spawned task
        let execution_order = graph.execution_order.clone();
        let graph_sinks = graph.sinks.clone();
        let graph_nodes = graph.nodes.clone();

        // Collect nodes marked as output nodes (spec 021 User Story 3)
        // These intermediate nodes will also stream their outputs to the client
        let output_node_ids: std::collections::HashSet<String> = manifest
            .nodes
            .iter()
            .filter(|n| n.is_output_node)
            .map(|n| n.id.clone())
            .collect();

        tokio::spawn(async move {
            tracing::info!("StreamSession {} started with {} nodes in execution order", session_id_clone, execution_order.len());

            // Validate we have nodes to execute
            if execution_order.is_empty() {
                tracing::error!("Session {}: No nodes in execution order", session_id_clone);
                return;
            }

            // Create nodes ONCE at session creation and cache them for reuse
            // This prevents recreating Python processes and reloading models for each request
            let cached_nodes: Arc<HashMap<String, Box<dyn crate::nodes::StreamingNode>>> = {
                let mut n = HashMap::new();
                for node_spec in &manifest_clone.nodes {
                    // Docker support is now integrated into the multiprocess system
                    // Use executor: multiprocess with use_docker: true in node config
                    #[cfg(feature = "docker")]
                    let node: Box<dyn crate::nodes::StreamingNode> = {
                        // Always use streaming registry - Docker is handled by multiprocess executor
                        let mut params_with_session = node_spec.params.clone();
                        params_with_session["__session_id__"] =
                            serde_json::Value::String(session_id_clone.clone());

                        match streaming_registry.create_node(
                            &node_spec.node_type,
                            node_spec.id.clone(),
                            &params_with_session,
                            Some(session_id_clone.clone()),
                        ) {
                            Ok(node) => node,
                            Err(e) => {
                                tracing::error!(
                                    "Session {}: Failed to create node {}: {}",
                                    session_id_clone,
                                    node_spec.id,
                                    e
                                );
                                return;
                            }
                        }
                    };

                    #[cfg(not(feature = "docker"))]
                    let node: Box<dyn crate::nodes::StreamingNode> = {
                        // Inject session_id into params for multiprocess execution
                        let mut params_with_session = node_spec.params.clone();
                        params_with_session["__session_id__"] =
                            serde_json::Value::String(session_id_clone.clone());

                        match streaming_registry.create_node(
                            &node_spec.node_type,
                            node_spec.id.clone(),
                            &params_with_session,
                            Some(session_id_clone.clone()),
                        ) {
                            Ok(node) => node,
                            Err(e) => {
                                tracing::error!(
                                    "Session {}: Failed to create node {}: {}",
                                    session_id_clone,
                                    node_spec.id,
                                    e
                                );
                                return;
                            }
                        }
                    };

                    tracing::debug!(
                        "Session {} cached node {} (type: {})",
                        session_id_clone,
                        node_spec.id,
                        node_spec.node_type
                    );
                    n.insert(node_spec.id.clone(), node);
                }
                Arc::new(n)
            };

            // Keep output_tx alive for the entire session by holding it here
            // This prevents the channel from closing after the first execution
            let _output_tx_holder = output_tx.clone();

            loop {
                tokio::select! {
                    Some(input_data) = input_rx.recv() => {
                        tracing::info!("[SessionRunner] Session {} received input: type={}", session_id_clone, input_data.data_type());

                        // Spawn execution as a background task so the select loop can continue
                        // processing new inputs while this one executes
                        let output_tx_clone = output_tx.clone();
                        let session_id_for_exec = session_id_clone.clone();
                        let session_id_for_log = session_id_clone.clone();
                        let cached_nodes_clone = Arc::clone(&cached_nodes);
                        let execution_order_clone = execution_order.clone();
                        let graph_sinks_clone = graph_sinks.clone();
                        let graph_nodes_clone = graph_nodes.clone();
                        let manifest_for_exec = Arc::clone(&manifest_clone);
                        let output_node_ids_clone = output_node_ids.clone();

                        tracing::info!("[SessionRunner] Session {} spawning background task to process input through {} nodes", session_id_clone, execution_order_clone.len());
                        tokio::spawn(async move {
                            tracing::info!("[SessionRunner] Session {} background task started, processing through graph", session_id_for_exec);

                            // Track outputs from each node for routing to dependents
                            let mut all_node_outputs: HashMap<String, Vec<crate::data::RuntimeData>> = HashMap::new();

                            // Source nodes receive the input data
                            // Find which nodes have no inputs (sources) and give them the input
                            for node_id in &execution_order_clone {
                                if let Some(graph_node) = graph_nodes_clone.get(node_id) {
                                    if graph_node.inputs.is_empty() {
                                        // This is a source node - it gets the original input
                                        all_node_outputs.insert(node_id.clone(), vec![input_data.clone()]);
                                    }
                                }
                            }

                            // Process nodes in topological order
                            for node_id in &execution_order_clone {
                                let node = match cached_nodes_clone.get(node_id) {
                                    Some(n) => n,
                                    None => {
                                        tracing::error!("Session {}: Node {} not found in cached nodes", session_id_for_exec, node_id);
                                        return;
                                    }
                                };

                                // Collect inputs for this node
                                let inputs: Vec<crate::data::RuntimeData> = if all_node_outputs.contains_key(node_id) {
                                    // Source node - already has input
                                    all_node_outputs.get(node_id).cloned().unwrap_or_default()
                                } else {
                                    // Collect inputs from predecessor nodes via connections
                                    let mut collected_inputs = Vec::new();
                                    for conn in manifest_for_exec.connections.iter().filter(|c| c.to == *node_id) {
                                        if let Some(predecessor_outputs) = all_node_outputs.get(&conn.from) {
                                            collected_inputs.extend(predecessor_outputs.clone());
                                        }
                                    }
                                    collected_inputs
                                };

                                if inputs.is_empty() {
                                    tracing::warn!("Session {}: Node {} has no inputs, skipping", session_id_for_exec, node_id);
                                    continue;
                                }

                                tracing::debug!("Session {} processing node '{}' with {} input(s)", session_id_for_exec, node_id, inputs.len());

                                // Collect outputs from this node using a shared buffer
                                let node_outputs_ref = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

                                // Process each input through the node
                                for input in inputs {
                                    let node_outputs_clone = node_outputs_ref.clone();
                                    let callback = Box::new(move |output: crate::data::RuntimeData| {
                                        node_outputs_clone.lock().unwrap().push(output);
                                        Ok(())
                                    });

                                    if let Err(e) = node.process_streaming_async(input, Some(session_id_for_exec.clone()), callback).await {
                                        tracing::error!("Session {}: Node {} execution error: {}", session_id_for_exec, node_id, e);
                                        // Continue with other nodes - don't fail entire pipeline
                                    }
                                }

                                // Get collected outputs
                                let node_outputs = node_outputs_ref.lock().unwrap().clone();
                                tracing::debug!("Session {} node '{}' produced {} output(s)", session_id_for_exec, node_id, node_outputs.len());

                                // Store outputs for downstream nodes
                                all_node_outputs.insert(node_id.clone(), node_outputs);
                            }

                            // Send outputs from sink nodes (terminal nodes) AND nodes marked as output nodes
                            // (spec 021 User Story 3: intermediate output nodes)
                            //
                            // By default only terminal nodes (sinks) send data to the client.
                            // Nodes marked with `is_output_node: true` also stream their outputs,
                            // enabling debugging, monitoring, or branching use cases.
                            let mut sent_node_ids = std::collections::HashSet::new();

                            // First send from terminal nodes (sinks)
                            for sink_id in &graph_sinks_clone {
                                if let Some(outputs) = all_node_outputs.get(sink_id) {
                                    for output in outputs {
                                        tracing::debug!("Session {} sending output from sink '{}' to client", session_id_for_exec, sink_id);
                                        if let Err(e) = output_tx_clone.send(output.clone()) {
                                            tracing::warn!("Session {}: Failed to send output from sink {}: {}", session_id_for_exec, sink_id, e);
                                        }
                                    }
                                }
                                sent_node_ids.insert(sink_id.clone());
                            }

                            // Then send from intermediate nodes marked as output nodes
                            // (avoid duplicates if a sink is also marked as output node)
                            for output_node_id in &output_node_ids_clone {
                                if sent_node_ids.contains(output_node_id) {
                                    continue; // Already sent as a sink
                                }
                                if let Some(outputs) = all_node_outputs.get(output_node_id) {
                                    for output in outputs {
                                        tracing::debug!("Session {} sending output from intermediate output node '{}' to client", session_id_for_exec, output_node_id);
                                        if let Err(e) = output_tx_clone.send(output.clone()) {
                                            tracing::warn!("Session {}: Failed to send output from output node {}: {}", session_id_for_exec, output_node_id, e);
                                        }
                                    }
                                }
                            }

                            tracing::info!("[SessionRunner] Session {} background task finished, processed {} nodes", session_id_for_log, execution_order_clone.len());
                        });
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Session {} shutdown requested", session_id_clone);
                        break;
                    }
                }
            }

            tracing::info!("StreamSession {} ended", session_id_clone);
        });

        Ok(StreamSessionHandle::new(
            session_id,
            input_tx,
            output_rx,
            shutdown_tx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};

    /// Test that PipelineGraph is correctly built from manifest with connections
    #[test]
    fn test_graph_building_from_manifest() {
        // Create a 2-node pipeline: A -> B
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "test-pipeline".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
            ],
            connections: vec![Connection {
                from: "A".to_string(),
                to: "B".to_string(),
            }],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // Verify graph structure
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.execution_order, vec!["A", "B"]);
        assert_eq!(graph.sources, vec!["A"]);
        assert_eq!(graph.sinks, vec!["B"]);
    }

    /// Test 3-node linear pipeline graph building
    #[test]
    fn test_three_node_linear_pipeline() {
        // Create A -> B -> C pipeline
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "three-node-pipeline".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "C".to_string(),
                },
            ],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // Verify graph structure
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.execution_order, vec!["A", "B", "C"]);
        assert_eq!(graph.sources, vec!["A"]);
        assert_eq!(graph.sinks, vec!["C"]);

        // Verify node connections
        let node_a = graph.nodes.get("A").unwrap();
        assert!(node_a.inputs.is_empty());
        assert_eq!(node_a.outputs, vec!["B"]);

        let node_b = graph.nodes.get("B").unwrap();
        assert_eq!(node_b.inputs, vec!["A"]);
        assert_eq!(node_b.outputs, vec!["C"]);

        let node_c = graph.nodes.get("C").unwrap();
        assert_eq!(node_c.inputs, vec!["B"]);
        assert!(node_c.outputs.is_empty());
    }

    /// Test single-node pipeline (backward compatibility)
    #[test]
    fn test_single_node_pipeline() {
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "single-node-pipeline".to_string(),
                ..Default::default()
            },
            nodes: vec![NodeManifest {
                id: "only_node".to_string(),
                node_type: "TestNode".to_string(),
                params: serde_json::json!({}),
                ..Default::default()
            }],
            connections: vec![],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // Single node should be both source and sink
        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph.execution_order, vec!["only_node"]);
        assert_eq!(graph.sources, vec!["only_node"]);
        assert_eq!(graph.sinks, vec!["only_node"]);
    }

    /// Test cycle detection - should fail for A -> B -> C -> A
    #[test]
    fn test_cycle_detection() {
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "cyclic-pipeline".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "C".to_string(),
                },
                Connection {
                    from: "C".to_string(),
                    to: "A".to_string(), // Creates cycle!
                },
            ],
        };

        let result = PipelineGraph::from_manifest(&manifest);
        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("cycle"), "Error should mention cycle: {}", error);
    }

    /// Test invalid connection to non-existent node
    #[test]
    fn test_invalid_connection_to_missing_node() {
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "invalid-connection".to_string(),
                ..Default::default()
            },
            nodes: vec![NodeManifest {
                id: "A".to_string(),
                node_type: "TestNode".to_string(),
                params: serde_json::json!({}),
                ..Default::default()
            }],
            connections: vec![Connection {
                from: "A".to_string(),
                to: "NonExistent".to_string(), // This node doesn't exist!
            }],
        };

        let result = PipelineGraph::from_manifest(&manifest);
        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("NonExistent") || error.contains("unknown"),
            "Error should identify missing node: {}",
            error
        );
    }

    /// Test fan-out pipeline (A -> B, A -> C)
    #[test]
    fn test_fan_out_pipeline() {
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "fan-out-pipeline".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "A".to_string(),
                    to: "C".to_string(),
                },
            ],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // A is source, B and C are sinks
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.sources, vec!["A"]);
        assert_eq!(graph.sinks.len(), 2);
        assert!(graph.sinks.contains(&"B".to_string()));
        assert!(graph.sinks.contains(&"C".to_string()));

        // A should come before both B and C
        let a_idx = graph.execution_order.iter().position(|x| x == "A").unwrap();
        let b_idx = graph.execution_order.iter().position(|x| x == "B").unwrap();
        let c_idx = graph.execution_order.iter().position(|x| x == "C").unwrap();
        assert!(a_idx < b_idx);
        assert!(a_idx < c_idx);
    }

    /// Test fan-in pipeline (A -> C, B -> C)
    #[test]
    fn test_fan_in_pipeline() {
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "fan-in-pipeline".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "C".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "C".to_string(),
                },
            ],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // A and B are sources, C is sink
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.sources.len(), 2);
        assert!(graph.sources.contains(&"A".to_string()));
        assert!(graph.sources.contains(&"B".to_string()));
        assert_eq!(graph.sinks, vec!["C"]);

        // C should have both A and B as inputs
        let node_c = graph.nodes.get("C").unwrap();
        assert_eq!(node_c.inputs.len(), 2);
        assert!(node_c.inputs.contains(&"A".to_string()));
        assert!(node_c.inputs.contains(&"B".to_string()));

        // Both A and B should come before C
        let a_idx = graph.execution_order.iter().position(|x| x == "A").unwrap();
        let b_idx = graph.execution_order.iter().position(|x| x == "B").unwrap();
        let c_idx = graph.execution_order.iter().position(|x| x == "C").unwrap();
        assert!(a_idx < c_idx);
        assert!(b_idx < c_idx);
    }

    /// Test diamond pipeline (A -> B, A -> C, B -> D, C -> D)
    #[test]
    fn test_diamond_pipeline() {
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "diamond-pipeline".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "D".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "A".to_string(),
                    to: "C".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "D".to_string(),
                },
                Connection {
                    from: "C".to_string(),
                    to: "D".to_string(),
                },
            ],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // A is source, D is sink
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.sources, vec!["A"]);
        assert_eq!(graph.sinks, vec!["D"]);

        // Verify execution order respects dependencies
        let exec_order = &graph.execution_order;
        let a_idx = exec_order.iter().position(|x| x == "A").unwrap();
        let b_idx = exec_order.iter().position(|x| x == "B").unwrap();
        let c_idx = exec_order.iter().position(|x| x == "C").unwrap();
        let d_idx = exec_order.iter().position(|x| x == "D").unwrap();

        // A must come before B and C
        assert!(a_idx < b_idx);
        assert!(a_idx < c_idx);
        // B and C must come before D
        assert!(b_idx < d_idx);
        assert!(c_idx < d_idx);
    }

    /// Test is_output_node field parsing in manifest (spec 021 User Story 3)
    #[test]
    fn test_is_output_node_field_parsing() {
        // Create A -> B -> C pipeline where B is marked as output node
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "output-node-test".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    is_output_node: false,
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    is_output_node: true, // Marked as intermediate output
                    ..Default::default()
                },
                NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    is_output_node: false,
                    ..Default::default()
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "C".to_string(),
                },
            ],
        };

        // Verify graph builds correctly
        let graph = PipelineGraph::from_manifest(&manifest).unwrap();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.sources, vec!["A"]);
        assert_eq!(graph.sinks, vec!["C"]);

        // Verify B is intermediate (not a sink but marked as output)
        assert!(!graph.sinks.contains(&"B".to_string()));

        // Verify the is_output_node field is correctly set
        assert!(!manifest.nodes[0].is_output_node); // A
        assert!(manifest.nodes[1].is_output_node);  // B - marked as output
        assert!(!manifest.nodes[2].is_output_node); // C
    }

    /// Test that output_node_ids collection correctly identifies output nodes
    #[test]
    fn test_output_node_ids_collection() {
        use std::collections::HashSet;

        // Create A -> B -> C pipeline where B is marked as output node
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "output-node-collection-test".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    is_output_node: false,
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    is_output_node: true, // Intermediate output
                    ..Default::default()
                },
                NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    is_output_node: false,
                    ..Default::default()
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "C".to_string(),
                },
            ],
        };

        // Collect output node IDs (same logic as in create_stream_session)
        let output_node_ids: HashSet<String> = manifest
            .nodes
            .iter()
            .filter(|n| n.is_output_node)
            .map(|n| n.id.clone())
            .collect();

        // Only B should be in output_node_ids
        assert_eq!(output_node_ids.len(), 1);
        assert!(output_node_ids.contains("B"));
        assert!(!output_node_ids.contains("A"));
        assert!(!output_node_ids.contains("C"));
    }

    /// Test that unmarked nodes don't appear in output_node_ids
    #[test]
    fn test_unmarked_nodes_not_in_output_ids() {
        use std::collections::HashSet;

        // Create A -> B -> C pipeline with NO output nodes marked
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "no-output-nodes-test".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default() // is_output_node defaults to false
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "C".to_string(),
                },
            ],
        };

        // Verify graph builds - C is the only sink
        let graph = PipelineGraph::from_manifest(&manifest).unwrap();
        assert_eq!(graph.sinks, vec!["C"]);

        // Collect output node IDs
        let output_node_ids: HashSet<String> = manifest
            .nodes
            .iter()
            .filter(|n| n.is_output_node)
            .map(|n| n.id.clone())
            .collect();

        // Should be empty - no nodes marked as output
        assert!(output_node_ids.is_empty());
    }

    /// Test sink marked as output node doesn't cause duplicates
    #[test]
    fn test_sink_also_marked_as_output_node() {
        use std::collections::HashSet;

        // Create A -> B where B is both sink AND marked as output node
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "sink-output-node-test".to_string(),
                ..Default::default()
            },
            nodes: vec![
                NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    ..Default::default()
                },
                NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    is_output_node: true, // B is both sink and output node
                    ..Default::default()
                },
            ],
            connections: vec![Connection {
                from: "A".to_string(),
                to: "B".to_string(),
            }],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // B is the sink
        assert_eq!(graph.sinks, vec!["B"]);

        // B is also in output_node_ids
        let output_node_ids: HashSet<String> = manifest
            .nodes
            .iter()
            .filter(|n| n.is_output_node)
            .map(|n| n.id.clone())
            .collect();

        assert!(output_node_ids.contains("B"));

        // The deduplication logic should handle this:
        // sent_node_ids.contains(sink_id) check prevents double-sending
        let mut sent_node_ids = HashSet::new();
        for sink_id in &graph.sinks {
            sent_node_ids.insert(sink_id.clone());
        }
        // When iterating output_node_ids, B would be skipped because it's already sent
        let would_send_from_output: Vec<_> = output_node_ids
            .iter()
            .filter(|id| !sent_node_ids.contains(*id))
            .collect();

        // B is already in sent_node_ids, so nothing extra to send
        assert!(would_send_from_output.is_empty());
    }
}
