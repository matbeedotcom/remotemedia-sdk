//! Session management for SRT ingest
//!
//! This module handles the lifecycle of ingest sessions, from creation
//! to termination, including state tracking and cleanup.

use chrono::{DateTime, Utc};
use remotemedia_health_analyzer::HealthEvent;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::task::JoinHandle;

use crate::webhook::WebhookWorker;

/// Session state enum
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    /// Session created but no SRT connection yet
    Created,

    /// SRT connection established, waiting for media
    Connected,

    /// Receiving and processing media
    Streaming {
        /// When streaming started
        started_at: DateTime<Utc>,
    },

    /// Session has ended
    Ended {
        /// Reason for ending
        reason: EndReason,
        /// When the session ended
        ended_at: DateTime<Utc>,
    },
}

/// Reason for session ending
#[derive(Debug, Clone, PartialEq)]
pub enum EndReason {
    /// Client disconnected normally
    ClientDisconnect,

    /// Session reached max duration limit
    MaxDuration,

    /// Session was explicitly deleted via API
    Deleted,

    /// Session expired (token TTL)
    Expired,

    /// Error occurred during processing
    Error(String),
}

impl std::fmt::Display for EndReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndReason::ClientDisconnect => write!(f, "client_disconnect"),
            EndReason::MaxDuration => write!(f, "max_duration"),
            EndReason::Deleted => write!(f, "deleted"),
            EndReason::Expired => write!(f, "expired"),
            EndReason::Error(e) => write!(f, "error: {}", e),
        }
    }
}

/// Configuration for creating a session
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Pipeline template ID to use
    pub pipeline: String,

    /// Optional webhook URL for event delivery
    pub webhook_url: Option<String>,

    /// Enable audio analysis
    pub audio_enabled: bool,

    /// Enable video analysis
    pub video_enabled: bool,

    /// Maximum session duration in seconds
    pub max_duration_seconds: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            pipeline: "demo_audio_quality_v1".to_string(),
            webhook_url: None,
            audio_enabled: true,
            video_enabled: false,
            max_duration_seconds: 300, // 5 minutes
        }
    }
}

/// Session limits enforced during streaming
#[derive(Debug, Clone)]
pub struct SessionLimits {
    /// Maximum bitrate in bits per second
    pub max_bitrate: Option<u64>,

    /// Maximum frames per second
    pub max_fps: Option<u32>,

    /// Maximum duration in seconds
    pub max_duration: u64,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            max_bitrate: None,
            max_fps: None,
            max_duration: 3600,
        }
    }
}

/// An ingest session
pub struct IngestSession {
    /// Unique session ID
    pub id: String,

    /// Pipeline template ID
    pub pipeline_id: String,

    /// When the session was created
    pub created_at: DateTime<Utc>,

    /// Current session state
    state: RwLock<SessionState>,

    /// Broadcast channel for events (SSE, webhooks)
    pub event_tx: broadcast::Sender<HealthEvent>,

    /// Input channel for RuntimeData from SRT listener
    input_tx: mpsc::Sender<Vec<u8>>,

    /// Session configuration
    pub config: SessionConfig,

    /// Session limits
    pub limits: SessionLimits,

    /// Webhook worker task handle (if webhook URL configured)
    /// Stored to keep the task alive but not read directly
    #[allow(dead_code)]
    webhook_handle: RwLock<Option<JoinHandle<()>>>,
}

impl IngestSession {
    /// Create a new ingest session
    pub fn new(
        id: String,
        config: SessionConfig,
        limits: SessionLimits,
    ) -> (Self, mpsc::Receiver<Vec<u8>>) {
        let (event_tx, _) = broadcast::channel(256);
        let (input_tx, input_rx) = mpsc::channel(100);

        // Spawn webhook worker if URL is configured
        let webhook_handle = if let Some(ref url) = config.webhook_url {
            let event_rx = event_tx.subscribe();
            let worker = WebhookWorker::new(url.clone(), id.clone(), event_rx);
            let handle = tokio::spawn(async move {
                worker.run().await;
            });
            Some(handle)
        } else {
            None
        };

        let session = Self {
            id,
            pipeline_id: config.pipeline.clone(),
            created_at: Utc::now(),
            state: RwLock::new(SessionState::Created),
            event_tx,
            input_tx,
            config,
            limits,
            webhook_handle: RwLock::new(webhook_handle),
        };

        (session, input_rx)
    }

    /// Get the current session state
    pub async fn state(&self) -> SessionState {
        self.state.read().await.clone()
    }

    /// Check if the session is active (not ended)
    pub async fn is_active(&self) -> bool {
        !matches!(*self.state.read().await, SessionState::Ended { .. })
    }

