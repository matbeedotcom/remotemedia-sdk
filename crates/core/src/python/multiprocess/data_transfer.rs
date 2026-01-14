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
    pub fn audio(samples: &[f32], _sample_rate: u32, _channels: u16, session_id: &str) -> Self {
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

    /// Create file reference runtime data (Spec 001: RuntimeData.File)
    ///
    /// Serializes file reference with metadata for IPC transfer.
    ///
    /// # Binary Format
    /// ```text
    /// path_len (2 bytes) | path (variable) |
    /// filename_len (2 bytes) | filename (variable) |
    /// mime_type_len (2 bytes) | mime_type (variable) |
    /// size (8 bytes) | offset (8 bytes) | length (8 bytes) |
    /// stream_id_len (2 bytes) | stream_id (variable)
    /// ```
    ///
    /// # Arguments
    /// * `path` - File path (absolute or relative)
    /// * `filename` - Original filename (optional, empty string if None)
    /// * `mime_type` - MIME type hint (optional, empty string if None)
    /// * `size` - File size in bytes (0 if unknown)
    /// * `offset` - Byte offset for range requests (0 for start)
    /// * `length` - Length for range requests (0 for to-EOF)
    /// * `stream_id` - Stream identifier for multi-track routing (optional)
    /// * `session_id` - Session identifier
    pub fn file(
        path: &str,
        filename: Option<&str>,
        mime_type: Option<&str>,
        size: Option<u64>,
        offset: Option<u64>,
        length: Option<u64>,
        stream_id: Option<&str>,
        session_id: &str,
    ) -> Self {
        let mut payload = Vec::new();

        // Path (required)
        let path_bytes = path.as_bytes();
        payload.extend_from_slice(&(path_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(path_bytes);

        // Filename (optional, empty if None)
        let filename_bytes = filename.unwrap_or("").as_bytes();
        payload.extend_from_slice(&(filename_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(filename_bytes);

        // MIME type (optional, empty if None)
        let mime_type_bytes = mime_type.unwrap_or("").as_bytes();
        payload.extend_from_slice(&(mime_type_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(mime_type_bytes);

        // Size (0 if None)
        payload.extend_from_slice(&size.unwrap_or(0).to_le_bytes());

        // Offset (0 if None)
        payload.extend_from_slice(&offset.unwrap_or(0).to_le_bytes());

        // Length (0 if None)
        payload.extend_from_slice(&length.unwrap_or(0).to_le_bytes());

        // Stream ID (optional, empty if None)
        let stream_id_bytes = stream_id.unwrap_or("").as_bytes();
        payload.extend_from_slice(&(stream_id_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(stream_id_bytes);

        Self {
            data_type: DataType::File,
            session_id: session_id.to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload,
        }
    }

    /// Deserialize file reference from payload (Spec 001)
    ///
    /// Extracts file metadata from the payload and returns a tuple:
    /// (path, filename, mime_type, size, offset, length, stream_id)
    ///
    /// # Returns
    /// * `Ok(tuple)` - File metadata
    /// * `Err(String)` - If payload is malformed
    pub fn file_metadata(
        &self,
    ) -> Result<(String, Option<String>, Option<String>, Option<u64>, Option<u64>, Option<u64>, Option<String>), String>
    {
        if self.data_type != DataType::File {
            return Err("Not a file reference".to_string());
        }

        if self.payload.len() < 32 {
            // Minimum: 2+0 + 2+0 + 2+0 + 8 + 8 + 8 + 2+0 = 32
            return Err("File payload too short".to_string());
        }

        let mut pos = 0;

        // Path
        let path_len = u16::from_le_bytes([self.payload[pos], self.payload[pos + 1]]) as usize;
        pos += 2;
        if pos + path_len > self.payload.len() {
            return Err("Invalid path length".to_string());
        }
        let path = String::from_utf8_lossy(&self.payload[pos..pos + path_len]).to_string();
        pos += path_len;

        // Filename
        if pos + 2 > self.payload.len() {
            return Err("Invalid filename length".to_string());
        }
        let filename_len = u16::from_le_bytes([self.payload[pos], self.payload[pos + 1]]) as usize;
        pos += 2;
        let filename = if filename_len > 0 {
            if pos + filename_len > self.payload.len() {
                return Err("Invalid filename".to_string());
            }
            Some(String::from_utf8_lossy(&self.payload[pos..pos + filename_len]).to_string())
        } else {
            None
        };
        pos += filename_len;

        // MIME type
        if pos + 2 > self.payload.len() {
            return Err("Invalid mime_type length".to_string());
        }
        let mime_type_len = u16::from_le_bytes([self.payload[pos], self.payload[pos + 1]]) as usize;
        pos += 2;
        let mime_type = if mime_type_len > 0 {
            if pos + mime_type_len > self.payload.len() {
                return Err("Invalid mime_type".to_string());
            }
            Some(String::from_utf8_lossy(&self.payload[pos..pos + mime_type_len]).to_string())
        } else {
            None
        };
        pos += mime_type_len;

        // Size, Offset, Length (8 bytes each)
        if pos + 24 > self.payload.len() {
            return Err("Invalid size/offset/length fields".to_string());
        }
        let size = u64::from_le_bytes([
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
        let offset = u64::from_le_bytes([
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
        let length = u64::from_le_bytes([
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

        // Stream ID
        if pos + 2 > self.payload.len() {
            return Err("Invalid stream_id length".to_string());
        }
        let stream_id_len = u16::from_le_bytes([self.payload[pos], self.payload[pos + 1]]) as usize;
        pos += 2;
        let stream_id = if stream_id_len > 0 {
            if pos + stream_id_len > self.payload.len() {
                return Err("Invalid stream_id".to_string());
            }
            Some(String::from_utf8_lossy(&self.payload[pos..pos + stream_id_len]).to_string())
        } else {
            None
        };

        // Convert 0 values to None for optional fields
        let size = if size == 0 { None } else { Some(size) };
        let offset = if offset == 0 { None } else { Some(offset) };
        let length = if length == 0 { None } else { Some(length) };

        Ok((path, filename, mime_type, size, offset, length, stream_id))
    }

    /// Create numpy runtime data
    ///
    /// Serializes numpy array metadata and data for zero-copy IPC transfer.
    /// The payload format is:
    /// - shape_len (2 bytes)
    /// - shape (8 bytes per dimension)
    /// - strides_len (2 bytes)
    /// - strides (8 bytes per dimension)
    /// - dtype_len (2 bytes)
    /// - dtype (variable bytes, UTF-8)
    /// - flags (1 byte: bit 0 = c_contiguous, bit 1 = f_contiguous)
    /// - data (remaining bytes)
    pub fn numpy(
        data: &[u8],
        shape: &[usize],
        dtype: &str,
        strides: &[isize],
        c_contiguous: bool,
        f_contiguous: bool,
        session_id: &str,
    ) -> Self {
        let mut payload = Vec::new();

        // Serialize shape
        payload.extend_from_slice(&(shape.len() as u16).to_le_bytes());
        for &dim in shape {
            payload.extend_from_slice(&(dim as u64).to_le_bytes());
        }

        // Serialize strides
        payload.extend_from_slice(&(strides.len() as u16).to_le_bytes());
        for &stride in strides {
            payload.extend_from_slice(&(stride as i64).to_le_bytes());
        }

        // Serialize dtype
        let dtype_bytes = dtype.as_bytes();
        payload.extend_from_slice(&(dtype_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(dtype_bytes);

        // Serialize flags
        let mut flags: u8 = 0;
        if c_contiguous {
            flags |= 0x01;
        }
        if f_contiguous {
            flags |= 0x02;
        }
        payload.push(flags);

        // Append array data
        payload.extend_from_slice(data);

        Self {
            data_type: DataType::Numpy,
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
            6 => DataType::Numpy,
            7 => DataType::File,
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
    Numpy = 6,          // Numpy arrays with metadata for zero-copy passthrough
    File = 7,           // Spec 001: File reference with metadata
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
    fn test_numpy_float32_roundtrip() {
        // Test numpy array serialization/deserialization
        let data = vec![0.1f32, 0.2, 0.3, 0.4, 0.5];
        let data_bytes: Vec<u8> = data
            .iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect();
        
        let shape = vec![5];
        let strides = vec![4]; // 4 bytes per f32
        let dtype = "float32";
        
        let numpy_data = RuntimeData::numpy(
            &data_bytes,
            &shape,
            dtype,
            &strides,
            true,  // c_contiguous
            false, // f_contiguous
            "test_session"
        );
        
        let bytes = numpy_data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();
        
        assert_eq!(recovered.data_type, DataType::Numpy);
        assert_eq!(recovered.session_id, "test_session");
        
        // Verify payload contains shape + strides + dtype + flags + data
        assert!(recovered.payload.len() > data_bytes.len());
    }
    
    #[test]
    fn test_numpy_multidimensional() {
        // Test 2D numpy array (e.g., stereo audio: frames × channels)
        let frames = 960; // 20ms at 48kHz
        let channels = 2;
        let total_samples = frames * channels;
        
        let data: Vec<f32> = (0..total_samples)
            .map(|i| (i as f32) * 0.001)
            .collect();
        let data_bytes: Vec<u8> = data
            .iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect();
        
        let shape = vec![frames, channels];
        let strides = vec![8, 4]; // row stride = 2 floats = 8 bytes, col stride = 4 bytes
        
        let numpy_data = RuntimeData::numpy(
            &data_bytes,
            &shape,
            "float32",
            &strides,
            true,
            false,
            "test_session"
        );
        
        let bytes = numpy_data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();
        
        assert_eq!(recovered.data_type, DataType::Numpy);
        assert_eq!(recovered.payload.len(), 2 + 16 + 2 + 16 + 2 + 7 + 1 + data_bytes.len());
        // shape_len(2) + shape(2×8) + strides_len(2) + strides(2×8) + dtype_len(2) + dtype(7) + flags(1) + data
    }
    
    #[test]
    fn test_numpy_fortran_order() {
        // Test Fortran-contiguous array
        let data_bytes = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let shape = vec![2, 2];
        let strides = vec![4, 8]; // F-order: column-major
        
        let numpy_data = RuntimeData::numpy(
            &data_bytes,
            &shape,
            "float32",
            &strides,
            false, // not c_contiguous
            true,  // f_contiguous
            "test_session"
        );
        
        let bytes = numpy_data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();
        
        assert_eq!(recovered.data_type, DataType::Numpy);
        
        // Parse flags from payload to verify F-contiguous flag is set
        // Flags are after: shape_len(2) + shape(16) + strides_len(2) + strides(16) + dtype_len(2) + dtype
        let flags_offset = 2 + 16 + 2 + 16 + 2 + 7; // = 45
        let flags = recovered.payload[flags_offset];
        assert_eq!(flags & 0x02, 0x02); // F-contiguous bit should be set
        assert_eq!(flags & 0x01, 0x00); // C-contiguous bit should not be set
    }
    
    #[test]
    fn test_numpy_different_dtypes() {
        // Test various dtypes
        let test_cases = vec![
            ("float32", 4),
            ("float64", 8),
            ("int16", 2),
            ("int32", 4),
            ("uint8", 1),
        ];
        
        for (dtype, bytes_per_element) in test_cases {
            let data_bytes = vec![0u8; bytes_per_element * 10]; // 10 elements
            let shape = vec![10];
            let strides = vec![bytes_per_element as isize];
            
            let numpy_data = RuntimeData::numpy(
                &data_bytes,
                &shape,
                dtype,
                &strides,
                true,
                false,
                "test_session"
            );
            
            let bytes = numpy_data.to_bytes();
            let recovered = RuntimeData::from_bytes(&bytes).unwrap();
            
            assert_eq!(recovered.data_type, DataType::Numpy);
            assert_eq!(recovered.session_id, "test_session");
        }
    }
    
    #[test]
    fn test_numpy_metadata_preservation() {
        // Test that all metadata is preserved through serialization
        let data_bytes = vec![1.0f32, 2.0, 3.0, 4.0]
            .iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect::<Vec<u8>>();
        
        let shape = vec![2, 2];
        let strides = vec![8, 4];
        let dtype = "float32";
        let c_contiguous = true;
        let f_contiguous = false;
        
        let numpy_data = RuntimeData::numpy(
            &data_bytes,
            &shape,
            dtype,
            &strides,
            c_contiguous,
            f_contiguous,
            "test_session"
        );
        
        let bytes = numpy_data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();
        
        // Deserialize and verify metadata
        let payload = &recovered.payload;
        let mut pos = 0;
        
        // Read shape
        let shape_len = u16::from_le_bytes([payload[pos], payload[pos + 1]]) as usize;
        pos += 2;
        assert_eq!(shape_len, 2);
        
        let mut recovered_shape = Vec::new();
        for _ in 0..shape_len {
            let dim = u64::from_le_bytes([
                payload[pos], payload[pos + 1], payload[pos + 2], payload[pos + 3],
                payload[pos + 4], payload[pos + 5], payload[pos + 6], payload[pos + 7],
            ]) as usize;
            recovered_shape.push(dim);
            pos += 8;
        }
        assert_eq!(recovered_shape, shape);
        
        // Read strides
        let strides_len = u16::from_le_bytes([payload[pos], payload[pos + 1]]) as usize;
        pos += 2;
        assert_eq!(strides_len, 2);
        
        let mut recovered_strides = Vec::new();
        for _ in 0..strides_len {
            let stride = i64::from_le_bytes([
                payload[pos], payload[pos + 1], payload[pos + 2], payload[pos + 3],
                payload[pos + 4], payload[pos + 5], payload[pos + 6], payload[pos + 7],
            ]) as isize;
            recovered_strides.push(stride);
            pos += 8;
        }
        assert_eq!(recovered_strides, strides);
        
        // Read dtype
        let dtype_len = u16::from_le_bytes([payload[pos], payload[pos + 1]]) as usize;
        pos += 2;
        let recovered_dtype = String::from_utf8_lossy(&payload[pos..pos + dtype_len]).to_string();
        assert_eq!(recovered_dtype, dtype);
        pos += dtype_len;
        
        // Read flags
        let flags = payload[pos];
        assert_eq!(flags & 0x01, if c_contiguous { 0x01 } else { 0x00 });
        assert_eq!(flags & 0x02, if f_contiguous { 0x02 } else { 0x00 });
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

    #[test]
    fn test_file_roundtrip() {
        // Test file reference serialization/deserialization with all fields
        let data = RuntimeData::file(
            "/data/input/video.mp4",
            Some("video.mp4"),
            Some("video/mp4"),
            Some(104_857_600), // 100 MB
            Some(1_048_576),   // 1 MB offset
            Some(65_536),      // 64 KB chunk
            Some("video_track"),
            "test_session",
        );

        // Verify data type
        assert_eq!(data.data_type, DataType::File);
        assert_eq!(data.session_id, "test_session");

        // Roundtrip through binary serialization
        let bytes = data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.data_type, DataType::File);
        assert_eq!(recovered.session_id, "test_session");

        // Verify metadata extraction
        let (path, filename, mime_type, size, offset, length, stream_id) =
            recovered.file_metadata().unwrap();

        assert_eq!(path, "/data/input/video.mp4");
        assert_eq!(filename, Some("video.mp4".to_string()));
        assert_eq!(mime_type, Some("video/mp4".to_string()));
        assert_eq!(size, Some(104_857_600));
        assert_eq!(offset, Some(1_048_576));
        assert_eq!(length, Some(65_536));
        assert_eq!(stream_id, Some("video_track".to_string()));
    }

    #[test]
    fn test_file_minimal_roundtrip() {
        // Test file reference with only required path field
        let data = RuntimeData::file(
            "/tmp/output.bin",
            None,
            None,
            None,
            None,
            None,
            None,
            "test_session",
        );

        // Roundtrip through binary serialization
        let bytes = data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.data_type, DataType::File);

        // Verify metadata extraction
        let (path, filename, mime_type, size, offset, length, stream_id) =
            recovered.file_metadata().unwrap();

        assert_eq!(path, "/tmp/output.bin");
        assert_eq!(filename, None);
        assert_eq!(mime_type, None);
        assert_eq!(size, None);
        assert_eq!(offset, None);
        assert_eq!(length, None);
        assert_eq!(stream_id, None);
    }

    #[test]
    fn test_file_byte_range() {
        // Test file reference for byte range request
        let data = RuntimeData::file(
            "/data/large_file.bin",
            None,
            None,
            Some(1_073_741_824), // 1 GB total size
            Some(10 * 1024 * 1024), // 10 MB offset
            Some(64 * 1024),        // 64 KB chunk
            None,
            "test_session",
        );

        let bytes = data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();

        let (path, _, _, size, offset, length, _) = recovered.file_metadata().unwrap();

        assert_eq!(path, "/data/large_file.bin");
        assert_eq!(size, Some(1_073_741_824));
        assert_eq!(offset, Some(10 * 1024 * 1024));
        assert_eq!(length, Some(64 * 1024));
    }
}
