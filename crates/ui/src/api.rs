//! API handlers for the embedded web UI server.
//!
//! Provides HTTP endpoints for pipeline execution and status,
//! mirroring the HTTP transport server pattern.

use crate::AppState;
use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::stream::Stream;
use remotemedia_core::manifest::Manifest;
use remotemedia_core::transport::{StreamSession, TransportData};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

use crate::TransportInfo;

// ---------------------------------------------------------------------------
// Session handle types
// ---------------------------------------------------------------------------

/// Handle to a streaming session
#[allow(dead_code)]
pub(crate) struct SessionHandle {
    /// Session ID
    pub session_id: String,
    /// Stream session from executor
    pub session: SessionHandleWrapper,
    /// Broadcast channel for sending outputs to multiple SSE subscribers
    pub output_tx: broadcast::Sender<TransportData>,
}

/// Wrapper to adapt PipelineExecutor's SessionHandle to StreamSession trait
pub(crate) struct SessionHandleWrapper(pub remotemedia_core::transport::SessionHandle);

#[async_trait]
impl StreamSession for SessionHandleWrapper {
    async fn send_input(&mut self, data: TransportData) -> remotemedia_core::Result<()> {
        self.0.send_input(data).await
    }

    async fn recv_output(&mut self) -> remotemedia_core::Result<Option<TransportData>> {
        self.0.recv_output().await
    }

    async fn close(&mut self) -> remotemedia_core::Result<()> {
        self.0.close().await
    }

    fn session_id(&self) -> &str {
        &self.0.session_id
    }

    fn is_active(&self) -> bool {
        self.0.is_active()
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for /api/execute
#[derive(Debug, Deserialize)]
pub(crate) struct ExecuteRequest {
    /// Pipeline manifest (optional - falls back to server manifest)
    manifest: Option<serde_json::Value>,
    /// Input data
    input: TransportData,
}

/// Response body for /api/execute
#[derive(Debug, Serialize)]
pub(crate) struct ExecuteResponse {
    output: TransportData,
}

/// Response body for /api/status
#[derive(Debug, Serialize)]
pub(crate) struct StatusResponse {
    version: String,
    transport: Option<TransportInfo>,
    active_sessions: usize,
}

/// Request body for POST /api/stream
#[derive(Debug, Deserialize)]
pub(crate) struct CreateStreamRequest {
    manifest: Option<serde_json::Value>,
}

/// Response body for POST /api/stream
#[derive(Debug, Serialize)]
pub(crate) struct CreateStreamResponse {
    session_id: String,
}

/// Request body for POST /api/stream/:session_id/input
#[derive(Debug, Deserialize)]
pub(crate) struct StreamInputRequest {
    data: TransportData,
}

/// Error response body
#[derive(Debug, Serialize)]
pub(crate) struct ErrorResponse {
    error_type: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    validation_errors: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// Map runtime errors to appropriate HTTP status codes and structured responses
fn map_runtime_error(e: remotemedia_core::Error) -> (StatusCode, Json<ErrorResponse>) {
    match e {
        remotemedia_core::Error::Validation(ref validation_errors) => {
            let errors_json = serde_json::to_value(validation_errors).ok();
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error_type: "validation".to_string(),
                    message: format!(
                        "{} validation error(s) in node parameters",
                        validation_errors.len()
                    ),
                    validation_errors: errors_json,
                }),
            )
        }
        remotemedia_core::Error::Manifest(msg)
        | remotemedia_core::Error::InvalidManifest(msg) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error_type: "manifest".to_string(),
                message: msg,
                validation_errors: None,
            }),
        ),
        remotemedia_core::Error::InvalidData(msg)
        | remotemedia_core::Error::InvalidInput { message: msg, .. } => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error_type: "input".to_string(),
                message: msg,
                validation_errors: None,
            }),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error_type: "execution".to_string(),
                message: e.to_string(),
                validation_errors: None,
            }),
        ),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/status - Server status and info
pub(crate) async fn status_handler(
    State(state): State<AppState>,
) -> Json<StatusResponse> {
    let sessions = state.sessions.read().await;
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        transport: state.transport_info.clone(),
        active_sessions: sessions.len(),
    })
}