    /// Transition to connected state
    pub async fn set_connected(&self) -> Result<(), SessionError> {
        let mut state = self.state.write().await;
        match *state {
            SessionState::Created => {
                *state = SessionState::Connected;
                Ok(())
            }
            _ => Err(SessionError::InvalidStateTransition {
                from: format!("{:?}", *state),
                to: "Connected".to_string(),
            }),
        }
    }

    /// Transition to streaming state
    pub async fn set_streaming(&self) -> Result<(), SessionError> {
        let mut state = self.state.write().await;
        match *state {
            SessionState::Created | SessionState::Connected => {
                *state = SessionState::Streaming {
                    started_at: Utc::now(),
                };
                // Emit stream started event
                let _ = self
                    .event_tx
                    .send(HealthEvent::stream_started(Some(self.id.clone())));
                Ok(())
            }
            _ => Err(SessionError::InvalidStateTransition {
                from: format!("{:?}", *state),
                to: "Streaming".to_string(),
            }),
        }
    }

    /// End the session
    pub async fn end(&self, reason: EndReason) -> Result<(), SessionError> {
        let mut state = self.state.write().await;
        if matches!(*state, SessionState::Ended { .. }) {
            return Ok(()); // Already ended
        }

        let relative_ms = match *state {
            SessionState::Streaming { started_at } => {
                (Utc::now() - started_at).num_milliseconds() as u64
            }
            _ => 0,
        };

        *state = SessionState::Ended {
            reason: reason.clone(),
            ended_at: Utc::now(),
        };

        // Emit stream ended event
        let _ = self.event_tx.send(HealthEvent::stream_ended(
            relative_ms,
            reason.to_string(),
            Some(self.id.clone()),
        ));

        Ok(())
    }

    /// Subscribe to session events
    pub fn subscribe(&self) -> broadcast::Receiver<HealthEvent> {
        self.event_tx.subscribe()
    }

    /// Get input sender for sending data to the session
    pub fn input_sender(&self) -> mpsc::Sender<Vec<u8>> {
        self.input_tx.clone()
    }

    /// Get the duration in milliseconds since streaming started (if streaming)
    pub async fn streaming_duration_ms(&self) -> Option<u64> {
        let state = self.state.read().await;
        match *state {
            SessionState::Streaming { started_at } => {
                Some((Utc::now() - started_at).num_milliseconds() as u64)
            }
            _ => None,
        }
    }
}

/// Session manager for tracking all active sessions
pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<IngestSession>>>,
    jwt_secret: String,
    max_sessions: usize,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(jwt_secret: String, max_sessions: usize) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            jwt_secret,
            max_sessions,
        }
    }

    /// Create a new session
    pub async fn create_session(
        &self,
        config: SessionConfig,
        limits: SessionLimits,
    ) -> Result<(Arc<IngestSession>, mpsc::Receiver<Vec<u8>>, String), SessionError> {
        let mut sessions = self.sessions.write().await;

        // Check session limit
        if sessions.len() >= self.max_sessions {
            return Err(SessionError::MaxSessionsReached);
        }

        // Generate session ID
        let session_id = format!("sess_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string());

        // Generate JWT token
        let jwt_validator = crate::jwt::JwtValidator::new(self.jwt_secret.clone());
        let token = jwt_validator
            .generate(&session_id, limits.max_duration as i64)
            .map_err(|e| SessionError::TokenGeneration(e.to_string()))?;

        // Create session
        let (session, input_rx) = IngestSession::new(session_id.clone(), config, limits);
        let session = Arc::new(session);

        sessions.insert(session_id.clone(), session.clone());

        // Record metrics
        crate::metrics::global_metrics().session_created();

        Ok((session, input_rx, token))
    }

    /// Get a session by ID
    pub async fn get_session(&self, id: &str) -> Option<Arc<IngestSession>> {
        self.sessions.read().await.get(id).cloned()
    }

    /// End a session
    pub async fn end_session(&self, id: &str, reason: EndReason) -> Result<(), SessionError> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or(SessionError::NotFound)?;
        session.end(reason).await
    }

    /// Remove a session from the manager (after it's ended)
    pub async fn remove_session(&self, id: &str) -> Option<Arc<IngestSession>> {
        self.sessions.write().await.remove(id)
    }

    /// Get all active session IDs
    pub async fn active_session_ids(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        let mut ids = Vec::new();
        for (id, session) in sessions.iter() {
            if session.is_active().await {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// Clean up expired sessions and enforce duration limits
    pub async fn cleanup_expired(&self) {
        let sessions = self.sessions.read().await;
        let mut to_remove = Vec::new();
        let mut to_expire = Vec::new();

        for (id, session) in sessions.iter() {
            let state = session.state().await;

            // Check if already ended - mark for removal
            if matches!(state, SessionState::Ended { .. }) {
                to_remove.push(id.clone());
                continue;
            }

            // Check if streaming duration exceeded max_duration
            if let SessionState::Streaming { started_at } = state {
                let duration_secs = (Utc::now() - started_at).num_seconds() as u64;
                if duration_secs >= session.limits.max_duration {
                    to_expire.push((id.clone(), session.clone()));
                }
            }
        }

        // Drop read lock before modifying
        drop(sessions);

        // End expired sessions
        for (id, session) in to_expire {
            tracing::info!(
                session_id = %id,
                max_duration = session.limits.max_duration,
                "Session expired due to max duration"
            );
            let _ = session.end(EndReason::MaxDuration).await;
            to_remove.push(id);
        }

        // Remove ended sessions
        if !to_remove.is_empty() {
            let mut sessions = self.sessions.write().await;
            for id in to_remove {
                sessions.remove(&id);
                tracing::debug!(session_id = %id, "Removed ended session");
            }
        }
    }

    /// Run periodic cleanup task
    ///
    /// This should be spawned as a background task to periodically clean up
    /// expired sessions and enforce duration limits.
    pub async fn run_cleanup_loop(self: Arc<Self>, interval_secs: u64, mut shutdown_rx: broadcast::Receiver<()>) {
        tracing::info!("Session cleanup task started (interval: {}s)", interval_secs);

        loop {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)) => {
                    self.cleanup_expired().await;
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Session cleanup task shutting down");
                    break;
                }
            }
        }
    }

    /// Get session count
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

