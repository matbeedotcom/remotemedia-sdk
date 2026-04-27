//! PipelineExecutor - Unified facade for transport layers
//!
//! This module provides:
//! - SessionHandle for streaming sessions
//! - PipelineExecutor as unified entry point
//! - Unary and streaming execution modes
//! - Factory registration support
//!
//! # Usage
//!
//! ```ignore
//! let executor = PipelineExecutor::new()?;
//! let result = executor.execute_unary(manifest, input).await?;
//! ```
//!
//! # Architecture
//!
//! PipelineExecutor wraps SessionRouter with StreamingScheduler to provide:
//! - Production-grade execution with timeout/retry/circuit breaker
//! - DriftMetrics for stream health monitoring
//! - Unified API for all transports (HTTP, gRPC, WebRTC, FFI)
//!
//! # Spec Reference
//!
//! See `/specs/026-streaming-scheduler-migration/` for full specification.

use crate::data::RuntimeData;
use crate::executor::streaming_scheduler::{SchedulerConfig, StreamingScheduler};
use crate::executor::DriftThresholds;
use crate::manifest::Manifest;
use crate::nodes::{StreamingNodeFactory, StreamingNodeRegistry};
use crate::transport::session_control::{SessionControl, SessionControlBus};
use crate::transport::session_router::{DataPacket, SessionRouter};
use crate::transport::TransportData;
use crate::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

/// Configuration for PipelineExecutor
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Scheduler configuration
    pub scheduler_config: SchedulerConfig,
    /// Drift metrics thresholds
    pub drift_thresholds: DriftThresholds,
    /// Enable drift metrics collection
    pub enable_drift_metrics: bool,
    /// Session ID prefix for generated sessions
    pub session_id_prefix: String,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            scheduler_config: SchedulerConfig::default(),
            drift_thresholds: DriftThresholds::default(),
            enable_drift_metrics: true,
            session_id_prefix: "session".to_string(),
        }
    }
}

/// Handle to an active streaming session
///
/// SessionHandle provides a type-safe interface for streaming data
/// through a pipeline session. It owns the channels and task handle.
pub struct SessionHandle {
    /// Unique session identifier
    pub session_id: String,
    /// Channel for sending input data to the session (None after input complete).
    ///
    /// Bounded — see `DEFAULT_ROUTER_INPUT_CAPACITY` in `session_router`.
    input_tx: Option<mpsc::Sender<DataPacket>>,
    /// Channel for receiving output data from the session.
    ///
    /// Bounded — see `DEFAULT_ROUTER_OUTPUT_CAPACITY` in `session_router`.
    output_rx: mpsc::Receiver<RuntimeData>,
    /// Shutdown signal sender
    shutdown_tx: mpsc::Sender<()>,
    /// Handle to the session router task
    task_handle: JoinHandle<Result<()>>,
    /// Whether the session is still active
    is_active: bool,
    /// Active trace recorder, if `REMOTEMEDIA_RECORD_DIR` was set when
    /// the session was created. Held here so its tap subscriptions +
    /// writer task live exactly as long as the session does; on drop
    /// the writer finishes the JSONL file and the tap relays exit.
    _recorder: Option<crate::transport::session_recorder::SessionRecorder>,
}

