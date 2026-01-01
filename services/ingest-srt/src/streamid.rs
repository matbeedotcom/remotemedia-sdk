//! SRT streamid parsing and serialization
//!
//! The SRT protocol supports a `streamid` parameter that can carry arbitrary
//! key-value data. This module parses and serializes streamid parameters in
//! the format: `#!::key=value,key=value,...`
//!
//! # Format
//!
//! ```text
//! #!::r=sess_abc123,token=eyJhbGciOiJIUzI1NiJ9...,p=demo_v1,audio=1,video=0
//! ```
//!
//! # Parameters
//!
//! | Key | Required | Description |
//! |-----|----------|-------------|
//! | `r` | Yes | Session ID for routing |
//! | `token` | Yes | JWT authentication token |
//! | `p` | Yes | Pipeline template ID |
//! | `audio` | No | Enable audio analysis (default: 1) |
//! | `video` | No | Enable video analysis (default: 0) |

use serde::{Deserialize, Serialize};

/// Parameters extracted from an SRT streamid
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamIdParams {
    /// Session ID for routing (maps to `r` in streamid)
    pub session_id: String,

    /// JWT authentication token
    pub token: String,

    /// Pipeline template ID
    pub pipeline: String,

    /// Enable audio analysis (default: true)
    #[serde(default = "default_true")]
    pub audio_enabled: bool,

    /// Enable video analysis (default: false)
    #[serde(default)]
    pub video_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl StreamIdParams {
    /// Create a new StreamIdParams
    pub fn new(session_id: String, token: String, pipeline: String) -> Self {
        Self {
            session_id,
            token,
            pipeline,
            audio_enabled: true,
            video_enabled: false,
        }
    }

    /// Parse a streamid string into StreamIdParams
    ///
    /// # Format
    ///
    /// The streamid should be in the format:
    /// `#!::key=value,key=value,...`
    ///
    /// Required keys: `r` (session_id), `token`, `p` (pipeline)
    /// Optional keys: `audio` (0/1), `video` (0/1)
    ///
    /// # Errors
    ///
    /// Returns an error if required parameters are missing or the format is invalid.
    pub fn parse(streamid: &str) -> Result<Self, StreamIdError> {
        // Strip the prefix if present
        let content = streamid
            .strip_prefix("#!::")
            .unwrap_or(streamid);

        // Parse key=value pairs
        let mut session_id: Option<String> = None;
        let mut token: Option<String> = None;
        let mut pipeline: Option<String> = None;
        let mut audio_enabled = true;
        let mut video_enabled = false;

        for pair in content.split(',') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }

            let mut parts = pair.splitn(2, '=');
            let key = parts.next().ok_or(StreamIdError::InvalidFormat)?;
            let value = parts.next().ok_or(StreamIdError::InvalidFormat)?;

            match key {
                "r" => session_id = Some(value.to_string()),
                "token" => token = Some(value.to_string()),
                "p" => pipeline = Some(value.to_string()),
                "audio" => audio_enabled = value == "1" || value == "true",
                "video" => video_enabled = value == "1" || value == "true",
                _ => {
                    // Ignore unknown keys for forward compatibility
                }
            }
        }

        // Validate required fields
        let session_id = session_id.ok_or(StreamIdError::MissingSessionId)?;
        let token = token.ok_or(StreamIdError::MissingToken)?;
        let pipeline = pipeline.ok_or(StreamIdError::MissingPipeline)?;

        Ok(Self {
            session_id,
            token,
            pipeline,
            audio_enabled,
            video_enabled,
        })
    }

    /// Serialize to streamid format
    ///
    /// Returns a string in the format:
    /// `#!::r=<session_id>,token=<token>,p=<pipeline>,audio=<0|1>,video=<0|1>`
    pub fn to_streamid(&self) -> String {
        format!(
            "#!::r={},token={},p={},audio={},video={}",
            self.session_id,
            self.token,
            self.pipeline,
            if self.audio_enabled { "1" } else { "0" },
            if self.video_enabled { "1" } else { "0" }
        )
    }

    /// Build a full SRT URL with this streamid
    pub fn to_srt_url(&self, host: &str, port: u16) -> String {
        let streamid = self.to_streamid();
        format!(
            "srt://{}:{}?mode=caller&transtype=live&streamid={}",
            host, port, streamid
        )
    }
}

