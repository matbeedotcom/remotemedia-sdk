//! WebRTC peer connection management

// Phase 4 (US2) peer connection infrastructure
#![allow(dead_code)]

use crate::media::audio::AudioEncoderConfig;
use crate::media::tracks::AudioTrack;
use crate::media::tracks::VideoTrack;
use crate::media::video::VideoEncoderConfig;
use crate::{Error, Result};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{debug, info};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection as WebRTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use webrtc::rtp_transceiver::rtp_sender::RTCRtpSender;
use webrtc::rtp_transceiver::RTCRtpTransceiver;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_remote::TrackRemote;

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

    /// Actual WebRTC peer connection
    peer_connection: Arc<WebRTCPeerConnection>,

    /// Timestamp when connection was created
    created_at: SystemTime,

    /// Timestamp when connection was established
    connected_at: Arc<RwLock<Option<SystemTime>>>,

    /// Audio track for sending audio (if configured)
    audio_track: Arc<RwLock<Option<Arc<AudioTrack>>>>,

    /// Video track for sending video (if configured)
    video_track: Arc<RwLock<Option<Arc<VideoTrack>>>>,

    /// Audio RTP sender (retained to prevent track cleanup)
    audio_sender: Arc<RwLock<Option<Arc<RTCRtpSender>>>>,

    /// Video RTP sender (retained to prevent track cleanup)
    video_sender: Arc<RwLock<Option<Arc<RTCRtpSender>>>>,
}

