//! Error event emitter

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Error event payload
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEvent {
    /// Error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Whether the error is recoverable
    pub recoverable: bool,
    /// Suggested action
    pub action: Option<String>,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
}

/// Emit an error event
pub fn emit_error(app: &AppHandle, event: ErrorEvent) -> anyhow::Result<()> {
    app.emit("error", event)?;
    Ok(())
}

/// Emit a pipeline error
pub fn emit_pipeline_error(app: &AppHandle, message: &str, recoverable: bool) -> anyhow::Result<()> {
    emit_error(
        app,
        ErrorEvent {
            code: "PIPELINE_ERROR".to_string(),
            message: message.to_string(),
            recoverable,
            action: if recoverable {
                Some("Retry the operation".to_string())
            } else {
                Some("Restart the pipeline".to_string())
            },
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
    )
}

/// Emit a connection error
pub fn emit_connection_error(app: &AppHandle, message: &str) -> anyhow::Result<()> {
    emit_error(
        app,
        ErrorEvent {
            code: "CONNECTION_ERROR".to_string(),
            message: message.to_string(),
            recoverable: true,
            action: Some("Check network connection and retry".to_string()),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
    )
}

/// Emit an audio error
pub fn emit_audio_error(app: &AppHandle, message: &str) -> anyhow::Result<()> {
    emit_error(
        app,
        ErrorEvent {
            code: "AUDIO_ERROR".to_string(),
            message: message.to_string(),
            recoverable: true,
            action: Some("Check microphone permissions and device".to_string()),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
    )
}
