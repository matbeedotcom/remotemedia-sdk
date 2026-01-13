//! HTTP API for the SRT Ingest Gateway
//!
//! This module provides the REST API for managing ingest sessions:
//! - `POST /api/ingest/sessions` - Create a new session
//! - `GET /api/ingest/sessions/:id` - Get session status
//! - `DELETE /api/ingest/sessions/:id` - End a session
//! - `GET /api/ingest/sessions/:id/events` - SSE event stream
//! - `GET /metrics` - Gateway metrics
//! - Static file serving for demo UI

pub mod events;
pub mod sessions;

use axum::{
    routing::{delete, get, post},
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::metrics::{global_metrics, MetricsSnapshot};
use crate::session::SessionManager;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// Session manager
    pub session_manager: Arc<SessionManager>,
    /// Service configuration
    pub config: Arc<Config>,
}

impl AppState {
    /// Create a new AppState
    pub fn new(session_manager: Arc<SessionManager>, config: Arc<Config>) -> Self {
        Self {
            session_manager,
            config,
        }
    }
}

/// Build the HTTP API router
pub fn build_router(state: AppState) -> Router {
    // CORS configuration - allow any origin for demo
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Static file serving for demo UI
    // Default to the crate's static directory, or override via environment variable
    let static_dir = std::env::var("INGEST_STATIC_DIR").unwrap_or_else(|_| {
        // Use the crate's manifest directory to find static files
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        format!("{}/static", manifest_dir)
    });

    Router::new()
        // Session endpoints
        .route("/api/ingest/sessions", post(sessions::create_session))
        .route("/api/ingest/sessions/:id", get(sessions::get_session))
        .route("/api/ingest/sessions/:id", delete(sessions::delete_session))
        // SSE events endpoint
        .route("/api/ingest/sessions/:id/events", get(events::events_stream))
        // Health and metrics
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        // Static files - serve at root, fallback to index.html
        .fallback_service(ServeDir::new(&static_dir).fallback(
            tower_http::services::ServeFile::new(format!("{}/index.html", static_dir)),
        ))
        // Middleware
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// Metrics endpoint
async fn metrics_handler() -> Json<MetricsSnapshot> {
    Json(global_metrics().snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_creation() {
        let manager = Arc::new(SessionManager::new("secret".to_string(), 10));
        let config = Arc::new(Config::default());
        let state = AppState::new(manager, config);

        assert_eq!(state.config.server.http_port, 8080);
    }
}