impl PeerConnection {
    /// Create a new peer connection
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Unique identifier for the remote peer
    /// * `config` - Configuration for STUN/TURN servers and codec preferences
    pub async fn new(
        peer_id: String,
        config: &crate::config::WebRtcTransportConfig,
    ) -> Result<Self> {
        let connection_id = uuid::Uuid::new_v4().to_string();

        info!(
            "Creating peer connection: peer_id={}, connection_id={}",
            peer_id, connection_id
        );

        // Create MediaEngine with default codecs
        let mut media_engine = MediaEngine::default();

        // Register default codecs (Opus for audio, VP8/VP9/H.264 for video)
        // Phase 5 T064-T066: VP8 and H.264 codecs are registered by webrtc-rs internally
        // AV1 support requires webrtc-rs v0.15+ (currently using v0.14)
        // Actual encoding/decoding delegated to runtime-core VideoEncoderNode/VideoDecoderNode
        media_engine
            .register_default_codecs()
            .map_err(|e| Error::WebRtcError(format!("Failed to register codecs: {}", e)))?;

        // Create InterceptorRegistry with default interceptors
        let interceptor_registry =
            register_default_interceptors(Default::default(), &mut media_engine).map_err(|e| {
                Error::WebRtcError(format!("Failed to register interceptors: {}", e))
            })?;

        // Build WebRTC API
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(interceptor_registry)
            .build();

        // Configure ICE servers (STUN/TURN)
        let ice_servers: Vec<RTCIceServer> = config
            .stun_servers
            .iter()
            .map(|url| RTCIceServer {
                urls: vec![url.clone()],
                ..Default::default()
            })
            .chain(config.turn_servers.iter().map(|turn| RTCIceServer {
                urls: vec![turn.url.clone()],
                username: turn.username.clone(),
                credential: turn.credential.clone(),
                ..Default::default()
            }))
            .collect();

        let rtc_config = RTCConfiguration {
            ice_servers,
            ..Default::default()
        };

        // Create peer connection
        let peer_connection =
            Arc::new(api.new_peer_connection(rtc_config).await.map_err(|e| {
                Error::WebRtcError(format!("Failed to create peer connection: {}", e))
            })?);

        let state = Arc::new(RwLock::new(ConnectionState::New));
        let connected_at = Arc::new(RwLock::new(None));

        // Set up connection state change handler
        let state_clone = Arc::clone(&state);
        let connected_at_clone = Arc::clone(&connected_at);
        let peer_id_clone = peer_id.clone();

        peer_connection.on_peer_connection_state_change(Box::new(
            move |s: RTCPeerConnectionState| {
                let state_clone = Arc::clone(&state_clone);
                let connected_at_clone = Arc::clone(&connected_at_clone);
                let peer_id = peer_id_clone.clone();

                Box::pin(async move {
                    let new_state = match s {
                        RTCPeerConnectionState::New => ConnectionState::New,
                        RTCPeerConnectionState::Connecting => ConnectionState::Connecting,
                        RTCPeerConnectionState::Connected => {
                            *connected_at_clone.write().await = Some(SystemTime::now());
                            ConnectionState::Connected
                        }
                        RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Closed => {
                            ConnectionState::Closed
                        }
                        RTCPeerConnectionState::Failed => ConnectionState::Failed,
                        _ => return,
                    };

                    let mut state_guard = state_clone.write().await;
                    let old_state = *state_guard;

                    if old_state != new_state {
                        debug!(
                            "Peer {} state transition: {:?} -> {:?}",
                            peer_id, old_state, new_state
                        );
                        *state_guard = new_state;
                    }
                })
            },
        ));

        Ok(Self {
            peer_id,
            connection_id,
            state,
            peer_connection,
            created_at: SystemTime::now(),
            connected_at,
            audio_track: Arc::new(RwLock::new(None)),
            video_track: Arc::new(RwLock::new(None)),
            audio_sender: Arc::new(RwLock::new(None)),
            video_sender: Arc::new(RwLock::new(None)),
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

        // Create SDP offer
        let offer = self
            .peer_connection
            .create_offer(None)
            .await
            .map_err(|e| Error::SdpError(format!("Failed to create offer: {}", e)))?;

        // Set local description
        self.peer_connection
            .set_local_description(offer)
            .await
            .map_err(|e| Error::SdpError(format!("Failed to set local description: {}", e)))?;

        // Get the SDP string
        let local_desc = self
            .peer_connection
            .local_description()
            .await
            .ok_or_else(|| {
                Error::SdpError("No local description after setting offer".to_string())
            })?;

        let sdp = local_desc.sdp.clone();

        debug!("Created SDP offer for peer {}", self.peer_id);

        Ok(sdp)
    }

    /// Create an SDP answer in response to an offer
    ///
    /// # Arguments
    ///
    /// * `offer_sdp` - The SDP offer received from the remote peer
    pub async fn create_answer(&self, offer_sdp: String) -> Result<String> {
        self.set_state(ConnectionState::GatheringIce).await;

        // Set remote description (the offer)
        let offer = RTCSessionDescription::offer(offer_sdp)
            .map_err(|e| Error::SdpError(format!("Failed to parse offer: {}", e)))?;

        self.peer_connection
            .set_remote_description(offer)
            .await
            .map_err(|e| Error::SdpError(format!("Failed to set remote description: {}", e)))?;

        // Create SDP answer
        let answer = self
            .peer_connection
            .create_answer(None)
            .await
            .map_err(|e| Error::SdpError(format!("Failed to create answer: {}", e)))?;

        // Set local description
        self.peer_connection
            .set_local_description(answer)
            .await
            .map_err(|e| Error::SdpError(format!("Failed to set local description: {}", e)))?;

        // Get the SDP string
        let local_desc = self
            .peer_connection
            .local_description()
            .await
            .ok_or_else(|| {
                Error::SdpError("No local description after setting answer".to_string())
            })?;

        let sdp = local_desc.sdp.clone();

        debug!("Created SDP answer for peer {}", self.peer_id);

        Ok(sdp)
    }

    /// Set the remote SDP description
    ///
    /// # Arguments
    ///
    /// * `sdp` - The SDP string (answer) from the remote peer
    pub async fn set_remote_description(&self, sdp: String) -> Result<()> {
        debug!("Setting remote description for peer {}", self.peer_id);

        // Assume it's an answer (since offers are handled in create_answer)
        let answer = RTCSessionDescription::answer(sdp)
            .map_err(|e| Error::SdpError(format!("Failed to parse answer: {}", e)))?;

        self.peer_connection
            .set_remote_description(answer)
            .await
            .map_err(|e| Error::SdpError(format!("Failed to set remote description: {}", e)))?;

        self.set_state(ConnectionState::Connecting).await;

        Ok(())
    }

    /// Add an ICE candidate
    ///
    /// # Arguments
    ///
    /// * `candidate` - ICE candidate JSON string from remote peer
    pub async fn add_ice_candidate(&self, candidate: String) -> Result<()> {
        debug!(
            "Adding ICE candidate for peer {}: {}",
            self.peer_id, candidate
        );

        // Parse the ICE candidate from JSON
        let ice_candidate: webrtc::ice_transport::ice_candidate::RTCIceCandidateInit =
            serde_json::from_str(&candidate).map_err(|e| {
                Error::IceCandidateError(format!("Failed to parse ICE candidate: {}", e))
            })?;

        self.peer_connection
            .add_ice_candidate(ice_candidate)
            .await
            .map_err(|e| Error::IceCandidateError(format!("Failed to add ICE candidate: {}", e)))?;

        Ok(())
    }

    /// Close the connection
    pub async fn close(&self) -> Result<()> {
        info!("Closing peer connection for peer {}", self.peer_id);

        self.set_state(ConnectionState::Closed).await;

        // Close the underlying RTCPeerConnection
        self.peer_connection.close().await.map_err(|e| {
            Error::PeerConnectionError(format!("Failed to close connection: {}", e))
        })?;

        Ok(())
    }

    /// Get connection duration (if connected)
    pub async fn connection_duration(&self) -> Option<std::time::Duration> {
        self.connected_at
            .read()
            .await
            .and_then(|connected_at| connected_at.elapsed().ok())
    }

    /// Add an audio track to this peer connection
    ///
    /// Creates a new audio track with Opus codec and adds it to the peer connection.
    /// The track will be used for sending audio data via RTP.
    ///
    /// # Arguments
    ///
    /// * `config` - Audio encoder configuration (sample rate, channels, bitrate)
    ///
    /// # Returns
    ///
    /// Returns the created AudioTrack wrapped in Arc for shared ownership.
    pub async fn add_audio_track(&self, config: AudioEncoderConfig) -> Result<Arc<AudioTrack>> {
        info!("Adding audio track to peer {}", self.peer_id);

        // Create TrackLocalStaticSample with Opus codec
        let track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "audio/opus".to_string(),
                clock_rate: config.sample_rate,
                channels: config.channels,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            format!("audio-{}", self.peer_id),
            format!("stream-{}", self.connection_id),
        ));

        // Add track to peer connection
        let sender = self
            .peer_connection
            .add_track(track.clone() as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .map_err(|e| Error::MediaTrackError(format!("Failed to add audio track: {}", e)))?;

        // Create AudioTrack wrapper
        let audio_track = Arc::new(AudioTrack::new(track, config)?);

        // Store track and sender
        *self.audio_track.write().await = Some(audio_track.clone());
        *self.audio_sender.write().await = Some(sender);

        debug!("Audio track added to peer {}", self.peer_id);

        Ok(audio_track)
    }

    /// Add a video track to this peer connection
    ///
    /// Creates a new video track with VP9 codec and adds it to the peer connection.
    /// The track will be used for sending video data via RTP.
    ///
    /// # Arguments
    ///
    /// * `config` - Video encoder configuration (resolution, framerate, bitrate)
    ///
    /// # Returns
    ///
    /// Returns the created VideoTrack wrapped in Arc for shared ownership.
    pub async fn add_video_track(&self, config: VideoEncoderConfig) -> Result<Arc<VideoTrack>> {
        info!("Adding video track to peer {}", self.peer_id);

        // Create TrackLocalStaticSample with VP9 codec
        let track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/VP9".to_string(),
                clock_rate: 90000, // Standard 90kHz clock for video
                channels: 0,       // Not used for video
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            format!("video-{}", self.peer_id),
            format!("stream-{}", self.connection_id),
        ));

