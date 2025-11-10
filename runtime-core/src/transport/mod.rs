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
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Re-export submodules
pub mod client;
pub mod data;
pub mod plugin_registry;
pub mod runner;
pub mod session;

// Re-export key types for convenience
pub use client::{ClientStreamSession, PipelineClient, TransportType};
pub use data::TransportData;
pub use plugin_registry::TransportPluginRegistry;
pub use runner::PipelineRunner;
pub use session::{StreamSession, StreamSessionHandle};

/// Configuration for creating a transport client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Base URL or address for the client connection
    pub address: String,
    /// Optional authentication token
    pub auth_token: Option<String>,
    /// Connection timeout in milliseconds
    pub timeout_ms: Option<u64>,
    /// Transport-specific configuration (JSON)
    ///
    /// Different transports may require additional configuration:
    /// - **gRPC**: No extra config needed
    /// - **WebRTC**: `{"ice_servers": ["stun:..."]}`
    /// - **HTTP**: `{"retry_count": 3, "headers": {...}}`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_config: Option<serde_json::Value>,
}

/// Configuration for creating a transport server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server bind address
    pub address: String,
    /// Optional TLS/SSL configuration
    pub tls_config: Option<TlsConfig>,
}

/// TLS/SSL configuration for servers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to certificate file
    pub cert_path: String,
    /// Path to private key file
    pub key_path: String,
}

/// Transport plugin trait for registering transport implementations
///
/// This trait allows transport implementations (gRPC, WebRTC, HTTP, etc.)
/// to be dynamically registered and instantiated by name.
#[async_trait]
pub trait TransportPlugin: Send + Sync {
    /// Get the unique name of this transport plugin (e.g., "grpc", "webrtc")
    fn name(&self) -> &'static str;

    /// Create a client for this transport
    ///
    /// # Arguments
    ///
    /// * `config` - Client configuration including address and auth
    ///
    /// # Returns
    ///
    /// * `Ok(Box<dyn PipelineClient>)` - Configured client instance
    /// * `Err(Error)` - Client creation failed
    async fn create_client(&self, config: &ClientConfig) -> Result<Box<dyn PipelineClient>>;

    /// Create a server for this transport
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration including bind address
    /// * `runner` - Pipeline runner for executing pipelines
    ///
    /// # Returns
    ///
    /// * `Ok(Box<dyn PipelineTransport>)` - Configured server instance
    /// * `Err(Error)` - Server creation failed
    async fn create_server(
        &self,
        config: &ServerConfig,
        runner: Arc<PipelineRunner>,
    ) -> Result<Box<dyn PipelineTransport>>;

    /// Validate transport-specific configuration
    ///
    /// This method should be called before creating clients or servers to
    /// validate any transport-specific configuration in ClientConfig or ServerConfig.
    ///
    /// # Arguments
    ///
    /// * `extra_config` - Transport-specific configuration as JSON
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Configuration is valid
    /// * `Err(Error)` - Configuration validation failed
    ///
    /// # Example
    ///
    /// For WebRTC, this might validate ice_servers structure:
    /// ```json
    /// {
    ///   "ice_servers": ["stun:stun.l.google.com:19302"]
    /// }
    /// ```
    fn validate_config(&self, extra_config: &serde_json::Value) -> Result<()> {
        // Default implementation: no validation needed
        let _ = extra_config;
        Ok(())
    }
}

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
