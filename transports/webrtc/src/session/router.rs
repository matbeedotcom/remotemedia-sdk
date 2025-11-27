//! Session router for WebRTC pipeline integration
//!
//! Routes data between WebRTC peers and pipeline execution.

// Phase 4/5 session routing infrastructure
#![allow(dead_code)]

use crate::media::tracks::rtp_to_runtime_data;
use crate::peer::PeerManager;
use crate::session::{Session, SessionId};
use crate::{Error, Result};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::data::video::PixelFormat;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Router for session data flow
///
/// Routes incoming RTP data from peers to application layer, and outgoing
/// application data back to target peers.
///
/// Note: This router does NOT run the pipeline directly - it's a routing
/// layer between WebRTC peers and the application (which uses PipelineTransport).
pub struct SessionRouter {
    /// Session ID
    session_id: SessionId,

    /// Associated session
    session: Arc<Session>,

    /// Peer manager for WebRTC connections
    peer_manager: Arc<PeerManager>,

    /// Per-peer input channels (peer_id -> sender)
    peer_inputs: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<RuntimeData>>>>,

    /// Shared output channel for all nodes
    output_tx: mpsc::UnboundedSender<RuntimeData>,

    /// Shared output receiver
    output_rx: Arc<RwLock<mpsc::UnboundedReceiver<RuntimeData>>>,

    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Arc<RwLock<mpsc::Receiver<()>>>,
}

impl SessionRouter {
    /// Create a new session router
    ///
    /// # Arguments
    ///
    /// * `session_id` - Unique session identifier
    /// * `session` - Session handle
    /// * `peer_manager` - Peer manager for WebRTC connections
    pub fn new(
        session_id: SessionId,
        session: Arc<Session>,
        peer_manager: Arc<PeerManager>,
    ) -> Result<Self> {
        info!("Creating session router for session: {}", session_id);

        let (output_tx, output_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Ok(Self {
            session_id,
            session,
            peer_manager,
            peer_inputs: Arc::new(RwLock::new(HashMap::new())),
            output_tx,
            output_rx: Arc::new(RwLock::new(output_rx)),
            shutdown_tx,
            shutdown_rx: Arc::new(RwLock::new(shutdown_rx)),
        })
    }

    /// Start the routing task
    ///
    /// Spawns a background task that continuously routes data between
    /// peers.
    pub async fn start(self: Arc<Self>) -> Result<()> {
        info!("Starting session router for: {}", self.session_id);

        // Spawn routing task
        let router = Arc::clone(&self);
        tokio::spawn(async move {
            if let Err(e) = router.run_routing_loop().await {
                error!("Session router error for {}: {}", router.session_id, e);
            }
        });

        Ok(())
    }

    /// Main routing loop
    async fn run_routing_loop(&self) -> Result<()> {
        debug!("Session router loop started for: {}", self.session_id);

        let mut shutdown_rx = self.shutdown_rx.write().await;

        loop {
            tokio::select! {
                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received for session: {}", self.session_id);
                    break;
                }

                // Handle output to send to peers
                Some(output) = self.recv_output_internal() => {
                    if let Err(e) = self.route_outgoing(output).await {
                        error!("Failed to route outgoing data: {}", e);
                    }
                }
            }
        }

        info!("Session router stopped for: {}", self.session_id);
        Ok(())
    }

    /// Route incoming RTP data from peer (T126)
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Peer ID that sent the data
    /// * `payload` - RTP payload (encoded audio or video)
    /// * `is_audio` - True for audio, false for video
    pub async fn route_incoming(
        &self,
        peer_id: &str,
        payload: &[u8],
        is_audio: bool,
    ) -> Result<RuntimeData> {
        debug!(
            "Routing incoming {} data from peer {} (session: {})",
            if is_audio { "audio" } else { "video" },
            peer_id,
            self.session_id
        );

        // Get peer connection
        let peer = self.peer_manager.get_peer(peer_id).await?;

        // Convert RTP to RuntimeData
        let runtime_data =
            if is_audio {
                let audio_track = peer.audio_track().await.ok_or_else(|| {
                    Error::MediaTrackError("No audio track configured".to_string())
                })?;
                rtp_to_runtime_data(payload, true, Some(&audio_track), None).await?
            } else {
                let video_track = peer.video_track().await.ok_or_else(|| {
                    Error::MediaTrackError("No video track configured".to_string())
                })?;
                rtp_to_runtime_data(payload, false, None, Some(&video_track)).await?
            };

        Ok(runtime_data)
    }

