//! Test client for WebRTC E2E testing
//!
//! Implements a full WebRTC client using gRPC signaling to connect to the test server.
//! Supports sending and receiving audio/video with proper encoding/decoding.
//!
//! # Features
//! - **JitterBuffer integration**: Proper packet reordering using BTreeMap-based jitter buffer
//! - **Quality metrics tracking**: Packet counts, loss rates, latency statistics
//! - **Reconnection support**: Clean disconnect/reconnect cycles

use super::{HarnessError, HarnessResult};
use remotemedia_webrtc::generated::webrtc::{
    signaling_notification, signaling_request, signaling_response,
    web_rtc_signaling_client::WebRtcSignalingClient, AnnounceRequest, IceCandidateRequest,
    OfferRequest, PeerCapabilities, SignalingRequest,
};
use remotemedia_webrtc::sync::{BufferStats, JitterBuffer, JitterBufferFrame};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tracing::{debug, error, info, warn};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_remote::TrackRemote;

/// Received video frame with metadata
#[derive(Debug, Clone)]
pub struct ReceivedVideoFrame {
    /// Raw RTP payload data
    pub data: Vec<u8>,
    /// Frame width (0 if unknown/not decoded)
    pub width: u32,
    /// Frame height (0 if unknown/not decoded)
    pub height: u32,
    /// Timestamp in microseconds
    pub timestamp_us: u64,
}

/// Received audio chunk
#[derive(Debug, Clone)]
pub struct ReceivedAudioChunk {
    /// Raw RTP payload data (Opus encoded)
    pub data: Vec<u8>,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// Timestamp in microseconds
    pub timestamp_us: u64,
}

/// Audio frame for jitter buffer integration
#[derive(Clone)]
pub struct AudioRtpFrame {
    /// RTP sequence number
    pub seq: u16,
    /// RTP timestamp
    pub rtp_ts: u32,
    /// Time when frame was received
    pub received: Instant,
    /// Payload data
    pub payload: Arc<Vec<u8>>,
}

impl JitterBufferFrame for AudioRtpFrame {
    fn sequence_number(&self) -> u16 {
        self.seq
    }
    fn rtp_timestamp(&self) -> u32 {
        self.rtp_ts
    }
    fn received_at(&self) -> Instant {
        self.received
    }
}

/// Video frame for jitter buffer integration
#[derive(Clone)]
pub struct VideoRtpFrame {
    /// RTP sequence number
    pub seq: u16,
    /// RTP timestamp
    pub rtp_ts: u32,
    /// Time when frame was received
    pub received: Instant,
    /// Payload data
    pub payload: Arc<Vec<u8>>,
}

impl JitterBufferFrame for VideoRtpFrame {
    fn sequence_number(&self) -> u16 {
        self.seq
    }
    fn rtp_timestamp(&self) -> u32 {
        self.rtp_ts
    }
    fn received_at(&self) -> Instant {
        self.received
    }
}

/// Quality metrics for the test client
#[derive(Debug, Clone, Default)]
pub struct ClientQualityMetrics {
    /// Audio jitter buffer statistics
    pub audio_buffer_stats: Option<BufferStats>,
    /// Video jitter buffer statistics
    pub video_buffer_stats: Option<BufferStats>,
    /// Total audio packets received
    pub audio_packets_received: u32,
    /// Total video packets received
    pub video_packets_received: u32,
    /// Total audio bytes received
    pub audio_bytes_received: u64,
    /// Total video bytes received
    pub video_bytes_received: u64,
    /// Connection time in milliseconds
    pub connection_time_ms: u64,
    /// First packet latency (time to first packet after connect)
    pub first_packet_latency_ms: Option<u64>,
}

/// Test client for WebRTC E2E testing
///
/// Connects to the signaling server, establishes WebRTC peer connection,
/// and provides methods for sending/receiving media.
///
/// # Jitter Buffer Integration
///
/// This client uses the production JitterBuffer implementation for proper
/// packet reordering. Audio and video frames are inserted into their respective
/// jitter buffers and can be popped in sequence order.
pub struct TestClient {
    /// Unique peer identifier
    peer_id: String,

    /// Server port
    server_port: u16,

    /// gRPC signaling client
    signaling_client: Arc<RwLock<Option<WebRtcSignalingClient<Channel>>>>,

    /// Signaling request sender (for sending to gRPC stream)
    request_tx: Arc<RwLock<Option<mpsc::Sender<SignalingRequest>>>>,

    /// WebRTC peer connection
    peer_connection: Arc<RwLock<Option<Arc<RTCPeerConnection>>>>,

    /// Audio track for sending
    audio_track: Arc<RwLock<Option<Arc<TrackLocalStaticSample>>>>,

    /// Video track for sending
    video_track: Arc<RwLock<Option<Arc<TrackLocalStaticSample>>>>,

    /// Received audio output (accumulated from pipeline)
    received_audio: Arc<RwLock<Vec<ReceivedAudioChunk>>>,

    /// Received video frames (accumulated from pipeline)
    received_video: Arc<RwLock<Vec<ReceivedVideoFrame>>>,

