//! Configuration types for WebRTC transport

use serde::{Deserialize, Serialize};

/// Main configuration for WebRtcTransport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcTransportConfig {
    /// WebSocket signaling server URL (ws:// or wss://)
    pub signaling_url: String,

    /// STUN server URLs (at least one required)
    pub stun_servers: Vec<String>,

    /// TURN server configurations (optional)
    pub turn_servers: Vec<TurnServerConfig>,

    /// Local peer ID (auto-generated if None)
    pub peer_id: Option<String>,

    /// Maximum peers in mesh (default: 10, max: 10)
    pub max_peers: u32,

    /// Audio codec preference (default: Opus)
    pub audio_codec: AudioCodec,

    /// Video codec preference (default: VP9)
    pub video_codec: VideoCodec,

    /// Enable data channel (default: true)
    pub enable_data_channel: bool,

    /// Data channel mode (default: Reliable)
    pub data_channel_mode: DataChannelMode,

    /// Jitter buffer size in milliseconds (default: 50ms, range: 50-200ms)
    pub jitter_buffer_size_ms: u32,

    /// Enable RTCP Sender Reports (default: true, required for A/V sync)
    pub enable_rtcp: bool,

    /// RTCP report interval in milliseconds (default: 5000ms)
    pub rtcp_interval_ms: u32,

    /// Additional configuration options
    pub options: ConfigOptions,
}

/// TURN server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnServerConfig {
    /// TURN server URL (turn:// or turns://)
    pub url: String,

    /// Username for TURN authentication
    pub username: String,

    /// Credential for TURN authentication
    pub credential: String,
}

/// Additional configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOptions {
    /// Enable adaptive bitrate adjustment (default: true)
    pub adaptive_bitrate_enabled: bool,

    /// Target bitrate in kbps (default: 2000)
    pub target_bitrate_kbps: u32,

    /// Maximum video resolution (default: 720p)
    pub max_video_resolution: VideoResolution,

    /// Video framerate in fps (default: 30)
    pub video_framerate_fps: u32,

    /// ICE connection timeout in seconds (default: 30)
    pub ice_timeout_secs: u32,
}

/// Supported audio codecs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioCodec {
    /// Opus codec (default, required for WebRTC)
    Opus,
}

/// Supported video codecs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoCodec {
    /// VP9 codec (default, recommended)
    VP9,
    /// H.264 codec (fallback for compatibility)
    H264,
}

/// Video resolution options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoResolution {
    /// 640x480
    #[serde(rename = "480p")]
    P480,
    /// 1280x720
    #[serde(rename = "720p")]
    P720,
    /// 1920x1080
    #[serde(rename = "1080p")]
    P1080,
}

/// Data channel mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataChannelMode {
    /// Reliable, ordered delivery (default)
    Reliable,
    /// Unreliable, unordered delivery (low latency)
    Unreliable,
}

impl Default for WebRtcTransportConfig {
    fn default() -> Self {
        Self {
            signaling_url: "ws://localhost:8080".to_string(),
            stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
            turn_servers: Vec::new(),
            peer_id: None,
            max_peers: 10,
            audio_codec: AudioCodec::Opus,
            video_codec: VideoCodec::VP9,
            enable_data_channel: true,
            data_channel_mode: DataChannelMode::Reliable,
            jitter_buffer_size_ms: 50,
            enable_rtcp: true,
            rtcp_interval_ms: 5000,
            options: ConfigOptions::default(),
        }
    }
}

impl Default for ConfigOptions {
    fn default() -> Self {
        Self {
            adaptive_bitrate_enabled: true,
            target_bitrate_kbps: 2000,
            max_video_resolution: VideoResolution::P720,
            video_framerate_fps: 30,
            ice_timeout_secs: 30,
        }
    }
}

impl WebRtcTransportConfig {
    /// Validate configuration parameters
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `stun_servers` is empty
    /// - `max_peers` is not in range 1-10
    /// - `jitter_buffer_size_ms` is not in range 50-200
    /// - `rtcp_interval_ms` is not in range 1000-10000
    /// - `signaling_url` is not a valid WebSocket URL
    pub fn validate(&self) -> crate::Result<()> {
        use crate::Error;

        // Validate STUN servers
        if self.stun_servers.is_empty() {
            return Err(Error::InvalidConfig(
                "At least one STUN server is required".to_string(),
            ));
        }

        // Validate max peers
        if self.max_peers == 0 || self.max_peers > 10 {
            return Err(Error::InvalidConfig(format!(
                "max_peers must be in range 1-10, got {}",
                self.max_peers
            )));
        }

        // Validate jitter buffer size
        if self.jitter_buffer_size_ms < 50 || self.jitter_buffer_size_ms > 200 {
            return Err(Error::InvalidConfig(format!(
                "jitter_buffer_size_ms must be in range 50-200, got {}",
                self.jitter_buffer_size_ms
            )));
        }

        // Validate RTCP interval
        if self.rtcp_interval_ms < 1000 || self.rtcp_interval_ms > 10000 {
            return Err(Error::InvalidConfig(format!(
                "rtcp_interval_ms must be in range 1000-10000, got {}",
                self.rtcp_interval_ms
            )));
        }

        // Validate signaling URL
        if !self.signaling_url.starts_with("ws://") && !self.signaling_url.starts_with("wss://") {
            return Err(Error::InvalidConfig(format!(
                "signaling_url must start with ws:// or wss://, got {}",
                self.signaling_url
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = WebRtcTransportConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_empty_stun_servers_fails() {
        let mut config = WebRtcTransportConfig::default();
        config.stun_servers.clear();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_max_peers_fails() {
        let mut config = WebRtcTransportConfig::default();
        config.max_peers = 0;
        assert!(config.validate().is_err());

        config.max_peers = 11;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_jitter_buffer_size_fails() {
        let mut config = WebRtcTransportConfig::default();
        config.jitter_buffer_size_ms = 49;
        assert!(config.validate().is_err());

        config.jitter_buffer_size_ms = 201;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_signaling_url_fails() {
        let mut config = WebRtcTransportConfig::default();
        config.signaling_url = "http://localhost:8080".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_serialization() {
        let config = WebRtcTransportConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: WebRtcTransportConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.signaling_url, deserialized.signaling_url);
    }
}
