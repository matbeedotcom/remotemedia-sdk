//! Data transfer structures for zero-copy IPC
//!
//! Defines the RuntimeData format for transferring audio, video, text,
//! and tensor data between processes with minimal overhead.

use std::time::SystemTime;

/// Runtime data container for IPC
#[derive(Debug, Clone)]
pub struct RuntimeData {
    /// Data type
    pub data_type: DataType,

    /// Session ID
    pub session_id: String,

    /// Timestamp
    pub timestamp: u64,

    /// Variable-size payload (raw bytes)
    pub payload: Vec<u8>,
}

impl RuntimeData {
    /// Create audio runtime data
    pub fn audio(samples: &[f32], sample_rate: u32, channels: u16, session_id: &str) -> Self {
        let payload = unsafe {
            std::slice::from_raw_parts(
                samples.as_ptr() as *const u8,
                samples.len() * std::mem::size_of::<f32>(),
            )
        }
        .to_vec();

        Self {
            data_type: DataType::Audio,
            session_id: session_id.to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload,
        }
    }

    /// Create text runtime data
    pub fn text(text: &str, session_id: &str) -> Self {
        Self {
            data_type: DataType::Text,
            session_id: session_id.to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload: text.as_bytes().to_vec(),
        }
    }

    /// Create video runtime data (Spec 012: Video Codec Support)
    ///
    /// Serializes video frame with metadata for zero-copy IPC transfer.
    ///
    /// # Binary Format
    /// ```text
    /// width (4 bytes) | height (4 bytes) | format (1 byte) | codec (1 byte) |
    /// frame_number (8 bytes) | is_keyframe (1 byte) | pixel_data (variable)
    /// ```
    ///
    /// Total metadata overhead: 19 bytes + pixel_data
    ///
    /// # Arguments
    /// * `pixel_data` - Raw pixel data or encoded bitstream
    /// * `width` - Frame width in pixels
    /// * `height` - Frame height in pixels
    /// * `format` - Pixel format (0-255, see PixelFormat enum)
    /// * `codec` - Video codec (0=None/raw, 1=VP8, 2=H264, 3=AV1)
    /// * `frame_number` - Sequential frame number
    /// * `is_keyframe` - True for I-frames
    /// * `session_id` - Session identifier
    pub fn video(
        pixel_data: &[u8],
        width: u32,
        height: u32,
        format: u8,
        codec: u8,
        frame_number: u64,
        is_keyframe: bool,
        session_id: &str,
    ) -> Self {
        let mut payload = Vec::with_capacity(19 + pixel_data.len());

        // Video metadata (19 bytes)
        payload.extend_from_slice(&width.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        payload.push(format);
        payload.push(codec);
        payload.extend_from_slice(&frame_number.to_le_bytes());
        payload.push(if is_keyframe { 1 } else { 0 });

        // Pixel data (zero-copy via extend)
        payload.extend_from_slice(pixel_data);

        Self {
            data_type: DataType::Video,
            session_id: session_id.to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload,
        }
    }

    /// Create control message runtime data (spec 007)
    ///
    /// Serializes control message as JSON for IPC transfer per wire format spec.
    ///
    /// # Arguments
    /// * `message_type` - The control message type
    /// * `segment_id` - Optional segment ID for cancellation
    /// * `timestamp_ms` - Message timestamp in milliseconds
    /// * `metadata` - Additional metadata
    /// * `session_id` - Session identifier
    pub fn control_message(
        message_type: &crate::data::ControlMessageType,
        segment_id: Option<&str>,
        timestamp_ms: u64,
        metadata: &serde_json::Value,
        session_id: &str,
    ) -> Self {
        // Serialize control message fields as JSON payload
        let payload_json = serde_json::json!({
            "message_type": message_type,
            "segment_id": segment_id,
            "timestamp_ms": timestamp_ms,
            "metadata": metadata,
        });

        let payload = serde_json::to_vec(&payload_json).unwrap_or_default();

        Self {
            data_type: DataType::ControlMessage,
            session_id: session_id.to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload,
        }
    }

    /// Convert to bytes for IPC transfer
    pub fn to_bytes(&self) -> Vec<u8> {
        // Simple format: type (1 byte) | session_len (2 bytes) | session | timestamp (8 bytes) | payload_len (4 bytes) | payload
        let mut bytes = Vec::new();

        // Data type
        bytes.push(self.data_type as u8);

        // Session ID
        let session_bytes = self.session_id.as_bytes();
        bytes.extend_from_slice(&(session_bytes.len() as u16).to_le_bytes());
        bytes.extend_from_slice(session_bytes);

        // Timestamp
        bytes.extend_from_slice(&self.timestamp.to_le_bytes());

        // Payload
        bytes.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.payload);

        bytes
    }