impl SessionHandle {
    /// Send input data to the session.
    ///
    /// The data will be processed through the pipeline and outputs
    /// will be available via `recv_output()`.
    ///
    /// # **REAL-TIME UNSAFE**
    ///
    /// This method is `async` and awaits on a bounded tokio channel. It
    /// must not be called from a real-time-priority thread (Core Audio
    /// HAL IO proc, AU render callback, JACK process callback, AAudio
    /// data callback, etc.) — `.await` returns control to the tokio
    /// scheduler, and a full queue parks the caller. For RT audio hosts,
    /// use the [`remotemedia-rt-bridge`] crate, which pumps data from
    /// RT threads into the async pipeline through pinned-thread SPSC
    /// rings, or call [`crate::nodes::process_sync`] directly on a
    /// [`crate::nodes::SyncStreamingNode`] to skip the executor entirely.
    pub async fn send_input(&self, data: TransportData) -> Result<()> {
        if !self.is_active {
            return Err(crate::Error::Execution("Session is closed".to_string()));
        }

        let packet = DataPacket {
            data: data.data,
            from_node: "client".to_string(),
            to_node: None, // Route to sources
            session_id: self.session_id.clone(),
            sequence: data.sequence.unwrap_or(0),
            sub_sequence: data.sequence.unwrap_or(0),
        };

        let tx = self.input_tx.as_ref().ok_or_else(|| {
            crate::Error::Execution("Input channel closed (input complete signalled)".to_string())
        })?;
        // Bounded channel: this `.await` is the ingress backpressure point.
        // When the router's input queue is full, the producer stalls here
        // rather than growing memory unboundedly.
        tx.send(packet).await.map_err(|e| {
            crate::Error::Execution(format!("Failed to send input: {}", e))
        })?;

        Ok(())
    }

    /// Receive output data from the session
    ///
    /// Returns `None` if the session is closed or no more outputs are available.
    pub async fn recv_output(&mut self) -> Result<Option<TransportData>> {
        match self.output_rx.recv().await {
            Some(data) => Ok(Some(TransportData::new(data))),
            None => Ok(None),
        }
    }

    /// Try to receive output data without blocking
    ///
    /// Returns `None` if no output is immediately available.
    pub fn try_recv_output(&mut self) -> Result<Option<TransportData>> {
        match self.output_rx.try_recv() {
            Ok(data) => Ok(Some(TransportData::new(data))),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => Ok(None),
        }
    }

    /// Check if the session is still active
    pub fn is_active(&self) -> bool {
        self.is_active && !self.task_handle.is_finished()
    }

    /// Signal that no more input will be sent
    ///
    /// This closes the input channel, allowing the session router to detect
    /// end-of-input and shut down gracefully after processing remaining data.
    /// Outputs can still be received via `recv_output()` after calling this.
    pub fn signal_input_complete(&mut self) {
        self.input_tx = None;
    }

    /// Clone-able, send-only handle onto this session's input.
    ///
    /// Transport adapters (WebRTC, gRPC) need to forward inputs on one
    /// task while draining outputs on another — without this split, a
    /// full router input channel blocks the same task that's supposed
    /// to be pulling outputs, which can deadlock if the router's
    /// output channel is also full (classic bounded-channel ring
    /// deadlock). This returns a lightweight handle that owns a clone
    /// of the input `Sender` and can be moved into its own task.
    ///
    /// Returns `None` after `signal_input_complete()` has been called.
    pub fn input_sender(&self) -> Option<SessionInputSender> {
        self.input_tx
            .as_ref()
            .map(|tx| SessionInputSender {
                tx: tx.clone(),
                session_id: self.session_id.clone(),
            })
    }

    /// Close the session gracefully
    pub async fn close(&mut self) -> Result<()> {
        if !self.is_active {
            return Ok(());
        }

        self.is_active = false;

        // Send shutdown signal
        let _ = self.shutdown_tx.send(()).await;

        Ok(())
    }

    /// Wait for the session to complete
    pub async fn wait(self) -> Result<()> {
        self.task_handle.await.map_err(|e| {
            crate::Error::Execution(format!("Session task panicked: {}", e))
        })?
    }
}

/// Clone-able, send-only side of a [`SessionHandle`].
///
/// See [`SessionHandle::input_sender`] for why this exists.
#[derive(Clone)]
pub struct SessionInputSender {
    tx: mpsc::Sender<DataPacket>,
    session_id: String,
}

impl SessionInputSender {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Send input data. Blocks when the router input channel is full
    /// — call from a dedicated task so a full queue doesn't wedge
    /// the output-drain loop.
    pub async fn send(&self, data: TransportData) -> Result<()> {
        let packet = DataPacket {
            data: data.data,
            from_node: "client".to_string(),
            to_node: None,
            session_id: self.session_id.clone(),
            sequence: data.sequence.unwrap_or(0),
            sub_sequence: data.sequence.unwrap_or(0),
        };
        self.tx.send(packet).await.map_err(|e| {
            crate::Error::Execution(format!("Failed to send input: {}", e))
        })
    }
}

