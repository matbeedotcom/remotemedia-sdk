//! Core pipeline execution engine exposed to transports

use crate::transport::{StreamSessionHandle, TransportData};
use crate::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

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
    /// ```ignore
    /// let runner = PipelineRunner::new()?;
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
    pub fn create_streaming_registry(&self) -> Arc<crate::nodes::streaming_node::StreamingNodeRegistry> {
        Arc::new(crate::nodes::streaming_registry::create_default_streaming_registry())
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
}

impl PipelineRunnerInner {
    fn new() -> Result<Self> {
        // Initialize the real executor
        let executor = Arc::new(crate::executor::Executor::new());

        Ok(Self {
            executor,
            session_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }

    async fn execute_unary(
        &self,
        manifest: Arc<crate::manifest::Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
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
        // Generate unique session ID
        let session_num = self
            .session_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let session_id = format!("session_{}", session_num);

        // Create channels for communication
        let (input_tx, mut input_rx) = mpsc::unbounded_channel();
        let (output_tx, output_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        // Create streaming session router task
        let session_id_clone = session_id.clone();
        let executor = Arc::clone(&self.executor);
        let manifest_clone = Arc::clone(&manifest);

        tokio::spawn(async move {
            tracing::info!("StreamSession {} started", session_id_clone);

            // Find first node ID for routing
            let first_node_id = match manifest_clone.nodes.first() {
                Some(n) => n.id.clone(),
                None => {
                    tracing::error!("Session {}: No nodes in manifest", session_id_clone);
                    return;
                }
            };

            // Create nodes ONCE at session creation and cache them for reuse
            // This prevents recreating Python processes and reloading models for each request
            use crate::nodes::streaming_registry::create_default_streaming_registry;
            use std::collections::HashMap;

            let streaming_registry = create_default_streaming_registry();
            let cached_nodes: Arc<HashMap<String, Box<dyn crate::nodes::StreamingNode>>> = {
                let mut n = HashMap::new();
                for node_spec in &manifest_clone.nodes {
                    // Inject session_id into params for multiprocess execution
                    let mut params_with_session = node_spec.params.clone();
                    params_with_session["__session_id__"] = serde_json::Value::String(session_id_clone.clone());

                    match streaming_registry.create_node(
                        &node_spec.node_type,
                        node_spec.id.clone(),
                        &params_with_session,
                        Some(session_id_clone.clone()),
                    ) {
                        Ok(node) => {
                            tracing::info!("Session {} cached node {} (type: {})", session_id_clone, node_spec.id, node_spec.node_type);
                            n.insert(node_spec.id.clone(), node);
                        }
                        Err(e) => {
                            tracing::error!("Session {}: Failed to create node {}: {}", session_id_clone, node_spec.id, e);
                            return;
                        }
                    }
                }
                Arc::new(n)
            };

            // Keep output_tx alive for the entire session by holding it here
            // This prevents the channel from closing after the first execution
            let _output_tx_holder = output_tx.clone();

            loop {
                tokio::select! {
                    Some(input_data) = input_rx.recv() => {
                        tracing::debug!("Session {} processing input", session_id_clone);

                        // Spawn execution as a background task so the select loop can continue
                        // processing new inputs while this one executes
                        let output_tx_clone = output_tx.clone();
                        let first_node_clone = first_node_id.clone();
                        let session_id_for_exec = session_id_clone.clone();
                        let session_id_for_log = session_id_clone.clone();
                        let cached_nodes_clone = Arc::clone(&cached_nodes);

                        tracing::info!("Session {} spawning background task to process input", session_id_clone);
                        tokio::spawn(async move {
                            tracing::info!("Session {} background task started", session_id_for_exec);
                            let session_id_for_callback = session_id_for_exec.clone();

                            // Get reference to the cached node
                            tracing::debug!("Session {} looking up cached node '{}'", session_id_for_exec, first_node_clone);
                            let node = match cached_nodes_clone.get(&first_node_clone) {
                                Some(n) => {
                                    tracing::info!("Session {} found cached node '{}', type: {}", session_id_for_exec, first_node_clone, n.node_type());
                                    n
                                },
                                None => {
                                    tracing::error!("Session {}: Node {} not found in cached nodes", session_id_for_exec, first_node_clone);
                                    return;
                                }
                            };

                            // Use the cached node directly instead of calling execute_with_streaming_callback
                            // which would create new nodes each time
                            let callback = Box::new(move |output| {
                                tracing::debug!("Session {} received streaming output, sending to client", session_id_for_callback);
                                if let Err(e) = output_tx_clone.send(output) {
                                    tracing::warn!("Session {}: Failed to send streaming output: {}", session_id_for_callback, e);
                                }
                                Ok(())
                            });

                            tracing::info!("Session {} calling process_streaming_async on node '{}'", session_id_for_exec, first_node_clone);
                            match node.process_streaming_async(input_data, Some(session_id_for_exec.clone()), callback).await {
                                Ok(_) => {
                                    tracing::info!("Session {} execution completed successfully", session_id_for_log);
                                }
                                Err(e) => {
                                    tracing::error!("Session {}: Pipeline execution error: {}", session_id_for_log, e);
                                }
                            }
                            tracing::info!("Session {} background task finished", session_id_for_log);
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
