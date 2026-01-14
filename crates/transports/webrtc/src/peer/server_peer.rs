//! Server-side WebRTC peer with pipeline integration
//!
//! ServerPeer represents a server-side WebRTC peer connection that automatically
//! routes media through a RemoteMedia pipeline. Created when clients announce
//! via gRPC signaling.
//!
//! ## Multi-Track Streaming (Spec 013)
//!
//! ServerPeer supports dynamic multi-track streaming via TrackRegistry and FrameRouter:
//! - Tracks are created lazily when first frame with new stream_id arrives
//! - Frames are routed to appropriate tracks based on stream_id field
//! - Backward compatible: frames without stream_id use default track

// Phase 4 (US2) server peer infrastructure
#![allow(dead_code)]
use crate::{config::WebRtcTransportConfig, peer::PeerConnection, Error, Result};
use crate::media::{
    TrackRegistry, FrameRouter, DEFAULT_STREAM_ID,
    tracks::{AudioTrack, VideoTrack},
    extract_stream_id,
};
#[cfg(feature = "ws-signaling")]
use crate::signaling::{WebRtcEventBridge, current_timestamp_ns};
use prost::Message;
use remotemedia_core::{
    data::RuntimeData,
    manifest::Manifest,
    transport::{PipelineExecutor, SessionHandle, StreamSession, TransportData},
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

/// Server-side WebRTC peer with pipeline integration
///
/// Automatically created when a client announces via gRPC signaling.
/// Handles bidirectional media routing: client ↔ WebRTC ↔ pipeline ↔ WebRTC ↔ client
///
/// ## Multi-Track Support (Spec 013)
///
/// The ServerPeer now supports dynamic multi-track streaming:
/// - Uses `TrackRegistry` to manage multiple audio/video tracks per peer
/// - Routes frames based on `stream_id` field in RuntimeData
/// - Auto-creates tracks on first frame with new stream_id
/// - Maintains backward compatibility via DEFAULT_STREAM_ID fallback
pub struct ServerPeer {
    /// Unique identifier for the remote client
    peer_id: String,

    /// WebRTC peer connection
    peer_connection: Arc<PeerConnection>,

    /// Pipeline runner
    executor: Arc<PipelineExecutor>,

    /// Pipeline manifest
    manifest: Arc<Manifest>,

    /// Track registry for multi-track support (Spec 013)
    track_registry: Arc<TrackRegistry<AudioTrack, VideoTrack>>,

    /// Optional event sender for FFI integration
    #[cfg(feature = "ws-signaling")]
    event_tx: Option<mpsc::Sender<WebRtcEventBridge>>,

    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Arc<RwLock<Option<mpsc::Receiver<()>>>>,
}

impl ServerPeer {
    /// Create a new server peer without event forwarding
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Unique identifier for the remote client
    /// * `config` - WebRTC transport configuration (STUN/TURN servers, etc.)
    /// * `runner` - Pipeline runner for media processing
    /// * `manifest` - Pipeline manifest (defines processing graph)
    ///
    /// # Returns
    ///
    /// ServerPeer ready to accept offers and stream media through pipeline
    pub async fn new(
        peer_id: String,
        config: &WebRtcTransportConfig,
        executor: Arc<PipelineExecutor>,
        manifest: Arc<Manifest>,
    ) -> Result<Self> {
        info!("Creating server peer: {}", peer_id);

        // Create WebRTC peer connection
        let peer_connection = Arc::new(PeerConnection::new(peer_id.clone(), config).await?);

        // Create track registry for multi-track support (Spec 013)
        let track_registry = Arc::new(TrackRegistry::new(peer_id.clone()));

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Ok(Self {
            peer_id,
            peer_connection,
            executor,
            manifest,
            track_registry,
            #[cfg(feature = "ws-signaling")]
            event_tx: None,
            shutdown_tx,
            shutdown_rx: Arc::new(RwLock::new(Some(shutdown_rx))),
        })
    }

    /// Create a new server peer with optional event forwarding
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Unique identifier for the remote client
    /// * `config` - WebRTC transport configuration (STUN/TURN servers, etc.)
    /// * `runner` - Pipeline runner for media processing
    /// * `manifest` - Pipeline manifest (defines processing graph)
    /// * `event_tx` - Optional event sender for FFI integration (only with ws-signaling feature)
    ///
    /// # Returns
    ///
    /// ServerPeer ready to accept offers and stream media through pipeline
    #[cfg(feature = "ws-signaling")]
    pub async fn new_with_events(
        peer_id: String,
        config: &WebRtcTransportConfig,
        executor: Arc<PipelineExecutor>,
        manifest: Arc<Manifest>,
        event_tx: Option<mpsc::Sender<WebRtcEventBridge>>,
    ) -> Result<Self> {
        info!("Creating server peer: {}", peer_id);

        // Create WebRTC peer connection
        let peer_connection = Arc::new(PeerConnection::new(peer_id.clone(), config).await?);

        // Create track registry for multi-track support (Spec 013)
        let track_registry = Arc::new(TrackRegistry::new(peer_id.clone()));

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Ok(Self {
            peer_id,
            peer_connection,
            executor,
            manifest,
            track_registry,
            event_tx,
            shutdown_tx,
            shutdown_rx: Arc::new(RwLock::new(Some(shutdown_rx))),
        })
    }

    /// Handle incoming SDP offer from client
    ///
    /// Processes the offer, creates a pipeline session, sets up media routing,
    /// and generates a real SDP answer.
    ///
    /// # Arguments
    ///
    /// * `offer_sdp` - SDP offer string from client
    ///
    /// # Returns
    ///
    /// SDP answer string to send back to client
    pub async fn handle_offer(&self, offer_sdp: String) -> Result<String> {
        info!("ServerPeer {} handling offer", self.peer_id);

        // Create pipeline session FIRST
        let session_handle = self
            .executor
            .create_session(Arc::clone(&self.manifest))
            .await
            .map_err(|e| {
                Error::InternalError(format!("Failed to create pipeline session: {}", e))
            })?;

        info!("Created pipeline session for peer {}", self.peer_id);

        // Add audio track for sending pipeline audio output to client (requires opus-codec feature)
        // Note: This sets the Opus clock rate in SDP - must match pipeline output sample rate
        let audio_config = crate::media::audio::AudioEncoderConfig {
            sample_rate: 48000, // Match client input/output sample rate
            channels: 1,
            bitrate: 64000,
            complexity: 10,
            ..Default::default() // Use default ring_buffer_capacity (1500 frames = 30s)
        };

        self.peer_connection
            .add_audio_track(audio_config)
            .await
            .map_err(|e| Error::InternalError(format!("Failed to add audio track: {}", e)))?;

        // Register default audio track in registry (Spec 013: backward compatibility)
        if let Some(audio_track) = self.peer_connection.audio_track().await {
            self.track_registry
                .register_audio_track(DEFAULT_STREAM_ID, audio_track)
                .await
                .map_err(|e| Error::InternalError(format!("Failed to register default audio track: {}", e)))?;
        }

        info!("Added audio track to peer connection for {}", self.peer_id);

        // Add video track for sending pipeline video output to client
        let video_config = crate::media::video::VideoEncoderConfig {
            width: 1280,
            height: 720,
            framerate: 30,
            bitrate: 2_000_000,
            keyframe_interval: 60,
        };

        self.peer_connection
            .add_video_track(video_config)
            .await
            .map_err(|e| Error::InternalError(format!("Failed to add video track: {}", e)))?;

        // Register default video track in registry (Spec 013: backward compatibility)
        if let Some(video_track) = self.peer_connection.video_track().await {
            self.track_registry
                .register_video_track(DEFAULT_STREAM_ID, video_track)
                .await
                .map_err(|e| Error::InternalError(format!("Failed to register default video track: {}", e)))?;
        }

        info!("Added video track to peer connection for {}", self.peer_id);

        // Set up bidirectional media routing and data channel (this will set up the data channel handler)
        self.setup_media_routing_and_data_channel(session_handle)
            .await?;

        // Now set remote description (offer) - data channel handler is already registered
        let offer = RTCSessionDescription::offer(offer_sdp)
            .map_err(|e| Error::WebRtcError(format!("Invalid offer SDP: {}", e)))?;

        self.peer_connection
            .peer_connection()
            .set_remote_description(offer)
            .await
            .map_err(|e| Error::WebRtcError(format!("Failed to set remote description: {}", e)))?;

        // Create answer
        let answer = self
            .peer_connection
            .peer_connection()
            .create_answer(None)
            .await
            .map_err(|e| Error::WebRtcError(format!("Failed to create answer: {}", e)))?;

        // Set local description (answer)
        self.peer_connection
            .peer_connection()
            .set_local_description(answer.clone())
            .await
            .map_err(|e| Error::WebRtcError(format!("Failed to set local description: {}", e)))?;

        info!("Generated SDP answer for peer {}", self.peer_id);

        Ok(answer.sdp)
    }

    /// Set up bidirectional media routing and data channel
    ///
    /// - Incoming: WebRTC tracks + data channel → RuntimeData → pipeline input
    /// - Outgoing: pipeline output → RuntimeData → WebRTC tracks + data channel
    async fn setup_media_routing_and_data_channel(
        &self,
        mut session_handle: SessionHandle,
    ) -> Result<()> {
        info!(
            "Setting up media routing and data channel for peer {}",
            self.peer_id
        );

        // Create channel for data channel messages to pipeline
        let (dc_input_tx, mut dc_input_rx) = mpsc::channel::<TransportData>(32);

        // Clone dc_input_tx before moving into closures
        let dc_input_tx_for_dc = dc_input_tx.clone();
        let dc_input_tx_for_track = dc_input_tx.clone();

        // Clone event_tx for closures (FFI event forwarding)
        #[cfg(feature = "ws-signaling")]
        let event_tx_for_dc = self.event_tx.clone();
        #[cfg(feature = "ws-signaling")]
        let event_tx_for_output = self.event_tx.clone();

        // Create shared data channel reference for output routing
        let data_channel_ref: Arc<RwLock<Option<Arc<RTCDataChannel>>>> =
            Arc::new(RwLock::new(None));
        let data_channel_ref_for_dc = Arc::clone(&data_channel_ref);

        // Set up data channel handler
        let peer_id_for_dc = self.peer_id.clone();
        #[cfg(feature = "ws-signaling")]
        let event_tx_for_dc_clone = event_tx_for_dc.clone();
        self.peer_connection
            .peer_connection()
            .on_data_channel(Box::new(move |data_channel| {
                let peer_id = peer_id_for_dc.clone();
                let dc_input_tx = dc_input_tx_for_dc.clone();
                let data_channel_ref = Arc::clone(&data_channel_ref_for_dc);
                #[cfg(feature = "ws-signaling")]
                let event_tx = event_tx_for_dc_clone.clone();
                let data_channel = Arc::new(data_channel);

                Box::pin(async move {
                    info!("Data channel opened: label={}, id={:?} for peer {}",
                        data_channel.label(), data_channel.id(), peer_id);

                    // Store data channel reference for output routing
                    {
                        let mut dc_ref = data_channel_ref.write().await;
                        *dc_ref = Some(Arc::clone(&data_channel));
                        info!("Stored data channel reference for output routing (peer {})", peer_id);
                    }

                    // Clone data_channel for the message handler
                    let dc_for_handler = Arc::clone(&data_channel);

                    // Set up message handler - expects Protobuf-encoded DataBuffer
                    #[cfg(feature = "ws-signaling")]
                    let event_tx_for_msg = event_tx.clone();
                    dc_for_handler.on_message(Box::new(move |msg| {
                        let peer_id = peer_id.clone();
                        let dc_input_tx = dc_input_tx.clone();
                        #[cfg(feature = "ws-signaling")]
                        let event_tx = event_tx_for_msg.clone();

                        Box::pin(async move {
                            info!("Received data channel message: {} bytes from peer {}", msg.data.len(), peer_id);

                            // Emit raw data received event for FFI integration
                            #[cfg(feature = "ws-signaling")]
                            if let Some(ref tx) = event_tx {
                                let event = WebRtcEventBridge::data_received(
                                    peer_id.clone(),
                                    msg.data.to_vec(),
                                    current_timestamp_ns(),
                                );
                                if let Err(e) = tx.send(event).await {
                                    warn!("Failed to emit data_received event: {}", e);
                                }
                            }

                            // Deserialize Protobuf DataBuffer
                            match crate::generated::DataBuffer::decode(&msg.data[..]) {
                                Ok(data_buffer) => {
                                    // Convert Protobuf DataBuffer → RuntimeData
                                    match crate::adapters::data_buffer_to_runtime_data(&data_buffer) {
                                        Some(runtime_data) => {
                                            info!("Decoded RuntimeData from data channel: type={}", runtime_data.data_type());

                                            // Create TransportData
                                            let transport_data = remotemedia_core::transport::TransportData {
                                                data: runtime_data,
                                                sequence: None,
                                                metadata: std::collections::HashMap::new(),
                                            };

                                            if let Err(e) = dc_input_tx.send(transport_data).await {
                                                error!("Failed to forward data channel message to pipeline: {}", e);
                                            }
                                        }
                                        None => {
                                            error!("Failed to convert DataBuffer to RuntimeData: invalid data type");
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to decode Protobuf DataBuffer from data channel: {}", e);
                                }
                            }
                        })
                    }));
                })
            }));

        // Set up incoming track handlers (audio from client microphone)
        let peer_id_for_track = self.peer_id.clone();
        let peer_connection_for_track = Arc::clone(&self.peer_connection);

        self.peer_connection.on_track(move |track, _receiver, _transceiver| {
                let peer_id = peer_id_for_track.clone();
                let dc_input_tx = dc_input_tx_for_track.clone();
                let peer_connection = Arc::clone(&peer_connection_for_track);

                Box::pin(async move {
                    info!("Remote track added for peer {}: kind={}", peer_id, track.kind());

                    // Only process audio tracks
                    if track.kind() != webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Audio {
                        info!("Ignoring non-audio track for peer {}", peer_id);
                        return;
                    }

                    info!("Starting audio reception task for peer {}", peer_id);

                    // Spawn task to continuously read RTP packets and decode audio
                    tokio::spawn(async move {
                        // Get the audio track decoder
                        let audio_track = match peer_connection.audio_track().await {
                            Some(track) => track,
                            None => {
                                error!("No audio track available for decoding for peer {}", peer_id);
                                return;
                            }
                        };

                        loop {
                            // Read RTP packet
                            let (rtp_packet, _) = match track.read_rtp().await {
                                Ok(packet) => packet,
                                Err(e) => {
                                    debug!("RTP read error for peer {}: {} (connection may be closed)", peer_id, e);
                                    break;
                                }
                            };

                            // Decode Opus payload to audio samples
                            match audio_track.on_rtp_packet(&rtp_packet.payload).await {
                                Ok(samples) => {
                                    debug!("Decoded {} audio samples from peer {}", samples.len(), peer_id);

                                    // Send decoded audio to pipeline
                                    let transport_data = TransportData {
                                        data: RuntimeData::Audio {
                                            samples,
                                            sample_rate: 48000, // Opus always decodes to 48kHz
                                            channels: 1,
                                            stream_id: None, // From client, uses default track
                                            timestamp_us: Some(0),
                                            arrival_ts_us: None,
                                        },
                                        sequence: None,
                                        metadata: std::collections::HashMap::new(),
                                    };

                                    if let Err(e) = dc_input_tx.send(transport_data).await {
                                        error!("Failed to send decoded audio to pipeline for peer {}: {}", peer_id, e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to decode audio packet for peer {}: {}", peer_id, e);
                                    // Continue processing next packets
                                }
                            }
                        }

                        info!("Audio reception task ended for peer {}", peer_id);
                    });
                })
            }).await;

        // Spawn task to handle bidirectional routing
        let peer_id = self.peer_id.clone();
        let _peer_connection = Arc::clone(&self.peer_connection);
        let track_registry = Arc::clone(&self.track_registry);
        let data_channel_for_output = Arc::clone(&data_channel_ref);
        #[cfg(feature = "ws-signaling")]
        let event_tx_for_output = event_tx_for_output;
        let mut shutdown_rx =
            self.shutdown_rx.write().await.take().ok_or_else(|| {
                Error::InternalError("Shutdown receiver already taken".to_string())
            })?;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Use biased to process shutdown and inputs with priority over outputs
                    biased;

                    // Check for shutdown signal (highest priority)
                    _ = shutdown_rx.recv() => {
                        info!("Shutting down media routing for peer {}", peer_id);
                        break;
                    }

                    // Forward data channel inputs to pipeline (high priority)
                    Some(transport_data) = dc_input_rx.recv() => {
                        debug!("Received data from data channel for peer {}, forwarding to session", peer_id);
                        if let Err(e) = session_handle.send_input(transport_data).await {
                            error!("Failed to send data channel input to pipeline for peer {}: {}", peer_id, e);
                        } else {
                            debug!("Successfully forwarded data channel input to session for peer {}", peer_id);
                        }
                    }

                    // Receive output from pipeline with timeout to avoid blocking inputs
                    output_result = tokio::time::timeout(
                        tokio::time::Duration::from_millis(10),
                        session_handle.recv_output()
                    ) => {
                        match output_result {
                            Ok(Ok(Some(transport_data))) => {
                                debug!("Received output from pipeline for peer {}", peer_id);

                                // Emit pipeline output event for FFI integration
                                #[cfg(feature = "ws-signaling")]
                                if let Some(ref tx) = event_tx_for_output {
                                    // Serialize RuntimeData to JSON for cross-crate transport
                                    let data_json = serde_json::to_string(&transport_data.data)
                                        .unwrap_or_else(|_| "{}".to_string());
                                    let event = WebRtcEventBridge::pipeline_output(
                                        peer_id.clone(),
                                        data_json,
                                        current_timestamp_ns(),
                                    );
                                    if let Err(e) = tx.send(event).await {
                                        warn!("Failed to emit pipeline_output event: {}", e);
                                    }
                                }

                                // Send to WebRTC
                                if let Err(e) = Self::send_to_webrtc_multitrack(&track_registry, &data_channel_for_output, transport_data).await {
                                    error!("Failed to send output to WebRTC for peer {}: {}", peer_id, e);
                                }
                            }
                            Ok(Ok(None)) => {
                                // Channel temporarily empty - this is normal between requests
                                // DON'T break the loop - keep waiting for new inputs
                                tracing::trace!("No output available for peer {}, waiting for next input", peer_id);
                            }
                            Ok(Err(e)) => {
                                // recv_output should never return Err in normal operation
                                error!("Error receiving pipeline output for peer {}: {}", peer_id, e);
                                // DON'T break - log and continue
                                tracing::warn!("Continuing despite recv_output error for peer {}", peer_id);
                            }
                            Err(_timeout) => {
                                // Timeout waiting for output - this is normal, continue to check inputs
                                // Using trace level to avoid spam
                                tracing::trace!("Timeout waiting for output from peer {}", peer_id);
                            }
                        }
                    }
                }
            }

            // CRITICAL: Explicitly close the session to terminate the pipeline and cleanup resources
            // This ensures any background processing tasks know to stop and prevents stale data
            info!("Closing pipeline session for peer {}", peer_id);
            if let Err(e) = session_handle.close().await {
                warn!("Error closing pipeline session for peer {}: {}", peer_id, e);
            }

            info!("Media routing task ended for peer {}", peer_id);
        });

        Ok(())
    }

    /// Send TransportData to WebRTC peer connection using multi-track routing (Spec 013)
    ///
    /// Routes RuntimeData to appropriate tracks based on stream_id field.
    /// Falls back to DEFAULT_STREAM_ID for backward compatibility.
    /// Json and Text data are sent through the data channel.
    ///
    /// # Arguments
    ///
    /// * `track_registry` - Registry of audio/video tracks keyed by stream_id
    /// * `data_channel` - Optional data channel for Json/Text output
    /// * `transport_data` - Data to send, containing RuntimeData with optional stream_id
    async fn send_to_webrtc_multitrack(
        track_registry: &Arc<TrackRegistry<AudioTrack, VideoTrack>>,
        data_channel: &Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
        transport_data: TransportData,
    ) -> Result<()> {
        // Get RuntimeData and extract stream_id
        let runtime_data = transport_data.data;
        let stream_id = extract_stream_id(&runtime_data)
            .unwrap_or(DEFAULT_STREAM_ID);

        match &runtime_data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                ..
            } => {
                debug!(
                    "Sending audio to stream '{}': {} samples, {}Hz, {} channels",
                    stream_id,
                    samples.len(),
                    sample_rate,
                    channels
                );

                // Get the audio track from registry
                if let Some(audio_track) = track_registry.get_audio_track(stream_id).await {
                    // Send audio samples through the track with dynamic sample rate
                    audio_track
                        .send_audio(Arc::new(samples.clone()), *sample_rate)
                        .await?;

                    // Record frame for activity tracking
                    track_registry.record_audio_frame(stream_id).await;
                    debug!("Audio sent successfully to stream '{}'", stream_id);
                } else {
                    warn!(
                        "No audio track for stream_id '{}', cannot send audio (registered tracks: {:?})",
                        stream_id,
                        track_registry.audio_stream_ids().await
                    );
                }
            }
            RuntimeData::Video { .. } => {
                debug!("Sending video frame to stream '{}' via WebRTC", stream_id);

                // Get the video track from registry
                if let Some(video_track) = track_registry.get_video_track(stream_id).await {
                    // Send video using VideoTrack's send_video_runtime_data method
                    video_track
                        .send_video_runtime_data(runtime_data.clone())
                        .await?;

                    // Record frame for activity tracking
                    track_registry.record_video_frame(stream_id).await;
                    debug!("Video sent successfully to stream '{}'", stream_id);
                } else {
                    warn!(
                        "No video track for stream_id '{}', cannot send video (registered tracks: {:?})",
                        stream_id,
                        track_registry.video_stream_ids().await
                    );
                }
            }
            RuntimeData::Json(_) | RuntimeData::Text(_) => {
                // Send Json/Text data through data channel
                let dc_guard = data_channel.read().await;
                if let Some(dc) = dc_guard.as_ref() {
                    // Convert RuntimeData to Protobuf DataBuffer
                    let data_buffer = crate::adapters::runtime_data_to_data_buffer(&runtime_data);
                    let encoded = data_buffer.encode_to_vec();

                    debug!(
                        "Sending {} data ({} bytes) through data channel",
                        runtime_data.data_type(),
                        encoded.len()
                    );

                    // Send through data channel
                    if let Err(e) = dc.send(&bytes::Bytes::from(encoded)).await {
                        error!("Failed to send data through data channel: {}", e);
                        return Err(Error::WebRtcError(format!(
                            "Data channel send failed: {}",
                            e
                        )));
                    }

                    debug!("Successfully sent {} data through data channel", runtime_data.data_type());
                } else {
                    warn!(
                        "No data channel available to send {} output",
                        runtime_data.data_type()
                    );
                }
            }
            _ => {
                debug!("Unsupported RuntimeData type for WebRTC output: {}", runtime_data.data_type());
            }
        }

        Ok(())
    }

    /// Handle incoming ICE candidate from client
    pub async fn handle_ice_candidate(
        &self,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    ) -> Result<()> {
        use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

        info!("ServerPeer {} adding ICE candidate", self.peer_id);

        debug!(
            "ICE candidate: candidate={}, sdp_mid={:?}, sdp_mline_index={:?}",
            candidate, sdp_mid, sdp_mline_index
        );

        // Create ICE candidate init
        let ice_candidate_init = RTCIceCandidateInit {
            candidate: candidate.clone(),
            sdp_mid,
            sdp_mline_index,
            username_fragment: None,
        };

        // Add ICE candidate to peer connection
        self.peer_connection
            .peer_connection()
            .add_ice_candidate(ice_candidate_init)
            .await
            .map_err(|e| Error::WebRtcError(format!("Failed to add ICE candidate: {}", e)))?;

        info!("ICE candidate added successfully for peer {}", self.peer_id);

        Ok(())
    }

    /// Get the peer ID
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    /// Get the underlying peer connection
    pub fn peer_connection(&self) -> &Arc<PeerConnection> {
        &self.peer_connection
    }

    /// Get the track registry for multi-track management (Spec 013)
    ///
    /// The registry allows external code to:
    /// - Query registered tracks by stream_id
    /// - Register new tracks dynamically
    /// - Monitor track activity
    pub fn track_registry(&self) -> &Arc<TrackRegistry<AudioTrack, VideoTrack>> {
        &self.track_registry
    }

    /// Get the number of registered audio tracks
    pub async fn audio_track_count(&self) -> usize {
        self.track_registry.audio_track_count().await
    }

    /// Get the number of registered video tracks
    pub async fn video_track_count(&self) -> usize {
        self.track_registry.video_track_count().await
    }

    /// Get all registered audio stream IDs
    pub async fn audio_stream_ids(&self) -> Vec<String> {
        self.track_registry.audio_stream_ids().await
    }

    /// Get all registered video stream IDs
    pub async fn video_stream_ids(&self) -> Vec<String> {
        self.track_registry.video_stream_ids().await
    }

    /// Shutdown the server peer
    ///
    /// Closes the pipeline session and WebRTC connection
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down server peer: {}", self.peer_id);

        // Signal shutdown to media routing task
        let _ = self.shutdown_tx.send(()).await;

        // Note: Pipeline session is owned by the media routing task
        // and will be cleaned up when that task ends

        // Close WebRTC connection
        if let Err(e) = self.peer_connection.close().await {
            warn!("Error closing peer connection for {}: {}", self.peer_id, e);
        }

        info!("Server peer {} shut down", self.peer_id);

        Ok(())
    }
}

impl Drop for ServerPeer {
    fn drop(&mut self) {
        debug!("ServerPeer {} dropped", self.peer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_peer_creation() {
        // This is a placeholder test
        // Real tests would require mock WebRTC and pipeline components
        assert!(true);
    }
}