// Implement StreamSession for SessionHandle to allow use in PipelineTransport
#[async_trait::async_trait]
impl crate::transport::StreamSession for SessionHandle {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn send_input(&mut self, data: TransportData) -> Result<()> {
        // SessionHandle::send_input takes &self, so we can call it with &mut self
        <SessionHandle>::send_input(self, data).await
    }

    async fn recv_output(&mut self) -> Result<Option<TransportData>> {
        <SessionHandle>::recv_output(self).await
    }

    async fn close(&mut self) -> Result<()> {
        <SessionHandle>::close(self).await
    }

    fn is_active(&self) -> bool {
        <SessionHandle>::is_active(self)
    }
}

/// Unified facade for transport pipeline execution
///
/// PipelineExecutor provides a clean API with production-grade execution features.
///
/// # Features
///
/// - **Unary execution**: Single input → single output
/// - **Streaming sessions**: Multiple inputs → multiple outputs via SessionHandle
/// - **Factory registration**: Custom node type registration
/// - **Schema validation**: Manifest validation before execution
/// - **Metrics**: Prometheus-format metrics export
pub struct PipelineExecutor {
    /// Configuration
    config: ExecutorConfig,
    /// Node registry for creating nodes (wrapped in RwLock for mutable access)
    registry: Arc<RwLock<StreamingNodeRegistry>>,
    /// Streaming scheduler for node execution
    scheduler: Arc<StreamingScheduler>,
    /// Session counter for ID generation
    session_counter: std::sync::atomic::AtomicU64,
    /// Process-wide control bus for client-side pub/sub/intercept.
    ///
    /// Populated automatically for every session created via
    /// [`Self::create_session`]. Transport layers (gRPC, WebRTC) look
    /// up a session here when a client sends an `Attach(session_id)`
    /// control frame.
    control_bus: Arc<SessionControlBus>,
}

