//! Audio output event emitter

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Audio output event payload
#[derive(Debug, Clone, Serialize)]
pub struct AudioOutputEvent {
    /// Whether audio is currently playing
    pub playing: bool,
    /// Duration of the audio in milliseconds
    pub duration_ms: Option<u64>,
    /// Progress through playback (0.0 - 1.0)
    pub progress: Option<f32>,
}

/// Emit an audio output event
pub fn emit_audio_output(app: &AppHandle, event: AudioOutputEvent) -> anyhow::Result<()> {
    app.emit("audio_output", event)?;
    Ok(())
}

/// Emit audio playback started
pub fn emit_audio_started(app: &AppHandle, duration_ms: u64) -> anyhow::Result<()> {
    emit_audio_output(
        app,
        AudioOutputEvent {
            playing: true,
            duration_ms: Some(duration_ms),
            progress: Some(0.0),
        },
    )
}

/// Emit audio playback progress
pub fn emit_audio_progress(app: &AppHandle, progress: f32) -> anyhow::Result<()> {
    emit_audio_output(
        app,
        AudioOutputEvent {
            playing: true,
            duration_ms: None,
            progress: Some(progress),
        },
    )
}

/// Emit audio playback finished
pub fn emit_audio_finished(app: &AppHandle) -> anyhow::Result<()> {
    emit_audio_output(
        app,
        AudioOutputEvent {
            playing: false,
            duration_ms: None,
            progress: Some(1.0),
        },
    )
}
