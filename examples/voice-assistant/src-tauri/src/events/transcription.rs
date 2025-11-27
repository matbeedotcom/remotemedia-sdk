//! Transcription event emitter

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Transcription event payload
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptionEvent {
    /// Transcribed text
    pub text: String,
    /// Whether this is a final transcription
    pub is_final: bool,
    /// Confidence score (0.0 - 1.0)
    pub confidence: Option<f32>,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
}

/// Emit a transcription event
pub fn emit_transcription(app: &AppHandle, event: TranscriptionEvent) -> anyhow::Result<()> {
    app.emit("transcription", event)?;
    Ok(())
}

/// Emit a partial transcription (interim result)
pub fn emit_partial_transcription(app: &AppHandle, text: &str) -> anyhow::Result<()> {
    emit_transcription(
        app,
        TranscriptionEvent {
            text: text.to_string(),
            is_final: false,
            confidence: None,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
    )
}

/// Emit a final transcription
pub fn emit_final_transcription(
    app: &AppHandle,
    text: &str,
    confidence: Option<f32>,
) -> anyhow::Result<()> {
    emit_transcription(
        app,
        TranscriptionEvent {
            text: text.to_string(),
            is_final: true,
            confidence,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
    )
}
