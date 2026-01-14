//! SRT Listener module
//!
//! Accepts incoming SRT connections from FFmpeg and other clients.
//! Uses srt-tokio for native SRT support with streamid-based session routing.

use std::sync::Arc;

use futures::StreamExt;
use srt_tokio::SrtListener;
use tokio::sync::broadcast;

use remotemedia_health_analyzer::HealthEvent;
use remotemedia_core::ingestion::AudioConfig;

use crate::jwt::JwtValidator;
use crate::session::{EndReason, SessionManager};
use crate::streamid::StreamIdParams;

/// SRT Listener for accepting inbound SRT connections
///
/// Uses srt-tokio to get streamid from each incoming connection,
/// enabling proper multi-user session routing.
pub struct SrtIngestListener {
    /// Port to listen on
    port: u16,

    /// Session manager for looking up sessions
    session_manager: Arc<SessionManager>,

    /// JWT validator for token verification
    jwt_validator: Arc<JwtValidator>,

    /// Audio configuration for decoding
    audio_config: AudioConfig,

    /// Shutdown signal receiver
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl SrtIngestListener {
    /// Create a new SRT listener
    pub fn new(
        port: u16,
        session_manager: Arc<SessionManager>,
        jwt_validator: Arc<JwtValidator>,
    ) -> Self {
        Self {
            port,
            session_manager,
            jwt_validator,
            audio_config: AudioConfig {
                sample_rate: 16000,
                channels: 1,
            },
            shutdown_tx: None,
        }
    }

    /// Set audio configuration
    pub fn with_audio_config(mut self, config: AudioConfig) -> Self {
        self.audio_config = config;
        self
    }

    /// Set shutdown signal sender
    pub fn with_shutdown(mut self, shutdown_tx: broadcast::Sender<()>) -> Self {
        self.shutdown_tx = Some(shutdown_tx);
        self
    }

    /// Run the SRT listener
    ///
    /// Accepts incoming SRT connections, extracts streamid for session routing,
    /// validates JWT tokens, and processes media data.
    pub async fn run(self) -> Result<(), SrtListenerError> {
        let bind_addr = format!("0.0.0.0:{}", self.port);
        tracing::info!("Starting SRT listener on {}", bind_addr);

        let socket_addr: std::net::SocketAddr = bind_addr
            .parse()
            .map_err(|e| SrtListenerError::Bind(format!("Invalid address: {}", e)))?;

        let (_listener, mut srt_incoming) = SrtListener::builder()
            .bind(socket_addr)
            .await
            .map_err(|e| SrtListenerError::Bind(e.to_string()))?;

        tracing::info!("SRT listener ready, waiting for connections...");

        let incoming = srt_incoming.incoming();
        let mut shutdown_rx = self.shutdown_tx.as_ref().map(|tx| tx.subscribe());

        loop {
            // Check for shutdown
            if let Some(ref mut rx) = shutdown_rx {
                match rx.try_recv() {
                    Ok(()) | Err(broadcast::error::TryRecvError::Closed) => {
                        tracing::info!("SRT listener shutdown requested");
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Empty) |
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                }
            }

            // Accept next connection with timeout
            let incoming_result = tokio::select! {
                result = incoming.next() => result,
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => continue,
            };

            let request = match incoming_result {
                Some(req) => req,
                None => {
                    tracing::info!("Listener closed");
                    break;
                }
            };

            // Extract streamid from the connection request
            let streamid: Option<String> = request.stream_id().map(|s| s.to_string());

            tracing::info!("Incoming SRT connection with streamid: {:?}", streamid);

            // Parse streamid to get session info
            let params = match &streamid {
                Some(sid) => match StreamIdParams::parse(sid) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("Invalid streamid '{}': {}", sid, e);
                        // Reject connection by not accepting
                        continue;
                    }
                },
                None => {
                    tracing::warn!("Connection without streamid, rejecting");
                    continue;
                }
            };

            // Validate JWT token
            if let Err(e) = self.jwt_validator.validate_for_session(&params.token, &params.session_id) {
                tracing::warn!("JWT validation failed for session {}: {}", params.session_id, e);
                continue;
            }

            // Look up session
            let session = match self.session_manager.get_session(&params.session_id).await {
                Some(s) => s,
                None => {
                    tracing::warn!("Session not found: {}", params.session_id);
                    continue;
                }
            };

            tracing::info!(
                "Accepting connection for session {} (pipeline: {})",
                params.session_id,
                params.pipeline
            );

            // Accept the connection
            let socket = match request.accept(None).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to accept connection: {}", e);
                    continue;
                }
            };

            // Spawn handler task for this connection
            let session_clone = session.clone();
            let audio_config = self.audio_config.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(socket, session_clone, audio_config).await {
                    tracing::error!("Connection handler error: {}", e);
                }
            });
        }

        tracing::info!("SRT listener stopped");
        Ok(())
    }
}

