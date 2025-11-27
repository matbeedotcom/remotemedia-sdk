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
    // Check if pipeline is initialized
    let pipeline = state.pipeline.read();
    if pipeline.is_none() {
        return Err("Pipeline not initialized".to_string());
    }

    tracing::info!("Sending text input: {}", text);

    // Emit transcription event (as if user said it)
    app.emit("transcription", serde_json::json!({
        "text": text,
        "is_final": true,
        "source": "text_input"
    }))
    .map_err(|e| e.to_string())?;

    // TODO: Actually send to pipeline for processing
    // For now, simulate with placeholder response

    // Simulate LLM response
    let response = format!("I received your message: \"{}\"", text);

    app.emit("response", serde_json::json!({
        "text": response,
        "model": "placeholder"
    }))
    .map_err(|e| e.to_string())?;

    Ok(())
}
