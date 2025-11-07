// Contract: PipelineTransport Trait
// Location: runtime-core/src/transport/mod.rs
// Status: Design (to be implemented in Phase 2)

use crate::manifest::Manifest;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Transport-agnostic pipeline execution interface
///
/// All transport implementations (gRPC, FFI, WebRTC, custom) must implement
/// this trait to integrate with the RemoteMedia runtime core.
///
/// # Thread Safety
///
/// Implementations must be Send + Sync to allow concurrent access from
/// multiple async tasks.
///
/// # Cancellation
///
/// Methods should respect tokio cancellation (tokio::select! or similar)
/// and clean up resources appropriately.
#[async_trait]
pub trait PipelineTransport: Send + Sync {
    /// Execute a pipeline with unary semantics (single request → single response)
    ///
    /// This method is suitable for batch processing or simple request/response
    /// scenarios where the entire input is available upfront.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Pipeline configuration defining nodes and connections
    /// * `input` - Input data wrapped in transport-agnostic container
    ///
    /// # Returns
    ///
    /// * `Ok(TransportData)` - Pipeline output after all nodes execute
    /// * `Err(Error)` - Pipeline execution failed (see Error for details)
    ///
    /// # Errors
    ///
    /// * `Error::InvalidManifest` - Manifest parsing or validation failed
    /// * `Error::NodeExecutionFailed` - A node in the pipeline failed
    /// * `Error::InvalidData` - Input data format incompatible with pipeline
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let manifest = Arc::new(Manifest::from_json(json)?);
    /// let input = TransportData::new(RuntimeData::Audio { ... });
    /// let output = transport.execute(manifest, input).await?;
    /// ```
    async fn execute(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData>;

    /// Start a streaming pipeline session (multiple requests/responses)
    ///
    /// This method creates a stateful session for continuous data streaming.
    /// The transport can send multiple inputs and receive multiple outputs
    /// over the lifetime of the session.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Pipeline configuration (shared across session)
    ///
    /// # Returns
    ///
    /// * `Ok(Box<dyn StreamSession>)` - Session handle for streaming I/O
    /// * `Err(Error)` - Session creation failed
    ///
    /// # Errors
    ///
    /// * `Error::InvalidManifest` - Manifest parsing or validation failed
    /// * `Error::ResourceLimit` - Too many concurrent sessions
    ///
    /// # Session Lifecycle
    ///
    /// 1. Call `stream(manifest)` to create session
    /// 2. Use returned `StreamSession` to send/receive data
    /// 3. Call `session.close()` when done
    /// 4. Core automatically cleans up after close or error
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let manifest = Arc::new(Manifest::from_json(json)?);
    /// let mut session = transport.stream(manifest).await?;
    ///
    /// // Send input
    /// let input = TransportData::new(RuntimeData::Audio { ... });
    /// session.send_input(input).await?;
    ///
    /// // Receive output
    /// while let Some(output) = session.recv_output().await? {
    ///     // Process output
    /// }
    ///
    /// session.close().await?;
    /// ```
    async fn stream(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn StreamSession>>;
}

/// Transport-agnostic data container
///
/// Wraps core RuntimeData with optional metadata for transport-specific
/// information (sequence numbers, headers, tags, etc.).
pub struct TransportData {
    /// Core data payload (audio, text, image, binary)
    pub data: RuntimeData,

    /// Optional sequence number for ordering in streams
    ///
    /// Transports should set this for streaming sessions to maintain
    /// message order. Core may use this for metrics and debugging.
    pub sequence: Option<u64>,

    /// Transport-specific metadata (extensible key-value pairs)
    ///
    /// Examples:
    /// - gRPC: HTTP headers, request IDs
    /// - FFI: Python call context
    /// - Custom: Any transport-specific info
    pub metadata: std::collections::HashMap<String, String>,
}

impl TransportData {
    /// Create new TransportData with just payload (no metadata)
    pub fn new(data: RuntimeData) -> Self {
        Self {
            data,
            sequence: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Builder pattern: add sequence number
    pub fn with_sequence(mut self, seq: u64) -> Self {
        self.sequence = Some(seq);
        self
    }

    /// Builder pattern: add metadata key-value pair
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

// NOTE: StreamSession trait defined in separate contract file
// See: StreamSession.trait.rs
