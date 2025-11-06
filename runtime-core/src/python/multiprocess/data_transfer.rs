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
        }.to_vec();

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
            _ => return Err("Invalid data type".to_string()),
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
            bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3],
            bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7],
        ]);
        pos += 8;

        // Payload
        if pos + 4 > bytes.len() {
            return Err("Invalid payload length".to_string());
        }
        let payload_len = u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
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
}
