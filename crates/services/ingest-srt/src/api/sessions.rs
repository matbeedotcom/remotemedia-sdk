//! Session management endpoints
//!
//! Handles creating, querying, and deleting ingest sessions.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::session::{EndReason, SessionConfig, SessionLimits, SessionState};
use crate::streamid::StreamIdParams;

/// Request body for creating a session
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    /// Pipeline template ID
    #[serde(default = "default_pipeline")]
    pub pipeline: String,

    /// Optional webhook URL for event delivery
    #[serde(default)]
    pub webhook_url: Option<String>,

    /// Enable audio analysis
    #[serde(default = "default_true")]
    pub audio_enabled: bool,

    /// Enable video analysis
    #[serde(default)]
    pub video_enabled: bool,

    /// Maximum session duration in seconds
    #[serde(default = "default_max_duration")]
    pub max_duration_seconds: u64,
}

fn default_pipeline() -> String {
    "demo_audio_quality_v1".to_string()
}

fn default_true() -> bool {
    true
}

fn default_max_duration() -> u64 {
    300 // 5 minutes
}

impl Default for CreateSessionRequest {
    fn default() -> Self {
        Self {
            pipeline: default_pipeline(),
            webhook_url: None,
            audio_enabled: true,
            video_enabled: false,
            max_duration_seconds: default_max_duration(),
        }
    }
}

/// Response body for session creation
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    /// Session ID
    pub session_id: String,

    /// Full SRT URL for FFmpeg to push to
    pub srt_url: String,

    /// FFmpeg command with copy mode (lowest CPU)
    pub ffmpeg_command_copy: String,

    /// FFmpeg command with transcode mode (most compatible)
    pub ffmpeg_command_transcode: String,

    /// SSE events URL
    pub events_url: String,

    /// Session expiration timestamp (ISO 8601)
    pub expires_at: String,
}

/// Response body for session status
#[derive(Debug, Serialize)]
pub struct SessionStatusResponse {
    /// Session ID
    pub session_id: String,

    /// Current session state
    pub state: String,

    /// Pipeline template ID
    pub pipeline: String,

    /// When the session was created (ISO 8601)
    pub created_at: String,

    /// Streaming duration in milliseconds (if streaming)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming_duration_ms: Option<u64>,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

/// Create a new ingest session
///
/// POST /api/ingest/sessions
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    // Convert request to session config
    let config = SessionConfig {
        pipeline: req.pipeline.clone(),
        webhook_url: req.webhook_url,
        audio_enabled: req.audio_enabled,
        video_enabled: req.video_enabled,
        max_duration_seconds: req.max_duration_seconds,
    };

    // Calculate limits
    let limits = SessionLimits {
        max_bitrate: Some(state.config.limits.max_bitrate_bps),
        max_fps: Some(30),
        max_duration: req.max_duration_seconds.min(state.config.limits.max_session_duration_seconds),
    };

    // Create the session
    let (session, input_rx, token) = match state.session_manager.create_session(config, limits).await {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "session_creation_failed".to_string(),
                    message: e.to_string(),
                }),
            )
                .into_response();
        }
    };

    // Spawn pipeline processing task to consume incoming MPEG-TS data
    // This keeps the input channel alive and processes the stream
    let session_for_pipeline = session.clone();
    tokio::spawn(async move {
        crate::pipeline::run_pipeline(session_for_pipeline, input_rx).await;
    });

    // Build SRT URL
    let params = StreamIdParams {
        session_id: session.id.clone(),
        token,
        pipeline: req.pipeline,
        audio_enabled: req.audio_enabled,
        video_enabled: req.video_enabled,
    };

    let srt_url = params.to_srt_url(
        &state.config.server.public_host,
        state.config.server.srt_port,
    );

    // Build FFmpeg commands
    let ffmpeg_copy = build_ffmpeg_command_copy(&srt_url);
    let ffmpeg_transcode = build_ffmpeg_command_transcode(&srt_url);

    // Build events URL
    let events_url = format!("/api/ingest/sessions/{}/events", session.id);

    // Calculate expiration
    let expires_at = chrono::Utc::now()
        + chrono::Duration::seconds(session.limits.max_duration as i64);

    (
        StatusCode::CREATED,
        Json(SessionResponse {
            session_id: session.id.clone(),
            srt_url,
            ffmpeg_command_copy: ffmpeg_copy,
            ffmpeg_command_transcode: ffmpeg_transcode,
            events_url,
            expires_at: expires_at.to_rfc3339(),
        }),
    )
        .into_response()
}

/// Get session status
///
/// GET /api/ingest/sessions/:id
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let session = match state.session_manager.get_session(&session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "session_not_found".to_string(),
                    message: format!("Session {} not found", session_id),
                }),
            )
                .into_response();
        }
    };

    let session_state = session.state().await;
    let state_str = match &session_state {
        SessionState::Created => "created",
        SessionState::Connected => "connected",
        SessionState::Streaming { .. } => "streaming",
        SessionState::Ended { .. } => "ended",
    };

    let streaming_duration_ms = session.streaming_duration_ms().await;

    (
        StatusCode::OK,
        Json(SessionStatusResponse {
            session_id: session.id.clone(),
            state: state_str.to_string(),
            pipeline: session.pipeline_id.clone(),
            created_at: session.created_at.to_rfc3339(),
            streaming_duration_ms,
        }),
    )
        .into_response()
}

/// Delete (end) a session
///
/// DELETE /api/ingest/sessions/:id
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state
        .session_manager
        .end_session(&session_id, EndReason::Deleted)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "session_not_found".to_string(),
                message: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Build FFmpeg copy-mode command
/// Uses single quotes around SRT URL to prevent bash from interpreting special characters
/// (the streamid contains #!:: which bash would otherwise treat as history modifiers)
fn build_ffmpeg_command_copy(srt_url: &str) -> String {
    format!(
        "ffmpeg -re -i '<YOUR_SOURCE>' -c copy -f mpegts '{}'",
        srt_url
    )
}

/// Build FFmpeg transcode-mode command
/// Uses single quotes around SRT URL to prevent bash from interpreting special characters
fn build_ffmpeg_command_transcode(srt_url: &str) -> String {
    format!(
        "ffmpeg -re -i '<YOUR_SOURCE>' -c:v libx264 -preset veryfast -tune zerolatency -g 60 -keyint_min 60 -b:v 1500k -c:a aac -ar 48000 -b:a 128k -f mpegts '{}'",
        srt_url
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_create_request() {
        let req = CreateSessionRequest::default();
        assert_eq!(req.pipeline, "demo_audio_quality_v1");
        assert!(req.audio_enabled);
        assert!(!req.video_enabled);
        assert_eq!(req.max_duration_seconds, 300);
    }

    #[test]
    fn test_ffmpeg_command_copy() {
        let url = "srt://localhost:9000?mode=caller&streamid=test";
        let cmd = build_ffmpeg_command_copy(url);
        assert!(cmd.contains("-c copy"));
        assert!(cmd.contains("-f mpegts"));
        assert!(cmd.contains(url));
    }

    #[test]
    fn test_ffmpeg_command_transcode() {
        let url = "srt://localhost:9000?mode=caller&streamid=test";
        let cmd = build_ffmpeg_command_transcode(url);
        assert!(cmd.contains("-c:v libx264"));
        assert!(cmd.contains("-preset veryfast"));
        assert!(cmd.contains("-tune zerolatency"));
        assert!(cmd.contains("-c:a aac"));
    }
}