/// Errors that can occur when parsing a streamid
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum StreamIdError {
    #[error("Invalid streamid format")]
    InvalidFormat,

    #[error("Missing required parameter: session_id (r)")]
    MissingSessionId,

    #[error("Missing required parameter: token")]
    MissingToken,

    #[error("Missing required parameter: pipeline (p)")]
    MissingPipeline,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_streamid() {
        let streamid = "#!::r=sess_abc123,token=eyJhbGc,p=demo_v1,audio=1,video=0";
        let params = StreamIdParams::parse(streamid).unwrap();

        assert_eq!(params.session_id, "sess_abc123");
        assert_eq!(params.token, "eyJhbGc");
        assert_eq!(params.pipeline, "demo_v1");
        assert!(params.audio_enabled);
        assert!(!params.video_enabled);
    }

    #[test]
    fn test_parse_without_prefix() {
        let streamid = "r=sess_123,token=abc,p=demo";
        let params = StreamIdParams::parse(streamid).unwrap();

        assert_eq!(params.session_id, "sess_123");
        assert_eq!(params.token, "abc");
        assert_eq!(params.pipeline, "demo");
    }

    #[test]
    fn test_parse_with_defaults() {
        let streamid = "#!::r=sess_123,token=abc,p=demo";
        let params = StreamIdParams::parse(streamid).unwrap();

        // audio defaults to true, video defaults to false
        assert!(params.audio_enabled);
        assert!(!params.video_enabled);
    }

    #[test]
    fn test_parse_missing_session_id() {
        let streamid = "#!::token=abc,p=demo";
        let result = StreamIdParams::parse(streamid);
        assert_eq!(result.unwrap_err(), StreamIdError::MissingSessionId);
    }

    #[test]
    fn test_parse_missing_token() {
        let streamid = "#!::r=sess_123,p=demo";
        let result = StreamIdParams::parse(streamid);
        assert_eq!(result.unwrap_err(), StreamIdError::MissingToken);
    }

    #[test]
    fn test_parse_missing_pipeline() {
        let streamid = "#!::r=sess_123,token=abc";
        let result = StreamIdParams::parse(streamid);
        assert_eq!(result.unwrap_err(), StreamIdError::MissingPipeline);
    }

    #[test]
    fn test_to_streamid() {
        let params = StreamIdParams {
            session_id: "sess_abc123".to_string(),
            token: "eyJhbGc".to_string(),
            pipeline: "demo_v1".to_string(),
            audio_enabled: true,
            video_enabled: false,
        };

        let streamid = params.to_streamid();
        assert_eq!(
            streamid,
            "#!::r=sess_abc123,token=eyJhbGc,p=demo_v1,audio=1,video=0"
        );
    }

    #[test]
    fn test_roundtrip() {
        let original = StreamIdParams {
            session_id: "sess_xyz".to_string(),
            token: "token123".to_string(),
            pipeline: "audio_quality".to_string(),
            audio_enabled: true,
            video_enabled: true,
        };

        let streamid = original.to_streamid();
        let parsed = StreamIdParams::parse(&streamid).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_to_srt_url() {
        let params = StreamIdParams::new(
            "sess_123".to_string(),
            "token".to_string(),
            "demo".to_string(),
        );

        let url = params.to_srt_url("ingest.example.com", 9000);
        assert!(url.starts_with("srt://ingest.example.com:9000?"));
        assert!(url.contains("mode=caller"));
        assert!(url.contains("transtype=live"));
        assert!(url.contains("streamid=#!::r=sess_123"));
    }

    #[test]
    fn test_parse_ignores_unknown_keys() {
        let streamid = "#!::r=sess_123,token=abc,p=demo,unknown=value,foo=bar";
        let params = StreamIdParams::parse(streamid).unwrap();

        assert_eq!(params.session_id, "sess_123");
        assert_eq!(params.token, "abc");
        assert_eq!(params.pipeline, "demo");
    }
}
