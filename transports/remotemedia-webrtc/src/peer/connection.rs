//! WebRTC peer connection management

use crate::Result;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Peer connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state, connection not yet started
    New,
    /// ICE candidates being gathered
    GatheringIce,
    /// Connection negotiation in progress
    Connecting,
    /// Connection established successfully
    Connected,
    /// Connection failed
    Failed,
    /// Connection closed
    Closed,
}

/// WebRTC peer connection wrapper
///
/// Wraps a webrtc::RTCPeerConnection and provides high-level management
/// for peer-to-peer connections.
pub struct PeerConnection {
    /// Unique identifier for remote peer
    peer_id: String,

    /// Unique identifier for this connection instance
    connection_id: String,

    /// Current connection state
    state: Arc<RwLock<ConnectionState>>,

    /// Local SDP (offer or answer)
    local_sdp: Arc<RwLock<Option<String>>>,

    /// Remote SDP (offer or answer)
    remote_sdp: Arc<RwLock<Option<String>>>,

    /// Collected ICE candidates
    ice_candidates: Arc<RwLock<Vec<String>>>,

    /// Timestamp when connection was created
    created_at: SystemTime,

    /// Timestamp when connection was established
    connected_at: Arc<RwLock<Option<SystemTime>>>,

    // Note: Media tracks and data channels will be added in Phase 3
    // when codec integration is implemented
}

impl PeerConnection {
    /// Create a new peer connection
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Unique identifier for the remote peer
    /// * `config` - Configuration for STUN/TURN servers and codec preferences
    pub fn new(peer_id: String, _config: &crate::config::WebRtcTransportConfig) -> Result<Self> {
        let connection_id = uuid::Uuid::new_v4().to_string();

        info!(
            "Creating peer connection: peer_id={}, connection_id={}",
            peer_id, connection_id
        );

        Ok(Self {
            peer_id,
            connection_id,
            state: Arc::new(RwLock::new(ConnectionState::New)),
            local_sdp: Arc::new(RwLock::new(None)),
            remote_sdp: Arc::new(RwLock::new(None)),
            ice_candidates: Arc::new(RwLock::new(Vec::new())),
            created_at: SystemTime::now(),
            connected_at: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the peer ID
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    /// Get the connection ID
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    /// Get the current connection state
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Set the connection state
    async fn set_state(&self, new_state: ConnectionState) {
        let mut state = self.state.write().await;
        let old_state = *state;

        if old_state != new_state {
            debug!(
                "Peer {} state transition: {:?} -> {:?}",
                self.peer_id, old_state, new_state
            );
            *state = new_state;

            if new_state == ConnectionState::Connected {
                *self.connected_at.write().await = Some(SystemTime::now());
            }
        }
    }

    /// Create an SDP offer
    ///
    /// Generates a local SDP offer for initiating a connection.
    /// Returns the SDP string to be sent to the remote peer via signaling.
    pub async fn create_offer(&self) -> Result<String> {
        self.set_state(ConnectionState::GatheringIce).await;

        // TODO: Integrate with webrtc crate in Phase 3
        // For now, return a placeholder SDP
        let sdp = format!(
            "v=0\r\no=- {} 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n",
            self.connection_id
        );

        *self.local_sdp.write().await = Some(sdp.clone());

        debug!("Created SDP offer for peer {}", self.peer_id);

        Ok(sdp)
    }

    /// Create an SDP answer in response to an offer
    ///
    /// # Arguments
    ///
    /// * `offer_sdp` - The SDP offer received from the remote peer
    pub async fn create_answer(&self, offer_sdp: String) -> Result<String> {
        *self.remote_sdp.write().await = Some(offer_sdp);
        self.set_state(ConnectionState::GatheringIce).await;

        // TODO: Integrate with webrtc crate in Phase 3
        let sdp = format!(
            "v=0\r\no=- {} 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n",
            self.connection_id
        );

        *self.local_sdp.write().await = Some(sdp.clone());

        debug!("Created SDP answer for peer {}", self.peer_id);

        Ok(sdp)
    }

    /// Set the remote SDP description
    ///
    /// # Arguments
    ///
    /// * `sdp` - The SDP string (offer or answer) from the remote peer
    pub async fn set_remote_description(&self, sdp: String) -> Result<()> {
        debug!("Setting remote description for peer {}", self.peer_id);

        *self.remote_sdp.write().await = Some(sdp);
        self.set_state(ConnectionState::Connecting).await;

        // TODO: Integrate with webrtc crate in Phase 3

        Ok(())
    }

    /// Add an ICE candidate
    ///
    /// # Arguments
    ///
    /// * `candidate` - ICE candidate string from remote peer
    pub async fn add_ice_candidate(&self, candidate: String) -> Result<()> {
        debug!(
            "Adding ICE candidate for peer {}: {}",
            self.peer_id, candidate
        );

        self.ice_candidates.write().await.push(candidate);

        // TODO: Integrate with webrtc crate in Phase 3

        Ok(())
    }

    /// Close the connection
    pub async fn close(&self) -> Result<()> {
        info!("Closing peer connection for peer {}", self.peer_id);

        self.set_state(ConnectionState::Closed).await;

        // TODO: Integrate with webrtc crate in Phase 3
        // Close underlying RTCPeerConnection

        Ok(())
    }

    /// Get connection duration (if connected)
    pub async fn connection_duration(&self) -> Option<std::time::Duration> {
        self.connected_at
            .read()
            .await
            .and_then(|connected_at| connected_at.elapsed().ok())
    }

    /// Get number of collected ICE candidates
    pub async fn ice_candidate_count(&self) -> usize {
        self.ice_candidates.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WebRtcTransportConfig;

    #[tokio::test]
    async fn test_peer_connection_creation() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config).unwrap();

        assert_eq!(pc.peer_id(), "peer-test");
        assert_eq!(pc.state().await, ConnectionState::New);
    }

    #[tokio::test]
    async fn test_create_offer() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config).unwrap();

        let sdp = pc.create_offer().await.unwrap();
        assert!(!sdp.is_empty());
        assert_eq!(pc.state().await, ConnectionState::GatheringIce);
    }

    #[tokio::test]
    async fn test_create_answer() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config).unwrap();

        let offer_sdp = "v=0\r\no=- 123 2 IN IP4 127.0.0.1\r\n".to_string();
        let answer_sdp = pc.create_answer(offer_sdp).await.unwrap();

        assert!(!answer_sdp.is_empty());
        assert_eq!(pc.state().await, ConnectionState::GatheringIce);
    }

    #[tokio::test]
    async fn test_set_remote_description() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config).unwrap();

        let sdp = "v=0\r\no=- 123 2 IN IP4 127.0.0.1\r\n".to_string();
        pc.set_remote_description(sdp).await.unwrap();

        assert_eq!(pc.state().await, ConnectionState::Connecting);
    }

    #[tokio::test]
    async fn test_add_ice_candidate() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config).unwrap();

        pc.add_ice_candidate("candidate:123...".to_string())
            .await
            .unwrap();
        assert_eq!(pc.ice_candidate_count().await, 1);
    }

    #[tokio::test]
    async fn test_close() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config).unwrap();

        pc.close().await.unwrap();
        assert_eq!(pc.state().await, ConnectionState::Closed);
    }
}
