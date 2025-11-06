//! Core pipeline execution engine exposed to transports

use crate::transport::{TransportData, StreamSessionHandle};
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
        // Execute via real Executor (no conversion needed - same RuntimeData)
        let output_data = self.executor
            .execute(&manifest, input.data)
            .await?;

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
        let session_num = self.session_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let session_id = format!("session_{}", session_num);

        // Create channels for communication
        let (input_tx, mut input_rx) = mpsc::unbounded_channel();
        let (output_tx, output_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        // TODO: Wire to SessionRouter for real streaming
        // For now, echo mode
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            tracing::info!("StreamSession {} started (TODO: wire to SessionRouter)", session_id_clone);

            loop {
                tokio::select! {
                    Some(data) = input_rx.recv() => {
                        tracing::debug!("Session {} echoing data", session_id_clone);
                        if output_tx.send(data).is_err() {
                            break;
                        }
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
