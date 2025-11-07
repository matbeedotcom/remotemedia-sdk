// Contract: PipelineRunner API
// Location: runtime-core/src/transport/runner.rs
// Status: Design (to be implemented in Phase 2)

use crate::manifest::Manifest;
use crate::transport::{TransportData, StreamSessionHandle};
use crate::{Error, Result};
use std::sync::Arc;

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
    /// - Tokio runtime (if not already in tokio context)
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
    /// * `Error::InitializationFailed` - Failed to set up internal resources
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
    /// # Flow
    ///
    /// 1. Parse and validate manifest
    /// 2. Initialize pipeline nodes
    /// 3. Route input through node graph
    /// 4. Collect final output
    /// 5. Clean up resources
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
    /// * `Error::NodeExecutionFailed` - A node failed during execution
    /// * `Error::InvalidData` - Input data incompatible with pipeline
    ///
    /// # Performance
    ///
    /// Unary execution is optimized for:
    /// - Low latency (no session setup overhead)
    /// - Simple pipelines (1-3 nodes)
    /// - Infrequent calls
    ///
    /// For high-throughput streaming, use `create_stream_session()` instead.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let manifest = Arc::new(Manifest::from_json(json)?);
    /// let input = TransportData::new(RuntimeData::Audio {
    ///     samples: vec![...],
    ///     sample_rate: 16000,
    ///     channels: 1,
    /// });
    ///
    /// let output = runner.execute_unary(manifest, input).await?;
    /// assert_eq!(output.data.data_type(), "audio");
    /// ```
    pub async fn execute_unary(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        self.inner.execute_unary(manifest, input).await
    }

    /// Create streaming session
    ///
    /// Establishes a persistent session for bidirectional streaming.
    /// The session maintains state across multiple inputs/outputs.
    ///
    /// # Flow
    ///
    /// 1. Parse and validate manifest
    /// 2. Initialize session (SessionRouter, node tasks)
    /// 3. Return session handle to transport
    /// 4. Transport uses handle to send/receive data
    /// 5. Transport calls close() when done
    /// 6. Core cleans up session
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
    /// * `Error::ResourceLimit` - Too many concurrent sessions
    /// * `Error::InitializationFailed` - Node initialization failed
    ///
    /// # Resource Management
    ///
    /// Sessions consume resources:
    /// - Tokio tasks (1 per node + 1 for router)
    /// - Memory (node state, buffers)
    /// - OS threads (for multiprocess Python nodes)
    ///
    /// Always call `session.close()` when done to release resources.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let manifest = Arc::new(Manifest::from_json(json)?);
    /// let mut session = runner.create_stream_session(manifest).await?;
    ///
    /// // Stream data
    /// for chunk in audio_chunks {
    ///     let data = TransportData::new(RuntimeData::Audio {
    ///         samples: chunk,
    ///         sample_rate: 16000,
    ///         channels: 1,
    ///     });
    ///     session.send_input(data).await?;
    ///
    ///     while let Some(output) = session.recv_output().await? {
    ///         // Process output
    ///     }
    /// }
    ///
    /// session.close().await?;
    /// ```
    pub async fn create_stream_session(
        &self,
        manifest: Arc<Manifest>,
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
    // Executor instance
    executor: Arc<crate::executor::Executor>,

    // Node registries
    native_registry: Arc<crate::nodes::NodeRegistry>,
    streaming_registry: Arc<crate::nodes::StreamingNodeRegistry>,

    // Multiprocess executor (optional, feature-gated)
    #[cfg(feature = "multiprocess")]
    multiprocess_executor: Option<Arc<crate::python::multiprocess::MultiprocessExecutor>>,

    // Session tracking
    sessions: Arc<tokio::sync::RwLock<std::collections::HashMap<String, SessionState>>>,
}

impl PipelineRunnerInner {
    fn new() -> Result<Self> {
        // Initialize executor
        let executor = Arc::new(crate::executor::Executor::new());

        // Initialize registries
        let native_registry = Arc::new(crate::nodes::create_default_registry());
        let streaming_registry = Arc::new(crate::nodes::create_default_streaming_registry());

        // Initialize multiprocess executor if feature enabled
        #[cfg(feature = "multiprocess")]
        let multiprocess_executor = Some(Arc::new(
            crate::python::multiprocess::MultiprocessExecutor::new()
                .map_err(|e| Error::InitializationFailed(format!("Multiprocess: {}", e)))?
        ));

        Ok(Self {
            executor,
            native_registry,
            streaming_registry,
            #[cfg(feature = "multiprocess")]
            multiprocess_executor,
            sessions: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }

    async fn execute_unary(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        // Implementation delegates to internal executor
        // Details hidden from transports
        todo!("Implementation in Phase 2")
    }

    async fn create_stream_session(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<StreamSessionHandle> {
        // Implementation creates SessionRouter and returns handle
        // Details hidden from transports
        todo!("Implementation in Phase 2")
    }
}

// Internal session state tracking
struct SessionState {
    session_id: String,
    router_handle: tokio::task::JoinHandle<()>,
    created_at: std::time::Instant,
}
