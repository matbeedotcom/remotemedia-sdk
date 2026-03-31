//! Embedded web UI for RemoteMedia pipeline interaction.
//!
//! This crate embeds a built Preact frontend and serves it via axum
//! alongside pipeline API endpoints.
//!
//! # Usage
//!
//! ```ignore
//! use remotemedia_ui::UiServerBuilder;
//! use remotemedia_core::transport::PipelineExecutor;
//! use std::sync::Arc;
//!
//! let executor = Arc::new(PipelineExecutor::new()?);
//! let server = UiServerBuilder::new()
//!     .bind("127.0.0.1:3001")
//!     .executor(executor)
//!     .build()?;
//! server.run().await?;
//! ```

pub mod api;
pub mod assets;

use api::SessionHandle;
use axum::{
    routing::{delete, get, post},
    Router,
};
pub use remotemedia_core::manifest::Manifest;
pub use remotemedia_core::transport::PipelineExecutor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Information about a companion transport (e.g., gRPC, WebRTC) running alongside the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportInfo {
    /// Transport type identifier (e.g., "grpc", "webrtc", "http")
    pub transport_type: String,
    /// Address the transport is listening on
    pub address: String,
}

/// Shared application state for all handlers.
#[derive(Clone)]
pub(crate) struct AppState {
    pub executor: Arc<PipelineExecutor>,
    pub manifest: Option<Arc<Manifest>>,
    pub transport_info: Option<TransportInfo>,
    pub sessions: Arc<RwLock<HashMap<String, SessionHandle>>>,
}

/// Builder for configuring and creating a [`UiServer`].
///
/// # Example
///
/// ```ignore
/// use remotemedia_ui::UiServerBuilder;
/// use remotemedia_core::transport::PipelineExecutor;
/// use std::sync::Arc;
///
/// let executor = Arc::new(PipelineExecutor::new()?);
/// let server = UiServerBuilder::new()
///     .bind("0.0.0.0:3001")
///     .executor(executor)
///     .build()?;
/// server.run().await?;
/// ```
pub struct UiServerBuilder {
    bind_address: Option<String>,
    executor: Option<Arc<PipelineExecutor>>,
    manifest: Option<Arc<Manifest>>,
    transport_info: Option<TransportInfo>,
}

impl UiServerBuilder {
    /// Create a new builder with default values.
    ///
    /// Defaults:
    /// - `bind_address`: `"127.0.0.1:3001"`
    /// - `executor`: `None` (must be provided before calling `build`)
    pub fn new() -> Self {
        Self {
            bind_address: None,
            executor: None,
            manifest: None,
            transport_info: None,
        }
    }

    /// Set the address the server will bind to.
    ///
    /// If not called, defaults to `"127.0.0.1:3001"`.
    pub fn bind(mut self, addr: impl Into<String>) -> Self {
        self.bind_address = Some(addr.into());
        self
    }

    /// Set the pipeline executor used by the server.
    pub fn executor(mut self, executor: Arc<PipelineExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Set a default manifest for the server.
    ///
    /// When set, clients can omit the manifest in execute/stream requests.
    pub fn manifest(mut self, manifest: Arc<Manifest>) -> Self {
        self.manifest = Some(manifest);
        self
    }

    /// Set transport info describing a companion transport server.
    pub fn transport_info(mut self, info: TransportInfo) -> Self {
        self.transport_info = Some(info);
        self
    }

    /// Build the [`UiServer`].
    ///
    /// # Errors
    ///
    /// Returns an error if the executor has not been set.
    pub fn build(self) -> std::result::Result<UiServer, Box<dyn std::error::Error>> {
        let executor = self
            .executor
            .ok_or("executor is required - call .executor() before .build()")?;

        let bind_address = self
            .bind_address
            .unwrap_or_else(|| "127.0.0.1:3001".to_string());

        let state = AppState {
            executor,
            manifest: self.manifest,
            transport_info: self.transport_info,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        };

        Ok(UiServer {
            bind_address,
            state,
        })
    }
}

impl Default for UiServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A configured UI server ready to run.
///
/// Serves the embedded Preact frontend and pipeline API endpoints.
pub struct UiServer {
    bind_address: String,
    state: AppState,
}

impl UiServer {
    /// Build the axum router with API routes and static file serving.
    fn build_router(&self) -> Router {
        Router::new()
            // API routes
            .route("/api/status", get(api::status_handler))
            .route("/api/manifest", get(api::manifest_handler))
            .route("/api/execute", post(api::execute_handler))
            .route("/api/stream", post(api::create_stream_handler))
            .route(
                "/api/stream/:session_id/input",
                post(api::stream_input_handler),
            )
            .route(
                "/api/stream/:session_id/output",
                get(api::stream_output_handler),
            )
            .route(
                "/api/stream/:session_id",
                delete(api::close_stream_handler),
            )
            // Static files (fallback serves the SPA)
            .fallback(get(assets::static_handler))
            .with_state(self.state.clone())
            .layer(
                tower::ServiceBuilder::new()
                    .layer(tower_http::trace::TraceLayer::new_for_http())
                    .layer(tower_http::cors::CorsLayer::permissive()),
            )
    }

    /// Run the UI server, blocking until shutdown (ctrl-c).
    pub async fn run(self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let addr: std::net::SocketAddr = self.bind_address.parse()?;

        tracing::info!("Starting RemoteMedia UI server on {}", addr);

        let router = self.build_router();

        let listener = tokio::net::TcpListener::bind(addr).await?;

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        tracing::info!("UI server shut down");

        Ok(())
    }
}

/// Wait for ctrl-c shutdown signal.
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install ctrl-c handler");
    tracing::info!("Received ctrl-c, shutting down UI server");
}