/// Connection timeout in seconds (no data received)
const CONNECTION_TIMEOUT_SECS: u64 = 30;

/// Handle a single SRT connection
async fn handle_connection(
    mut socket: srt_tokio::SrtSocket,
    session: Arc<crate::session::IngestSession>,
    _audio_config: AudioConfig,
) -> Result<(), SrtListenerError> {
    let session_id = session.id.clone();
    let span = tracing::info_span!("srt_connection", session_id = %session_id);
    let _guard = span.enter();

    tracing::info!("Handling connection for session {}", session_id);

    // Transition session to connected state
    if let Err(e) = session.set_connected().await {
        tracing::warn!("Failed to set session connected: {}", e);
    }

    let mut first_packet = true;
    let mut packets_received: u64 = 0;
    let mut bytes_received: u64 = 0;
    let mut errors_count: u32 = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 10;

    // Receive MPEG-TS packets from the SRT stream
    loop {
        // Apply timeout for receiving data
        let receive_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(CONNECTION_TIMEOUT_SECS),
            socket.next(),
        )
        .await;

        match receive_result {
            Ok(Some(Ok((_, data)))) => {
                // Reset error counter on successful receive
                errors_count = 0;

                // On first packet, transition to streaming
                if first_packet {
                    if let Err(e) = session.set_streaming().await {
                        tracing::warn!("Failed to set session streaming: {}", e);
                    }
                    first_packet = false;
                    tracing::info!(
                        session_id = %session_id,
                        "Stream started receiving data"
                    );
                }

                // Track statistics
                packets_received += 1;
                bytes_received += data.len() as u64;

                // Validate MPEG-TS packet structure (basic sanity check)
                // MPEG-TS sync byte is 0x47
                if !data.is_empty() && data[0] != 0x47 && data.len() >= 188 {
                    // Not a valid MPEG-TS packet, but continue processing
                    // The demuxer will handle malformed data gracefully
                    tracing::trace!(
                        session_id = %session_id,
                        first_byte = data[0],
                        len = data.len(),
                        "Received non-standard MPEG-TS data"
                    );
                }

                // Send raw MPEG-TS data to session for demuxing
                let bytes: Vec<u8> = data.to_vec();
                if session.input_sender().send(bytes).await.is_err() {
                    tracing::warn!(
                        session_id = %session_id,
                        "Session input channel closed"
                    );
                    break;
                }
            }
            Ok(Some(Err(e))) => {
                errors_count += 1;
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    error_count = errors_count,
                    "SRT receive error"
                );

                // Too many consecutive errors - terminate connection
                if errors_count >= MAX_CONSECUTIVE_ERRORS {
                    tracing::error!(
                        session_id = %session_id,
                        error_count = errors_count,
                        "Too many consecutive errors, terminating connection"
                    );
                    session
                        .end(EndReason::Error(format!("Too many errors: {}", e)))
                        .await
                        .ok();
                    return Err(SrtListenerError::Receive(e.to_string()));
                }

                // Continue trying to receive
                continue;
            }
            Ok(None) => {
                // Stream ended normally (socket closed)
                tracing::info!(
                    session_id = %session_id,
                    packets = packets_received,
                    bytes = bytes_received,
                    "SRT stream ended normally"
                );
                break;
            }
            Err(_) => {
                // Timeout - no data received
                tracing::warn!(
                    session_id = %session_id,
                    timeout_secs = CONNECTION_TIMEOUT_SECS,
                    "Connection timeout - no data received"
                );
                session
                    .end(EndReason::Error("Connection timeout".to_string()))
                    .await
                    .ok();
                return Err(SrtListenerError::Connection("Timeout".to_string()));
            }
        }
    }

    // Stream ended normally
    tracing::info!(
        session_id = %session_id,
        packets = packets_received,
        bytes = bytes_received,
        "Connection closed"
    );
    session.end(EndReason::ClientDisconnect).await.ok();

    Ok(())
}

