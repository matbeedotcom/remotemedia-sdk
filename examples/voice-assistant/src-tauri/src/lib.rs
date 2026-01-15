//! Voice Assistant Tauri application library

pub mod commands;
pub mod events;
pub mod modes;

use parking_lot::RwLock;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::transport::executor::SessionHandle;
use remotemedia_core::transport::PipelineExecutor;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex as TokioMutex;

/// Application state shared across commands
pub struct AppState {
    /// Current execution mode
    pub mode: RwLock<modes::ExecutionMode>,
    /// Pipeline executor (shared across sessions)
    pub executor: Arc<PipelineExecutor>,
    /// Active pipeline session (if any)
    pub session: TokioMutex<Option<SessionHandle>>,
    /// Settings
    pub settings: RwLock<Settings>,
    /// Audio capture active flag (atomic for thread-safe access)
    pub audio_active: Arc<AtomicBool>,
    /// Channel for sending audio data to the pipeline processing task
    pub audio_tx: TokioMutex<Option<tokio::sync::mpsc::UnboundedSender<RuntimeData>>>,
}

impl AppState {
    /// Create a new AppState with initialized executor
    pub fn new() -> Result<Self, String> {
        let executor = PipelineExecutor::new()
            .map_err(|e| format!("Failed to create pipeline executor: {}", e))?;

        Ok(Self {
            mode: RwLock::new(modes::ExecutionMode::Local),
            executor: Arc::new(executor),
            session: TokioMutex::new(None),
            settings: RwLock::new(Settings::default()),
            audio_active: Arc::new(AtomicBool::new(false)),
            audio_tx: TokioMutex::new(None),
        })
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new().expect("Failed to create default AppState")
    }
}

/// Application settings
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// Execution mode preference
    pub mode: String,
    /// Remote server URL for hybrid/remote modes
    pub remote_server: Option<String>,
    /// LLM model to use
    pub llm_model: String,
    /// TTS voice
    pub tts_voice: String,
    /// VAD threshold
    pub vad_threshold: f32,
    /// Enable auto-listen after response
    pub auto_listen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: "local".to_string(),
            remote_server: None,
            llm_model: "llama3.2:1b".to_string(),
            tts_voice: "af_bella".to_string(),
            vad_threshold: 0.5,
            auto_listen: true,
        }
    }
}

/// Run the Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Initialize application state
            app.manage(Arc::new(AppState::default()));

            // Initialize progress event system and forward to Tauri events
            let mut progress_rx = remotemedia_core::nodes::progress::init_progress_events();
            let app_handle = app.handle().clone();
            
            // Use tauri's async runtime (not tokio::spawn) since we're in setup callback
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                
                loop {
                    match progress_rx.recv().await {
                        Ok(event) => {
                            // Forward progress event to frontend
                            let _ = app_handle.emit("model_progress", serde_json::json!({
                                "node_type": event.node_type,
                                "node_id": event.node_id,
                                "event_type": event.event_type,
                                "message": event.message,
                                "progress_pct": event.progress_pct,
                                "details": event.details,
                            }));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("Progress event receiver lagged by {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            });

            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::pipeline::initialize_pipeline,
            commands::pipeline::shutdown_pipeline,
            commands::audio::start_listening,
            commands::audio::stop_listening,
            commands::input::send_text_input,
            commands::settings::get_settings,
            commands::settings::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod commands_test;
