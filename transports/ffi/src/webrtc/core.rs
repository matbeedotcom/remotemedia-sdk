//! WebRTC server core implementation
//!
//! This module provides the core WebRTC server functionality that is shared
//! between Node.js and Python bindings. It wraps the `remotemedia-webrtc`
//! crate's signaling service and provides event forwarding.

use super::config::{PeerInfo, PeerState, ServerState, WebRtcServerConfig};
use super::error::{WebRtcError, WebRtcResult};
use super::events::{
    DataReceivedEvent, ErrorEvent, PeerConnectedEvent, PeerDisconnectedEvent,
    PipelineOutputEvent, SessionEvent, WebRtcEvent,
};
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::transport::PipelineRunner;
use remotemedia_webrtc::signaling::WebSocketSignalingServer;
use remotemedia_webrtc::signaling::websocket::WebSocketServerHandle;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Channel capacity for event forwarding
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// WebRTC server core that wraps the signaling service
///
/// This struct is language-agnostic and is wrapped by language-specific
/// bindings (NapiWebRtcServer, PyWebRtcServer).
pub struct WebRtcServerCore {
    /// Unique server instance ID
    id: String,

    /// Server configuration
    config: WebRtcServerConfig,

    /// Current server state
    state: Arc<RwLock<ServerState>>,

    /// Connected peers
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,

    /// Active sessions
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,

    /// Event sender for forwarding to FFI callbacks
    event_tx: mpsc::Sender<WebRtcEvent>,

    /// Event receiver for FFI layer to poll
    event_rx: Arc<RwLock<Option<mpsc::Receiver<WebRtcEvent>>>>,

    /// Shutdown flag
    shutdown: Arc<AtomicBool>,

    /// Tokio runtime handle for async operations
    runtime_handle: Option<tokio::runtime::Handle>,

    /// WebSocket signaling server handle
    ws_server_handle: Arc<RwLock<Option<WebSocketServerHandle>>>,
}

/// Session information for room/group management
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Unique session identifier
    pub session_id: String,
    /// Peer IDs in this session
    pub peers: Vec<String>,
    /// Unix timestamp of creation
    pub created_at: u64,
    /// Custom session metadata
    pub metadata: HashMap<String, String>,
}

