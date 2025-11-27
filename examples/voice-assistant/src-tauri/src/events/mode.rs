//! Mode change event emitter

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Mode change event payload
#[derive(Debug, Clone, Serialize)]
pub struct ModeChangedEvent {
    /// Previous mode
    pub from: String,
    /// New mode
    pub to: String,
    /// Reason for change (e.g., "user_request", "fallback", "reconnect")
    pub reason: String,
    /// Additional details
    pub details: Option<String>,
}

/// Emit a mode change event
pub fn emit_mode_changed(app: &AppHandle, event: ModeChangedEvent) -> anyhow::Result<()> {
    app.emit("mode_changed", event)?;
    Ok(())
}

/// Emit fallback to local mode
pub fn emit_fallback_to_local(app: &AppHandle, from: &str, details: &str) -> anyhow::Result<()> {
    emit_mode_changed(
        app,
        ModeChangedEvent {
            from: from.to_string(),
            to: "local".to_string(),
            reason: "fallback".to_string(),
            details: Some(details.to_string()),
        },
    )
}

/// Emit reconnect to remote mode
pub fn emit_reconnect_to_remote(app: &AppHandle, to: &str) -> anyhow::Result<()> {
    emit_mode_changed(
        app,
        ModeChangedEvent {
            from: "local".to_string(),
            to: to.to_string(),
            reason: "reconnect".to_string(),
            details: None,
        },
    )
}
