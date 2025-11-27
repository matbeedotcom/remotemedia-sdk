//! Response event emitter

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Response event payload
#[derive(Debug, Clone, Serialize)]
pub struct ResponseEvent {
    /// Response text
    pub text: String,
    /// Model that generated the response
    pub model: String,
    /// Whether response is still streaming
    pub streaming: bool,
    /// Whether this is the final chunk
    pub is_final: bool,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
}

/// Emit a response event
pub fn emit_response(app: &AppHandle, event: ResponseEvent) -> anyhow::Result<()> {
    app.emit("response", event)?;
    Ok(())
}

/// Emit a streaming response chunk
pub fn emit_response_chunk(app: &AppHandle, text: &str, model: &str) -> anyhow::Result<()> {
    emit_response(
        app,
        ResponseEvent {
            text: text.to_string(),
            model: model.to_string(),
            streaming: true,
            is_final: false,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
    )
}

/// Emit a final response
pub fn emit_final_response(app: &AppHandle, text: &str, model: &str) -> anyhow::Result<()> {
    emit_response(
        app,
        ResponseEvent {
            text: text.to_string(),
            model: model.to_string(),
            streaming: false,
            is_final: true,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
    )
}