impl WebRtcServerCore {
    /// Create a new WebRTC server core
    pub fn new(config: WebRtcServerConfig) -> WebRtcResult<Self> {
        // Validate configuration
        config.validate()?;

        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            config,
            state: Arc::new(RwLock::new(ServerState::Created)),
            peers: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            shutdown: Arc::new(AtomicBool::new(false)),
            runtime_handle: tokio::runtime::Handle::try_current().ok(),
            ws_server_handle: Arc::new(RwLock::new(None)),
        })
    }

    /// Get server ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get server configuration
    pub fn config(&self) -> &WebRtcServerConfig {
        &self.config
    }

    /// Get current server state
    pub async fn state(&self) -> ServerState {
        *self.state.read().await
    }

    /// Get connected peers
    pub async fn peers(&self) -> HashMap<String, PeerInfo> {
        self.peers.read().await.clone()
    }

    /// Get active sessions
    pub async fn sessions(&self) -> HashMap<String, SessionInfo> {
        self.sessions.read().await.clone()
    }

    /// Take the event receiver for polling
    ///
    /// This can only be called once - subsequent calls return None.
    pub async fn take_event_receiver(&self) -> Option<mpsc::Receiver<WebRtcEvent>> {
        self.event_rx.write().await.take()
    }

    /// Get a clone of the event sender for forwarding events
    pub fn event_sender(&self) -> mpsc::Sender<WebRtcEvent> {
        self.event_tx.clone()
    }

    /// Start the server
    ///
    /// For embedded mode: binds to the configured port and starts WebSocket signaling server
    /// For external mode: connects to the signaling server
    pub async fn start(&self) -> WebRtcResult<()> {
        let mut state = self.state.write().await;

        if *state != ServerState::Created {
            return Err(WebRtcError::invalid_state(format!(
                "Cannot start server in {:?} state",
                *state
            )));
        }

        *state = ServerState::Starting;
        drop(state);

        tracing::info!(
            server_id = %self.id,
            port = ?self.config.port,
            signaling_url = ?self.config.signaling_url,
            "Starting WebRTC server with WebSocket JSON-RPC 2.0 signaling"
        );

        // Get the port to bind to
        let port = self.config.port.ok_or_else(|| {
            WebRtcError::config("Port is required for embedded signaling mode")
        })?;

        // Parse the pipeline manifest
        let manifest: Manifest = serde_json::from_value(self.config.manifest.clone())
            .map_err(|e| WebRtcError::config(format!("Invalid manifest: {}", e)))?;
        let manifest = Arc::new(manifest);

        // Create PipelineRunner
        let runner = Arc::new(
            PipelineRunner::new()
                .map_err(|e| WebRtcError::Internal(format!("Failed to create PipelineRunner: {}", e)))?,
        );

        // Build WebRTC transport configuration using the conversion helper
        let transport_config = Arc::new(self.config.to_webrtc_config());

        // Create WebSocket signaling server
        let ws_server = WebSocketSignalingServer::new(
            port as u16,
            Arc::clone(&transport_config),
            Arc::clone(&runner),
            Arc::clone(&manifest),
        );

        tracing::info!(
            "WebSocket signaling server listening on ws://0.0.0.0:{}/ws",
            port
        );

        // Start the WebSocket server
        let ws_handle = ws_server.start().await
            .map_err(|e| WebRtcError::Internal(format!("Failed to start WebSocket server: {}", e)))?;

        // Store the server handle
        *self.ws_server_handle.write().await = Some(ws_handle);

        let mut state = self.state.write().await;
        *state = ServerState::Running;

        tracing::info!(
            server_id = %self.id,
            port = port,
            "WebRTC server started with WebSocket JSON-RPC 2.0 signaling"
        );

        Ok(())
    }

    /// Shutdown the server gracefully
    ///
    /// Can be called from Created, Running, or Stopping states.
    /// If called from Created state, just transitions to Stopped.
    pub async fn shutdown(&self) -> WebRtcResult<()> {
        let mut state = self.state.write().await;

        if *state == ServerState::Stopped {
            return Ok(()); // Already stopped
        }

        // Allow shutdown from Created state (no cleanup needed)
        if *state == ServerState::Created {
            *state = ServerState::Stopped;
            self.shutdown.store(true, Ordering::SeqCst);
            return Ok(());
        }

        if *state != ServerState::Running && *state != ServerState::Stopping {
            return Err(WebRtcError::invalid_state(format!(
                "Cannot shutdown server in {:?} state",
                *state
            )));
        }

        *state = ServerState::Stopping;
        self.shutdown.store(true, Ordering::SeqCst);
        drop(state);

        tracing::info!(server_id = %self.id, "Shutting down WebRTC server");

        // Shutdown WebSocket server
        if let Some(handle) = self.ws_server_handle.write().await.take() {
            tracing::info!("Shutting down WebSocket signaling server...");
            handle.shutdown().await;
            tracing::info!("WebSocket signaling server shut down complete");
        }

        // Disconnect all peers
        let peers = self.peers.read().await.keys().cloned().collect::<Vec<_>>();
        for peer_id in peers {
            let _ = self.disconnect_peer_internal(&peer_id, Some("Server shutdown")).await;
        }

        // Clear sessions
        self.sessions.write().await.clear();

        let mut state = self.state.write().await;
        *state = ServerState::Stopped;

        tracing::info!(server_id = %self.id, "WebRTC server stopped");

        Ok(())
    }

    /// Check if shutdown was requested
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Send data to a specific peer
    pub async fn send_to_peer(&self, peer_id: &str, data: Vec<u8>) -> WebRtcResult<()> {
        let peers = self.peers.read().await;

        if !peers.contains_key(peer_id) {
            return Err(WebRtcError::PeerNotFound(peer_id.to_string()));
        }

        // TODO: Implement actual data sending via WebRTC data channel
        tracing::debug!(peer_id = %peer_id, data_len = data.len(), "Sending data to peer");

        Ok(())
    }

    /// Broadcast data to all connected peers
    pub async fn broadcast(&self, data: Vec<u8>) -> WebRtcResult<()> {
        let peers = self.peers.read().await.keys().cloned().collect::<Vec<_>>();

        for peer_id in peers {
            if let Err(e) = self.send_to_peer(&peer_id, data.clone()).await {
                tracing::warn!(peer_id = %peer_id, error = %e, "Failed to send to peer during broadcast");
            }
        }

        Ok(())
    }

    /// Disconnect a specific peer
    pub async fn disconnect_peer(&self, peer_id: &str, reason: Option<&str>) -> WebRtcResult<()> {
        self.disconnect_peer_internal(peer_id, reason).await
    }

    async fn disconnect_peer_internal(&self, peer_id: &str, reason: Option<&str>) -> WebRtcResult<()> {
        let mut peers = self.peers.write().await;

        if peers.remove(peer_id).is_none() {
            return Err(WebRtcError::PeerNotFound(peer_id.to_string()));
        }

        // Emit disconnect event
        let event = PeerDisconnectedEvent::new(peer_id.to_string());
        let event = if let Some(r) = reason {
            event.with_reason(r.to_string())
        } else {
            event
        };

        let _ = self.event_tx.send(WebRtcEvent::PeerDisconnected(event)).await;

        // Remove from all sessions
        drop(peers);
        let mut sessions = self.sessions.write().await;
        for (session_id, session) in sessions.iter_mut() {
            if session.peers.contains(&peer_id.to_string()) {
                session.peers.retain(|p| p != peer_id);

                let _ = self
                    .event_tx
                    .send(WebRtcEvent::Session(SessionEvent::peer_left(
                        session_id.clone(),
                        peer_id.to_string(),
                    )))
                    .await;
            }
        }

        tracing::info!(peer_id = %peer_id, reason = ?reason, "Disconnected peer");

        Ok(())
    }

    /// Create a new session (room)
    pub async fn create_session(
        &self,
        session_id: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> WebRtcResult<SessionInfo> {
        let mut sessions = self.sessions.write().await;

        if sessions.contains_key(session_id) {
            return Err(WebRtcError::Internal(format!(
                "Session already exists: {}",
                session_id
            )));
        }

        let session = SessionInfo {
            session_id: session_id.to_string(),
            peers: Vec::new(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            metadata: metadata.unwrap_or_default(),
        };

        sessions.insert(session_id.to_string(), session.clone());

        tracing::info!(session_id = %session_id, "Created session");

        Ok(session)
    }

    /// Get a session by ID
    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// Delete a session
    pub async fn delete_session(&self, session_id: &str) -> WebRtcResult<()> {
        let mut sessions = self.sessions.write().await;

        if sessions.remove(session_id).is_none() {
            return Err(WebRtcError::SessionNotFound(session_id.to_string()));
        }

        tracing::info!(session_id = %session_id, "Deleted session");

        Ok(())
    }

    /// Add a peer to a session
    pub async fn add_peer_to_session(&self, session_id: &str, peer_id: &str) -> WebRtcResult<()> {
        // Verify peer exists
        let peers = self.peers.read().await;
        if !peers.contains_key(peer_id) {
            return Err(WebRtcError::PeerNotFound(peer_id.to_string()));
        }
        drop(peers);

        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| WebRtcError::SessionNotFound(session_id.to_string()))?;

        if !session.peers.contains(&peer_id.to_string()) {
            session.peers.push(peer_id.to_string());

            let _ = self
                .event_tx
                .send(WebRtcEvent::Session(SessionEvent::peer_joined(
                    session_id.to_string(),
                    peer_id.to_string(),
                )))
                .await;
        }

        Ok(())
    }

    /// Remove a peer from a session
    pub async fn remove_peer_from_session(&self, session_id: &str, peer_id: &str) -> WebRtcResult<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| WebRtcError::SessionNotFound(session_id.to_string()))?;

        if session.peers.contains(&peer_id.to_string()) {
            session.peers.retain(|p| p != peer_id);

            let _ = self
                .event_tx
                .send(WebRtcEvent::Session(SessionEvent::peer_left(
                    session_id.to_string(),
                    peer_id.to_string(),
                )))
                .await;
        }

        Ok(())
    }

    /// Broadcast to all peers in a session
    pub async fn broadcast_to_session(&self, session_id: &str, data: Vec<u8>) -> WebRtcResult<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| WebRtcError::SessionNotFound(session_id.to_string()))?;

        let peers = session.peers.clone();
        drop(sessions);

        for peer_id in peers {
            if let Err(e) = self.send_to_peer(&peer_id, data.clone()).await {
                tracing::warn!(
                    peer_id = %peer_id,
                    session_id = %session_id,
                    error = %e,
                    "Failed to send to peer during session broadcast"
                );
            }
        }

        Ok(())
    }

    // ============ Internal event emission methods ============

    /// Emit a peer connected event (called by signaling layer)
    pub(crate) async fn emit_peer_connected(&self, event: PeerConnectedEvent) {
        // Add to peers map
        let peer_info = PeerInfo {
            peer_id: event.peer_id.clone(),
            capabilities: event.capabilities.clone(),
            metadata: event.metadata.clone(),
            state: PeerState::Connected,
            connected_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };

        self.peers
            .write()
            .await
            .insert(event.peer_id.clone(), peer_info);

        // Forward event
        let _ = self.event_tx.send(WebRtcEvent::PeerConnected(event)).await;
    }

    /// Emit a peer disconnected event (called by signaling layer)
    pub(crate) async fn emit_peer_disconnected(&self, event: PeerDisconnectedEvent) {
        // Remove from peers map
        self.peers.write().await.remove(&event.peer_id);

        // Forward event
        let _ = self
            .event_tx
            .send(WebRtcEvent::PeerDisconnected(event))
            .await;
    }

    /// Emit a pipeline output event (called by pipeline)
    pub(crate) async fn emit_pipeline_output(&self, event: PipelineOutputEvent) {
        let _ = self.event_tx.send(WebRtcEvent::PipelineOutput(event)).await;
    }

    /// Emit a data received event (called by data channel)
    pub(crate) async fn emit_data_received(&self, event: DataReceivedEvent) {
        let _ = self.event_tx.send(WebRtcEvent::DataReceived(event)).await;
    }

    /// Emit an error event
    pub(crate) async fn emit_error(&self, event: ErrorEvent) {
        let _ = self.event_tx.send(WebRtcEvent::Error(event)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WebRtcServerConfig {
        WebRtcServerConfig {
            port: Some(50051),
            signaling_url: None,
            manifest: serde_json::json!({}),
            stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_server_lifecycle() {
        let core = WebRtcServerCore::new(test_config()).unwrap();

        assert_eq!(core.state().await, ServerState::Created);

        core.start().await.unwrap();
        assert_eq!(core.state().await, ServerState::Running);

        core.shutdown().await.unwrap();
        assert_eq!(core.state().await, ServerState::Stopped);
    }

    #[tokio::test]
    async fn test_session_management() {
        let core = WebRtcServerCore::new(test_config()).unwrap();
        core.start().await.unwrap();

        // Create session
        let session = core
            .create_session("room-1", Some(HashMap::from([("name".to_string(), "Test".to_string())])))
            .await
            .unwrap();

        assert_eq!(session.session_id, "room-1");
        assert!(session.peers.is_empty());
        assert_eq!(session.metadata.get("name"), Some(&"Test".to_string()));

        // Get session
        let fetched = core.get_session("room-1").await.unwrap();
        assert_eq!(fetched.session_id, "room-1");

        // Delete session
        core.delete_session("room-1").await.unwrap();
        assert!(core.get_session("room-1").await.is_none());
    }

    #[tokio::test]
    async fn test_event_receiver() {
        let core = WebRtcServerCore::new(test_config()).unwrap();

        // Take receiver
        let rx = core.take_event_receiver().await;
        assert!(rx.is_some());

        // Second take returns None
        let rx2 = core.take_event_receiver().await;
        assert!(rx2.is_none());
    }
}