/// Session-related errors
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found")]
    NotFound,

    #[error("Maximum sessions reached")]
    MaxSessionsReached,

    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Token generation failed: {0}")]
    TokenGeneration(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_state_transitions() {
        let config = SessionConfig::default();
        let limits = SessionLimits::default();
        let (session, _rx) = IngestSession::new("sess_123".to_string(), config, limits);

        // Initial state should be Created
        assert!(matches!(session.state().await, SessionState::Created));

        // Transition to Connected
        session.set_connected().await.unwrap();
        assert!(matches!(session.state().await, SessionState::Connected));

        // Transition to Streaming
        session.set_streaming().await.unwrap();
        assert!(matches!(
            session.state().await,
            SessionState::Streaming { .. }
        ));

        // End the session
        session.end(EndReason::ClientDisconnect).await.unwrap();
        assert!(matches!(session.state().await, SessionState::Ended { .. }));
    }

    #[tokio::test]
    async fn test_session_manager_create_and_get() {
        let manager = SessionManager::new("test-secret".to_string(), 10);
        let config = SessionConfig::default();
        let limits = SessionLimits::default();

        let (session, _rx, token) = manager.create_session(config, limits).await.unwrap();
        assert!(!token.is_empty());

        let retrieved = manager.get_session(&session.id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, session.id);
    }

    #[tokio::test]
    async fn test_session_manager_max_sessions() {
        let manager = SessionManager::new("test-secret".to_string(), 2);

        // Create 2 sessions (at limit)
        let config = SessionConfig::default();
        manager
            .create_session(config.clone(), SessionLimits::default())
            .await
            .unwrap();
        manager
            .create_session(config.clone(), SessionLimits::default())
            .await
            .unwrap();

        // Try to create a 3rd session
        let result = manager
            .create_session(config, SessionLimits::default())
            .await;
        match result {
            Err(SessionError::MaxSessionsReached) => {}
            Err(e) => panic!("Expected MaxSessionsReached, got {:?}", e),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[tokio::test]
    async fn test_session_event_broadcast() {
        let config = SessionConfig::default();
        let limits = SessionLimits::default();
        let (session, _rx) = IngestSession::new("sess_123".to_string(), config, limits);

        // Subscribe before streaming starts
        let mut receiver = session.subscribe();

        // Transition to streaming (should emit stream_started)
        session.set_streaming().await.unwrap();

        // Should receive stream_started event
        let event = receiver.try_recv().unwrap();
        assert!(event.is_system());
        assert_eq!(event.event_type(), "stream_started");
    }

    #[tokio::test]
    async fn test_end_session_emits_event() {
        let config = SessionConfig::default();
        let limits = SessionLimits::default();
        let (session, _rx) = IngestSession::new("sess_123".to_string(), config, limits);

        session.set_streaming().await.unwrap();

        let mut receiver = session.subscribe();
        session.end(EndReason::ClientDisconnect).await.unwrap();

        // Should receive stream_ended event
        let event = receiver.try_recv().unwrap();
        assert!(event.is_system());
        assert_eq!(event.event_type(), "stream_ended");
    }
}
