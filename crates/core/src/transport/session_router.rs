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
//! use remotemedia_core::transport::SessionRouter;
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
use crate::nodes::{InitializeContext, StreamingNode, StreamingNodeRegistry};
use crate::transport::session_control::{
    aux_port_of, CloseReason, SessionControl, BARGE_IN_PORT,
};
use crate::Result;
use parking_lot::RwLock as DriftRwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// Default capacity for the router's internal input channel.
///
/// Sized for ~160 ms of headroom at 48 kHz / 20 ms frames (8 frames × 20 ms).
/// Bounded channels apply block-producer backpressure on the transport side,
/// which is the desired behavior for real-time media pipelines — we'd rather
/// stall the ingress than grow memory unboundedly.
///
/// Override at session creation via `REMOTEMEDIA_ROUTER_INPUT_CAPACITY`.
pub const DEFAULT_ROUTER_INPUT_CAPACITY: usize = 8;

/// Default capacity for per-session client output channels.
///
/// Callers (`PipelineExecutor::create_session`, transport streaming handlers)
/// create the output channel and pass the sender in; this constant is the
/// recommended default. Same sizing rationale as the input capacity.
pub const DEFAULT_ROUTER_OUTPUT_CAPACITY: usize = 256;

/// Read the input-channel capacity from the environment, falling back to
/// [`DEFAULT_ROUTER_INPUT_CAPACITY`].
fn input_capacity_from_env() -> usize {
    std::env::var("REMOTEMEDIA_ROUTER_INPUT_CAPACITY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_ROUTER_INPUT_CAPACITY)
}

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

    /// Channel to send outputs to client.
    ///
    /// Bounded — caller picks the capacity when creating the channel. Sends
    /// block when full, producing natural backpressure toward the sink node.
    output_tx: mpsc::Sender<RuntimeData>,

    /// Channel to receive inputs from client.
    ///
    /// Bounded — see [`DEFAULT_ROUTER_INPUT_CAPACITY`].
    ///
    /// Wrapped in `std::sync::Mutex` so the router struct stays `Sync`
    /// once it is shared across per-packet spawn tasks (`tokio::spawn`
    /// requires `Arc<Self>: Send`, which requires `Self: Sync`). The
    /// receiver is `take()`-n once at the top of `run()` and used
    /// locally from then on, so lock contention is irrelevant.
    input_rx: std::sync::Mutex<Option<mpsc::Receiver<DataPacket>>>,

    /// Channel to send inputs to router (held by external code, dropped in run()).
    input_tx: Option<mpsc::Sender<DataPacket>>,

    /// Shutdown signal receiver. See [`Self::input_rx`] for why the
    /// receiver is wrapped in a `std::sync::Mutex`.
    shutdown_rx: std::sync::Mutex<Option<mpsc::Receiver<()>>>,

    /// Shutdown signal sender (held externally)
    _shutdown_tx: mpsc::Sender<()>,

    /// Capability resolution context (spec 025)
    /// Stores pending capability updates for downstream nodes
    resolution_ctx: Option<ResolutionContext>,

    /// StreamingScheduler for node execution with timeout, retry, circuit breaker (spec 026)
    scheduler: Arc<StreamingScheduler>,

    /// Per-stream drift metrics for health monitoring (spec 026).
    ///
    /// Map is `DashMap` (lock-free sharded hashmap). Phase B1 swaps
    /// the inner `tokio::sync::RwLock` for `parking_lot::RwLock` —
    /// no `.await` is ever held across the lock (writes finish in
    /// `record_sample` before any await), so the async RwLock was
    /// pure overhead on the per-packet path. `parking_lot::RwLock`
    /// uncontended fast path is a single CAS.
    ///
    /// Key: stream_id (from RuntimeData.stream_id or "default").
    drift_metrics: Arc<dashmap::DashMap<String, Arc<DriftRwLock<DriftMetrics>>>>,

    /// Real-time latency probes (Phase 0 of the tokio-off-data-plane
    /// migration). Records into `ingress` and `egress` today; `route_in`,
    /// `node_in`, `node_out` are reserved for follow-up wiring inside
    /// `process_input`.
    ///
    /// Accessed out-of-band via [`Self::probe_snapshots`]. Shared via
    /// `Arc` because consumers may want to poll from a separate task.
    probes: Arc<crate::metrics::RtProbeSet>,

    /// Drift thresholds for new streams
    drift_thresholds: DriftThresholds,

    /// Optional per-session control bus (client-side pub/sub/intercept).
    /// Attached via [`Self::attach_control`] after construction, before
    /// `start()`. When `None`, the router's hot path skips the control
    /// hook entirely — no overhead for sessions without attaches.
    control: Option<Arc<SessionControl>>,
}

