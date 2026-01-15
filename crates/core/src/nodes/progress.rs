//! Progress event system for long-running node operations
//!
//! This module provides a way for nodes to emit progress events during
//! initialization or model downloads, which can be captured by applications.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::sync::broadcast;

/// Progress event emitted by nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Node type that's emitting the progress
    pub node_type: String,
    /// Node ID (if available)
    pub node_id: Option<String>,
    /// Type of progress event
    pub event_type: ProgressEventType,
    /// Human-readable message
    pub message: String,
    /// Progress percentage (0.0 - 100.0) if applicable
    pub progress_pct: Option<f64>,
    /// Additional details as JSON
    pub details: Option<serde_json::Value>,
}

/// Type of progress event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressEventType {
    /// Starting a download
    DownloadStarted,
    /// Download progress update
    DownloadProgress,
    /// Download completed
    DownloadComplete,
    /// Starting to load a model
    LoadingStarted,
    /// Model loading progress
    LoadingProgress,
    /// Model loading completed
    LoadingComplete,
    /// Initialization completed
    InitComplete,
    /// An error occurred
    Error,
}

/// Global progress event broadcaster
static PROGRESS_SENDER: OnceLock<broadcast::Sender<ProgressEvent>> = OnceLock::new();

/// Initialize the progress event system
///
/// Returns a receiver that can be used to subscribe to progress events.
/// Can be called multiple times - subsequent calls just return new receivers.
pub fn init_progress_events() -> broadcast::Receiver<ProgressEvent> {
    let sender = PROGRESS_SENDER.get_or_init(|| {
        let (tx, _) = broadcast::channel(100);
        tx
    });
    sender.subscribe()
}

/// Get a subscriber for progress events
///
/// Returns None if the progress system hasn't been initialized.
pub fn subscribe_progress() -> Option<broadcast::Receiver<ProgressEvent>> {
    PROGRESS_SENDER.get().map(|s| s.subscribe())
}

/// Emit a progress event
///
/// If no subscribers are listening, this is a no-op.
pub fn emit_progress(event: ProgressEvent) {
    // Log first (before moving event)
    match &event.event_type {
        ProgressEventType::DownloadProgress | ProgressEventType::LoadingProgress => {
            if let Some(pct) = event.progress_pct {
                tracing::debug!("[{}] {}: {:.1}%", event.node_type, event.message, pct);
            }
        }
        ProgressEventType::Error => {
            tracing::error!("[{}] {}", event.node_type, event.message);
        }
        _ => {
            tracing::info!("[{}] {}", event.node_type, event.message);
        }
    }
    
    // Then send to subscribers
    if let Some(sender) = PROGRESS_SENDER.get() {
        // Ignore send errors (no receivers)
        let _ = sender.send(event);
    }
}

/// Helper to emit download progress
pub fn emit_download_progress(node_type: &str, source: &str, progress_pct: f64) {
    emit_progress(ProgressEvent {
        node_type: node_type.to_string(),
        node_id: None,
        event_type: ProgressEventType::DownloadProgress,
        message: format!("Downloading model from {}", source),
        progress_pct: Some(progress_pct),
        details: Some(serde_json::json!({ "source": source })),
    });
}

/// Helper to emit loading progress
pub fn emit_loading_progress(node_type: &str, progress_pct: f64) {
    emit_progress(ProgressEvent {
        node_type: node_type.to_string(),
        node_id: None,
        event_type: ProgressEventType::LoadingProgress,
        message: "Loading model".to_string(),
        progress_pct: Some(progress_pct),
        details: None,
    });
}

/// Helper to emit initialization complete
pub fn emit_init_complete(node_type: &str, node_id: Option<&str>) {
    emit_progress(ProgressEvent {
        node_type: node_type.to_string(),
        node_id: node_id.map(|s| s.to_string()),
        event_type: ProgressEventType::InitComplete,
        message: "Initialization complete".to_string(),
        progress_pct: Some(100.0),
        details: None,
    });
}
