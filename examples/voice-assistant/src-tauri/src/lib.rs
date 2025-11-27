//! Voice Assistant Tauri application library

pub mod commands;
pub mod events;
pub mod modes;

use parking_lot::RwLock;
use std::sync::Arc;
use tauri::Manager;

/// Application state shared across commands
pub struct AppState {
    /// Current execution mode
    pub mode: RwLock<modes::ExecutionMode>,
    /// Pipeline state
    pub pipeline: RwLock<Option<PipelineState>>,
    /// Settings
    pub settings: RwLock<Settings>,
    /// Audio capture state
    pub audio_active: RwLock<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: RwLock::new(modes::ExecutionMode::Local),
            pipeline: RwLock::new(None),
            settings: RwLock::new(Settings::default()),
            audio_active: RwLock::new(false),
        }
    }
}

/// Pipeline runtime state
pub struct PipelineState {
    pub session_id: String,
    pub manifest: String,
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
