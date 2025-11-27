//! Audio capture commands

use crate::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Start listening from the microphone
#[tauri::command]
pub async fn start_listening(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Check if pipeline is initialized
    if state.pipeline.read().is_none() {
        return Err("Pipeline not initialized".to_string());
    }

    // Check if already listening
    if *state.audio_active.read() {
        return Err("Already listening".to_string());
    }

    tracing::info!("Starting audio capture");
    *state.audio_active.write() = true;

    // Emit VAD state change
    app.emit("vad_state", serde_json::json!({
        "active": true,
        "speaking": false
    }))
    .map_err(|e| e.to_string())?;

    // TODO: Actually start audio capture using cpal
    // For now, simulate with placeholder

    Ok(())
}

/// Stop listening from the microphone
#[tauri::command]
pub async fn stop_listening(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if !*state.audio_active.read() {
        return Ok(()); // Already stopped
    }

    tracing::info!("Stopping audio capture");
    *state.audio_active.write() = false;

    // Emit VAD state change
    app.emit("vad_state", serde_json::json!({
        "active": false,
        "speaking": false
    }))
    .map_err(|e| e.to_string())?;

    Ok(())
}
