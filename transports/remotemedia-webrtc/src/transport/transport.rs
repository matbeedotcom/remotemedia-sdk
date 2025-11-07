//! Main WebRTC transport implementation

use crate::{
    config::WebRtcTransportConfig,
    peer::{PeerConnection, PeerInfo, PeerManager},
    signaling::{IceCandidateParams, SignalingClient},
    Error, Result,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// WebRTC transport for RemoteMedia pipelines
///
/// Manages WebRTC peer connections in a mesh topology, coordinating
/// signaling, media streams, and pipeline integration.
pub struct WebRtcTransport {
    /// Transport configuration
    config: WebRtcTransportConfig,

    /// Signaling client for peer discovery
    signaling_client: Arc<RwLock<Option<SignalingClient>>>,

    /// Peer connection manager
    peer_manager: Arc<PeerManager>,

    /// Local peer ID (assigned after announcement)
    local_peer_id: Arc<RwLock<Option<String>>>,

    // Note: Session manager will be added in Phase 5
}

impl WebRtcTransport {
    /// Create a new WebRTC transport
    ///
    /// # Arguments
    ///
    /// * `config` - Transport configuration
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid.
    pub fn new(config: WebRtcTransportConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        info!("Creating WebRTC transport");

        let peer_manager = Arc::new(PeerManager::new(config.max_peers)?);

        let signaling_client = SignalingClient::new(&config.signaling_url);

        Ok(Self {
            config,
            signaling_client: Arc::new(RwLock::new(Some(signaling_client))),
            peer_manager,
            local_peer_id: Arc::new(RwLock::new(None)),
        })
    }

    /// Start the transport
    ///
    /// Connects to the signaling server and announces this peer.
    pub async fn start(&self) -> Result<()> {
        info!("Starting WebRTC transport");

        // Generate or use configured peer ID
        let peer_id = self
            .config
            .peer_id
            .clone()
            .unwrap_or_else(|| format!("peer-{}", uuid::Uuid::new_v4()));

        *self.local_peer_id.write().await = Some(peer_id.clone());

        // Connect to signaling server
        {
            let mut client_guard = self.signaling_client.write().await;
            if let Some(client) = client_guard.as_mut() {
                client.connect().await?;

                // Set up callbacks
                self.setup_signaling_callbacks(client).await;

                // Announce this peer
                let capabilities = self.get_capabilities();
                client.announce_peer(peer_id, capabilities).await?;
            }
        }

        info!("WebRTC transport started");

        Ok(())
    }

    /// Get capabilities based on configuration
    fn get_capabilities(&self) -> Vec<String> {
        let mut caps = Vec::new();

        match self.config.audio_codec {
            crate::config::AudioCodec::Opus => caps.push("audio".to_string()),
        }

        match self.config.video_codec {
            crate::config::VideoCodec::VP9 => caps.push("video".to_string()),
            crate::config::VideoCodec::H264 => caps.push("video".to_string()),
        }

        if self.config.enable_data_channel {
            caps.push("data".to_string());
        }

        caps
    }

    /// Setup signaling callbacks
    async fn setup_signaling_callbacks(&self, client: &mut SignalingClient) {
        let peer_manager = self.peer_manager.clone();
        let config = self.config.clone();

        // On offer received: create answer
        client
            .on_offer_received(move |from, _to, sdp| {
                let peer_manager = peer_manager.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    if let Err(e) = Self::handle_offer_received(peer_manager, config, from, sdp).await
                    {
                        warn!("Failed to handle offer: {}", e);
                    }
                });
            })
            .await;

        let peer_manager = self.peer_manager.clone();

        // On answer received: set remote description
        client
            .on_answer_received(move |from, _to, sdp| {
                let peer_manager = peer_manager.clone();

                tokio::spawn(async move {
                    if let Err(e) = Self::handle_answer_received(peer_manager, from, sdp).await {
                        warn!("Failed to handle answer: {}", e);
                    }
                });
            })
            .await;

        let peer_manager = self.peer_manager.clone();

        // On ICE candidate received: add to peer connection
        client
            .on_ice_candidate_received(move |from, _to, params| {
                let peer_manager = peer_manager.clone();

                tokio::spawn(async move {
                    if let Err(e) = Self::handle_ice_candidate(peer_manager, from, params).await {
                        warn!("Failed to handle ICE candidate: {}", e);
                    }
                });
            })
            .await;
    }

    /// Handle received offer
    async fn handle_offer_received(
        peer_manager: Arc<PeerManager>,
        config: WebRtcTransportConfig,
        from: String,
        sdp: String,
    ) -> Result<()> {
        debug!("Handling offer from peer: {}", from);

        // Create or get peer connection
        let peer = if peer_manager.has_peer(&from).await {
            peer_manager.get_peer(&from).await?
        } else {
            let connection = Arc::new(PeerConnection::new(from.clone(), &config)?);
            peer_manager
                .add_peer(from.clone(), connection.clone())
                .await?;
            connection
        };

        // Create answer
        let _answer = peer.create_answer(sdp).await?;

        // TODO: Send answer via signaling in Phase 3

        Ok(())
    }

    /// Handle received answer
    async fn handle_answer_received(
        peer_manager: Arc<PeerManager>,
        from: String,
        sdp: String,
    ) -> Result<()> {
        debug!("Handling answer from peer: {}", from);

        let peer = peer_manager.get_peer(&from).await?;
        peer.set_remote_description(sdp).await?;

        Ok(())
    }

    /// Handle received ICE candidate
    async fn handle_ice_candidate(
        peer_manager: Arc<PeerManager>,
        from: String,
        params: IceCandidateParams,
    ) -> Result<()> {
        debug!("Handling ICE candidate from peer: {}", from);

        let peer = peer_manager.get_peer(&from).await?;
        peer.add_ice_candidate(params.candidate).await?;

        Ok(())
    }

    /// Connect to a remote peer
    ///
    /// Initiates a connection by creating an offer and sending it via signaling.
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Identifier of the remote peer to connect to
    pub async fn connect_peer(&self, peer_id: String) -> Result<String> {
        info!("Connecting to peer: {}", peer_id);

        // Create peer connection
        let connection = Arc::new(PeerConnection::new(peer_id.clone(), &self.config)?);

        // Add to manager
        self.peer_manager
            .add_peer(peer_id.clone(), connection.clone())
            .await?;

        // Create offer
        let offer_sdp = connection.create_offer().await?;

        // Send offer via signaling
        {
            let client_guard = self.signaling_client.read().await;
            if let Some(client) = client_guard.as_ref() {
                client.send_offer(peer_id.clone(), offer_sdp).await?;
            } else {
                return Err(Error::SignalingError(
                    "Signaling client not initialized".to_string(),
                ));
            }
        }

        Ok(peer_id)
    }

    /// Disconnect from a peer
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Identifier of the peer to disconnect from
    pub async fn disconnect_peer(&self, peer_id: &str) -> Result<()> {
        info!("Disconnecting from peer: {}", peer_id);

        // Remove from peer manager (will close connection)
        self.peer_manager.remove_peer(peer_id).await?;

        // Send disconnect notification via signaling
        {
            let client_guard = self.signaling_client.read().await;
            if let Some(client) = client_guard.as_ref() {
                // Note: disconnect() sends own peer_id, not target peer_id
                client.disconnect().await?;
            }
        }

        Ok(())
    }

    /// List all connected peers
    pub async fn list_peers(&self) -> Vec<PeerInfo> {
        self.peer_manager.list_connected_peers().await
    }

    /// List all peers (any state)
    pub async fn list_all_peers(&self) -> Vec<PeerInfo> {
        self.peer_manager.list_all_peers().await
    }

    /// Get the local peer ID
    pub async fn local_peer_id(&self) -> Option<String> {
        self.local_peer_id.read().await.clone()
    }

    /// Shutdown the transport
    ///
    /// Closes all peer connections and disconnects from signaling server.
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down WebRTC transport");

        // Send disconnect notification
        {
            let client_guard = self.signaling_client.read().await;
            if let Some(client) = client_guard.as_ref() {
                if let Err(e) = client.disconnect().await {
                    warn!("Failed to send disconnect notification: {}", e);
                }
            }
        }

        // Close all peer connections
        self.peer_manager.clear().await?;

        // Clear signaling client
        *self.signaling_client.write().await = None;

        info!("WebRTC transport shutdown complete");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_creation() {
        let config = WebRtcTransportConfig::default();
        let transport = WebRtcTransport::new(config).unwrap();

        assert!(transport.local_peer_id.blocking_read().is_none());
    }

    #[test]
    fn test_invalid_config() {
        let mut config = WebRtcTransportConfig::default();
        config.max_peers = 0;

        assert!(WebRtcTransport::new(config).is_err());
    }

    #[test]
    fn test_get_capabilities() {
        let config = WebRtcTransportConfig::default();
        let transport = WebRtcTransport::new(config).unwrap();

        let caps = transport.get_capabilities();
        assert!(caps.contains(&"audio".to_string()));
        assert!(caps.contains(&"video".to_string()));
        assert!(caps.contains(&"data".to_string()));
    }

    #[tokio::test]
    async fn test_list_peers_empty() {
        let config = WebRtcTransportConfig::default();
        let transport = WebRtcTransport::new(config).unwrap();

        let peers = transport.list_peers().await;
        assert_eq!(peers.len(), 0);
    }
}