    /// Deserialize video frame from payload (Spec 012)
    ///
    /// Extracts video metadata from the payload and returns a tuple:
    /// (width, height, format, codec, frame_number, is_keyframe, pixel_data)
    ///
    /// # Returns
    /// * `Ok((u32, u32, u8, u8, u64, bool, &[u8]))` - Video metadata and pixel data slice
    /// * `Err(String)` - If payload is malformed
    pub fn video_metadata(&self) -> Result<(u32, u32, u8, u8, u64, bool, &[u8]), String> {
        if self.data_type != DataType::Video {
            return Err("Not a video frame".to_string());
        }

        if self.payload.len() < 19 {
            return Err("Video payload too short".to_string());
        }

        let mut pos = 0;

        // Width (4 bytes)
        let width = u32::from_le_bytes([
            self.payload[pos],
            self.payload[pos + 1],
            self.payload[pos + 2],
            self.payload[pos + 3],
        ]);
        pos += 4;

        // Height (4 bytes)
        let height = u32::from_le_bytes([
            self.payload[pos],
            self.payload[pos + 1],
            self.payload[pos + 2],
            self.payload[pos + 3],
        ]);
        pos += 4;

        // Format (1 byte)
        let format = self.payload[pos];
        pos += 1;

        // Codec (1 byte)
        let codec = self.payload[pos];
        pos += 1;

        // Frame number (8 bytes)
        let frame_number = u64::from_le_bytes([
            self.payload[pos],
            self.payload[pos + 1],
            self.payload[pos + 2],
            self.payload[pos + 3],
            self.payload[pos + 4],
            self.payload[pos + 5],
            self.payload[pos + 6],
            self.payload[pos + 7],
        ]);
        pos += 8;

        // Is keyframe (1 byte)
        let is_keyframe = self.payload[pos] != 0;
        pos += 1;

        // Pixel data (rest of payload)
        let pixel_data = &self.payload[pos..];

        Ok((width, height, format, codec, frame_number, is_keyframe, pixel_data))
    }

    /// Convert from bytes after IPC transfer
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 15 {
            return Err("Invalid data: too short".to_string());
        }

        let mut pos = 0;

        // Data type
        let data_type = match bytes[pos] {
            1 => DataType::Audio,
            2 => DataType::Video,
            3 => DataType::Text,
            4 => DataType::Tensor,
            5 => DataType::ControlMessage,
            _ => return Err(format!("Invalid data type: {}", bytes[pos])),
        };
        pos += 1;

        // Session ID
        let session_len = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;
        if pos + session_len > bytes.len() {
            return Err("Invalid session length".to_string());
        }
        let session_id = String::from_utf8_lossy(&bytes[pos..pos + session_len]).to_string();
        pos += session_len;

        // Timestamp
        if pos + 8 > bytes.len() {
            return Err("Invalid timestamp".to_string());
        }
        let timestamp = u64::from_le_bytes([
            bytes[pos],
            bytes[pos + 1],
            bytes[pos + 2],
            bytes[pos + 3],
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]);
        pos += 8;

        // Payload
        if pos + 4 > bytes.len() {
            return Err("Invalid payload length".to_string());
        }
        let payload_len =
            u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
                as usize;
        pos += 4;
        if pos + payload_len > bytes.len() {
            return Err("Invalid payload".to_string());
        }
        let payload = bytes[pos..pos + payload_len].to_vec();

        Ok(Self {
            data_type,
            session_id,
            timestamp,
            payload,
        })
    }
}

/// Data type discriminator
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Audio = 1,
    Video = 2,
    Text = 3,
    Tensor = 4,
    ControlMessage = 5, // Spec 007: Control messages for low-latency streaming
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_roundtrip() {
        let samples = vec![0.1f32, 0.2, 0.3, 0.4];
        let data = RuntimeData::audio(&samples, 24000, 1, "test_session");

        let bytes = data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.data_type, DataType::Audio);
        assert_eq!(recovered.session_id, "test_session");
        assert_eq!(recovered.payload.len(), 16); // 4 f32s = 16 bytes
    }

    #[test]
    fn test_text_roundtrip() {
        let data = RuntimeData::text("Hello, IPC!", "test_session");

        let bytes = data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.data_type, DataType::Text);
        assert_eq!(recovered.session_id, "test_session");
        assert_eq!(String::from_utf8_lossy(&recovered.payload), "Hello, IPC!");
    }

    #[test]
    fn test_control_message_roundtrip() {
        // Test control message serialization/deserialization
        let message_type = crate::data::ControlMessageType::CancelSpeculation {
            from_timestamp: 1000,
            to_timestamp: 2000,
        };

        let metadata = serde_json::json!({
            "reason": "test_cancellation",
            "confidence": 0.85,
        });

        let data = RuntimeData::control_message(
            &message_type,
            Some("segment_123"),
            1500,
            &metadata,
            "test_session",
        );

        // Verify data type
        assert_eq!(data.data_type, DataType::ControlMessage);
        assert_eq!(data.session_id, "test_session");

        // Roundtrip through binary serialization
        let bytes = data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.data_type, DataType::ControlMessage);
        assert_eq!(recovered.session_id, "test_session");

        // Deserialize payload as JSON
        let payload_json: serde_json::Value = serde_json::from_slice(&recovered.payload).unwrap();

        assert_eq!(payload_json["segment_id"].as_str().unwrap(), "segment_123");
        assert_eq!(payload_json["timestamp_ms"].as_u64().unwrap(), 1500);
        assert_eq!(
            payload_json["metadata"]["reason"].as_str().unwrap(),
            "test_cancellation"
        );
    }
}
