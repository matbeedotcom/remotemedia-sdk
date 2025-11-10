//! Transport client abstractions for remote pipeline execution
//!
//! This module defines client-side interfaces for executing pipelines on remote servers.
//! It mirrors the server-side `PipelineTransport` trait but from the client perspective.
//!
//! # Architecture
//!
//! ```text
//! Local Pipeline                    Remote Server
//! ┌─────────────┐                  ┌──────────────┐
//! │ RemoteNode  │  PipelineClient  │ gRPC/WebRTC  │
//! │             │ ───────────────> │ Server       │
//! │             │                  │              │
//! │             │ <─────────────── │ PipelineRunner│
//! └─────────────┘                  └──────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use remotemedia_runtime_core::transport::client::{PipelineClient, TransportType};
//!
//! // Create a gRPC client
//! let client = create_client(TransportType::Grpc, "localhost:50051").await?;
//!
//! // Execute remote pipeline
//! let result = client.execute_unary(manifest, input).await?;
//! ```

use crate::manifest::Manifest;
use crate::transport::TransportData;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Re-export submodules
pub mod circuit_breaker;
pub mod load_balancer;
pub mod retry;

// gRPC client (feature-gated)
#[cfg(feature = "grpc-client")]
pub mod grpc;

// Re-export key types
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use load_balancer::{Endpoint, EndpointPool, LoadBalanceStrategy};
pub use retry::{RetryConfig, RetryExecutor};

#[cfg(feature = "grpc-client")]
pub use grpc::{GrpcPipelineClient, GrpcStreamSession};

/// Transport protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    /// gRPC transport
    Grpc,
    /// WebRTC transport
    Webrtc,
    /// HTTP/REST transport
    Http,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::Grpc => write!(f, "grpc"),
            TransportType::Webrtc => write!(f, "webrtc"),
            TransportType::Http => write!(f, "http"),
        }
    }
}

/// Client-side pipeline execution interface
///
/// This trait defines the interface for executing pipelines on remote servers.
/// It's the client-side counterpart to the server-side `PipelineTransport` trait.
///
/// # Thread Safety
///
/// Implementations must be Send + Sync to allow concurrent access from
/// multiple async tasks.
///
/// # Cancellation
///
/// Methods should respect tokio cancellation and clean up resources appropriately.
#[async_trait]
pub trait PipelineClient: Send + Sync {
    /// Execute a pipeline with unary semantics (single request → single response)
    ///
    /// Sends the manifest and input data to a remote server, waits for processing,
    /// and returns the result.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Pipeline configuration to execute on remote server
    /// * `input` - Input data for the pipeline
    ///
    /// # Returns
    ///
    /// * `Ok(TransportData)` - Pipeline output from remote execution
    /// * `Err(Error)` - Network failure, timeout, or remote execution error
    ///
    /// # Errors
    ///
    /// * `Error::RemoteExecutionFailed` - Remote pipeline execution failed
    /// * `Error::RemoteTimeout` - Execution exceeded timeout
    /// * `Error::Transport` - Network or protocol error
    async fn execute_unary(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData>;

    /// Create a streaming session with the remote server
    ///
    /// Establishes a long-lived bidirectional connection for continuous data streaming.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Pipeline configuration (shared across session)
    ///
    /// # Returns
    ///
    /// * `Ok(Box<dyn ClientStreamSession>)` - Session handle for streaming I/O
    /// * `Err(Error)` - Failed to establish connection
    ///
    /// # Errors
    ///
    /// * `Error::Transport` - Connection failed
    /// * `Error::InvalidManifest` - Server rejected manifest
    async fn create_stream_session(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn ClientStreamSession>>;

    /// Check if remote endpoint is healthy
    ///
    /// Performs a lightweight health check to determine if the endpoint is responsive.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Endpoint is healthy
    /// * `Ok(false)` - Endpoint is unhealthy
    /// * `Err(Error)` - Health check failed (network error)
    async fn health_check(&self) -> Result<bool>;
}

/// Client-side streaming session handle
///
/// Represents an active bidirectional streaming connection to a remote server.
/// Allows sending multiple inputs and receiving multiple outputs over the
/// session lifetime.
///
/// # Lifecycle
///
/// 1. Create via `PipelineClient::create_stream_session()`
/// 2. Send inputs via `send()`
/// 3. Receive outputs via `receive()`
/// 4. Close gracefully via `close()`
///
/// # Example
///
/// ```ignore
/// let mut session = client.create_stream_session(manifest).await?;
///
/// // Send audio chunks
/// session.send(audio_chunk1).await?;
/// session.send(audio_chunk2).await?;
///
/// // Receive processed outputs
/// while let Some(output) = session.receive().await? {
///     process_output(output);
/// }
///
/// session.close().await?;
/// ```
#[async_trait]
pub trait ClientStreamSession: Send + Sync {
    /// Get unique session identifier
    fn session_id(&self) -> &str;

    /// Send input data to the remote pipeline
    ///
    /// # Arguments
    ///
    /// * `data` - Input data to process
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Data sent successfully
    /// * `Err(Error)` - Send failed (connection closed, buffer full, etc.)
    async fn send(&mut self, data: TransportData) -> Result<()>;

    /// Receive output data from the remote pipeline
    ///
    /// This is a blocking call that waits for the next output.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(data))` - Received output data
    /// * `Ok(None)` - Stream closed (no more outputs)
    /// * `Err(Error)` - Receive failed (connection error, timeout, etc.)
    async fn receive(&mut self) -> Result<Option<TransportData>>;

    /// Close the session gracefully
    ///
    /// Flushes any pending data and closes the connection.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Session closed successfully
    /// * `Err(Error)` - Failed to close (already closed, network error, etc.)
    async fn close(&mut self) -> Result<()>;

    /// Check if session is still active
    ///
    /// # Returns
    ///
    /// * `true` - Session is active and can send/receive
    /// * `false` - Session is closed or failed
    fn is_active(&self) -> bool;
}