impl PipelineExecutor {
    /// Create a new PipelineExecutor with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(ExecutorConfig::default())
    }

    /// Create a new PipelineExecutor with custom configuration
    pub fn with_config(config: ExecutorConfig) -> Result<Self> {
        let scheduler = Arc::new(StreamingScheduler::new(config.scheduler_config.clone()));
        // Use the default registry with all built-in nodes registered
        let registry = Arc::new(RwLock::new(
            crate::nodes::streaming_registry::create_default_streaming_registry(),
        ));

        let control_bus = SessionControlBus::new();
        // Install as the process-wide singleton so Rust nodes can reach
        // their session's control handle for cross-node aux publishes
        // (e.g. ConversationCoordinatorNode → llm.in.barge_in). Safe to
        // call repeatedly: first-writer-wins, later calls no-op.
        SessionControlBus::install_global(control_bus.clone());

        Ok(Self {
            config,
            registry,
            scheduler,
            session_counter: std::sync::atomic::AtomicU64::new(0),
            control_bus,
        })
    }

    /// Access the process-wide [`SessionControlBus`].
    ///
    /// Transport servers (gRPC `PipelineControl`, WebRTC control data
    /// channel) use this to look up a [`SessionControl`] by `session_id`
    /// when a client opens a control-plane attach.
    pub fn control_bus(&self) -> Arc<SessionControlBus> {
        self.control_bus.clone()
    }

    /// Get the scheduler reference
    pub fn scheduler(&self) -> &Arc<StreamingScheduler> {
        &self.scheduler
    }

    /// Get the node registry reference (wrapped in RwLock)
    pub fn registry(&self) -> &Arc<RwLock<StreamingNodeRegistry>> {
        &self.registry
    }

    /// Generate a unique session ID
    pub fn generate_session_id(&self) -> String {
        let count = self
            .session_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("{}_{}", self.config.session_id_prefix, count)
    }

    /// Register a custom node factory
    ///
    /// # Arguments
    ///
    /// * `factory` - Factory for creating node instances (includes node_type internally)
    ///
    /// # Example
    ///
    /// ```ignore
    /// executor.register_factory(Arc::new(MyCustomNodeFactory)).await;
    /// ```
    pub async fn register_factory(&self, factory: Arc<dyn StreamingNodeFactory>) {
        let mut registry = self.registry.write().await;
        registry.register(factory);
    }

    /// List all registered node types
    ///
    /// Returns a sorted list of node type names that can be used in pipelines.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let executor = PipelineExecutor::new()?;
    /// let types = executor.list_node_types().await;
    /// for node_type in types {
    ///     println!("Available: {}", node_type);
    /// }
    /// ```
    pub async fn list_node_types(&self) -> Vec<String> {
        let registry = self.registry.read().await;
        registry.list_types()
    }

    /// Validate a manifest before execution
    ///
    /// Checks:
    /// - All referenced node types are registered
    /// - Connection graph is valid (no cycles, all endpoints exist)
    /// - Node parameters are valid
    pub async fn validate_manifest(&self, manifest: &Manifest) -> Result<()> {
        // Build the graph to validate connections
        crate::executor::PipelineGraph::from_manifest(manifest)?;

        // Verify all node types are registered
        let registry = self.registry.read().await;
        for node in &manifest.nodes {
            if !registry.has_node_type(&node.node_type) {
                return Err(crate::Error::Execution(format!(
                    "Unknown node type '{}' for node '{}'",
                    node.node_type, node.id
                )));
            }
        }

        Ok(())
    }

    /// Execute a pipeline with unary semantics (single input → single output)
    ///
    /// This creates a temporary session, processes the input, and returns the output.
    /// For multiple inputs/outputs, use `create_session()` instead.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Pipeline configuration
    /// * `input` - Input data to process
    ///
    /// # Returns
    ///
    /// The output from the pipeline's sink nodes
    pub async fn execute_unary(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        // Validate manifest
        self.validate_manifest(&manifest).await?;

        // Create a temporary session
        let mut session = self.create_session(manifest).await?;

        // Send input
        session.send_input(input).await?;

        // Wait for output BEFORE closing (close() shuts down the router)
        let output = session.recv_output().await?;

        // Close session after receiving output
        session.close().await?;

        match output {
            Some(output) => Ok(output),
            None => Err(crate::Error::Execution(
                "No output from pipeline".to_string(),
            )),
        }
    }

    /// Create a streaming session for multiple inputs/outputs
    ///
    /// # Arguments
    ///
    /// * `manifest` - Pipeline configuration
    ///
    /// # Returns
    ///
    /// A SessionHandle for sending inputs and receiving outputs
    pub async fn create_session(&self, manifest: Arc<Manifest>) -> Result<SessionHandle> {
        // Validate manifest
        self.validate_manifest(&manifest).await?;

        let session_id = self.generate_session_id();

        // Create bounded output channel. Capacity mirrors the router input
        // default so producer and consumer side backpressure are balanced.
        let (output_tx, output_rx) = mpsc::channel(
            crate::transport::session_router::DEFAULT_ROUTER_OUTPUT_CAPACITY,
        );

        // Get a snapshot of the registry for the session
        let registry_snapshot = {
            let registry = self.registry.read().await;
            Arc::new(registry.clone())
        };

        // Create session router with scheduler config and drift thresholds
        let (mut router, shutdown_tx) = SessionRouter::with_config(
            session_id.clone(),
            manifest.clone(),
            registry_snapshot,
            output_tx,
            Some(self.config.scheduler_config.clone()),
            if self.config.enable_drift_metrics {
                Some(self.config.drift_thresholds.clone())
            } else {
                None
            },
        )?;

        // Create and attach the per-session control bus. Must happen before
        // `start()` consumes the router's input_tx.
        let control = SessionControl::new(session_id.clone());
        router.attach_control(control.clone()).await;
        self.control_bus.register(control.clone());

        // Trace recorder: if `REMOTEMEDIA_RECORD_DIR` is set, attach
        // now so the taps are in place BEFORE the router starts —
        // otherwise we'd miss the first few frames. Failures log and
        // degrade to "no recording" (they must never take the
        // session out). The recorder handle is moved into
        // SessionHandle below so its lifetime matches the session.
        let recorder = crate::transport::session_recorder::SessionRecorder::maybe_attach_from_env(
            session_id.clone(),
            control.clone(),
            &manifest,
        )
        .await;

        // Get input sender before moving router
        let input_tx = router.get_input_sender();

        // Spawn router task. When the router exits, remove the session
        // entry from the bus so late attaches cleanly see SessionNotFound.
        let bus = self.control_bus.clone();
        let unregister_sid = session_id.clone();
        let task_handle = tokio::spawn(async move {
            let result = router.run_public().await;
            if let Err(ref e) = result {
                tracing::error!(
                    session_id = %unregister_sid,
                    error = %e,
                    "Session router task exited with error"
                );
            }
            bus.unregister(&unregister_sid);
            result
        });

        Ok(SessionHandle {
            session_id,
            input_tx: Some(input_tx),
            output_rx,
            shutdown_tx,
            task_handle,
            is_active: true,
            _recorder: recorder,
        })
    }

    /// Get scheduler metrics in Prometheus format
    pub async fn prometheus_metrics(&self) -> String {
        self.scheduler.to_prometheus().await
    }

    /// Get scheduler statistics for all nodes
    pub async fn get_node_stats(
        &self,
    ) -> std::collections::HashMap<String, crate::executor::streaming_scheduler::NodeStats> {
        self.scheduler.get_all_node_stats().await
    }
}

