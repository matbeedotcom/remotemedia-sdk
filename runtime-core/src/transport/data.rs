//! Transport-agnostic data container
//!
//! Provides `TransportData` which wraps core `RuntimeData` with optional
//! metadata for transport-specific information.

use crate::data::RuntimeData;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Transport-agnostic data container
///
/// Wraps core RuntimeData with optional metadata for transport-specific
/// information (sequence numbers, headers, tags, etc.).
///
/// # Design
///
/// - **data**: Core payload (Audio, Text, Image, Binary) - required
/// - **sequence**: Optional sequence number for stream ordering
/// - **metadata**: Extensible key-value pairs for transport-specific info
///
/// # Examples
///
/// ```ignore
/// use remotemedia_runtime_core::transport::TransportData;
/// use remotemedia_runtime_core::data::RuntimeData;
///
/// // Simple usage
/// let data = TransportData::new(RuntimeData::Text("hello".into()));
///
/// // With sequence number
/// let data = TransportData::new(RuntimeData::Audio { ... })
///     .with_sequence(42);
///
/// // With metadata
/// let data = TransportData::new(RuntimeData::Text("hello".into()))
///     .with_metadata("request_id".into(), "abc123".into());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportData {
    /// Core data payload (audio, text, image, binary)
    pub data: RuntimeData,

    /// Optional sequence number for ordering in streams
    ///
    /// Transports should set this for streaming sessions to maintain
    /// message order. Core may use this for metrics and debugging.
    pub sequence: Option<u64>,

    /// Transport-specific metadata (extensible key-value pairs)
    ///
    /// Examples:
    /// - gRPC: HTTP headers, request IDs, client info
    /// - FFI: Python call context, thread info
    /// - Custom: Any transport-specific info
    pub metadata: HashMap<String, String>,
}

impl TransportData {
    /// Create new TransportData with just payload (no metadata)
    ///
    /// # Arguments
    ///
    /// * `data` - Core RuntimeData payload
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let data = TransportData::new(RuntimeData::Text("hello".into()));
    /// assert!(data.sequence.is_none());
    /// assert!(data.metadata.is_empty());
    /// ```
    pub fn new(data: RuntimeData) -> Self {
        Self {
            data,
            sequence: None,
            metadata: HashMap::new(),
        }
    }

    /// Builder pattern: add sequence number
    ///
    /// # Arguments
    ///
    /// * `seq` - Sequence number for ordering
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let data = TransportData::new(audio_data)
    ///     .with_sequence(1);
    /// assert_eq!(data.sequence, Some(1));
    /// ```
    pub fn with_sequence(mut self, seq: u64) -> Self {
        self.sequence = Some(seq);
        self
    }

    /// Builder pattern: add metadata key-value pair
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key
    /// * `value` - Metadata value
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let data = TransportData::new(audio_data)
    ///     .with_metadata("client_id".into(), "user123".into())
    ///     .with_metadata("request_id".into(), "req456".into());
    /// assert_eq!(data.metadata.get("client_id"), Some(&"user123".to_string()));
    /// ```
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Get metadata value by key
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key to lookup
    ///
    /// # Returns
    ///
    /// * `Some(&String)` - Value if key exists
    /// * `None` - Key not found
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

/// Convert RuntimeData directly to TransportData
impl From<RuntimeData> for TransportData {
    fn from(data: RuntimeData) -> Self {
        TransportData::new(data)
    }
}
