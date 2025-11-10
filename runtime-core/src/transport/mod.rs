//! Transport abstraction layer
//!
//! This module defines traits that transport implementations must satisfy.
//! The core runtime knows nothing about specific transports (gRPC, WebRTC, FFI).
//!
//! # Architecture
//!
//! Transport implementations (gRPC, FFI, WebRTC) depend on runtime-core and
//! implement the `PipelineTransport` trait. This inverts the dependency from
//! the original monolithic design where transports were embedded in the runtime.
//!
//! # Example
//!
//! ```ignore
//! use remotemedia_runtime_core::transport::{PipelineTransport, TransportData};
//! use async_trait::async_trait;
//!
//! struct MyCustomTransport {
//!     runner: PipelineRunner,
//! }
//!
//! #[async_trait]
//! impl PipelineTransport for MyCustomTransport {
//!     async fn execute(&self, manifest: Arc<Manifest>, input: TransportData)
//!         -> Result<TransportData>
//!     {
//!         self.runner.execute_unary(manifest, input).await
//!     }
//!
//!     async fn stream(&self, manifest: Arc<Manifest>)
//!         -> Result<Box<dyn StreamSession>>
//!     {
//!         let session = self.runner.create_stream_session(manifest).await?;
//!         Ok(Box::new(session))
//!     }
//! }
//! ```

use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

// Re-export submodules
pub mod client;
pub mod data;
pub mod runner;
pub mod session;

// Re-export key types for convenience
pub use client::{ClientStreamSession, PipelineClient, TransportType};
pub use data::TransportData;
pub use runner::PipelineRunner;
pub use session::{StreamSession, StreamSessionHandle};

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
    async fn execute(
        &self,
        manifest: Arc<crate::manifest::Manifest>,
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
    async fn stream(
        &self,
        manifest: Arc<crate::manifest::Manifest>,
    ) -> Result<Box<dyn StreamSession>>;
}