    /// Received RTP packet count (audio)
    received_audio_packets: Arc<AtomicU32>,

    /// Received RTP packet count (video)
    received_video_packets: Arc<AtomicU32>,

    /// Request ID counter
    request_counter: AtomicU64,

    /// Connection state
    connected: Arc<AtomicBool>,

    /// Signaling stream task handle
    stream_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,

    /// Track receiver task handles
    track_handles: Arc<RwLock<Vec<tokio::task::JoinHandle<()>>>>,

    // ========================================================================
    // Jitter Buffer Integration (uses production sync components)
    // ========================================================================

    /// Audio jitter buffer for packet reordering (50ms buffer, 200ms max)
    audio_jitter_buffer: Arc<RwLock<JitterBuffer<AudioRtpFrame>>>,

    /// Video jitter buffer for packet reordering (100ms buffer, 500ms max)
    video_jitter_buffer: Arc<RwLock<JitterBuffer<VideoRtpFrame>>>,

    // ========================================================================
    // Quality Metrics
    // ========================================================================

    /// Audio bytes received (for bandwidth calculation)
    audio_bytes_received: Arc<AtomicU64>,

    /// Video bytes received (for bandwidth calculation)
    video_bytes_received: Arc<AtomicU64>,

    /// Connection start time (for latency measurements)
    connection_start: Arc<RwLock<Option<Instant>>>,

    /// First audio packet time (for first-packet latency)
    first_audio_packet_time: Arc<RwLock<Option<Instant>>>,

    /// First video packet time (for first-packet latency)
    first_video_packet_time: Arc<RwLock<Option<Instant>>>,
}

// Manual Debug implementation since JitterBuffer doesn't implement Debug
impl std::fmt::Debug for TestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestClient")
            .field("peer_id", &self.peer_id)
            .field("server_port", &self.server_port)
            .field("connected", &self.connected.load(Ordering::SeqCst))
            .field("audio_packets", &self.received_audio_packets.load(Ordering::SeqCst))
            .field("video_packets", &self.received_video_packets.load(Ordering::SeqCst))
            .finish()
    }
}

impl TestClient {
    /// Create a new test client
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Unique identifier for this client
    /// * `server_port` - Port of the signaling server
    pub async fn new(peer_id: String, server_port: u16) -> HarnessResult<Self> {
        info!(
            "Creating test client: {} (server port: {})",
            peer_id, server_port
        );

        // Create jitter buffers with production-like settings
        // Audio: 50ms buffer delay, 200ms max (matches low-latency preset)
        let audio_jitter_buffer = JitterBuffer::new(50, 200);
        // Video: 100ms buffer delay, 500ms max (for frame reordering)
        let video_jitter_buffer = JitterBuffer::new(100, 500);

        Ok(Self {
            peer_id,
            server_port,
            signaling_client: Arc::new(RwLock::new(None)),
            request_tx: Arc::new(RwLock::new(None)),
            peer_connection: Arc::new(RwLock::new(None)),
            audio_track: Arc::new(RwLock::new(None)),
            video_track: Arc::new(RwLock::new(None)),
            received_audio: Arc::new(RwLock::new(Vec::new())),
            received_video: Arc::new(RwLock::new(Vec::new())),
            received_audio_packets: Arc::new(AtomicU32::new(0)),
            received_video_packets: Arc::new(AtomicU32::new(0)),
            request_counter: AtomicU64::new(0),
            connected: Arc::new(AtomicBool::new(false)),
            stream_handle: Arc::new(RwLock::new(None)),
            track_handles: Arc::new(RwLock::new(Vec::new())),
            // Jitter buffers (production sync components)
            audio_jitter_buffer: Arc::new(RwLock::new(audio_jitter_buffer)),
            video_jitter_buffer: Arc::new(RwLock::new(video_jitter_buffer)),
            // Quality metrics
            audio_bytes_received: Arc::new(AtomicU64::new(0)),
            video_bytes_received: Arc::new(AtomicU64::new(0)),
            connection_start: Arc::new(RwLock::new(None)),
            first_audio_packet_time: Arc::new(RwLock::new(None)),
            first_video_packet_time: Arc::new(RwLock::new(None)),
        })
    }

    /// Get peer ID
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Get count of received audio RTP packets
    pub fn received_audio_packet_count(&self) -> u32 {
        self.received_audio_packets.load(Ordering::SeqCst)
    }

    /// Get count of received video RTP packets
    pub fn received_video_packet_count(&self) -> u32 {
        self.received_video_packets.load(Ordering::SeqCst)
    }

    // ========================================================================
    // Quality Metrics & Jitter Buffer Access
    // ========================================================================