    /// Route outgoing data to peers (T127)
    ///
    /// # Arguments
    ///
    /// * `data` - RuntimeData to send
    async fn route_outgoing(&self, data: RuntimeData) -> Result<()> {
        debug!("Routing outgoing data (session: {})", self.session_id);

        // Get all peers in this session
        let peer_ids = self.session.list_peers().await;

        if peer_ids.is_empty() {
            warn!(
                "No peers connected to session {}, dropping output",
                self.session_id
            );
            return Ok(());
        }

        // Broadcast to all peers
        for peer_id in peer_ids {
            if let Err(e) = self.send_to_peer(&peer_id, &data).await {
                error!("Failed to send to peer {}: {}", peer_id, e);
            }
        }

        Ok(())
    }

    /// Send data to a specific peer (T070 integration)
    async fn send_to_peer(&self, peer_id: &str, data: &RuntimeData) -> Result<()> {
        let peer = self.peer_manager.get_peer(peer_id).await?;

        match data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                ..
            } => {
                let audio_track = peer.audio_track().await.ok_or_else(|| {
                    Error::MediaTrackError("No audio track configured".to_string())
                })?;

                // Send audio directly (handles encoding + RTP transmission)
                // Use sample rate from RuntimeData
                audio_track
                    .send_audio(Arc::new(samples.clone()), *sample_rate)
                    .await?;
                Ok(())
            }
            RuntimeData::Video {
                width,
                height,
                pixel_data,
                format,
                timestamp_us,
                ..
            } => {
                let video_track = peer.video_track().await.ok_or_else(|| {
                    Error::MediaTrackError("No video track configured".to_string())
                })?;

                // Convert PixelFormat enum to VideoFormat enum
                use crate::media::video::VideoFormat;
                let video_format = match format {
                    PixelFormat::Rgb24 => VideoFormat::RGB24,
                    PixelFormat::Yuv420p | PixelFormat::I420 => VideoFormat::I420,
                    _ => {
                        return Err(Error::EncodingError(format!(
                            "Unsupported video format: {:?}",
                            format
                        )))
                    }
                };

                // Create VideoFrame
                use crate::media::video::VideoFrame;
                let frame = VideoFrame {
                    width: *width,
                    height: *height,
                    format: video_format,
                    data: pixel_data.clone(),
                    timestamp_us: *timestamp_us,
                    is_keyframe: video_track.should_force_keyframe().await,
                };

                // Send video directly (handles encoding + RTP transmission)
                video_track.send_video(&frame).await?;
                Ok(())
            }
            _ => Err(Error::MediaTrackError(
                "Unsupported RuntimeData type for peer transmission".to_string(),
            )),
        }
    }

    /// Send data to output channel (T128)
    ///
    /// This is used by the application to send processed data back to peers.
    pub async fn send_output(&self, data: RuntimeData) -> Result<()> {
        debug!("Sending output to router (session: {})", self.session_id);

        self.output_tx
            .send(data)
            .map_err(|e| Error::SessionError(format!("Failed to send output: {}", e)))?;

        Ok(())
    }

    /// Receive output from router (internal)
    async fn recv_output_internal(&self) -> Option<RuntimeData> {
        let mut output_rx = self.output_rx.write().await;
        output_rx.recv().await
    }

    /// Get a per-peer input channel (T129)
    ///
    /// Creates a new input channel for the peer if one doesn't exist.
    pub async fn get_peer_input_channel(
        &self,
        peer_id: &str,
    ) -> Result<mpsc::UnboundedSender<RuntimeData>> {
        let mut peer_inputs = self.peer_inputs.write().await;

        if let Some(tx) = peer_inputs.get(peer_id) {
            return Ok(tx.clone());
        }

        // Create new channel for this peer
        let (tx, _rx) = mpsc::unbounded_channel();
        peer_inputs.insert(peer_id.to_string(), tx.clone());

        Ok(tx)
    }

    /// Get shared output channel (T130)
    pub fn get_output_channel(&self) -> mpsc::UnboundedSender<RuntimeData> {
        self.output_tx.clone()
    }

    /// Shutdown the router (T131)
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down session router for: {}", self.session_id);

        // Send shutdown signal
        if let Err(e) = self.shutdown_tx.send(()).await {
            warn!("Failed to send shutdown signal: {}", e);
        }

        // Clear peer input channels
        let mut peer_inputs = self.peer_inputs.write().await;
        peer_inputs.clear();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_router_creation() {
        let session = Arc::new(Session::new("test-session".to_string()));
        let peer_manager = Arc::new(PeerManager::new(10).unwrap());

        let router = SessionRouter::new("test-session".to_string(), session, peer_manager);

        assert!(router.is_ok());
    }
}
