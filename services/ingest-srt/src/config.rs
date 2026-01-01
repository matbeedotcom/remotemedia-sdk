//! Configuration for the SRT Ingest Gateway
//!
//! Configuration can be loaded from a TOML file and/or environment variables.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration for the SRT Ingest Gateway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// JWT configuration
    #[serde(default)]
    pub jwt: JwtConfig,

    /// Session limits
    #[serde(default)]
    pub limits: LimitsConfig,

    /// Webhook configuration
    #[serde(default)]
    pub webhooks: WebhookConfig,

    /// Pipeline configuration
    #[serde(default)]
    pub pipelines: PipelineConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// HTTP API port
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    /// SRT listener port
    #[serde(default = "default_srt_port")]
    pub srt_port: u16,

    /// Host to bind to
    #[serde(default = "default_host")]
    pub host: String,

    /// Public hostname for generating SRT URLs
    #[serde(default = "default_public_host")]
    pub public_host: String,
}

fn default_http_port() -> u16 {
    8080
}

fn default_srt_port() -> u16 {
    9000
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_public_host() -> String {
    "localhost".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            srt_port: default_srt_port(),
            host: default_host(),
            public_host: default_public_host(),
        }
    }
}

/// JWT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// Secret key for signing JWT tokens
    #[serde(default = "default_jwt_secret")]
    pub secret: String,

    /// Token time-to-live in seconds
    #[serde(default = "default_token_ttl")]
    pub token_ttl_seconds: u64,
}

fn default_jwt_secret() -> String {
    // In production, this should be set via environment variable
    "your-secret-key-change-in-production".to_string()
}

fn default_token_ttl() -> u64 {
    900 // 15 minutes
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: default_jwt_secret(),
            token_ttl_seconds: default_token_ttl(),
        }
    }
}

/// Session and resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    /// Maximum number of concurrent sessions
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    /// Maximum session duration in seconds
    #[serde(default = "default_max_duration")]
    pub max_session_duration_seconds: u64,

    /// Maximum bitrate in bits per second
    #[serde(default = "default_max_bitrate")]
    pub max_bitrate_bps: u64,

    /// Default audio sample rate
    #[serde(default = "default_audio_sample_rate")]
    pub default_audio_sample_rate: u32,

    /// Audio queue size in milliseconds
    #[serde(default = "default_audio_queue_ms")]
    pub audio_queue_ms: u64,

    /// Video queue size in frames
    #[serde(default = "default_video_queue_frames")]
    pub video_queue_frames: usize,
}

fn default_max_sessions() -> usize {
    100
}

fn default_max_duration() -> u64 {
    3600 // 1 hour
}

fn default_max_bitrate() -> u64 {
    10_000_000 // 10 Mbps
}

fn default_audio_sample_rate() -> u32 {
    16000
}

fn default_audio_queue_ms() -> u64 {
    500
}

fn default_video_queue_frames() -> usize {
    5
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_sessions: default_max_sessions(),
            max_session_duration_seconds: default_max_duration(),
            max_bitrate_bps: default_max_bitrate(),
            default_audio_sample_rate: default_audio_sample_rate(),
            audio_queue_ms: default_audio_queue_ms(),
            video_queue_frames: default_video_queue_frames(),
        }
    }
}

/// Webhook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Request timeout in seconds
    #[serde(default = "default_webhook_timeout")]
    pub timeout_seconds: u64,

    /// Maximum number of retries
    #[serde(default = "default_webhook_retries")]
    pub max_retries: u32,

    /// Initial retry backoff in milliseconds
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff_ms: u64,
}

fn default_webhook_timeout() -> u64 {
    10
}

fn default_webhook_retries() -> u32 {
    3
}

fn default_retry_backoff() -> u64 {
    1000
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: default_webhook_timeout(),
            max_retries: default_webhook_retries(),
            retry_backoff_ms: default_retry_backoff(),
        }
    }
}

/// Pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Directory containing pipeline templates
    #[serde(default = "default_templates_dir")]
    pub templates_dir: String,
}

fn default_templates_dir() -> String {
    "./pipelines".to_string()
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            templates_dir: default_templates_dir(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            jwt: JwtConfig::default(),
            limits: LimitsConfig::default(),
            webhooks: WebhookConfig::default(),
            pipelines: PipelineConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        let config: Config =
            toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;
        Ok(config)
    }

    /// Load configuration from environment variables, falling back to defaults
    pub fn from_env() -> Self {
        let mut config = Config::default();

        // Server
        if let Ok(port) = std::env::var("INGEST_HTTP_PORT") {
            if let Ok(p) = port.parse() {
                config.server.http_port = p;
            }
        }
        if let Ok(port) = std::env::var("INGEST_SRT_PORT") {
            if let Ok(p) = port.parse() {
                config.server.srt_port = p;
            }
        }
        if let Ok(host) = std::env::var("INGEST_HOST") {
            config.server.host = host;
        }
        if let Ok(host) = std::env::var("INGEST_PUBLIC_HOST") {
            config.server.public_host = host;
        }

        // JWT
        if let Ok(secret) = std::env::var("INGEST_JWT_SECRET") {
            config.jwt.secret = secret;
        }
        if let Ok(ttl) = std::env::var("INGEST_JWT_TTL") {
            if let Ok(t) = ttl.parse() {
                config.jwt.token_ttl_seconds = t;
            }
        }

        // Limits
        if let Ok(max) = std::env::var("INGEST_MAX_SESSIONS") {
            if let Ok(m) = max.parse() {
                config.limits.max_sessions = m;
            }
        }
        if let Ok(dur) = std::env::var("INGEST_MAX_DURATION") {
            if let Ok(d) = dur.parse() {
                config.limits.max_session_duration_seconds = d;
            }
        }

        // Pipelines
        if let Ok(dir) = std::env::var("INGEST_PIPELINES_DIR") {
            config.pipelines.templates_dir = dir;
        }

        config
    }

    /// Load configuration from file if it exists, otherwise from environment
    pub fn load<P: AsRef<Path>>(path: Option<P>) -> Result<Self, ConfigError> {
        if let Some(p) = path {
            if p.as_ref().exists() {
                return Self::from_file(p);
            }
        }
        Ok(Self::from_env())
    }
}

/// Configuration error types
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.http_port, 8080);
        assert_eq!(config.server.srt_port, 9000);
        assert_eq!(config.limits.max_sessions, 100);
        assert_eq!(config.jwt.token_ttl_seconds, 900);
    }

    #[test]
    fn test_config_from_toml() {
        let toml = r#"
[server]
http_port = 9090
srt_port = 9001
public_host = "ingest.example.com"

[jwt]
secret = "test-secret"
token_ttl_seconds = 600

[limits]
max_sessions = 50
"#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.server.http_port, 9090);
        assert_eq!(config.server.srt_port, 9001);
        assert_eq!(config.server.public_host, "ingest.example.com");
        assert_eq!(config.jwt.secret, "test-secret");
        assert_eq!(config.limits.max_sessions, 50);
    }
}
