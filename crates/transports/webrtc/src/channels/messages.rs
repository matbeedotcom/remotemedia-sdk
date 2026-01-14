//! Data channel message types (T157-T158)
//!
//! Defines the message format for WebRTC data channel communication.
//! Supports JSON, binary, and text message types.

use serde::{Deserialize, Serialize};

/// Maximum message size for data channels (16 MB)
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Message types that can be sent over a WebRTC data channel (T157)
///
/// Data channels support three message formats:
/// - JSON: Structured data for control messages and pipeline configuration
/// - Binary: Raw bytes for efficient data transfer
/// - Text: UTF-8 strings for simple text messages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum DataChannelMessage {
    /// JSON payload for structured data
    ///
    /// Used for control messages, pipeline reconfiguration,
    /// and other structured communication.
    Json(serde_json::Value),

    /// Binary payload for raw data
    ///
    /// Used for efficient transfer of binary data like
    /// compressed audio samples or encoded video frames.
    #[serde(with = "base64_bytes")]
    Binary(Vec<u8>),

    /// Text payload for simple string messages
    ///
    /// Used for simple text-based communication.
    Text(String),
}

impl DataChannelMessage {
    /// Create a new JSON message from a serializable value
    ///
    /// # Arguments
    /// * `value` - Any value that implements Serialize
    ///
    /// # Returns
    /// Result with DataChannelMessage::Json or serialization error
    pub fn json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        let json_value = serde_json::to_value(value)?;
        Ok(DataChannelMessage::Json(json_value))
    }

    /// Create a new binary message
    ///
    /// # Arguments
    /// * `data` - Raw bytes to send
    pub fn binary(data: Vec<u8>) -> Self {
        DataChannelMessage::Binary(data)
    }

    /// Create a new text message
    ///
    /// # Arguments
    /// * `text` - Text string to send
    pub fn text(text: impl Into<String>) -> Self {
        DataChannelMessage::Text(text.into())
    }

    /// Get the size of this message in bytes
    pub fn size(&self) -> usize {
        match self {
            DataChannelMessage::Json(v) => v.to_string().len(),
            DataChannelMessage::Binary(b) => b.len(),
            DataChannelMessage::Text(t) => t.len(),
        }
    }

    /// Check if this message exceeds the maximum size
    pub fn exceeds_max_size(&self) -> bool {
        self.size() > MAX_MESSAGE_SIZE
    }

    /// Serialize message to bytes for transmission
    ///
    /// Uses JSON serialization for all message types to maintain
    /// type information across the channel.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize message from bytes
    ///
    /// # Arguments
    /// * `bytes` - Raw bytes received from data channel
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Check if this is a JSON message
    pub fn is_json(&self) -> bool {
        matches!(self, DataChannelMessage::Json(_))
    }

    /// Check if this is a binary message
    pub fn is_binary(&self) -> bool {
        matches!(self, DataChannelMessage::Binary(_))
    }

    /// Check if this is a text message
    pub fn is_text(&self) -> bool {
        matches!(self, DataChannelMessage::Text(_))
    }

    /// Get the JSON payload if this is a JSON message
    pub fn as_json(&self) -> Option<&serde_json::Value> {
        match self {
            DataChannelMessage::Json(v) => Some(v),
            _ => None,
        }
    }

    /// Get the binary payload if this is a binary message
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            DataChannelMessage::Binary(b) => Some(b),
            _ => None,
        }
    }

    /// Get the text payload if this is a text message
    pub fn as_text(&self) -> Option<&str> {
        match self {
            DataChannelMessage::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// Custom serialization for binary data as base64
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

/// Control message types for pipeline management
///
/// These are common JSON message structures used for
/// controlling the media pipeline via data channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum ControlMessage {
    /// Request to reconfigure the pipeline
    Reconfigure {
        /// New manifest JSON for the pipeline
        manifest: serde_json::Value,
    },

    /// Request to pause media streaming
    Pause,

    /// Request to resume media streaming
    Resume,

    /// Request current pipeline status
    GetStatus,

    /// Status response
    Status {
        /// Current pipeline state
        state: String,
        /// Number of active nodes
        active_nodes: usize,
        /// Current timestamp
        timestamp_ms: u64,
    },

    /// Ping for latency measurement
    Ping {
        /// Timestamp when ping was sent
        timestamp_ms: u64,
    },

    /// Pong response to ping
    Pong {
        /// Original ping timestamp
        ping_timestamp_ms: u64,
        /// Timestamp when pong was sent
        pong_timestamp_ms: u64,
    },

    /// Custom application message
    Custom {
        /// Message type identifier
        message_type: String,
        /// Message payload
        data: serde_json::Value,
    },
}

impl ControlMessage {
    /// Convert to DataChannelMessage
    pub fn to_data_channel_message(&self) -> Result<DataChannelMessage, serde_json::Error> {
        DataChannelMessage::json(self)
    }

    /// Parse from DataChannelMessage
    pub fn from_data_channel_message(msg: &DataChannelMessage) -> Result<Self, serde_json::Error> {
        match msg {
            DataChannelMessage::Json(v) => serde_json::from_value(v.clone()),
            DataChannelMessage::Text(t) => serde_json::from_str(t),
            DataChannelMessage::Binary(b) => serde_json::from_slice(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_message() {
        let msg = DataChannelMessage::json(&serde_json::json!({
            "key": "value",
            "number": 42
        }))
        .unwrap();

        assert!(msg.is_json());
        assert!(!msg.is_binary());
        assert!(!msg.is_text());

        let json = msg.as_json().unwrap();
        assert_eq!(json["key"], "value");
        assert_eq!(json["number"], 42);
    }

    #[test]
    fn test_binary_message() {
        let data = vec![1, 2, 3, 4, 5];
        let msg = DataChannelMessage::binary(data.clone());

        assert!(msg.is_binary());
        assert_eq!(msg.as_binary(), Some(&data[..]));
    }

    #[test]
    fn test_text_message() {
        let msg = DataChannelMessage::text("Hello, World!");

        assert!(msg.is_text());
        assert_eq!(msg.as_text(), Some("Hello, World!"));
    }

    #[test]
    fn test_message_serialization() {
        let msg = DataChannelMessage::text("test");
        let bytes = msg.to_bytes().unwrap();
        let decoded = DataChannelMessage::from_bytes(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_binary_serialization() {
        let msg = DataChannelMessage::binary(vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let bytes = msg.to_bytes().unwrap();
        let decoded = DataChannelMessage::from_bytes(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_message_size() {
        let msg = DataChannelMessage::text("12345");
        assert_eq!(msg.size(), 5);
        assert!(!msg.exceeds_max_size());
    }

    #[test]
    fn test_control_message_reconfigure() {
        let ctrl = ControlMessage::Reconfigure {
            manifest: serde_json::json!({"nodes": []}),
        };
        let msg = ctrl.to_data_channel_message().unwrap();
        let decoded = ControlMessage::from_data_channel_message(&msg).unwrap();
        match decoded {
            ControlMessage::Reconfigure { manifest } => {
                assert_eq!(manifest["nodes"], serde_json::json!([]));
            }
            _ => panic!("Expected Reconfigure"),
        }
    }

    #[test]
    fn test_control_message_ping_pong() {
        let ping = ControlMessage::Ping {
            timestamp_ms: 1234567890,
        };
        let msg = ping.to_data_channel_message().unwrap();
        let decoded = ControlMessage::from_data_channel_message(&msg).unwrap();
        match decoded {
            ControlMessage::Ping { timestamp_ms } => {
                assert_eq!(timestamp_ms, 1234567890);
            }
            _ => panic!("Expected Ping"),
        }
    }
}