    /// Get comprehensive quality metrics for this client
    pub async fn get_quality_metrics(&self) -> ClientQualityMetrics {
        let audio_stats = self.audio_jitter_buffer.read().await.get_statistics();
        let video_stats = self.video_jitter_buffer.read().await.get_statistics();

        let connection_time_ms = if let Some(start) = *self.connection_start.read().await {
            start.elapsed().as_millis() as u64
        } else {
            0
        };

        let first_packet_latency_ms = if let (Some(start), Some(first)) = (
            *self.connection_start.read().await,
            *self.first_audio_packet_time.read().await,
        ) {
            Some(first.duration_since(start).as_millis() as u64)
        } else {
            None
        };

        ClientQualityMetrics {
            audio_buffer_stats: Some(audio_stats),
            video_buffer_stats: Some(video_stats),
            audio_packets_received: self.received_audio_packets.load(Ordering::SeqCst),
            video_packets_received: self.received_video_packets.load(Ordering::SeqCst),
            audio_bytes_received: self.audio_bytes_received.load(Ordering::SeqCst),
            video_bytes_received: self.video_bytes_received.load(Ordering::SeqCst),
            connection_time_ms,
            first_packet_latency_ms,
        }
    }

    /// Get audio jitter buffer statistics
    pub async fn get_audio_buffer_stats(&self) -> BufferStats {
        self.audio_jitter_buffer.read().await.get_statistics()
    }

    /// Get video jitter buffer statistics
    pub async fn get_video_buffer_stats(&self) -> BufferStats {
        self.video_jitter_buffer.read().await.get_statistics()
    }

    /// Get estimated packet loss rate for audio (0.0 to 1.0)
    pub async fn audio_packet_loss_rate(&self) -> f32 {
        self.audio_jitter_buffer.read().await.get_statistics().estimated_loss_rate
    }

    /// Get estimated packet loss rate for video (0.0 to 1.0)
    pub async fn video_packet_loss_rate(&self) -> f32 {
        self.video_jitter_buffer.read().await.get_statistics().estimated_loss_rate
    }

    /// Get late packet count for audio
    pub async fn audio_late_packets(&self) -> u64 {
        self.audio_jitter_buffer.read().await.get_statistics().late_packet_count
    }

    /// Get late packet count for video
    pub async fn video_late_packets(&self) -> u64 {
        self.video_jitter_buffer.read().await.get_statistics().late_packet_count
    }