/// GET /api/manifest - Return the loaded manifest
pub(crate) async fn manifest_handler(
    State(state): State<AppState>,
) -> std::result::Result<Json<serde_json::Value>, StatusCode> {
    match &state.manifest {
        Some(manifest) => {
            let value = serde_json::to_value(manifest.as_ref())
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(value))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /api/execute - Execute a pipeline
pub(crate) async fn execute_handler(
    State(state): State<AppState>,
    Json(request): Json<ExecuteRequest>,
) -> std::result::Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Resolve manifest: use request manifest, fall back to server manifest
    let manifest = if let Some(manifest_value) = request.manifest {
        let m: Manifest = serde_json::from_value(manifest_value).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error_type: "manifest".to_string(),
                    message: format!("Invalid manifest: {}", e),
                    validation_errors: None,
                }),
            )
        })?;
        Arc::new(m)
    } else if let Some(ref m) = state.manifest {
        Arc::clone(m)
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error_type: "manifest".to_string(),
                message: "No manifest provided and no default manifest loaded".to_string(),
                validation_errors: None,
            }),
        ));
    };

    let output = state
        .executor
        .execute_unary(manifest, request.input)
        .await
        .map_err(map_runtime_error)?;

    Ok(Json(ExecuteResponse { output }))
}

/// POST /api/stream - Create a streaming session
pub(crate) async fn create_stream_handler(
    State(state): State<AppState>,
    Json(request): Json<CreateStreamRequest>,
) -> std::result::Result<Json<CreateStreamResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Resolve manifest
    let manifest = if let Some(manifest_value) = request.manifest {
        let m: Manifest = serde_json::from_value(manifest_value).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error_type: "manifest".to_string(),
                    message: format!("Invalid manifest: {}", e),
                    validation_errors: None,
                }),
            )
        })?;
        Arc::new(m)
    } else if let Some(ref m) = state.manifest {
        Arc::clone(m)
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error_type: "manifest".to_string(),
                message: "No manifest provided and no default manifest loaded".to_string(),
                validation_errors: None,
            }),
        ));
    };

    let session = state
        .executor
        .create_session(manifest)
        .await
        .map_err(map_runtime_error)?;

    let session_id = session.session_id.clone();

    let (output_tx, _output_rx) = broadcast::channel(100);

    let handle = SessionHandle {
        session_id: session_id.clone(),
        session: SessionHandleWrapper(session),
        output_tx,
    };

    state
        .sessions
        .write()
        .await
        .insert(session_id.clone(), handle);

    tracing::info!("Created streaming session: {}", session_id);

    Ok(Json(CreateStreamResponse { session_id }))
}

/// POST /api/stream/:session_id/input - Send input to streaming session
pub(crate) async fn stream_input_handler(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<StreamInputRequest>,
) -> std::result::Result<StatusCode, (StatusCode, String)> {
    let mut sessions = state.sessions.write().await;

    let handle = sessions.get_mut(&session_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Session not found: {}", session_id),
        )
    })?;

    handle.session.send_input(request.data).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send input: {}", e),
        )
    })?;

    // Poll for outputs and broadcast to SSE subscribers
    while let Ok(Some(output)) = handle.session.recv_output().await {
        let _ = handle.output_tx.send(output);
    }

    Ok(StatusCode::OK)
}

/// GET /api/stream/:session_id/output - SSE stream of outputs
pub(crate) async fn stream_output_handler(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> std::result::Result<
    Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>,
    (StatusCode, String),
> {
    let rx = {
        let sessions = state.sessions.read().await;
        let handle = sessions.get(&session_id).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Session not found: {}", session_id),
            )
        })?;
        handle.output_tx.subscribe()
    };

    let session_id_clone = session_id.clone();
    let stream = BroadcastStream::new(rx).filter_map(move |result| match result {
        Ok(data) => {
            let json = serde_json::to_string(&data).unwrap_or_default();
            Some(Ok(Event::default().data(json)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!(
                "SSE stream lagged by {} messages for session {}",
                n,
                session_id_clone
            );
            None
        }
    });

    tracing::debug!("SSE connection established for session {}", session_id);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// DELETE /api/stream/:session_id - Close streaming session
pub(crate) async fn close_stream_handler(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> std::result::Result<StatusCode, (StatusCode, String)> {
    let mut sessions = state.sessions.write().await;

    let mut handle = sessions.remove(&session_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Session not found: {}", session_id),
        )
    })?;

    handle.session.close().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to close session: {}", e),
        )
    })?;

    tracing::info!("Closed streaming session: {}", session_id);

    Ok(StatusCode::OK)
}
