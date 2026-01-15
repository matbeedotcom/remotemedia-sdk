//! Pipeline initialization and management commands

use crate::{modes, AppState};
use remotemedia_core::data::RuntimeData;
use remotemedia_core::transport::TransportData;
use remotemedia_core::Manifest;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Initialize the voice assistant pipeline
#[tauri::command]
pub async fn initialize_pipeline(
    app: AppHandle,
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
    let manifest_str = match &exec_mode {
        modes::ExecutionMode::Local => modes::local::get_pipeline_manifest(),
        modes::ExecutionMode::Hybrid { remote_url } => {
            modes::hybrid::get_pipeline_manifest(remote_url)
        }
        modes::ExecutionMode::Remote { server_url } => {
            modes::remote::get_pipeline_manifest(server_url)
        }
    };

    // Parse manifest
    let manifest: Manifest = serde_yaml::from_str(&manifest_str)
        .map_err(|e| format!("Failed to parse manifest: {}", e))?;

    tracing::debug!("Parsed manifest: {:?}", manifest);

    // Log available node types for debugging
    let available_types = state.executor.list_node_types().await;
    tracing::info!("Available node types: {:?}", available_types);
    
    // Validate that all required node types are registered
    for node in &manifest.nodes {
        if !available_types.contains(&node.node_type) {
            return Err(format!(
                "Node type '{}' is not registered. Available types: {:?}",
                node.node_type, available_types
            ));
        }
    }

    // Create pipeline session
    let session = state
        .executor
        .create_session(Arc::new(manifest))
        .await
        .map_err(|e| format!("Failed to create pipeline session: {}", e))?;

    let session_id = session.session_id.clone();
    tracing::info!("Pipeline initialized with session: {}", session_id);
    
    // Note: Node initialization (including model downloads) happens asynchronously in the router task.
    // Progress events will be emitted to inform the user. We don't block here since model downloads
    // can take several minutes on first run.

    // Create channel for audio data
    let (audio_tx, mut audio_rx) = tokio::sync::mpsc::unbounded_channel::<RuntimeData>();

    // Store session and audio channel
    {
        let mut session_guard = state.session.lock().await;
        *session_guard = Some(session);
    }
    {
        let mut audio_tx_guard = state.audio_tx.lock().await;
        *audio_tx_guard = Some(audio_tx);
    }

    // Spawn task to process audio and handle pipeline outputs
    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    tokio::spawn(async move {
        tracing::info!("Pipeline processing task started");

        loop {
            // Check if we should stop
            if !state_clone.audio_active.load(std::sync::atomic::Ordering::SeqCst) {
                // Even when not capturing, keep processing pipeline outputs
                let mut session_guard = state_clone.session.lock().await;
                if let Some(ref mut session) = *session_guard {
                    // Try to receive outputs without blocking too long
                    match session.try_recv_output() {
                        Ok(Some(output)) => {
                            handle_pipeline_output(&app_clone, output.data);
                        }
                        Ok(None) => {
                            // No output ready, yield
                            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        }
                        Err(e) => {
                            tracing::error!("Pipeline output error: {}", e);
                            break;
                        }
                    }
                } else {
                    // Session closed, exit task
                    break;
                }
                continue;
            }

            tokio::select! {
                // Receive audio data from capture thread
                Some(audio_data) = audio_rx.recv() => {
                    let mut session_guard = state_clone.session.lock().await;
                    if let Some(ref mut session) = *session_guard {
                        let transport_data = TransportData::new(audio_data);
                        if let Err(e) = session.send_input(transport_data).await {
                            tracing::error!("Failed to send audio to pipeline: {}", e);
                        }
                    }
                }
                // Check for pipeline outputs
                _ = async {
                    let mut session_guard = state_clone.session.lock().await;
                    if let Some(ref mut session) = *session_guard {
                        match session.try_recv_output() {
                            Ok(Some(output)) => {
                                drop(session_guard); // Release lock before emitting
                                handle_pipeline_output(&app_clone, output.data);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                tracing::error!("Pipeline output error: {}", e);
                            }
                        }
                    }
                } => {}
                // Yield periodically
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {}
            }
        }

        tracing::info!("Pipeline processing task ended");
    });

    Ok(session_id)
}

/// Handle pipeline output and emit appropriate events to frontend
fn handle_pipeline_output(app: &AppHandle, data: RuntimeData) {
    match data {
        RuntimeData::Text(text) => {
            tracing::info!("Transcription: {}", text);
            let _ = app.emit(
                "transcription",
                serde_json::json!({
                    "text": text,
                    "final": true
                }),
            );
        }
        RuntimeData::Json(json) => {
            // Check if this is a VAD event
            if json.get("speech_active").is_some() || json.get("vad_state").is_some() {
                let is_speaking = json
                    .get("speech_active")
                    .or(json.get("is_speech"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                tracing::debug!("VAD state: speaking={}", is_speaking);
                let _ = app.emit(
                    "vad_state",
                    serde_json::json!({
                        "active": true,
                        "speaking": is_speaking
                    }),
                );
            }
            // Check if this is a transcription result
            else if json.get("text").is_some() && json.get("segments").is_some() {
                let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
                tracing::info!("Transcription (JSON): {}", text);
                let _ = app.emit(
                    "transcription",
                    serde_json::json!({
                        "text": text,
                        "segments": json.get("segments"),
                        "final": true
                    }),
                );
            } else {
                tracing::debug!("Pipeline JSON output: {:?}", json);
            }
        }
        RuntimeData::Audio {
            samples,
            sample_rate,
            channels,
            ..
        } => {
            // TTS output - emit for playback
            tracing::info!(
                "TTS audio output: {} samples, {} Hz",
                samples.len(),
                sample_rate
            );
            let _ = app.emit(
                "tts_audio",
                serde_json::json!({
                    "samples": samples,
                    "sample_rate": sample_rate,
                    "channels": channels
                }),
            );
        }
        _ => {
            tracing::debug!("Pipeline output: {:?}", data.data_type());
        }
    }
}

/// Shutdown the current pipeline
#[tauri::command]
pub async fn shutdown_pipeline(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    tracing::info!("Shutting down pipeline");

    // Stop audio if active
    state
        .audio_active
        .store(false, std::sync::atomic::Ordering::SeqCst);

    // Close and clear audio channel
    {
        let mut audio_tx_guard = state.audio_tx.lock().await;
        *audio_tx_guard = None;
    }

    // Close and clear session
    {
        let mut session_guard = state.session.lock().await;
        if let Some(ref mut session) = *session_guard {
            if let Err(e) = session.close().await {
                tracing::warn!("Error closing session: {}", e);
            }
        }
        *session_guard = None;
    }

    Ok(())
}
