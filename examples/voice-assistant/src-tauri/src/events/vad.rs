//! VAD state event emitter

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// VAD state event payload
#[derive(Debug, Clone, Serialize)]
pub struct VadStateEvent {
    /// Whether VAD is active (listening)
    pub active: bool,
    /// Whether speech is currently detected
    pub speaking: bool,
    /// Current VAD probability (0.0 - 1.0)
    pub probability: Option<f32>,
}

/// Emit a VAD state event
pub fn emit_vad_state(app: &AppHandle, event: VadStateEvent) -> anyhow::Result<()> {
    app.emit("vad_state", event)?;
    Ok(())
}

/// Emit speech started
pub fn emit_speech_started(app: &AppHandle, probability: f32) -> anyhow::Result<()> {
    emit_vad_state(
        app,
        VadStateEvent {
            active: true,
            speaking: true,
            probability: Some(probability),
        },
    )
}

/// Emit speech ended
pub fn emit_speech_ended(app: &AppHandle) -> anyhow::Result<()> {
    emit_vad_state(
        app,
        VadStateEvent {
            active: true,
            speaking: false,
            probability: None,
        },
    )
}

/// Emit VAD probability update
pub fn emit_vad_probability(app: &AppHandle, probability: f32, speaking: bool) -> anyhow::Result<()> {
    emit_vad_state(
        app,
        VadStateEvent {
            active: true,
            speaking,
            probability: Some(probability),
        },
    )
}