        // Add track to peer connection
        let sender = self
            .peer_connection
            .add_track(track.clone() as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .map_err(|e| Error::MediaTrackError(format!("Failed to add video track: {}", e)))?;

        // Create VideoTrack wrapper with runtime-core encoder/decoder
        // Use VP8 as default WebRTC video codec
        let video_track = Arc::new(VideoTrack::new(
            track,
            remotemedia_runtime_core::data::video::VideoCodec::Vp8,
            config.width,
            config.height,
            config.bitrate,
            config.framerate,
        )?);

        // Store track and sender
        *self.video_track.write().await = Some(video_track.clone());
        *self.video_sender.write().await = Some(sender);

        debug!("Video track added to peer {}", self.peer_id);

        Ok(video_track)
    }

    /// Get the audio track (if configured)
    pub async fn audio_track(&self) -> Option<Arc<AudioTrack>> {
        self.audio_track.read().await.clone()
    }

    /// Get the video track (if configured)
    pub async fn video_track(&self) -> Option<Arc<VideoTrack>> {
        self.video_track.read().await.clone()
    }

    /// Get the underlying WebRTC peer connection
    ///
    /// Provides access to the raw RTCPeerConnection for advanced operations.
    /// Used by ServerPeer for SDP handling and ICE candidates.
    pub fn peer_connection(&self) -> &Arc<WebRTCPeerConnection> {
        &self.peer_connection
    }