impl Default for PipelineExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default PipelineExecutor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Connection, ManifestMetadata, NodeManifest};

    fn create_test_manifest() -> Manifest {
        Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "test-pipeline".to_string(),
                ..Default::default()
            },
            nodes: vec![NodeManifest {
                id: "test_node".to_string(),
                node_type: "PassthroughNode".to_string(),
                params: serde_json::json!({}),
                ..Default::default()
            }],
            connections: vec![],
            python_env: None,
        }
    }

    #[test]
    fn test_executor_config_default() {
        let config = ExecutorConfig::default();
        assert!(config.enable_drift_metrics);
        assert_eq!(config.session_id_prefix, "session");
    }

    #[test]
    fn test_executor_creation() {
        let executor = PipelineExecutor::new().unwrap();
        assert!(executor.scheduler().config.max_concurrency > 0);
    }

    #[test]
    fn test_session_id_generation() {
        let executor = PipelineExecutor::new().unwrap();
        let id1 = executor.generate_session_id();
        let id2 = executor.generate_session_id();

        assert_ne!(id1, id2);
        assert!(id1.starts_with("session_"));
    }

    #[test]
    fn test_executor_with_custom_config() {
        let config = ExecutorConfig {
            scheduler_config: SchedulerConfig::with_concurrency(16),
            enable_drift_metrics: false,
            session_id_prefix: "custom".to_string(),
            ..Default::default()
        };

        let executor = PipelineExecutor::with_config(config).unwrap();
        assert_eq!(executor.scheduler().config.max_concurrency, 16);

        let session_id = executor.generate_session_id();
        assert!(session_id.starts_with("custom_"));
    }

    #[tokio::test]
    async fn test_validate_manifest_unknown_node() {
        let executor = PipelineExecutor::new().unwrap();
        let manifest = create_test_manifest();

        // Should fail because PassthroughNode isn't registered
        let result = executor.validate_manifest(&manifest).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown node type"));
    }

    #[tokio::test]
    async fn test_validate_manifest_cycle_detection() {
        let executor = PipelineExecutor::new().unwrap();

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
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "A".to_string(),
                },
            ],
            python_env: None,
        };

        let result = executor.validate_manifest(&manifest).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn test_registry_access() {
        let executor = PipelineExecutor::new().unwrap();

        // Registry should be accessible
        let registry = executor.registry();
        let reg_guard = registry.read().await;
        assert!(!reg_guard.has_node_type("NonExistentNode"));
    }
}
