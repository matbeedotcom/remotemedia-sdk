//! Event emitters for frontend communication

pub mod audio;
pub mod error;
pub mod mode;
pub mod response;
pub mod transcription;
pub mod vad;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Helper to emit events with error handling
pub fn emit_event<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) -> anyhow::Result<()> {
    app.emit(event, payload)?;
    Ok(())
}