/// Video frame sampler for reducing frame rate under load
///
/// Implements frame dropping to maintain target FPS while prioritizing
/// audio processing. This is configurable per-session.
pub struct VideoFrameSampler {
    /// Target frames per second
    target_fps: f32,

    /// Last frame timestamp in microseconds
    last_frame_ts: Option<u64>,

    /// Minimum interval between frames in microseconds
    min_interval_us: u64,

    /// Total frames received
    frames_received: u64,

    /// Frames passed through
    frames_passed: u64,

    /// Whether to drop frames under load
    drop_under_load: bool,
}

impl VideoFrameSampler {
    /// Create a new video frame sampler with target FPS
    pub fn new(target_fps: f32, drop_under_load: bool) -> Self {
        let min_interval_us = if target_fps > 0.0 {
            (1_000_000.0 / target_fps) as u64
        } else {
            0
        };

        Self {
            target_fps,
            last_frame_ts: None,
            min_interval_us,
            frames_received: 0,
            frames_passed: 0,
            drop_under_load,
        }
    }

    /// Create a sampler with default settings (2 FPS, drop under load)
    pub fn default_sampler() -> Self {
        Self::new(2.0, true)
    }

    /// Check if a video frame should be passed through or dropped
    pub fn should_pass(&mut self, timestamp_us: u64) -> bool {
        self.frames_received += 1;

        match self.last_frame_ts {
            None => {
                self.last_frame_ts = Some(timestamp_us);
                self.frames_passed += 1;
                true
            }
            Some(last_ts) => {
                let elapsed = timestamp_us.saturating_sub(last_ts);

                if elapsed >= self.min_interval_us {
                    self.last_frame_ts = Some(timestamp_us);
                    self.frames_passed += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Check if frame should be dropped due to load
    pub fn should_drop_for_load(&self) -> bool {
        self.drop_under_load
    }

    /// Get the drop rate (percentage of frames dropped)
    pub fn drop_rate(&self) -> f32 {
        if self.frames_received == 0 {
            0.0
        } else {
            1.0 - (self.frames_passed as f32 / self.frames_received as f32)
        }
    }

    /// Get statistics
    pub fn stats(&self) -> VideoSamplerStats {
        VideoSamplerStats {
            target_fps: self.target_fps,
            frames_received: self.frames_received,
            frames_passed: self.frames_passed,
            drop_rate: self.drop_rate(),
        }
    }

    /// Set target FPS (can be changed dynamically)
    pub fn set_target_fps(&mut self, fps: f32) {
        self.target_fps = fps;
        self.min_interval_us = if fps > 0.0 {
            (1_000_000.0 / fps) as u64
        } else {
            0
        };
    }
}

/// Video sampler statistics
#[derive(Debug, Clone)]
pub struct VideoSamplerStats {
    pub target_fps: f32,
    pub frames_received: u64,
    pub frames_passed: u64,
    pub drop_rate: f32,
}

/// SRT connection handler for tracking connection state
pub struct SrtConnection {
    pub session_id: String,
    pub params: StreamIdParams,
    event_tx: broadcast::Sender<HealthEvent>,
}

impl SrtConnection {
    pub fn new(
        session_id: String,
        params: StreamIdParams,
        event_tx: broadcast::Sender<HealthEvent>,
    ) -> Self {
        Self {
            session_id,
            params,
            event_tx,
        }
    }

    pub fn emit_event(&self, event: HealthEvent) {
        let _ = self.event_tx.send(event);
    }
}

/// SRT listener errors
#[derive(Debug, thiserror::Error)]
pub enum SrtListenerError {
    #[error("Failed to bind: {0}")]
    Bind(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Receive error: {0}")]
    Receive(String),

    #[error("No session found for connection")]
    NoSession,

    #[error("Invalid streamid: {0}")]
    InvalidStreamId(String),

    #[error("JWT validation failed: {0}")]
    JwtValidation(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Parse streamid from SRT connection
pub fn parse_streamid(streamid: &str) -> Result<StreamIdParams, SrtListenerError> {
    StreamIdParams::parse(streamid)
        .map_err(|e| SrtListenerError::InvalidStreamId(format!("{}: {}", streamid, e)))
}

/// Build SRT listener URL
pub fn build_listener_url(host: &str, port: u16) -> String {
    format!("srt://{}:{}?mode=listener", host, port)
}

/// Build SRT caller URL (for FFmpeg to push to)
pub fn build_caller_url(host: &str, port: u16, streamid: &str) -> String {
    let encoded = url::form_urlencoded::byte_serialize(streamid.as_bytes()).collect::<String>();
    format!("srt://{}:{}?mode=caller&streamid={}", host, port, encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_listener_url() {
        let url = build_listener_url("0.0.0.0", 9000);
        assert_eq!(url, "srt://0.0.0.0:9000?mode=listener");
    }

    #[test]
    fn test_build_caller_url() {
        let url = build_caller_url("localhost", 9000, "#!::r=sess_123,token=abc");
        assert!(url.contains("mode=caller"));
        assert!(url.contains("streamid="));
    }

    #[test]
    fn test_parse_streamid() {
        let params = parse_streamid("#!::r=sess_123,token=abc,p=demo,audio=1,video=0").unwrap();
        assert_eq!(params.session_id, "sess_123");
        assert_eq!(params.token, "abc");
        assert_eq!(params.pipeline, "demo");
        assert!(params.audio_enabled);
        assert!(!params.video_enabled);
    }

    #[test]
    fn test_parse_streamid_invalid() {
        assert!(parse_streamid("invalid").is_err());
    }

    #[test]
    fn test_video_sampler_first_frame_passes() {
        let mut sampler = VideoFrameSampler::new(2.0, true);
        assert!(sampler.should_pass(0));
        assert_eq!(sampler.frames_received, 1);
        assert_eq!(sampler.frames_passed, 1);
    }

    #[test]
    fn test_video_sampler_rate_limiting() {
        let mut sampler = VideoFrameSampler::new(2.0, true);

        assert!(sampler.should_pass(0));
        assert!(!sampler.should_pass(100_000));
        assert!(!sampler.should_pass(200_000));
        assert!(sampler.should_pass(500_000));

        assert_eq!(sampler.frames_received, 4);
        assert_eq!(sampler.frames_passed, 2);
    }

    #[test]
    fn test_video_sampler_drop_rate() {
        let mut sampler = VideoFrameSampler::new(5.0, true);

        for i in 0..30 {
            sampler.should_pass(i * 33_333);
        }

        assert!(sampler.frames_passed <= 6);
        assert!(sampler.frames_passed >= 4);
    }

    #[test]
    fn test_video_sampler_stats() {
        let mut sampler = VideoFrameSampler::new(1.0, true);

        sampler.should_pass(0);
        sampler.should_pass(500_000);
        sampler.should_pass(1_000_000);

        let stats = sampler.stats();
        assert_eq!(stats.target_fps, 1.0);
        assert_eq!(stats.frames_received, 3);
        assert_eq!(stats.frames_passed, 2);
    }

    #[test]
    fn test_video_sampler_dynamic_fps() {
        let mut sampler = VideoFrameSampler::new(1.0, true);
        sampler.set_target_fps(10.0);

        assert!(sampler.should_pass(0));
        assert!(sampler.should_pass(100_000));
    }
}