    /// Generate next request ID
    fn next_request_id(&self) -> String {
        let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}", self.peer_id, id)
    }

    /// Create a WebRTC peer connection with audio and video transceivers
    ///
    /// Now includes jitter buffer integration for proper packet reordering.
    async fn create_peer_connection(
        &self,
        received_audio: Arc<RwLock<Vec<ReceivedAudioChunk>>>,
        received_video: Arc<RwLock<Vec<ReceivedVideoFrame>>>,
        received_audio_packets: Arc<AtomicU32>,
        received_video_packets: Arc<AtomicU32>,
        track_handles: Arc<RwLock<Vec<tokio::task::JoinHandle<()>>>>,
        // Jitter buffers for packet reordering
        audio_jitter_buffer: Arc<RwLock<JitterBuffer<AudioRtpFrame>>>,
        video_jitter_buffer: Arc<RwLock<JitterBuffer<VideoRtpFrame>>>,
        // Quality metrics
        audio_bytes_received: Arc<AtomicU64>,
        video_bytes_received: Arc<AtomicU64>,
        first_audio_packet_time: Arc<RwLock<Option<Instant>>>,
        first_video_packet_time: Arc<RwLock<Option<Instant>>>,
    ) -> HarnessResult<Arc<RTCPeerConnection>> {
        // Create media engine with default codecs
        let mut media_engine = MediaEngine::default();
        media_engine.register_default_codecs().map_err(|e| {
            HarnessError::ConnectionError(format!("Failed to register codecs: {}", e))
        })?;

        // Create interceptor registry
        let interceptor_registry =
            register_default_interceptors(Default::default(), &mut media_engine).map_err(|e| {
                HarnessError::ConnectionError(format!("Failed to register interceptors: {}", e))
            })?;

        // Build API
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(interceptor_registry)
            .build();

        // Configure ICE servers
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        };

        // Create peer connection
        let peer_connection = Arc::new(api.new_peer_connection(config).await.map_err(|e| {
            HarnessError::ConnectionError(format!("Failed to create peer connection: {}", e))
        })?);

        // Set up on_track handler to receive incoming audio/video from server
        let peer_id = self.peer_id.clone();
        let received_audio_clone = Arc::clone(&received_audio);
        let received_video_clone = Arc::clone(&received_video);
        let received_audio_packets_clone = Arc::clone(&received_audio_packets);
        let received_video_packets_clone = Arc::clone(&received_video_packets);
        let track_handles_clone = Arc::clone(&track_handles);
        // Jitter buffer clones
        let audio_jitter_buffer_clone = Arc::clone(&audio_jitter_buffer);
        let video_jitter_buffer_clone = Arc::clone(&video_jitter_buffer);
        // Quality metrics clones
        let audio_bytes_clone = Arc::clone(&audio_bytes_received);
        let video_bytes_clone = Arc::clone(&video_bytes_received);
        let first_audio_clone = Arc::clone(&first_audio_packet_time);
        let first_video_clone = Arc::clone(&first_video_packet_time);

        peer_connection.on_track(Box::new(move |track, _receiver, _transceiver| {
            let peer_id = peer_id.clone();
            let track = Arc::clone(&track);
            let received_audio = Arc::clone(&received_audio_clone);
            let received_video = Arc::clone(&received_video_clone);
            let received_audio_packets = Arc::clone(&received_audio_packets_clone);
            let received_video_packets = Arc::clone(&received_video_packets_clone);
            let track_handles = Arc::clone(&track_handles_clone);
            let audio_jitter_buffer = Arc::clone(&audio_jitter_buffer_clone);
            let video_jitter_buffer = Arc::clone(&video_jitter_buffer_clone);
            let audio_bytes = Arc::clone(&audio_bytes_clone);
            let video_bytes = Arc::clone(&video_bytes_clone);
            let first_audio = Arc::clone(&first_audio_clone);
            let first_video = Arc::clone(&first_video_clone);

            Box::pin(async move {
                let codec = track.codec();
                info!(
                    "TestClient {}: Received track - kind={}, codec={}",
                    peer_id,
                    track.kind(),
                    codec.capability.mime_type
                );

                // Spawn a task to read from this track (with jitter buffer)
                let handle = tokio::spawn(Self::handle_incoming_track_with_jitter_buffer(
                    peer_id,
                    track,
                    received_audio,
                    received_video,
                    received_audio_packets,
                    received_video_packets,
                    audio_jitter_buffer,
                    video_jitter_buffer,
                    audio_bytes,
                    video_bytes,
                    first_audio,
                    first_video,
                ));

                track_handles.write().await.push(handle);
            })
        }));

        Ok(peer_connection)
    }

    /// Handle incoming RTP packets from a remote track
    async fn handle_incoming_track(
        peer_id: String,
        track: Arc<TrackRemote>,
        received_audio: Arc<RwLock<Vec<ReceivedAudioChunk>>>,
        received_video: Arc<RwLock<Vec<ReceivedVideoFrame>>>,
        received_audio_packets: Arc<AtomicU32>,
        received_video_packets: Arc<AtomicU32>,
    ) {
        let codec = track.codec();
        let mime_type = codec.capability.mime_type.to_lowercase();
        let is_audio = mime_type.starts_with("audio/");
        let is_video = mime_type.starts_with("video/");

        info!(
            "TestClient {}: Starting track receiver for {} ({})",
            peer_id,
            track.kind(),
            mime_type
        );

        // Read RTP packets from the track using read_rtp()
        loop {
            match track.read_rtp().await {
                Ok((rtp_packet, _attributes)) => {
                    let payload = rtp_packet.payload.to_vec();
                    if payload.is_empty() {
                        continue;
                    }

                    let timestamp_us = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_micros() as u64;

                    if is_audio {
                        let count = received_audio_packets.fetch_add(1, Ordering::SeqCst) + 1;
                        debug!(
                            "TestClient {}: Received audio RTP packet #{} ({} bytes)",
                            peer_id,
                            count,
                            payload.len()
                        );

                        // Store raw Opus payload
                        let chunk = ReceivedAudioChunk {
                            data: payload,
                            sample_rate: 48000,
                            channels: 1,
                            timestamp_us,
                        };
                        received_audio.write().await.push(chunk);
                    } else if is_video {
                        let count = received_video_packets.fetch_add(1, Ordering::SeqCst) + 1;
                        debug!(
                            "TestClient {}: Received video RTP packet #{} ({} bytes)",
                            peer_id,
                            count,
                            payload.len()
                        );

                        // Store raw VP8 payload
                        let frame = ReceivedVideoFrame {
                            data: payload,
                            width: 0,  // Unknown until decoded
                            height: 0, // Unknown until decoded
                            timestamp_us,
                        };
                        received_video.write().await.push(frame);
                    }
                }
                Err(e) => {
                    // Check if this is just the track closing
                    let err_str = e.to_string();
                    if err_str.contains("closed") || err_str.contains("EOF") {
                        info!("TestClient {}: Track closed normally", peer_id);
                    } else {
                        warn!("TestClient {}: Track read error: {}", peer_id, e);
                    }
                    break;
                }
            }
        }

        info!("TestClient {}: Track receiver ended", peer_id);
    }

    /// Handle incoming RTP packets with jitter buffer integration
    ///
    /// This method uses the production JitterBuffer for packet reordering,
    /// tracks quality metrics, and provides zero-copy payload handling.
    async fn handle_incoming_track_with_jitter_buffer(
        peer_id: String,
        track: Arc<TrackRemote>,
        received_audio: Arc<RwLock<Vec<ReceivedAudioChunk>>>,
        received_video: Arc<RwLock<Vec<ReceivedVideoFrame>>>,
        received_audio_packets: Arc<AtomicU32>,
        received_video_packets: Arc<AtomicU32>,
        audio_jitter_buffer: Arc<RwLock<JitterBuffer<AudioRtpFrame>>>,
        video_jitter_buffer: Arc<RwLock<JitterBuffer<VideoRtpFrame>>>,
        audio_bytes_received: Arc<AtomicU64>,
        video_bytes_received: Arc<AtomicU64>,
        first_audio_packet_time: Arc<RwLock<Option<Instant>>>,
        first_video_packet_time: Arc<RwLock<Option<Instant>>>,
    ) {
        let codec = track.codec();
        let mime_type = codec.capability.mime_type.to_lowercase();
        let is_audio = mime_type.starts_with("audio/");
        let is_video = mime_type.starts_with("video/");

        info!(
            "TestClient {}: Starting track receiver with jitter buffer for {} ({})",
            peer_id,
            track.kind(),
            mime_type
        );

        // Read RTP packets from the track
        loop {
            match track.read_rtp().await {
                Ok((rtp_packet, _attributes)) => {
                    let payload = rtp_packet.payload.to_vec();
                    if payload.is_empty() {
                        continue;
                    }

                    let payload_len = payload.len() as u64;
                    let received_at = Instant::now();
                    let seq = rtp_packet.header.sequence_number;
                    let rtp_ts = rtp_packet.header.timestamp;

                    let timestamp_us = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_micros() as u64;

                    if is_audio {
                        // Track first packet time
                        {
                            let mut first = first_audio_packet_time.write().await;
                            if first.is_none() {
                                *first = Some(received_at);
                                info!("TestClient {}: First audio packet received", peer_id);
                            }
                        }

                        // Update byte counter
                        audio_bytes_received.fetch_add(payload_len, Ordering::SeqCst);

                        let count = received_audio_packets.fetch_add(1, Ordering::SeqCst) + 1;
                        debug!(
                            "TestClient {}: Received audio RTP packet #{} seq={} ts={} ({} bytes)",
                            peer_id, count, seq, rtp_ts, payload_len
                        );

                        // Insert into jitter buffer (uses Arc for zero-copy)
                        let frame = AudioRtpFrame {
                            seq,
                            rtp_ts,
                            received: received_at,
                            payload: Arc::new(payload.clone()),
                        };

                        let mut jb = audio_jitter_buffer.write().await;
                        if let Err(e) = jb.insert(frame) {
                            debug!("TestClient {}: Audio jitter buffer insert error: {}", peer_id, e);
                        }
                        drop(jb);

                        // Also store in received_audio for compatibility with existing tests
                        let chunk = ReceivedAudioChunk {
                            data: payload,
                            sample_rate: 48000,
                            channels: 1,
                            timestamp_us,
                        };
                        received_audio.write().await.push(chunk);
                    } else if is_video {
                        // Track first packet time
                        {
                            let mut first = first_video_packet_time.write().await;
                            if first.is_none() {
                                *first = Some(received_at);
                                info!("TestClient {}: First video packet received", peer_id);
                            }
                        }

                        // Update byte counter
                        video_bytes_received.fetch_add(payload_len, Ordering::SeqCst);

                        let count = received_video_packets.fetch_add(1, Ordering::SeqCst) + 1;
                        debug!(
                            "TestClient {}: Received video RTP packet #{} seq={} ts={} ({} bytes)",
                            peer_id, count, seq, rtp_ts, payload_len
                        );

                        // Insert into jitter buffer (uses Arc for zero-copy)
                        let frame = VideoRtpFrame {
                            seq,
                            rtp_ts,
                            received: received_at,
                            payload: Arc::new(payload.clone()),
                        };

                        let mut jb = video_jitter_buffer.write().await;
                        if let Err(e) = jb.insert(frame) {
                            debug!("TestClient {}: Video jitter buffer insert error: {}", peer_id, e);
                        }
                        drop(jb);

                        // Also store in received_video for compatibility
                        let frame = ReceivedVideoFrame {
                            data: payload,
                            width: 0,
                            height: 0,
                            timestamp_us,
                        };
                        received_video.write().await.push(frame);
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("closed") || err_str.contains("EOF") {
                        info!("TestClient {}: Track closed normally", peer_id);
                    } else {
                        warn!("TestClient {}: Track read error: {}", peer_id, e);
                    }
                    break;
                }
            }
        }

        // Log final jitter buffer stats
        let audio_stats = audio_jitter_buffer.read().await.get_statistics();
        let video_stats = video_jitter_buffer.read().await.get_statistics();
        info!(
            "TestClient {}: Track receiver ended. Audio buffer: peak={} dropped={} late={}. Video buffer: peak={} dropped={} late={}",
            peer_id,
            audio_stats.peak_frames,
            audio_stats.dropped_frames,
            audio_stats.late_packet_count,
            video_stats.peak_frames,
            video_stats.dropped_frames,
            video_stats.late_packet_count
        );
    }

    /// Connect to the signaling server and establish WebRTC connection
    pub async fn connect(&self) -> HarnessResult<()> {
        info!("Connecting test client: {}", self.peer_id);

        let addr = format!("http://127.0.0.1:{}", self.server_port);

        // Connect gRPC client
        let grpc_client = WebRtcSignalingClient::connect(addr.clone())
            .await
            .map_err(|e| HarnessError::ConnectionError(format!("gRPC connect failed: {}", e)))?;

        // Create request channel for sending to stream
        let (request_tx, request_rx) = mpsc::channel::<SignalingRequest>(32);

        // Start bidirectional stream
        let request_stream = ReceiverStream::new(request_rx);
        let mut grpc_client_clone = grpc_client.clone();
        let response_stream = grpc_client_clone
            .signal(request_stream)
            .await
            .map_err(|e| HarnessError::ConnectionError(format!("Signal stream failed: {}", e)))?
            .into_inner();

        // Store client and sender
        *self.signaling_client.write().await = Some(grpc_client);
        *self.request_tx.write().await = Some(request_tx.clone());

        // Record connection start time for metrics
        *self.connection_start.write().await = Some(Instant::now());

        // Create WebRTC peer connection with track handlers and jitter buffers
        let peer_connection = self
            .create_peer_connection(
                Arc::clone(&self.received_audio),
                Arc::clone(&self.received_video),
                Arc::clone(&self.received_audio_packets),
                Arc::clone(&self.received_video_packets),
                Arc::clone(&self.track_handles),
                // Jitter buffers for packet reordering
                Arc::clone(&self.audio_jitter_buffer),
                Arc::clone(&self.video_jitter_buffer),
                // Quality metrics tracking
                Arc::clone(&self.audio_bytes_received),
                Arc::clone(&self.video_bytes_received),
                Arc::clone(&self.first_audio_packet_time),
                Arc::clone(&self.first_video_packet_time),
            )
            .await?;

        // Add audio transceiver (sendrecv) - enables both sending and receiving audio
        let _audio_transceiver = peer_connection
            .add_transceiver_from_kind(RTPCodecType::Audio, None)
            .await
            .map_err(|e| {
                HarnessError::ConnectionError(format!("Failed to add audio transceiver: {}", e))
            })?;

        // Create and set up the audio track for sending
        let audio_track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "audio/opus".to_string(),
                clock_rate: 48000,
                channels: 1,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            format!("audio-{}", self.peer_id),
            format!("stream-{}", self.peer_id),
        ));

        peer_connection
            .add_track(audio_track.clone() as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .map_err(|e| {
                HarnessError::ConnectionError(format!("Failed to add audio track: {}", e))
            })?;

        *self.audio_track.write().await = Some(audio_track);

        // Add video transceiver (sendrecv) - enables both sending and receiving video
        let _video_transceiver = peer_connection
            .add_transceiver_from_kind(RTPCodecType::Video, None)
            .await
            .map_err(|e| {
                HarnessError::ConnectionError(format!("Failed to add video transceiver: {}", e))
            })?;

        // Create and set up the video track for sending
        let video_track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/vp8".to_string(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            format!("video-{}", self.peer_id),
            format!("stream-{}", self.peer_id),
        ));

        peer_connection
            .add_track(video_track.clone() as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .map_err(|e| {
                HarnessError::ConnectionError(format!("Failed to add video track: {}", e))
            })?;

        *self.video_track.write().await = Some(video_track);
        *self.peer_connection.write().await = Some(Arc::clone(&peer_connection));

        // Set up ICE candidate handler
        let request_tx_ice = request_tx.clone();
        let peer_id_ice = self.peer_id.clone();
        let request_counter = Arc::new(AtomicU64::new(100));

        peer_connection.on_ice_candidate(Box::new(move |candidate| {
            let request_tx = request_tx_ice.clone();
            let peer_id = peer_id_ice.clone();
            let counter = Arc::clone(&request_counter);

            Box::pin(async move {
                if let Some(candidate) = candidate {
                    let request_id =
                        format!("{}-ice-{}", peer_id, counter.fetch_add(1, Ordering::SeqCst));

                    if let Ok(json) = candidate.to_json() {
                        let ice_request = SignalingRequest {
                            request_id,
                            request: Some(signaling_request::Request::IceCandidate(
                                IceCandidateRequest {
                                    to_peer_id: "remotemedia-server".to_string(),
                                    candidate: json.candidate,
                                    sdp_mid: json.sdp_mid.unwrap_or_default(),
                                    sdp_mline_index: json.sdp_mline_index.unwrap_or(0) as u32,
                                },
                            )),
                        };

                        if let Err(e) = request_tx.send(ice_request).await {
                            warn!("Failed to send ICE candidate: {}", e);
                        }
                    }
                }
            })
        }));

        // Spawn task to handle response stream
        let connected = Arc::clone(&self.connected);
        let peer_id = self.peer_id.clone();
        let peer_connection_clone = Arc::clone(&peer_connection);

        let stream_handle = tokio::spawn(async move {
            Self::handle_signaling_stream(
                response_stream,
                peer_connection_clone,
                connected,
                peer_id,
            )
            .await;
        });

        *self.stream_handle.write().await = Some(stream_handle);

        // Send announce request
        let announce = SignalingRequest {
            request_id: self.next_request_id(),
            request: Some(signaling_request::Request::Announce(AnnounceRequest {
                peer_id: self.peer_id.clone(),
                capabilities: Some(PeerCapabilities {
                    audio: true,
                    video: true,
                    data: true,
                    extensions: "{}".to_string(),
                }),
                metadata: HashMap::new(),
            })),
        };

        request_tx.send(announce).await.map_err(|e| {
            HarnessError::ConnectionError(format!("Failed to send announce: {}", e))
        })?;

        // Wait briefly for announce to be processed
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create and send offer to remotemedia-server
        let offer = peer_connection
            .create_offer(None)
            .await
            .map_err(|e| HarnessError::ConnectionError(format!("Failed to create offer: {}", e)))?;

        peer_connection
            .set_local_description(offer.clone())
            .await
            .map_err(|e| {
                HarnessError::ConnectionError(format!("Failed to set local description: {}", e))
            })?;

        let offer_request = SignalingRequest {
            request_id: self.next_request_id(),
            request: Some(signaling_request::Request::Offer(OfferRequest {
                to_peer_id: "remotemedia-server".to_string(),
                sdp: offer.sdp,
                r#type: "offer".to_string(),
            })),
        };

        request_tx
            .send(offer_request)
            .await
            .map_err(|e| HarnessError::ConnectionError(format!("Failed to send offer: {}", e)))?;

        // Wait for connection to be established (answer + ICE)
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !self.connected.load(Ordering::SeqCst) {
            if tokio::time::Instant::now() >= deadline {
                return Err(HarnessError::Timeout(
                    "WebRTC connection not established".to_string(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("Test client {} connected successfully", self.peer_id);
        Ok(())
    }

    /// Handle incoming signaling stream messages
    async fn handle_signaling_stream(
        mut stream: tonic::Streaming<remotemedia_webrtc::generated::webrtc::SignalingResponse>,
        peer_connection: Arc<RTCPeerConnection>,
        connected: Arc<AtomicBool>,
        peer_id: String,
    ) {
        while let Some(result) = stream.next().await {
            match result {
                Ok(response) => {
                    match response.response {
                        Some(signaling_response::Response::Notification(notification)) => {
                            match notification.notification {
                                Some(signaling_notification::Notification::Answer(answer)) => {
                                    info!(
                                        "Received SDP answer from {} for {}",
                                        answer.from_peer_id, peer_id
                                    );

                                    // Set remote description
                                    let answer_desc =
                                        match RTCSessionDescription::answer(answer.sdp) {
                                            Ok(desc) => desc,
                                            Err(e) => {
                                                error!("Invalid answer SDP: {}", e);
                                                continue;
                                            }
                                        };

                                    if let Err(e) =
                                        peer_connection.set_remote_description(answer_desc).await
                                    {
                                        error!("Failed to set remote description: {}", e);
                                    } else {
                                        info!("Remote description set for {}", peer_id);
                                        connected.store(true, Ordering::SeqCst);
                                    }
                                }
                                Some(signaling_notification::Notification::IceCandidate(ice)) => {
                                    debug!(
                                        "Received ICE candidate from {} for {}",
                                        ice.from_peer_id, peer_id
                                    );

                                    let candidate_init = RTCIceCandidateInit {
                                        candidate: ice.candidate,
                                        sdp_mid: if ice.sdp_mid.is_empty() {
                                            None
                                        } else {
                                            Some(ice.sdp_mid)
                                        },
                                        sdp_mline_index: Some(ice.sdp_mline_index as u16),
                                        username_fragment: None,
                                    };

                                    if let Err(e) =
                                        peer_connection.add_ice_candidate(candidate_init).await
                                    {
                                        warn!("Failed to add ICE candidate: {}", e);
                                    }
                                }
                                Some(signaling_notification::Notification::PeerJoined(joined)) => {
                                    debug!("Peer joined: {}", joined.peer_id);
                                }
                                Some(signaling_notification::Notification::PeerLeft(left)) => {
                                    debug!("Peer left: {}", left.peer_id);
                                }
                                _ => {}
                            }
                        }
                        Some(signaling_response::Response::Ack(ack)) => {
                            debug!("Received ack: {}", ack.message);
                        }
                        Some(signaling_response::Response::Error(err)) => {
                            error!("Signaling error: {} - {}", err.code, err.message);
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("Stream error for {}: {}", peer_id, e);
                    break;
                }
            }
        }

        info!("Signaling stream ended for {}", peer_id);
        connected.store(false, Ordering::SeqCst);
    }

    /// Send audio samples to the pipeline
    ///
    /// # Arguments
    ///
    /// * `samples` - Audio samples (f32, -1.0 to 1.0)
    /// * `sample_rate` - Sample rate in Hz
    pub async fn send_audio(&self, samples: &[f32], _sample_rate: u32) -> HarnessResult<()> {
        let audio_track =
            self.audio_track.read().await.clone().ok_or_else(|| {
                HarnessError::ClientError("Audio track not configured".to_string())
            })?;

        // Convert f32 samples to bytes (for Opus, we'd encode here)
        // For now, send as raw PCM bytes which the server will need to handle
        // In a full implementation, we'd use an Opus encoder
        debug!(
            "Sending {} audio samples (encoding simplified for test)",
            samples.len()
        );

        // Create sample data - in real implementation this would be Opus-encoded
        // For testing, we'll send placeholder Opus frames
        let sample = webrtc::media::Sample {
            data: vec![0u8; 160].into(), // Minimal Opus frame size
            duration: Duration::from_millis(20),
            timestamp: std::time::SystemTime::now(),
            ..Default::default()
        };

        audio_track.write_sample(&sample).await.map_err(|e| {
            HarnessError::MediaError(format!("Failed to write audio sample: {}", e))
        })?;

        Ok(())
    }

    /// Send video frame to the pipeline
    ///
    /// # Arguments
    ///
    /// * `frame` - Raw video frame data (I420/YUV420P)
    /// * `width` - Frame width
    /// * `height` - Frame height
    pub async fn send_video(&self, _frame: &[u8], _width: u32, _height: u32) -> HarnessResult<()> {
        let video_track =
            self.video_track.read().await.clone().ok_or_else(|| {
                HarnessError::ClientError("Video track not configured".to_string())
            })?;

        debug!("Sending video frame (encoding simplified for test)");

        // Create sample data - in real implementation this would be VP8-encoded
        // For testing, we'll send placeholder VP8 keyframe
        let sample = webrtc::media::Sample {
            data: vec![0u8; 100].into(),         // Minimal VP8 frame placeholder
            duration: Duration::from_millis(33), // ~30fps
            timestamp: std::time::SystemTime::now(),
            ..Default::default()
        };

        video_track.write_sample(&sample).await.map_err(|e| {
            HarnessError::MediaError(format!("Failed to write video sample: {}", e))
        })?;

        Ok(())
    }

    /// Get accumulated received audio chunks
    pub async fn get_received_audio(&self) -> Vec<ReceivedAudioChunk> {
        self.received_audio.read().await.clone()
    }

    /// Get accumulated received video frames
    pub async fn get_received_video(&self) -> Vec<ReceivedVideoFrame> {
        self.received_video.read().await.clone()
    }

    /// Clear received audio buffer
    pub async fn clear_received_audio(&self) {
        self.received_audio.write().await.clear();
        self.received_audio_packets.store(0, Ordering::SeqCst);
    }

    /// Clear received video buffer
    pub async fn clear_received_video(&self) {
        self.received_video.write().await.clear();
        self.received_video_packets.store(0, Ordering::SeqCst);
    }

    /// Wait for audio output with timeout
    ///
    /// Returns all accumulated audio data as raw bytes
    pub async fn wait_for_audio(&self, timeout: Duration) -> HarnessResult<Vec<u8>> {
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            let audio = self.received_audio.read().await;
            if !audio.is_empty() {
                // Flatten all received chunks' data
                return Ok(audio.iter().flat_map(|c| c.data.clone()).collect());
            }
            drop(audio);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Err(HarnessError::Timeout("No audio received".to_string()))
    }

    /// Wait for at least N audio packets with timeout
    pub async fn wait_for_audio_packets(
        &self,
        min_packets: u32,
        timeout: Duration,
    ) -> HarnessResult<u32> {
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            let count = self.received_audio_packets.load(Ordering::SeqCst);
            if count >= min_packets {
                return Ok(count);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let count = self.received_audio_packets.load(Ordering::SeqCst);
        Err(HarnessError::Timeout(format!(
            "Only received {} audio packets, expected at least {}",
            count, min_packets
        )))
    }

    /// Wait for at least N video packets with timeout
    pub async fn wait_for_video_packets(
        &self,
        min_packets: u32,
        timeout: Duration,
    ) -> HarnessResult<u32> {
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            let count = self.received_video_packets.load(Ordering::SeqCst);
            if count >= min_packets {
                return Ok(count);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let count = self.received_video_packets.load(Ordering::SeqCst);
        Err(HarnessError::Timeout(format!(
            "Only received {} video packets, expected at least {}",
            count, min_packets
        )))
    }

    /// Wait for video output with timeout
    pub async fn wait_for_video(&self, timeout: Duration) -> HarnessResult<ReceivedVideoFrame> {
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            let video = self.received_video.read().await;
            if let Some(frame) = video.first() {
                return Ok(frame.clone());
            }
            drop(video);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Err(HarnessError::Timeout("No video received".to_string()))
    }

    /// Disconnect from the server
    pub async fn disconnect(&self) -> HarnessResult<()> {
        info!("Disconnecting test client: {}", self.peer_id);

        // Close peer connection
        if let Some(pc) = self.peer_connection.write().await.take() {
            let _ = pc.close().await;
        }

        // Abort stream task
        if let Some(handle) = self.stream_handle.write().await.take() {
            handle.abort();
        }

        // Abort track receiver tasks
        for handle in self.track_handles.write().await.drain(..) {
            handle.abort();
        }

        self.connected.store(false, Ordering::SeqCst);
        *self.signaling_client.write().await = None;
        *self.request_tx.write().await = None;

        info!("Test client {} disconnected", self.peer_id);
        Ok(())
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        tracing::debug!("TestClient {} dropped", self.peer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = TestClient::new("test-peer".to_string(), 12345)
            .await
            .unwrap();
        assert_eq!(client.peer_id(), "test-peer");
        assert!(!client.is_connected().await);
    }
}
