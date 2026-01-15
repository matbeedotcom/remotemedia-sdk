//! Text input commands

use crate::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Send text input directly to the LLM (bypassing STT)
#[tauri::command]
pub async fn send_text_input(
    text: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Check if pipeline session is initialized
    {
        let session_guard = state.session.lock().await;
        if session_guard.is_none() {
            return Err("Pipeline not initialized".to_string());
        }
    }

    tracing::info!("Sending text input: {}", text);

    // Emit transcription event (as if user said it)
    app.emit(
        "transcription",
        serde_json::json!({
            "text": text,
            "is_final": true,
            "source": "text_input"
        }),
    )
    .map_err(|e| e.to_string())?;

    // Note: Text input bypasses the audio pipeline
    // The current pipeline is audio-focused (VAD -> STT)
    // For text input, we would need a separate text pipeline or direct LLM connection
    // For now, just acknowledge the input

    app.emit(
        "response",
        serde_json::json!({
            "text": format!("Text input received: \"{}\"", text),
            "model": "passthrough"
        }),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}
