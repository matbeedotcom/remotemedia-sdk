//! Settings management commands

use crate::{AppState, Settings};
use std::sync::Arc;
use tauri::State;

/// Get current settings
#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings, String> {
    Ok(state.settings.read().clone())
}

/// Update settings
#[tauri::command]
pub async fn update_settings(
    settings: Settings,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    tracing::info!("Updating settings: {:?}", settings);

    *state.settings.write() = settings;

    // TODO: Persist settings to disk

    Ok(())
}
