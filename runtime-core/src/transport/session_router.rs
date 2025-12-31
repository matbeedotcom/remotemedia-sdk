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

use crate::capabilities::{CapabilityBehavior, CapabilityResolver, ResolutionContext};
use crate::data::RuntimeData;
use crate::executor::{
    DriftMetrics, DriftThresholds, NodeStats, PipelineGraph, SchedulerConfig, StreamingScheduler,
};
use crate::manifest::Manifest;
use crate::nodes::{StreamingNode, StreamingNodeRegistry};
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
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
/// - Integrates StreamingScheduler for timeout, retry, and circuit breaker (spec 026)
/// - Tracks per-stream drift metrics for health monitoring (spec 026)
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

    /// Capability resolution context (spec 025)
    /// Stores pending capability updates for downstream nodes
    resolution_ctx: Option<ResolutionContext>,

    /// StreamingScheduler for node execution with timeout, retry, circuit breaker (spec 026)
    scheduler: Arc<StreamingScheduler>,

    /// Per-stream drift metrics for health monitoring (spec 026)
    /// Key: stream_id (from RuntimeData.stream_id or "default")
    drift_metrics: Arc<RwLock<HashMap<String, Arc<RwLock<DriftMetrics>>>>>,

    /// Drift thresholds for new streams
    drift_thresholds: DriftThresholds,
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
        Self::with_config(
            session_id,
            manifest,
            registry,
            output_tx,
            None,
            None,
        )
    }

    /// Create a new session router with optional scheduler and drift threshold configuration
    ///
    /// # Arguments
    ///
    /// * `session_id` - Unique identifier for this session
    /// * `manifest` - Pipeline manifest defining nodes and connections
    /// * `registry` - Registry for creating streaming nodes
    /// * `output_tx` - Channel for sending outputs to the client
    /// * `scheduler_config` - Optional scheduler configuration (uses defaults if None)
    /// * `drift_thresholds` - Optional drift threshold configuration (uses defaults if None)
    ///
    /// # Returns
    ///
    /// * `Ok((router, shutdown_tx))` - Router and shutdown signal sender
    /// * `Err` - Graph validation failed (cycles, missing nodes, etc.)
    pub fn with_config(
        session_id: String,
        manifest: Arc<Manifest>,
        registry: Arc<StreamingNodeRegistry>,
        output_tx: mpsc::UnboundedSender<RuntimeData>,
        scheduler_config: Option<SchedulerConfig>,
        drift_thresholds: Option<DriftThresholds>,
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

        // Create scheduler config, extracting fast_path nodes from manifest
        let mut config = scheduler_config.unwrap_or_default();
        for node_def in &manifest.nodes {
            if node_def.fast_path {
                config.fast_path_nodes.insert(node_def.id.clone());
                tracing::debug!(
                    "Session {}: Node '{}' configured for fast path execution",
                    session_id,
                    node_def.id
                );
            }
        }

        // Create scheduler with config
        let scheduler = Arc::new(StreamingScheduler::new(config));

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
            resolution_ctx: None,
            scheduler,
            drift_metrics: Arc::new(RwLock::new(HashMap::new())),
            drift_thresholds: drift_thresholds.unwrap_or_default(),
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

        // Spec 025: After initialization, propagate actual capabilities from
        // RuntimeDiscovered nodes to downstream Adaptive/Passthrough nodes
        self.propagate_runtime_capabilities().await?;

        Ok(())
    }

    /// Propagate capabilities from RuntimeDiscovered nodes after initialization (spec 025).
    ///
    /// This method is called after all nodes are initialized. For each node with
    /// `RuntimeDiscovered` behavior that reports actual capabilities, we:
    /// 1. Call `revalidate_and_propagate()` to update resolution context
    /// 2. Apply pending updates to downstream nodes via `configure_from_upstream()`
    async fn propagate_runtime_capabilities(&mut self) -> Result<()> {
        // Build resolution context from manifest if not already present
        if self.resolution_ctx.is_none() {
            let mut ctx = ResolutionContext::new();

            // Add node types and connections from manifest
            for node_spec in &self.manifest.nodes {
                ctx.node_types.insert(node_spec.id.clone(), node_spec.node_type.clone());
            }
            for conn in &self.manifest.connections {
                ctx.add_connection(&conn.from, &conn.to);
            }

            // Set behaviors from cached nodes
            for (node_id, node) in &self.cached_nodes {
                ctx.set_behavior(node_id, node.capability_behavior());
            }

            self.resolution_ctx = Some(ctx);
        }

        let ctx = self.resolution_ctx.as_mut().unwrap();
        let resolver = CapabilityResolver::new(&self.registry);

        // Find RuntimeDiscovered nodes that have actual capabilities
        let runtime_discovered_nodes: Vec<String> = self.cached_nodes
            .iter()
            .filter(|(_, node)| matches!(node.capability_behavior(), CapabilityBehavior::RuntimeDiscovered))
            .map(|(id, _)| id.clone())
            .collect();

        for node_id in runtime_discovered_nodes {
            if let Some(node) = self.cached_nodes.get(&node_id) {
                // Get actual capabilities from the node after initialization
                if let Some(actual_caps) = node.actual_capabilities() {
                    tracing::info!(
                        "Session {}: Node '{}' reported actual capabilities, propagating to downstream nodes",
                        self.session_id,
                        node_id
                    );

                    // Update resolution context with actual capabilities and create pending updates
                    if let Err(e) = resolver.revalidate_and_propagate(ctx, &node_id, actual_caps) {
                        tracing::warn!(
                            "Session {}: Failed to propagate capabilities from '{}': {}",
                            self.session_id,
                            node_id,
                            e
                        );
                    }
                }
            }
        }

        // Apply pending updates to downstream nodes
        self.apply_pending_updates().await?;

        Ok(())
    }

    /// Apply pending capability updates to downstream nodes (spec 025).
    ///
    /// Iterates through all pending updates in the resolution context and calls
    /// `configure_from_upstream()` on each target node.
    pub async fn apply_pending_updates(&mut self) -> Result<()> {
        let ctx = match &mut self.resolution_ctx {
            Some(ctx) => ctx,
            None => return Ok(()), // No context, nothing to apply
        };

        if !ctx.has_pending_updates() {
            return Ok(());
        }

        // Get the list of nodes with pending updates
        let nodes_to_update: Vec<String> = ctx.nodes_with_pending_updates()
            .iter()
            .map(|s| s.to_string())
            .collect();

        tracing::debug!(
            "Session {}: Applying {} pending capability updates",
            self.session_id,
            nodes_to_update.len()
        );

        for node_id in nodes_to_update {
            // Take the pending update (removes from context)
            if let Some(notification) = ctx.take_pending_update(&node_id) {
                if let Some(node) = self.cached_nodes.get(&node_id) {
                    tracing::debug!(
                        "Session {}: Configuring '{}' from upstream '{}'",
                        self.session_id,
                        node_id,
                        notification.upstream_node_id
                    );

                    // Apply the upstream capabilities to this node
                    if let Err(e) = node.configure_from_upstream(&notification.upstream_output) {
                        tracing::warn!(
                            "Session {}: Failed to configure '{}' from upstream: {}",
                            self.session_id,
                            node_id,
                            e
                        );
                    }
                }
            }
        }

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
    /// Run the session router main loop
    ///
    /// This is the public entry point for starting the session router.
    /// It takes ownership of the input/shutdown channels and processes
    /// data through the pipeline graph.
    pub async fn run_public(&mut self) -> Result<()> {
        self.run().await
    }

    async fn run(&mut self) -> Result<()> {
        // Initialize all nodes before processing starts
        self.initialize_nodes().await?;

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

        // Stamp arrival timestamp on incoming data (spec 026)
        let arrival_ts_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let mut input_data = packet.data.clone();
        input_data.set_arrival_timestamp(arrival_ts_us);

        // Record drift metrics for timed media (spec 026)
        if input_data.is_timed_media() {
            self.record_drift_sample(&input_data).await;
        }

        // Track outputs from each node for routing to dependents
        let mut all_node_outputs: HashMap<String, Vec<RuntimeData>> = HashMap::new();

        // Determine which nodes receive the initial input
        if let Some(ref target_node) = packet.to_node {
            // Direct routing: send to specific node
            all_node_outputs.insert(target_node.clone(), vec![input_data.clone()]);
        } else {
            // Default: send to all source nodes (nodes with no inputs)
            for source_id in &self.graph.sources {
                all_node_outputs.insert(source_id.clone(), vec![input_data.clone()]);
            }
        }

        // Process nodes in topological order
        // Clone execution_order to avoid borrow issues
        let execution_order: Vec<String> = self.graph.execution_order.clone();
        for node_id in &execution_order {
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

            // Process each input through the node via scheduler (spec 026)
            for input in inputs {
                let node_outputs_clone = node_outputs_ref.clone();
                let _callback = Box::new(move |output: RuntimeData| -> crate::Result<()> {
                    node_outputs_clone.lock().unwrap().push(output);
                    Ok(())
                });

                let session_id = self.session_id.clone();
                let node_id_owned = node_id.clone();

                // Use scheduler for execution with timeout, retry, and circuit breaker
                // Choose fast path or full path based on node configuration
                let scheduler = self.scheduler.clone();
                let use_fast_path = scheduler.config.is_fast_path(&node_id_owned);

                let result = if use_fast_path {
                    // Fast path: lock-free, no timeout, no HDR metrics
                    let node_ref = node;
                    let input_clone = input.clone();
                    let session_clone = session_id.clone();
                    let cb = Box::new({
                        let outputs = node_outputs_ref.clone();
                        move |output: RuntimeData| {
                            outputs.lock().unwrap().push(output);
                            Ok(())
                        }
                    });
                    scheduler
                        .execute_streaming_node_fast(&node_id_owned, || async move {
                            node_ref
                                .process_streaming_async(input_clone, Some(session_clone), cb)
                                .await
                                .map(|_| ())
                        })
                        .await
                } else {
                    // Full path: timeout, retry, HDR histogram metrics
                    scheduler
                        .execute_streaming_node(&node_id_owned, || {
                            let node_ref = node;
                            let input_clone = input.clone();
                            let session_clone = session_id.clone();
                            let cb = Box::new({
                                let outputs = node_outputs_ref.clone();
                                move |output: RuntimeData| {
                                    outputs.lock().unwrap().push(output);
                                    Ok(())
                                }
                            });
                            async move {
                                node_ref
                                    .process_streaming_async(input_clone, Some(session_clone), cb)
                                    .await
                                    .map(|_| ())
                            }
                        })
                        .await
                };

                match result {
                    Ok(scheduler_result) => {
                        tracing::debug!(
                            "Session {}: Node '{}' executed in {}μs (retries: {})",
                            self.session_id,
                            node_id,
                            scheduler_result.duration_us,
                            scheduler_result.retry_count
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "Session {}: Node '{}' execution error: {}",
                            self.session_id,
                            node_id,
                            e
                        );
                        // Continue with other nodes - don't fail entire pipeline
                    }
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

    /// Record drift sample for timed media (spec 026)
    async fn record_drift_sample(&self, data: &RuntimeData) {
        let (media_ts_us, arrival_ts_us) = data.timing();

        // Need both timestamps to record drift
        let (media_ts, arrival_ts) = match (media_ts_us, arrival_ts_us) {
            (Some(m), Some(a)) => (m, a),
            _ => return,
        };

        // Get or create drift metrics for this stream
        let stream_id = data.stream_id().unwrap_or("default").to_string();

        // Get or create metrics for this stream
        let metrics = {
            let metrics_map = self.drift_metrics.read().await;
            if let Some(m) = metrics_map.get(&stream_id) {
                m.clone()
            } else {
                drop(metrics_map);
                let mut metrics_map = self.drift_metrics.write().await;
                // Double-check after acquiring write lock
                if let Some(m) = metrics_map.get(&stream_id) {
                    m.clone()
                } else {
                    let new_metrics = Arc::new(RwLock::new(DriftMetrics::new(
                        stream_id.clone(),
                        self.drift_thresholds.clone(),
                    )));
                    metrics_map.insert(stream_id.clone(), new_metrics.clone());
                    new_metrics
                }
            }
        };

        // Record the sample
        let mut metrics_guard = metrics.write().await;

        // Use appropriate method based on media type
        let alert_changed = if data.is_audio() {
            metrics_guard.record_audio_sample(media_ts, arrival_ts)
        } else if data.is_video() {
            // For video, we'd ideally compute a content hash, but for now use None
            metrics_guard.record_video_sample(media_ts, arrival_ts, None)
        } else {
            metrics_guard.record_sample(media_ts, arrival_ts, None)
        };

        if alert_changed {
            let alerts = metrics_guard.alerts();
            tracing::warn!(
                "Session {}: Stream '{}' alert state changed: {:?}, health_score: {:.2}",
                self.session_id,
                stream_id,
                alerts,
                metrics_guard.health_score()
            );
        }
    }

    // ==================== Metrics API (spec 026) ====================

    /// Get scheduler metrics (node execution stats)
    pub async fn get_scheduler_metrics(&self) -> crate::executor::metrics::PipelineMetrics {
        self.scheduler.get_metrics().await
    }

    /// Get node-level execution statistics
    pub async fn get_node_stats(&self, node_id: &str) -> Option<NodeStats> {
        self.scheduler.get_node_stats(node_id).await
    }

    /// Get all node statistics
    pub async fn get_all_node_stats(&self) -> HashMap<String, NodeStats> {
        self.scheduler.get_all_node_stats().await
    }

    /// Get drift metrics for a specific stream
    pub async fn get_drift_metrics(&self, stream_id: &str) -> Option<serde_json::Value> {
        let metrics_map = self.drift_metrics.read().await;
        if let Some(metrics) = metrics_map.get(stream_id) {
            let m = metrics.read().await;
            Some(m.to_debug_json())
        } else {
            None
        }
    }

    /// Get all stream IDs with drift metrics
    pub async fn get_stream_ids(&self) -> Vec<String> {
        let metrics_map = self.drift_metrics.read().await;
        metrics_map.keys().cloned().collect()
    }

    /// Export all metrics in Prometheus format
    pub async fn prometheus_metrics(&self) -> String {
        let mut output = String::new();

        // Scheduler metrics
        output.push_str(&self.scheduler.to_prometheus().await);

        // Drift metrics (aggregated - per-stream detail available via debug endpoint)
        let metrics_map = self.drift_metrics.read().await;
        if !metrics_map.is_empty() {
            output.push_str(&format!(
                "session_router_active_streams{{session_id=\"{}\"}} {}\n",
                self.session_id,
                metrics_map.len()
            ));

            // Aggregate health score (minimum across streams)
            let mut min_health = 1.0f64;
            for metrics in metrics_map.values() {
                let m = metrics.read().await;
                min_health = min_health.min(m.health_score());
            }
            output.push_str(&format!(
                "session_router_min_health_score{{session_id=\"{}\"}} {:.6}\n",
                self.session_id, min_health
            ));
        }

        output
    }

    /// Export per-stream debug metrics as JSON
    pub async fn debug_stream_metrics(&self) -> serde_json::Value {
        let metrics_map = self.drift_metrics.read().await;
        let mut streams = serde_json::Map::new();

        for (stream_id, metrics) in metrics_map.iter() {
            let m = metrics.read().await;
            streams.insert(stream_id.clone(), m.to_debug_json());
        }

        serde_json::json!({
            "session_id": self.session_id,
            "stream_count": metrics_map.len(),
            "streams": streams,
            "scheduler": {
                "max_concurrency": self.scheduler.config.max_concurrency,
                "available_permits": "N/A", // Can't get this synchronously
            }
        })
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

    // ==================== Spec 026 Integration Tests ====================

    #[test]
    fn test_session_router_with_scheduler_config() {
        // Test creating router with custom scheduler config
        let manifest = create_test_manifest(
            vec![("A", "TestNode"), ("B", "TestNode")],
            vec![("A", "B")],
        );

        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) = mpsc::unbounded_channel();

        let scheduler_config = SchedulerConfig::with_concurrency(8)
            .with_timeout(5000)
            .with_circuit_breaker_threshold(3);

        let result = SessionRouter::with_config(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
            Some(scheduler_config),
            None,
        );

        assert!(result.is_ok());
        let (router, _shutdown_tx) = result.unwrap();
        assert_eq!(router.scheduler.config.max_concurrency, 8);
        assert_eq!(router.scheduler.config.default_timeout_ms, 5000);
        assert_eq!(router.scheduler.config.circuit_breaker_threshold, 3);
    }

    #[test]
    fn test_session_router_with_drift_thresholds() {
        // Test creating router with custom drift thresholds
        let manifest = create_test_manifest(
            vec![("A", "TestNode")],
            vec![],
        );

        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) = mpsc::unbounded_channel();

        let drift_thresholds = DriftThresholds {
            slope_threshold_ms_per_s: 10.0,
            av_skew_threshold_us: 100_000,
            ..Default::default()
        };

        let result = SessionRouter::with_config(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
            None,
            Some(drift_thresholds.clone()),
        );

        assert!(result.is_ok());
        let (router, _shutdown_tx) = result.unwrap();
        assert_eq!(router.drift_thresholds.slope_threshold_ms_per_s, 10.0);
        assert_eq!(router.drift_thresholds.av_skew_threshold_us, 100_000);
    }

    #[tokio::test]
    async fn test_session_router_drift_metrics_creation() {
        // Test that drift metrics are created for streams
        let manifest = create_test_manifest(
            vec![("A", "TestNode")],
            vec![],
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

        // Initially no streams
        assert!(router.get_stream_ids().await.is_empty());

        // Record a drift sample manually by accessing internals
        let audio_data = RuntimeData::Audio {
            samples: vec![0.1; 100],
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("stream_1".to_string()),
            timestamp_us: Some(1_000_000),
            arrival_ts_us: Some(1_001_000),
        };

        router.record_drift_sample(&audio_data).await;

        // Now we should have one stream
        let stream_ids = router.get_stream_ids().await;
        assert_eq!(stream_ids.len(), 1);
        assert!(stream_ids.contains(&"stream_1".to_string()));

        // Get drift metrics for the stream
        let metrics = router.get_drift_metrics("stream_1").await;
        assert!(metrics.is_some());
        let metrics_json = metrics.unwrap();
        assert_eq!(metrics_json["stream_id"], "stream_1");
    }

    #[tokio::test]
    async fn test_session_router_prometheus_metrics() {
        // Test Prometheus metrics export
        let manifest = create_test_manifest(
            vec![("A", "TestNode")],
            vec![],
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

        // Record a sample to create stream metrics
        let audio_data = RuntimeData::Audio {
            samples: vec![0.1; 100],
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("test_stream".to_string()),
            timestamp_us: Some(1_000_000),
            arrival_ts_us: Some(1_000_000),
        };
        router.record_drift_sample(&audio_data).await;

        let prom = router.prometheus_metrics().await;

        // Should contain scheduler metrics
        assert!(prom.contains("streaming_scheduler_max_concurrency"));

        // Should contain router metrics since we have a stream
        assert!(prom.contains("session_router_active_streams"));
        assert!(prom.contains("session_router_min_health_score"));
    }

    #[tokio::test]
    async fn test_session_router_debug_stream_metrics() {
        // Test debug JSON metrics export
        let manifest = create_test_manifest(
            vec![("A", "TestNode")],
            vec![],
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

        // Record samples from two streams
        let audio_1 = RuntimeData::Audio {
            samples: vec![0.1; 100],
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("audio_1".to_string()),
            timestamp_us: Some(1_000_000),
            arrival_ts_us: Some(1_000_500),
        };
        let audio_2 = RuntimeData::Audio {
            samples: vec![0.2; 100],
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("audio_2".to_string()),
            timestamp_us: Some(2_000_000),
            arrival_ts_us: Some(2_001_000),
        };

        router.record_drift_sample(&audio_1).await;
        router.record_drift_sample(&audio_2).await;

        let debug_json = router.debug_stream_metrics().await;

        assert_eq!(debug_json["session_id"], "test-session");
        assert_eq!(debug_json["stream_count"], 2);
        assert!(debug_json["streams"]["audio_1"].is_object());
        assert!(debug_json["streams"]["audio_2"].is_object());
    }

    #[tokio::test]
    async fn test_session_router_node_stats() {
        // Test node statistics retrieval
        let manifest = create_test_manifest(
            vec![("A", "TestNode")],
            vec![],
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

        // Initially no node stats (no executions yet)
        let stats = router.get_node_stats("A").await;
        assert!(stats.is_none());

        // Get all node stats (empty initially)
        let all_stats = router.get_all_node_stats().await;
        assert!(all_stats.is_empty());
    }
}
