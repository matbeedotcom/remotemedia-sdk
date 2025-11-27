//! Pipeline initialization and management commands

use crate::{modes, AppState, PipelineState};
use std::sync::Arc;
use tauri::State;

/// Initialize the voice assistant pipeline
#[tauri::command]
pub async fn initialize_pipeline(
    mode: String,
    remote_server: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    tracing::info!("Initializing pipeline in {} mode", mode);

    let exec_mode = match mode.as_str() {
        "local" => modes::ExecutionMode::Local,
        "hybrid" => modes::ExecutionMode::Hybrid {
            remote_url: remote_server.clone().unwrap_or_default(),
        },
        "remote" => modes::ExecutionMode::Remote {
            server_url: remote_server
                .clone()
                .ok_or_else(|| "Remote server URL required for remote mode".to_string())?,
        },
        _ => return Err(format!("Unknown mode: {}", mode)),
    };

    // Update mode
    *state.mode.write() = exec_mode.clone();

    // Load the appropriate pipeline manifest
    let manifest = match &exec_mode {
        modes::ExecutionMode::Local => modes::local::get_pipeline_manifest(),
        modes::ExecutionMode::Hybrid { remote_url } => {
            modes::hybrid::get_pipeline_manifest(remote_url)
        }
        modes::ExecutionMode::Remote { server_url } => {
            modes::remote::get_pipeline_manifest(server_url)
        }
    };

    // Create session
    let session_id = uuid::Uuid::new_v4().to_string();

    // Store pipeline state
    *state.pipeline.write() = Some(PipelineState {
        session_id: session_id.clone(),
        manifest,
    });

    tracing::info!("Pipeline initialized with session: {}", session_id);
    Ok(session_id)
}

/// Shutdown the current pipeline
#[tauri::command]
pub async fn shutdown_pipeline(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    tracing::info!("Shutting down pipeline");

    // Stop audio if active
    *state.audio_active.write() = false;

    // Clear pipeline state
    *state.pipeline.write() = None;

    Ok(())
}