/// Bounded capacity of each node's input channel.
///
/// Sized the same as the router's own ingress: enough to absorb a small burst
/// without drops, not so large that a stalled node hides backpressure. If a
/// node is a bottleneck, we want the upstream producer to block at `send`
/// rather than silently queuing megabytes of buffered audio.
const NODE_INPUT_CAPACITY: usize = 16;

/// Bounded capacity of each node's internal fan-out (callback → drain task).
///
/// Larger than the input because one input packet can produce many output
/// chunks (e.g. a TTS sentence → ~20 audio frames). `try_send` overflow
/// drops a chunk with a warning; sizing this generously avoids drops under
/// normal TTS/VAD burst patterns while still bounding memory growth.
const NODE_FANOUT_CAPACITY: usize = 1024;

/// Per-node task handles + input sender map, owned by the router for the
/// lifetime of the session. Built in [`SessionRouter::spawn_pipeline_tasks`]
/// and torn down in [`SessionRouter::teardown_pipeline_tasks`].
struct PipelineTasks {
    /// Input sender for each node (keyed by node id). The router pushes
    /// source-bound and `to_node`-addressed packets through these.
    input_txs: HashMap<String, mpsc::Sender<RuntimeData>>,
    /// All spawned tasks (main + fan-out per node). Awaited on shutdown.
    handles: Vec<JoinHandle<()>>,
}

impl SessionRouter {
    /// Create a new session router
    ///
    /// # Arguments
    ///
    /// * `session_id` - Unique identifier for this session
    /// * `manifest` - Pipeline manifest defining nodes and connections
    /// * `registry` - Registry for creating streaming nodes
    /// * `output_tx` - Bounded channel for sending outputs to the client.
    ///   Callers create this via `mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY)`
    ///   or similar; the capacity determines how much backpressure the sink
    ///   can absorb before the router's send awaits.
    ///
    /// # Returns
    ///
    /// * `Ok((router, shutdown_tx))` - Router and shutdown signal sender
    /// * `Err` - Graph validation failed (cycles, missing nodes, etc.)
    pub fn new(
        session_id: String,
        manifest: Arc<Manifest>,
        registry: Arc<StreamingNodeRegistry>,
        output_tx: mpsc::Sender<RuntimeData>,
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
    /// * `output_tx` - Bounded channel for sending outputs to the client.
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
        output_tx: mpsc::Sender<RuntimeData>,
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

        // Create input/shutdown channels.
        //
        // The input channel is bounded so that transport ingress applies
        // backpressure to the upstream (gRPC/WebRTC/etc.) when the pipeline
        // falls behind. Capacity is configurable via
        // `REMOTEMEDIA_ROUTER_INPUT_CAPACITY` (default 8 frames ≈ 160 ms at
        // 48 kHz/20 ms) — see [`DEFAULT_ROUTER_INPUT_CAPACITY`].
        let (input_tx, input_rx) = mpsc::channel(input_capacity_from_env());
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
            input_rx: std::sync::Mutex::new(Some(input_rx)),
            input_tx: Some(input_tx),
            shutdown_rx: std::sync::Mutex::new(Some(shutdown_rx)),
            _shutdown_tx: shutdown_tx,
            resolution_ctx: None,
            scheduler,
            drift_metrics: Arc::new(dashmap::DashMap::new()),
            probes: Arc::new(crate::metrics::RtProbeSet::new()),
            drift_thresholds: drift_thresholds.unwrap_or_default(),
            control: None,
        };