    /// Register a callback for when a remote track is added
    ///
    /// This handler fires when the remote peer adds a media track (audio/video).
    /// Used for receiving audio from client microphone for VAD, STT, etc.
    ///
    /// # Arguments
    ///
    /// * `handler` - Callback function that receives the remote track and RTP receiver
    ///
    /// # Example
    ///
    /// Register a callback to handle incoming media tracks.
    /// The callback receives the remote track, RTP receiver, and transceiver.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::pin::Pin;
    /// use std::future::Future;
    ///
    /// // Handler signature for on_track callback
    /// type TrackHandler = Box<dyn Fn(
    ///     Arc<webrtc::track::track_remote::TrackRemote>,
    ///     Arc<webrtc::rtp_transceiver::rtp_receiver::RTCRtpReceiver>,
    ///     Arc<webrtc::rtp_transceiver::RTCRtpTransceiver>,
    /// ) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
    /// ```
    pub async fn on_track<F>(&self, handler: F)
    where
        F: Fn(
                Arc<TrackRemote>,
                Arc<RTCRtpReceiver>,
                Arc<RTCRtpTransceiver>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.peer_connection.on_track(Box::new(handler));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WebRtcTransportConfig;

    #[tokio::test]
    async fn test_peer_connection_creation() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config)
            .await
            .unwrap();

        assert_eq!(pc.peer_id(), "peer-test");
        assert_eq!(pc.state().await, ConnectionState::New);
        assert!(pc.audio_track().await.is_none());
        assert!(pc.video_track().await.is_none());
    }

    #[tokio::test]
    async fn test_create_offer() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config)
            .await
            .unwrap();

        let sdp = pc.create_offer().await.unwrap();
        assert!(!sdp.is_empty());
        // Note: State transitions are handled asynchronously by webrtc callbacks
    }

    #[tokio::test]
    async fn test_add_audio_track() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config)
            .await
            .unwrap();

        let audio_config = AudioEncoderConfig::default();
        let track = pc.add_audio_track(audio_config).await.unwrap();

        assert!(pc.audio_track().await.is_some());
        assert!(Arc::ptr_eq(&track, &pc.audio_track().await.unwrap()));
    }

    #[tokio::test]
    async fn test_add_video_track() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config)
            .await
            .unwrap();

        let video_config = VideoEncoderConfig::default();
        let track = pc.add_video_track(video_config).await.unwrap();

        assert!(pc.video_track().await.is_some());
        assert!(Arc::ptr_eq(&track, &pc.video_track().await.unwrap()));
    }

    #[tokio::test]
    async fn test_add_both_tracks() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config)
            .await
            .unwrap();

        // Add audio track
        let audio_config = AudioEncoderConfig::default();
        pc.add_audio_track(audio_config).await.unwrap();

        // Add video track
        let video_config = VideoEncoderConfig::default();
        pc.add_video_track(video_config).await.unwrap();

        // Verify both tracks are present
        assert!(pc.audio_track().await.is_some());
        assert!(pc.video_track().await.is_some());
    }

    #[tokio::test]
    async fn test_create_offer_with_tracks() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config)
            .await
            .unwrap();

        // Add audio track before creating offer
        let audio_config = AudioEncoderConfig::default();
        pc.add_audio_track(audio_config).await.unwrap();

        // Create offer - should include audio track in SDP
        let sdp = pc.create_offer().await.unwrap();
        assert!(!sdp.is_empty());
        assert!(sdp.contains("audio"));
    }

    #[tokio::test]
    async fn test_close() {
        let config = WebRtcTransportConfig::default();
        let pc = PeerConnection::new("peer-test".to_string(), &config)
            .await
            .unwrap();

        pc.close().await.unwrap();
        // State will eventually become Closed via callback
    }
}
