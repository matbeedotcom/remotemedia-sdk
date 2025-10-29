// RuntimeData enum: In-memory representation of all data types
// Feature: 004-generic-streaming

use crate::grpc_service::generated::{AudioBuffer, VideoFrame, TensorBuffer, JsonData, TextBuffer, BinaryBuffer, DataTypeHint};
use prost::bytes::Bytes;

/// Runtime representation of data after proto deserialization
///
/// This enum provides a unified interface for all data types in the pipeline.
/// It's used internally by the executor and nodes for type-safe data handling.
///
/// Conversion:
/// - Protobuf DataBuffer → RuntimeData (via convert_proto_to_runtime_data)
/// - RuntimeData → Protobuf DataBuffer (via convert_runtime_to_proto_data)
///
/// Common operations:
/// - data_type(): Get the DataTypeHint for routing
/// - item_count(): Get count of items (samples, frames, tokens, etc.)
/// - size_bytes(): Get memory footprint
#[derive(Debug, Clone)]
pub enum RuntimeData {
    /// Audio samples with metadata
    /// Items: number of samples
    Audio(AudioBuffer),

    /// Video frame with pixel data
    /// Items: 1 frame
    Video(VideoFrame),

    /// Multi-dimensional tensor
    /// Items: total elements (shape.product())
    Tensor(TensorBuffer),

    /// JSON payload (parsed into serde_json::Value)
    /// Items: array length, object field count, or 1 for primitives
    Json(serde_json::Value),

    /// UTF-8 text
    /// Items: character count
    Text(String),

    /// Raw binary data
    /// Items: byte count
    Binary(Bytes),
}

impl RuntimeData {
    /// Get the data type hint for routing and validation
    pub fn data_type(&self) -> DataTypeHint {
        match self {
            RuntimeData::Audio(_) => DataTypeHint::Audio,
            RuntimeData::Video(_) => DataTypeHint::Video,
            RuntimeData::Tensor(_) => DataTypeHint::Tensor,
            RuntimeData::Json(_) => DataTypeHint::Json,
            RuntimeData::Text(_) => DataTypeHint::Text,
            RuntimeData::Binary(_) => DataTypeHint::Binary,
        }
    }

    /// Get the count of items in this data
    ///
    /// - Audio: number of samples (across all channels)
    /// - Video: 1 (one frame)
    /// - Tensor: total elements (product of shape)
    /// - Json: array length, object field count, or 1 for primitives
    /// - Text: UTF-8 character count
    /// - Binary: byte count
    pub fn item_count(&self) -> usize {
        match self {
            RuntimeData::Audio(buf) => buf.num_samples as usize,
            RuntimeData::Video(_) => 1, // One frame
            RuntimeData::Tensor(t) => {
                // Product of all dimensions
                t.shape.iter().map(|&d| d as usize).product()
            },
            RuntimeData::Json(value) => {
                match value {
                    serde_json::Value::Array(arr) => arr.len(),
                    serde_json::Value::Object(obj) => obj.len(),
                    _ => 1, // Primitives count as 1
                }
            },
            RuntimeData::Text(s) => s.chars().count(), // Unicode character count
            RuntimeData::Binary(b) => b.len(), // Byte count
        }
    }

    /// Get memory size in bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            RuntimeData::Audio(buf) => buf.samples.len(),
            RuntimeData::Video(frame) => frame.pixel_data.len(),
            RuntimeData::Tensor(t) => t.data.len(),
            RuntimeData::Json(value) => {
                // Approximate JSON size by serializing
                serde_json::to_string(value)
                    .map(|s| s.len())
                    .unwrap_or(0)
            },
            RuntimeData::Text(s) => s.len(), // UTF-8 byte length
            RuntimeData::Binary(b) => b.len(),
        }
    }

    /// Convert into audio bytes (zero-copy for audio, None for other types)
    pub fn into_audio_bytes(self) -> Option<Bytes> {
        match self {
            RuntimeData::Audio(buf) => Some(Bytes::from(buf.samples)),
            _ => None,
        }
    }

    /// Get data type name as string (for metrics and logging)
    pub fn type_name(&self) -> &'static str {
        match self {
            RuntimeData::Audio(_) => "audio",
            RuntimeData::Video(_) => "video",
            RuntimeData::Tensor(_) => "tensor",
            RuntimeData::Json(_) => "json",
            RuntimeData::Text(_) => "text",
            RuntimeData::Binary(_) => "binary",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_item_count() {
        let audio = RuntimeData::Audio(AudioBuffer {
            samples: vec![0u8; 4000], // 1000 F32 samples
            sample_rate: 16000,
            channels: 1,
            format: 1, // F32
            num_samples: 1000,
        });
        assert_eq!(audio.item_count(), 1000);
        assert_eq!(audio.size_bytes(), 4000);
        assert_eq!(audio.type_name(), "audio");
    }

    #[test]
    fn test_json_item_count() {
        let json_array = RuntimeData::Json(serde_json::json!([1, 2, 3, 4, 5]));
        assert_eq!(json_array.item_count(), 5);

        let json_object = RuntimeData::Json(serde_json::json!({"a": 1, "b": 2}));
        assert_eq!(json_object.item_count(), 2);

        let json_primitive = RuntimeData::Json(serde_json::json!(42));
        assert_eq!(json_primitive.item_count(), 1);
    }

    #[test]
    fn test_tensor_item_count() {
        let tensor = RuntimeData::Tensor(TensorBuffer {
            data: vec![0u8; 512 * 4], // 512 F32 elements
            shape: vec![512],
            dtype: 1, // F32
            layout: String::new(),
        });
        assert_eq!(tensor.item_count(), 512);
        assert_eq!(tensor.size_bytes(), 512 * 4);
    }

    #[test]
    fn test_video_item_count() {
        let video = RuntimeData::Video(VideoFrame {
            pixel_data: vec![0u8; 640 * 480 * 3],
            width: 640,
            height: 480,
            format: 1, // RGB24
            frame_number: 0,
            timestamp_us: 0,
        });
        assert_eq!(video.item_count(), 1); // One frame
        assert_eq!(video.size_bytes(), 640 * 480 * 3);
    }

    #[test]
    fn test_text_item_count() {
        let text = RuntimeData::Text("Hello, 世界!".to_string());
        assert_eq!(text.item_count(), 9); // 9 Unicode characters
        assert!(text.size_bytes() > 9); // UTF-8 bytes > character count
    }

    #[test]
    fn test_data_type() {
        let audio = RuntimeData::Audio(AudioBuffer::default());
        assert_eq!(audio.data_type(), DataTypeHint::Audio);

        let video = RuntimeData::Video(VideoFrame::default());
        assert_eq!(video.data_type(), DataTypeHint::Video);
    }
}