        Ok((router, shutdown_tx_clone))
    }

    /// Attach a [`SessionControl`] bus to this router.
    ///
    /// After this call, the control can:
    ///   - see every node output via `on_node_output` (tap + intercept)
    ///   - inject inputs via `publish` (the bus forwards to the router's
    ///     own input channel, with `to_node` set).
    ///
    /// Must be called before `start()` / `run()`. Safe to skip entirely —
    /// a router with `control = None` has zero control-bus overhead.
    pub async fn attach_control(&mut self, control: Arc<SessionControl>) {
        let input_tx = self
            .input_tx
            .clone()
            .expect("attach_control must be called before run() consumes input_tx");
        control.attach_input_sender(input_tx).await;
        self.control = Some(control);
    }

    /// Get the input sender for feeding data to the router.
    ///
    /// The returned sender is bounded; callers should `.send(...).await`
    /// and treat the back-pressure as real (drop-on-deadline is a policy
    /// decision for the transport ingress, not the router).
    pub fn get_input_sender(&self) -> mpsc::Sender<DataPacket> {
        self.input_tx.clone().expect("input_tx not yet consumed by run()")
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

        // Emit a "loading" event on the control bus so clients subscribed
        // to `__system__.out` can show an initializing indicator.
        self.emit_loading_event(
            "initializing",
            Some(format!("Starting {} nodes", self.manifest.nodes.len())),
        );

        for node_spec in &self.manifest.nodes {
            // Emit per-node loading event.
            self.emit_loading_event(
                "loading_node",
                Some(format!("Loading {} ({})", node_spec.id, node_spec.node_type)),
            );

            // Inject manifest-level python dependency info into params
            // so the multiprocess executor can provision the right venv
            let mut params = node_spec.params.clone();
            if let Some(ref py_deps) = node_spec.python_deps {
                if let Some(obj) = params.as_object_mut() {
                    obj.insert(
                        "__python_deps__".to_string(),
                        serde_json::json!(py_deps),
                    );
                }
            }
            if let Some(ref py_env) = self.manifest.python_env {
                if !py_env.extra_deps.is_empty() {
                    if let Some(obj) = params.as_object_mut() {
                        obj.insert(
                            "__python_extra_deps__".to_string(),
                            serde_json::json!(py_env.extra_deps),
                        );
                    }
                }
            }

            let node = self.registry.create_node(
                &node_spec.node_type,
                node_spec.id.clone(),
                &params,
                Some(self.session_id.clone()),
            )?;

            // Initialize the node (load models, etc.)
            // Pass the InitializeContext so nodes can emit progress events.
            let init_ctx = InitializeContext {
                session_id: self.session_id.clone(),
                node_id: node_spec.id.clone(),
                control: self.control.clone(),
            };
            if let Err(e) = node.initialize(&init_ctx).await {
                tracing::error!(
                    session_id = %self.session_id,
                    node_id = %node_spec.id,
                    node_type = %node_spec.node_type,
                    error = %e,
                    "Node initialization failed; aborting session startup"
                );
                self.emit_loading_event(
                    "error",
                    Some(format!(
                        "Node '{}' ({}) failed to initialize: {}",
                        node_spec.id, node_spec.node_type, e
                    )),
                );
                return Err(e);
            }

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

        // Emit a "ready" event: pipeline is fully loaded and processing.
        self.emit_loading_event("ready", Some("Pipeline ready".to_string()));

        // Spec 025: After initialization, propagate actual capabilities from
        // RuntimeDiscovered nodes to downstream Adaptive/Passthrough nodes
        self.propagate_runtime_capabilities().await?;

        Ok(())
    }

    /// Emit a loading-state event on the control bus.
    ///
    /// Clients subscribed to `__system__.out` receive these as JSON events
    /// with `kind: "loading"`. This lets the frontend show an initializing
    /// indicator while nodes load models, then switch to "ready" once the
    /// pipeline is fully loaded.
    fn emit_loading_event(&self, status: &str, message: Option<String>) {
        if let Some(ctrl) = &self.control {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let event = RuntimeData::Json(serde_json::json!({
                "kind": "loading",
                "status": status,
                "message": message,
                "ts_ms": ts,
            }));
            ctrl.publish_tap("__system__", None, event);
        }
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
    pub fn start(self) -> JoinHandle<()> {
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
    pub async fn run_public(self) -> Result<()> {
        self.run().await
    }

    async fn run(mut self) -> Result<()> {
        // Initialize all nodes before processing starts
        self.initialize_nodes().await?;

        // Spawn one task per node, wired through mpsc channels along the
        // manifest connections. From here on, every node runs concurrently:
        // an audio chunk yielded by node A reaches node B's input_rx on the
        // same scheduler tick, instead of being batched into a `Vec` until
        // A's `process_streaming_async` returns. Sink yields go straight to
        // the client `output_tx`. See `spawn_pipeline_tasks` for the wiring.
        let pipeline = self.spawn_pipeline_tasks();

        // Drop router's own input_tx so the transport channel closes when
        // all external senders (SessionHandle) are dropped.
        self.input_tx.take();

        let mut input_rx = self
            .input_rx
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| crate::Error::Execution("Input channel already taken".to_string()))?;

        let mut shutdown_rx = self
            .shutdown_rx
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| crate::Error::Execution("Shutdown channel already taken".to_string()))?;

        tracing::info!(
            "Session {}: Router running, waiting for input...",
            self.session_id
        );

        // Single-threaded ingress loop. We no longer spawn per-packet tasks
        // because work is performed by per-node tasks; the router's only
        // job here is to shovel input packets into the right source node's
        // input channel. Each node's bounded input mpsc provides natural
        // backpressure back through `input_rx`.
        loop {
            let ingress_start = std::time::Instant::now();
            tokio::select! {
                biased;

                result = input_rx.recv() => {
                    match result {
                        Some(packet) => {
                            self.probes.ingress.record_since(ingress_start);
                            self.route_input(packet, &pipeline.input_txs).await;
                        }
                        None => {
                            tracing::warn!(
                                "Session {}: Input channel closed (all senders dropped), shutting down pipeline",
                                self.session_id
                            );
                            break;
                        }
                    }
                }
                result = shutdown_rx.recv() => {
                    match result {
                        Some(()) => {
                            tracing::info!(
                                "Session {}: Shutdown signal received, closing pipeline",
                                self.session_id
                            );
                        }
                        None => {
                            tracing::warn!(
                                "Session {}: Shutdown channel closed (sender dropped), closing pipeline",
                                self.session_id
                            );
                        }
                    }
                    break;
                }
            }
        }

        // Graceful teardown. Dropping every source input_tx cascades through
        // the pipeline: each node's main task exits when its input_rx
        // closes, which drops its fan_tx, which closes the next hop.
        Self::teardown_pipeline_tasks(pipeline).await;

        // Wake every attached control client so they drain and exit.
        // Idempotent — harmless if no control is attached.
        if let Some(ctrl) = &self.control {
            ctrl.signal_close(CloseReason::Normal);
        }

        Ok(())
    }

    /// Build the per-node task pipeline.
    ///
    /// For every node in the graph we create a bounded input mpsc. We then
    /// walk the manifest connections to decide each node's successor set.
    /// Sinks additionally get a send-to-client hop.
    ///
    /// Drains `self.cached_nodes` — the node instances are moved into their
    /// owning tasks, so we can no longer borrow them from `self`. That's
    /// fine; after this call the router's only remaining role is shovelling
    /// packets into the source nodes' input channels.
    fn spawn_pipeline_tasks(&mut self) -> PipelineTasks {
        let mut input_txs: HashMap<String, mpsc::Sender<RuntimeData>> = HashMap::new();
        let mut input_rxs: HashMap<String, mpsc::Receiver<RuntimeData>> = HashMap::new();

        // Pass 1: create an input channel for every node we have cached.
        for node_id in self.cached_nodes.keys() {
            let (tx, rx) = mpsc::channel::<RuntimeData>(NODE_INPUT_CAPACITY);
            input_txs.insert(node_id.clone(), tx);
            input_rxs.insert(node_id.clone(), rx);
        }

        // Pass 2: compute each node's successor set from the graph. Fan-out
        // is native — one output gets cloned to every successor's input
        // channel. `successors[from]` yields a list of `input_txs[to]`
        // clones.
        let mut successors: HashMap<String, Vec<mpsc::Sender<RuntimeData>>> = HashMap::new();
        for node_id in self.cached_nodes.keys() {
            let sends = self
                .graph
                .nodes
                .get(node_id)
                .map(|gn| {
                    gn.outputs
                        .iter()
                        .filter_map(|succ| input_txs.get(succ).cloned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            successors.insert(node_id.clone(), sends);
        }

        let sinks: std::collections::HashSet<String> =
            self.graph.sinks.iter().cloned().collect();

        // Drain node instances out of the cache. Each task owns its node.
        let nodes = std::mem::take(&mut self.cached_nodes);

        let mut handles = Vec::with_capacity(nodes.len() * 2);

        for (node_id, node) in nodes {
            let input_rx = match input_rxs.remove(&node_id) {
                Some(rx) => rx,
                None => {
                    tracing::error!(
                        "Session {}: node '{}' has no input channel (bug)",
                        self.session_id,
                        node_id
                    );
                    continue;
                }
            };
            let succ_txs = successors.remove(&node_id).unwrap_or_default();
            let is_sink = sinks.contains(&node_id);

            let (main_handle, fan_handle) = Self::spawn_node_pipeline(
                node_id,
                node,
                input_rx,
                succ_txs,
                if is_sink {
                    Some(self.output_tx.clone())
                } else {
                    None
                },
                self.session_id.clone(),
                self.scheduler.clone(),
                self.control.clone(),
                self.probes.clone(),
            );

            handles.push(main_handle);
            handles.push(fan_handle);
        }

        PipelineTasks { input_txs, handles }
    }

    /// Spawn the two tasks that drive a single node:
    ///
    /// 1. **main**: owns the node instance, consumes `input_rx`, and calls
    ///    `process_streaming_async` once per input. The scheduler wraps the
    ///    call so timeouts, retries, circuit breakers, and metrics still
    ///    apply. The sync callback passed into the node `try_send`s each
    ///    yield into an internal fan-out channel (`fan_tx`).
    ///
    /// 2. **fan_out**: drains `fan_rx`, applies the control-bus hook (tap +
    ///    intercept), and forwards surviving outputs to every successor's
    ///    input channel AND — for sinks — to the client `output_tx`. The
    ///    hook must run async, so it cannot live inside the sync callback;
    ///    that's why we need the second task.
    ///
    /// Returns both `JoinHandle`s so the router can await clean shutdown.
    fn spawn_node_pipeline(
        node_id: String,
        node: Box<dyn StreamingNode>,
        mut input_rx: mpsc::Receiver<RuntimeData>,
        successor_txs: Vec<mpsc::Sender<RuntimeData>>,
        client_tx: Option<mpsc::Sender<RuntimeData>>,
        session_id: String,
        scheduler: Arc<StreamingScheduler>,
        control: Option<Arc<SessionControl>>,
        probes: Arc<crate::metrics::RtProbeSet>,
    ) -> (JoinHandle<()>, JoinHandle<()>) {
        let (fan_tx, mut fan_rx) = mpsc::channel::<RuntimeData>(NODE_FANOUT_CAPACITY);

        // ── Fan-out drain task ─────────────────────────────────────────
        let fan_node_id = node_id.clone();
        let fan_session_id = session_id.clone();
        let fan_control = control.clone();
        let fan_probes = probes.clone();
        let fan_handle = tokio::spawn(async move {
            while let Some(out) = fan_rx.recv().await {
                let kept = match &fan_control {
                    Some(ctrl) => ctrl.on_node_output(&fan_node_id, None, out).await,
                    None => Some(out),
                };
                let Some(kept) = kept else { continue };

                // Fan out to successors first. Bounded `send` awaits on
                // full, providing real backpressure all the way back to
                // the node's callback (via `fan_tx` filling up).
                for tx in &successor_txs {
                    if tx.send(kept.clone()).await.is_err() {
                        tracing::debug!(
                            "Session {}: node '{}' successor closed; drop",
                            fan_session_id, fan_node_id
                        );
                    }
                }

                // Sinks: also forward to the client.
                if let Some(ref out_tx) = client_tx {
                    let egress_start = std::time::Instant::now();
                    let res = out_tx.send(kept).await;
                    fan_probes.egress.record_since(egress_start);
                    if res.is_err() {
                        tracing::warn!(
                            "Session {}: sink '{}' client channel closed",
                            fan_session_id, fan_node_id
                        );
                    }
                }
            }
        });

        // ── Aux-port filter task ───────────────────────────────────────
        //
        // Sits between the router-level `input_rx` and the main node
        // task. It does TWO things every node benefits from for free:
        //
        //   1. `barge_in` envelopes are intercepted here. They never
        //      reach the node's `process_*`. Instead they fire
        //      `cancel.notify_waiters()` which aborts whatever the
        //      node is currently doing (see select! below). This is
        //      the universal cancellation primitive — every node
        //      becomes preemptible without any per-node code.
        //
        //   2. All other frames (normal data + non-barge aux ports
        //      like `context` for nodes that opt into them) are
        //      forwarded unchanged. Nodes that consume aux ports
        //      (e.g. lfm2_text, qwen_tts_mlx) keep working.
        //
        // Putting this filter in the runtime — rather than asking
        // every node to recognise barge envelopes — was a direct
        // response to barge frames leaking into LLM prompts and TTS
        // synthesis whenever a node forgot to filter them.
        let cancel = Arc::new(tokio::sync::Notify::new());
        let (filt_tx, mut filt_rx) =
            mpsc::channel::<RuntimeData>(NODE_FANOUT_CAPACITY);
        let filter_cancel = Arc::clone(&cancel);
        let filter_node_id = node_id.clone();
        let filter_session_id = session_id.clone();
        let filter_handle = tokio::spawn(async move {
            while let Some(input) = input_rx.recv().await {
                if let Some(port) = aux_port_of(&input) {
                    if port == BARGE_IN_PORT {
                        tracing::debug!(
                            session_id = %filter_session_id,
                            node_id = %filter_node_id,
                            "Runtime: barge_in received — cancelling in-flight call"
                        );
                        // Wake whatever future is currently waiting in
                        // the dispatch select!. If nothing is running,
                        // this is a no-op (notify_waiters does NOT
                        // queue a permit), which is what we want.
                        filter_cancel.notify_waiters();
                        continue;
                    }
                    // Other aux ports fall through — node-side aux
                    // handlers (e.g. `_handle_aux_port` in Python
                    // nodes) still receive them.
                }
                if filt_tx.send(input).await.is_err() {
                    break;
                }
            }
        });

        // ── Main node task ─────────────────────────────────────────────
        let main_node_id = node_id.clone();
        let main_session_id = session_id.clone();
        let main_cancel = Arc::clone(&cancel);
        let main_handle = tokio::spawn(async move {
            // Move the node into a raw pointer so we can hand out `&`
            // references to scheduler closures without fighting the
            // borrow checker. Safety: we never drop or share this Box
            // across tasks. The single owning task uses it through `&*`.
            let node = node; // bind
            let node_ref: &dyn StreamingNode = &*node;

            while let Some(input) = filt_rx.recv().await {
                // Node-state gate (Bypass / Disabled). Per-input, so a
                // runtime control-bus toggle takes effect on the next
                // packet.
                if let Some(ctrl) = &control {
                    use crate::transport::session_control::NodeState;
                    match ctrl.node_state(&main_node_id) {
                        NodeState::Enabled => {}
                        NodeState::Bypass => {
                            if fan_tx.send(input).await.is_err() {
                                break;
                            }
                            continue;
                        }
                        NodeState::Disabled => {
                            continue;
                        }
                    }
                }

                // Per-input sync callback: try_send into the fan-out
                // channel. A full `fan_tx` means the drain task is
                // behind — we warn and drop. In practice this only
                // happens under catastrophic downstream stalls; the
                // 1024-slot buffer covers normal burst patterns.
                let cb_fan_tx = fan_tx.clone();
                let cb_node_id = main_node_id.clone();
                let cb = Box::new(move |out: RuntimeData| {
                    if let Err(e) = cb_fan_tx.try_send(out) {
                        tracing::warn!(
                            "node '{}' fan_tx backpressure drop: {}",
                            cb_node_id, e
                        );
                    }
                    Ok(())
                });

                let use_fast = scheduler.config.is_fast_path(&main_node_id);
                let node_dispatch_start = std::time::Instant::now();

                // Build the dispatch future (not yet awaited). Once
                // we await it inside the select!, dropping it on
                // cancel will run all destructors — including HTTP
                // streams, multiprocess IPC sends in flight, etc. —
                // so cancellation is genuine, not just gated output.
                let dispatch_fut = async {
                    if use_fast {
                        let input_clone = input.clone();
                        let session_clone = main_session_id.clone();
                        scheduler
                            .execute_streaming_node_fast(&main_node_id, || async move {
                                node_ref
                                    .process_streaming_async(
                                        input_clone,
                                        Some(session_clone),
                                        cb,
                                    )
                                    .await
                                    .map(|_| ())
                            })
                            .await
                    } else {
                        // Full path retries the closure on transient failures,
                        // so each retry rebuilds the callback + input clone.
                        let input_outer = input.clone();
                        let session_outer = main_session_id.clone();
                        let cb_outer = std::sync::Mutex::new(Some(cb));
                        scheduler
                            .execute_streaming_node(&main_node_id, || {
                                let input_clone = input_outer.clone();
                                let session_clone = session_outer.clone();
                                // Take the callback for the first attempt;
                                // subsequent retries rebuild a fresh one
                                // against the same fan_tx so yields from a
                                // retry still reach the drain task.
                                let cb_taken = cb_outer.lock().unwrap().take();
                                let cb_for_attempt: Box<
                                    dyn FnMut(RuntimeData) -> Result<()> + Send,
                                > = match cb_taken {
                                    Some(c) => c,
                                    None => {
                                        let fan_tx = fan_tx.clone();
                                        let id = main_node_id.clone();
                                        Box::new(move |out| {
                                            if let Err(e) = fan_tx.try_send(out) {
                                                tracing::warn!(
                                                    "node '{}' fan_tx backpressure drop: {}",
                                                    id, e
                                                );
                                            }
                                            Ok(())
                                        })
                                    }
                                };
                                async move {
                                    node_ref
                                        .process_streaming_async(
                                            input_clone,
                                            Some(session_clone),
                                            cb_for_attempt,
                                        )
                                        .await
                                        .map(|_| ())
                                }
                            })
                            .await
                    }
                };

                let cancelled = tokio::select! {
                    biased;
                    _ = main_cancel.notified() => {
                        tracing::info!(
                            session_id = %main_session_id,
                            node_id = %main_node_id,
                            "Runtime: in-flight call cancelled by barge_in"
                        );
                        // Dropping the dispatch_fut here cascades drop
                        // through every awaited future inside the
                        // node call (HTTP streams, IPC sends, etc.),
                        // which is the cancellation mechanism.
                        true
                    }
                    r = dispatch_fut => {
                        if let Err(e) = r {
                            tracing::error!(
                                "Session {}: node '{}' execution error: {}",
                                main_session_id,
                                main_node_id,
                                e
                            );
                        }
                        false
                    }
                };
                let _ = cancelled;

                probes.node_out.record_since(node_dispatch_start);
            }
            // input_rx closed: drop fan_tx so the drain task exits once
            // it finishes forwarding any already-queued outputs.
            drop(fan_tx);
            // Filter task ends when input_rx closes (its sender drops
            // after the last source emits None). Await it so we don't
            // leak it on shutdown.
            let _ = filter_handle.await;
        });

        (main_handle, fan_handle)
    }

    /// Dispatch an incoming [`DataPacket`] to the right source/target
    /// node's input channel. Stamps the arrival timestamp and records a
    /// drift sample if the payload is timed media.
    async fn route_input(
        &self,
        packet: DataPacket,
        input_txs: &HashMap<String, mpsc::Sender<RuntimeData>>,
    ) {
        let arrival_ts_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let mut input_data = packet.data.clone();
        input_data.set_arrival_timestamp(arrival_ts_us);

        if input_data.is_timed_media() {
            self.record_drift_sample(&input_data).await;
        }

        let targets: Vec<&str> = if let Some(ref target) = packet.to_node {
            vec![target.as_str()]
        } else {
            self.graph.sources.iter().map(|s| s.as_str()).collect()
        };

        for target in targets {
            let Some(tx) = input_txs.get(target) else {
                tracing::warn!(
                    "Session {}: input routed to unknown node '{}'",
                    self.session_id,
                    target
                );
                continue;
            };
            if tx.send(input_data.clone()).await.is_err() {
                tracing::warn!(
                    "Session {}: node '{}' input channel closed; drop packet",
                    self.session_id,
                    target
                );
            }
        }
    }

    /// Await every spawned task after dropping the router's own input_txs.
    async fn teardown_pipeline_tasks(pipeline: PipelineTasks) {
        let PipelineTasks { input_txs, handles } = pipeline;
        // Drop the router's copies of every node's input sender; each source
        // node sees `input_rx.recv()` return `None` and exits, cascading
        // through the graph.
        drop(input_txs);
        for h in handles {
            let _ = h.await;
        }
    }

    /// Snapshot all RT latency probes in declaration order:
    /// `ingress, route_in, node_in, node_out, egress`.
    ///
    /// `ingress`, `egress`, and `node_out` are actively recorded;
    /// `route_in` and `node_in` will be wired as A-wave migrations
    /// land and the dispatch path gets more inspectable.
    pub fn probe_snapshots(
        &self,
    ) -> [(&'static str, crate::metrics::ProbeSnapshot); 5] {
        self.probes.snapshot_all()
    }

    /// Snapshot the router's operational counters (`spawn_count`,
    /// `loopback_depth`). Core router doesn't currently spawn per
    /// packet, so `spawn_count` stays at 0 — useful as a baseline and
    /// to flag regressions if any future code adds a per-packet spawn.
    pub fn operational_snapshot(&self) -> crate::metrics::OperationalSnapshot {
        self.probes.operational_snapshot()
    }

    /// Clone the router's probe handle.
    ///
    /// [`Self::start`] consumes the router, so anything that wants to
    /// read probe state *after* the router is running (benches, admin
    /// endpoints, test harnesses) needs the `Arc` before the `start`
    /// call. `probes().snapshot_all()` / `probes().operational_snapshot()`
    /// is the out-of-band equivalent of the `&self` accessors above.
    pub fn probes(&self) -> Arc<crate::metrics::RtProbeSet> {
        Arc::clone(&self.probes)
    }

    // The old `process_input` (topological per-packet loop with per-node
    // Vec batching) has been deleted. Per-node concurrent execution now
    // lives in `spawn_pipeline_tasks` + `spawn_node_pipeline`; ingress
    // routing is `route_input`.


    /// Record drift sample for timed media (spec 026).
    ///
    /// Phase B1: inner lock is `parking_lot::RwLock`; no `.await` is
    /// held across it. `async` kept on the method only so callers
    /// that already use `.await` here don't need to change.
    async fn record_drift_sample(&self, data: &RuntimeData) {
        let (media_ts_us, arrival_ts_us) = data.timing();

        // Need both timestamps to record drift
        let (media_ts, arrival_ts) = match (media_ts_us, arrival_ts_us) {
            (Some(m), Some(a)) => (m, a),
            _ => return,
        };

        // Get or create drift metrics for this stream
        let stream_id = data.stream_id().unwrap_or("default").to_string();

        // Lock-free per-packet lookup via DashMap. `entry().or_insert_with`
        // atomically gets-or-creates the per-stream metrics without the
        // read-upgrade-write double-check pattern the old RwLock needed.
        let metrics = self
            .drift_metrics
            .entry(stream_id.clone())
            .or_insert_with(|| {
                Arc::new(DriftRwLock::new(DriftMetrics::new(
                    stream_id.clone(),
                    self.drift_thresholds.clone(),
                )))
            })
            .clone();

        // Sync write lock — parking_lot, uncontended CAS fast path.
        let mut metrics_guard = metrics.write();

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

    /// Get drift metrics for a specific stream.
    ///
    /// `async` is preserved for API stability; inner `read()` is sync
    /// parking_lot (B1).
    pub async fn get_drift_metrics(&self, stream_id: &str) -> Option<serde_json::Value> {
        let metrics = self.drift_metrics.get(stream_id)?.clone();
        let m = metrics.read();
        Some(m.to_debug_json())
    }

    /// Get all stream IDs with drift metrics
    pub async fn get_stream_ids(&self) -> Vec<String> {
        self.drift_metrics
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    /// Export all metrics in Prometheus format
    pub async fn prometheus_metrics(&self) -> String {
        let mut output = String::new();

        // Scheduler metrics
        output.push_str(&self.scheduler.to_prometheus().await);

        // Drift metrics (aggregated — per-stream detail available via debug endpoint).
        // Snapshot via DashMap::iter (no global lock) so we don't hold
        // the shard guard across the per-stream reads.
        let stream_metrics: Vec<Arc<DriftRwLock<DriftMetrics>>> = self
            .drift_metrics
            .iter()
            .map(|e| e.value().clone())
            .collect();
        if !stream_metrics.is_empty() {
            output.push_str(&format!(
                "session_router_active_streams{{session_id=\"{}\"}} {}\n",
                self.session_id,
                stream_metrics.len()
            ));

            // Aggregate health score (minimum across streams). Sync
            // reads via parking_lot.
            let mut min_health = 1.0f64;
            for metrics in &stream_metrics {
                let m = metrics.read();
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
        // Snapshot pairs first so the per-stream reads happen off the
        // DashMap shard guard (even with sync reads now, we keep the
        // clone-and-iterate pattern).
        let pairs: Vec<(String, Arc<DriftRwLock<DriftMetrics>>)> = self
            .drift_metrics
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        let stream_count = pairs.len();
        let mut streams = serde_json::Map::new();
        for (stream_id, metrics) in pairs {
            let m = metrics.read();
            streams.insert(stream_id, m.to_debug_json());
        }

        serde_json::json!({
            "session_id": self.session_id,
            "stream_count": stream_count,
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
            python_env: None,
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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
            samples: vec![0.1; 100].into(),
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("stream_1".to_string()),
            timestamp_us: Some(1_000_000),
            arrival_ts_us: Some(1_001_000),
            metadata: None,
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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

        let (router, _shutdown_tx) = SessionRouter::new(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
        )
        .unwrap();

        // Record a sample to create stream metrics
        let audio_data = RuntimeData::Audio {
            samples: vec![0.1; 100].into(),
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("test_stream".to_string()),
            timestamp_us: Some(1_000_000),
            arrival_ts_us: Some(1_000_000),
            metadata: None,
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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

        let (router, _shutdown_tx) = SessionRouter::new(
            "test-session".to_string(),
            Arc::new(manifest),
            registry,
            output_tx,
        )
        .unwrap();

        // Record samples from two streams
        let audio_1 = RuntimeData::Audio {
            samples: vec![0.1; 100].into(),
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("audio_1".to_string()),
            timestamp_us: Some(1_000_000),
            arrival_ts_us: Some(1_000_500),
            metadata: None,
        };
        let audio_2 = RuntimeData::Audio {
            samples: vec![0.2; 100].into(),
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("audio_2".to_string()),
            timestamp_us: Some(2_000_000),
            arrival_ts_us: Some(2_001_000),
            metadata: None,
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
        let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

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
