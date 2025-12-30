//! RemoteMedia Runtime Core - Transport-agnostic execution engine
//!
//! This crate provides the core runtime functionality for executing RemoteMedia
//! pipelines without any transport-specific dependencies.
//!
//! # Architecture
//!
//! Runtime-core is a pure library that:
//! - Defines transport abstractions (`PipelineTransport`, `StreamSession` traits)
//! - Provides execution engine (`PipelineRunner`)
//! - Manages pipeline graphs, node execution, and session routing
//! - Has ZERO dependencies on transport crates (no tonic, prost, pyo3, etc.)
//!
//! Transport implementations (gRPC, FFI, WebRTC) are separate crates that:
//! - Depend on `remotemedia-runtime-core`
//! - Implement the `PipelineTransport` trait
//! - Handle their own serialization formats
//!
//! # Example
//!
//! ```
//! use remotemedia_runtime_core::transport::PipelineRunner;
//! use remotemedia_runtime_core::transport::TransportData;
//! use remotemedia_runtime_core::data::RuntimeData;
//!
//! // Create the pipeline runner
//! let runner = PipelineRunner::new().unwrap();
//!
//! // Create transport data
//! let input = TransportData::new(RuntimeData::Text("hello".into()));
//!
//! // Use runner.execute_unary(manifest, input).await for execution
//! ```

#![warn(clippy::all)]
#![allow(clippy::arc_with_non_send_sync)] // iceoryx2 types are intentionally !Send

// Allow the crate to refer to itself as `remotemedia_runtime_core` for proc-macro compatibility
extern crate self as remotemedia_runtime_core;

// Core execution modules
pub mod audio;
pub mod capabilities;
pub mod executor;
pub mod nodes;
pub mod python;
pub mod validation;
/// Public entrypoint for ergonomic registration macros.
pub mod registration_macros {
    pub use crate::{
        register_python_node, register_python_nodes, register_rust_node, register_rust_node_default,
    };
}

// Manifest
pub use manifest::Manifest;

// Validation - convenience re-exports for introspection API
pub use validation::{get_all_schemas, get_node_schema, SchemaValidator, ValidationResult};

// Transport abstraction layer
pub mod transport;

// Re-export core modules from existing runtime
// NOTE: For Phase 2, these are stub re-exports
// In later phases, we'll copy the actual implementations from runtime/

/// Data types module - transport-agnostic data representations
pub mod data {
    //! Core data types
    //!
    //! These types match the DataBuffer protobuf schema but are pure Rust types
    //! with no protobuf dependencies. Transports handle conversion.

    use serde::{Deserialize, Serialize};

    // Low-latency streaming data structures (spec 007)
    pub mod buffering_policy;
    pub mod control_message;
    pub mod ring_buffer;
    pub mod speculative_segment;

    pub use buffering_policy::{BufferingPolicy, MergeStrategy};
    pub use control_message::{ControlMessage, ControlMessageType};
    pub use ring_buffer::RingBuffer;
    pub use speculative_segment::{SegmentStatus, SpeculativeSegment};

    // Video codec support (spec 012)
    pub mod video;
    pub use video::{PixelFormat, VideoCodec};

    /// Audio format enumeration
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum AudioFormat {
        /// Unknown format
        Unspecified = 0,
        /// 32-bit float (little-endian)
        F32 = 1,
        /// 16-bit signed integer (little-endian)
        S16 = 2,
    }

    /// Data type hint for routing
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum DataTypeHint {
        /// Unspecified
        Unspecified = 0,
        /// Audio
        Audio = 1,
        /// Video
        Video = 2,
        /// Tensor
        Tensor = 3,
        /// JSON
        Json = 4,
        /// Text
        Text = 5,
        /// Binary
        Binary = 6,
        /// Any type
        Any = 7,
        /// File reference (spec 001)
        File = 8,
    }

    // Note: PixelFormat moved to data::video module (spec 012)

    /// Runtime data representation matching DataBuffer oneof types
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub enum RuntimeData {
        /// Audio samples (f32 PCM)
        Audio {
            /// Audio samples as f32
            samples: Vec<f32>,
            /// Sample rate in Hz
            sample_rate: u32,
            /// Number of channels (1=mono, 2=stereo)
            channels: u32,
            /// Optional stream identifier for multi-track routing (spec 013)
            /// When None, uses default track for backward compatibility
            #[serde(default, skip_serializing_if = "Option::is_none")]
            stream_id: Option<String>,
            /// Media timestamp in microseconds (spec 026)
            /// Represents the presentation timestamp of this audio chunk
            #[serde(default, skip_serializing_if = "Option::is_none")]
            timestamp_us: Option<u64>,
            /// Arrival timestamp in microseconds (spec 026)
            /// Set by transport ingest layer for drift monitoring
            #[serde(default, skip_serializing_if = "Option::is_none")]
            arrival_ts_us: Option<u64>,
        },
        /// Video frame
        Video {
            /// Pixel data (raw or encoded)
            /// - Raw: Depends on PixelFormat (e.g., YUV420P planar, RGB24 packed)
            /// - Encoded: Codec bitstream (VP8/AV1/H.264)
            pixel_data: Vec<u8>,
            /// Frame width in pixels
            width: u32,
            /// Frame height in pixels
            height: u32,
            /// Pixel format (Yuv420p, RGB24, etc., or Encoded for compressed)
            format: PixelFormat,
            /// Codec used (None for raw frames, Some(codec) for encoded)
            codec: Option<VideoCodec>,
            /// Sequential frame number (monotonic counter)
            frame_number: u64,
            /// Presentation timestamp in microseconds
            timestamp_us: u64,
            /// Keyframe indicator (true for I-frames, false for P/B-frames)
            is_keyframe: bool,
            /// Optional stream identifier for multi-track routing (spec 013)
            /// When None, uses default track for backward compatibility
            #[serde(default, skip_serializing_if = "Option::is_none")]
            stream_id: Option<String>,
            /// Arrival timestamp in microseconds (spec 026)
            /// Set by transport ingest layer for drift monitoring
            #[serde(default, skip_serializing_if = "Option::is_none")]
            arrival_ts_us: Option<u64>,
        },
    /// Tensor data
    Tensor {
        /// Flattened tensor data
        data: Vec<u8>,
        /// Tensor shape
        shape: Vec<i32>,
        /// Data type (0=float32, 1=int32, etc.)
        dtype: i32,
    },
    /// Numpy array data (zero-copy passthrough until IPC boundary)
    /// This allows numpy arrays to flow through the pipeline without conversion,
    /// only serializing once at the IPC boundary for iceoryx2 transport.
    Numpy {
        /// Raw array data
        data: Vec<u8>,
        /// Array shape (dimensions)
        shape: Vec<usize>,
        /// Data type string (e.g., "float32", "int16", "uint8")
        dtype: String,
        /// Array strides (bytes to step in each dimension)
        strides: Vec<isize>,
        /// Whether array is C-contiguous
        c_contiguous: bool,
        /// Whether array is Fortran-contiguous
        f_contiguous: bool,
    },
    /// JSON data
    Json(serde_json::Value),
        /// Text data
        Text(String),
        /// Binary data
        Binary(Vec<u8>),
        /// Control message for pipeline flow control (spec 007)
        ControlMessage {
            /// Type of control message
            message_type: ControlMessageType,
            /// Optional target segment ID for cancellation
            segment_id: Option<String>,
            /// Timestamp when message was created (milliseconds)
            timestamp_ms: u64,
            /// Extensible metadata
            metadata: serde_json::Value,
        },
        /// File reference with metadata and byte range support (spec 001)
        ///
        /// Represents a reference to a file on the local filesystem.
        /// Does NOT contain file contents - only metadata for referencing.
        ///
        /// # Example
        ///
        /// ```
        /// use remotemedia_runtime_core::data::RuntimeData;
        ///
        /// // Simple file reference
        /// let file = RuntimeData::File {
        ///     path: "/data/input/video.mp4".to_string(),
        ///     filename: Some("video.mp4".to_string()),
        ///     mime_type: Some("video/mp4".to_string()),
        ///     size: Some(104_857_600),  // 100 MB
        ///     offset: None,
        ///     length: None,
        ///     stream_id: None,
        /// };
        ///
        /// assert_eq!(file.data_type(), "file");
        /// ```
        File {
            /// File path (absolute or relative, UTF-8)
            path: String,

            /// Original filename (optional, preserved separately from path)
            #[serde(default, skip_serializing_if = "Option::is_none")]
            filename: Option<String>,

            /// MIME type hint (optional)
            #[serde(default, skip_serializing_if = "Option::is_none")]
            mime_type: Option<String>,

            /// File size in bytes (optional, None = unknown)
            #[serde(default, skip_serializing_if = "Option::is_none")]
            size: Option<u64>,

            /// Byte offset for range read/write (optional)
            #[serde(default, skip_serializing_if = "Option::is_none")]
            offset: Option<u64>,

            /// Number of bytes for range request (optional, None = to EOF)
            #[serde(default, skip_serializing_if = "Option::is_none")]
            length: Option<u64>,

            /// Stream identifier for multi-track routing
            #[serde(default, skip_serializing_if = "Option::is_none")]
            stream_id: Option<String>,
        },
    }

    impl RuntimeData {
        /// Get the type of this data as string
        pub fn data_type(&self) -> &str {
            match self {
                RuntimeData::Audio { .. } => "audio",
                RuntimeData::Video { .. } => "video",
                RuntimeData::Tensor { .. } => "tensor",
                RuntimeData::Numpy { .. } => "numpy",
                RuntimeData::Json(_) => "json",
                RuntimeData::Text(_) => "text",
                RuntimeData::Binary(_) => "binary",
                RuntimeData::ControlMessage { .. } => "control_message",
                RuntimeData::File { .. } => "file",
            }
        }

        /// Get timing information for drift monitoring (spec 026)
        ///
        /// Returns (media_timestamp_us, arrival_timestamp_us) for Audio and Video variants.
        /// For Audio, media timestamp comes from `timestamp_us` field.
        /// For Video, media timestamp comes from `timestamp_us` field (presentation timestamp).
        ///
        /// # Returns
        /// - `(Some(media_ts), Some(arrival_ts))` - Both timestamps available
        /// - `(Some(media_ts), None)` - Only media timestamp (arrival not stamped yet)
        /// - `(None, None)` - Not a timed media type or no timestamps set
        pub fn timing(&self) -> (Option<u64>, Option<u64>) {
            match self {
                RuntimeData::Audio {
                    timestamp_us,
                    arrival_ts_us,
                    ..
                } => (*timestamp_us, *arrival_ts_us),
                RuntimeData::Video {
                    timestamp_us,
                    arrival_ts_us,
                    ..
                } => (Some(*timestamp_us), *arrival_ts_us),
                _ => (None, None),
            }
        }

        /// Get stream identifier if present (spec 026)
        ///
        /// Returns the stream_id for Audio, Video, and File variants.
        /// Used for multi-track routing and per-stream drift monitoring.
        pub fn stream_id(&self) -> Option<&str> {
            match self {
                RuntimeData::Audio { stream_id, .. } => stream_id.as_deref(),
                RuntimeData::Video { stream_id, .. } => stream_id.as_deref(),
                RuntimeData::File { stream_id, .. } => stream_id.as_deref(),
                _ => None,
            }
        }

        /// Check if this is audio data (spec 026)
        pub fn is_audio(&self) -> bool {
            matches!(self, RuntimeData::Audio { .. })
        }

        /// Check if this is video data (spec 026)
        pub fn is_video(&self) -> bool {
            matches!(self, RuntimeData::Video { .. })
        }

        /// Check if this is a timed media type (audio or video)
        ///
        /// Timed media types have timestamps for drift monitoring.
        pub fn is_timed_media(&self) -> bool {
            self.is_audio() || self.is_video()
        }

        /// Set arrival timestamp for drift monitoring (spec 026)
        ///
        /// Should be called by transport ingest layer when data arrives.
        /// Only affects Audio and Video variants.
        ///
        /// # Returns
        /// `true` if timestamp was set, `false` if not applicable to this variant
        pub fn set_arrival_timestamp(&mut self, arrival_us: u64) -> bool {
            match self {
                RuntimeData::Audio { arrival_ts_us, .. } => {
                    *arrival_ts_us = Some(arrival_us);
                    true
                }
                RuntimeData::Video { arrival_ts_us, .. } => {
                    *arrival_ts_us = Some(arrival_us);
                    true
                }
                _ => false,
            }
        }

        /// Set media timestamp for audio (spec 026)
        ///
        /// Used to set presentation timestamp on audio chunks.
        /// Only affects Audio variant.
        ///
        /// # Returns
        /// `true` if timestamp was set, `false` if not an Audio variant
        pub fn set_audio_timestamp(&mut self, media_ts_us: u64) -> bool {
            match self {
                RuntimeData::Audio { timestamp_us, .. } => {
                    *timestamp_us = Some(media_ts_us);
                    true
                }
                _ => false,
            }
        }

        /// Get item count
        pub fn item_count(&self) -> usize {
            match self {
                RuntimeData::Audio { samples, .. } => samples.len(),
                RuntimeData::Video { .. } => 1,
                RuntimeData::Tensor { data, .. } => data.len(),
                RuntimeData::Numpy { shape, .. } => shape.iter().product(),
                RuntimeData::Json(value) => match value {
                    serde_json::Value::Array(arr) => arr.len(),
                    serde_json::Value::Object(obj) => obj.len(),
                    _ => 1,
                },
                RuntimeData::Text(s) => s.len(),
                RuntimeData::Binary(b) => b.len(),
                RuntimeData::ControlMessage { .. } => 1,
                RuntimeData::File { .. } => 1, // One file reference
            }
        }

        /// Get memory size in bytes
        pub fn size_bytes(&self) -> usize {
            match self {
                RuntimeData::Audio { samples, .. } => samples.len() * 4,
                RuntimeData::Video { pixel_data, .. } => pixel_data.len(),
                RuntimeData::Tensor { data, .. } => data.len(),
                RuntimeData::Numpy { data, shape, dtype, strides, .. } => {
                    // Size includes data + metadata overhead
                    let data_size = data.len();
                    let metadata_size = shape.len() * 8 + strides.len() * 8 + dtype.len() + 10;
                    data_size + metadata_size
                }
                RuntimeData::Json(value) => {
                    serde_json::to_string(value).map(|s| s.len()).unwrap_or(0)
                }
                RuntimeData::Text(s) => s.len(),
                RuntimeData::Binary(b) => b.len(),
                RuntimeData::ControlMessage {
                    segment_id,
                    metadata,
                    ..
                } => {
                    // Approximate size: type + timestamp + segment_id + metadata
                    let segment_id_size = segment_id.as_ref().map(|s| s.len()).unwrap_or(0);
                    let metadata_size = serde_json::to_string(metadata)
                        .map(|s| s.len())
                        .unwrap_or(0);
                    std::mem::size_of::<ControlMessageType>() + 8 + segment_id_size + metadata_size
                }
                RuntimeData::File {
                    path,
                    filename,
                    mime_type,
                    stream_id,
                    ..
                } => {
                    // Approximate memory footprint of the reference (not file contents)
                    path.len()
                        + filename.as_ref().map(|s| s.len()).unwrap_or(0)
                        + mime_type.as_ref().map(|s| s.len()).unwrap_or(0)
                        + stream_id.as_ref().map(|s| s.len()).unwrap_or(0)
                        + 24 // 3 u64 fields (size, offset, length)
                }
            }
        }

        /// Validate video frame structure (spec 012)
        ///
        /// Checks that video frame dimensions and buffer sizes are consistent
        ///
        /// # Errors
        ///
        /// Returns error if:
        /// - Width or height is 0
        /// - YUV format has odd dimensions
        /// - Buffer size doesn't match expected size for pixel format
        pub fn validate_video_frame(&self) -> Result<(), String> {
            match self {
                RuntimeData::Video {
                    width,
                    height,
                    format,
                    pixel_data,
                    ..
                } => {
                    // Check dimensions
                    if *width == 0 || *height == 0 {
                        return Err("Invalid dimensions: width and height must be > 0".to_string());
                    }

                    // Check even dimensions for YUV formats
                    if format.requires_even_dimensions() && (*width % 2 != 0 || *height % 2 != 0) {
                        return Err("YUV formats require even dimensions".to_string());
                    }

                    // Check buffer size (skip for encoded frames with variable size)
                    let expected_size = format.buffer_size(*width, *height);
                    if expected_size > 0 && pixel_data.len() != expected_size {
                        return Err(format!(
                            "Buffer size mismatch: expected {}, got {}",
                            expected_size,
                            pixel_data.len()
                        ));
                    }

                    Ok(())
                }
                _ => Err("Not a video frame".to_string()),
            }
        }
    }

    /// Audio buffer (standalone struct for nodes)
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct AudioBuffer {
        /// Raw audio samples (bytes for f32)
        pub samples: Vec<u8>,
        /// Sample rate in Hz
        pub sample_rate: u32,
        /// Number of channels
        pub channels: u32,
        /// Audio format
        pub format: i32,
        /// Number of samples
        pub num_samples: u64,
    }

    /// Video frame (standalone struct for nodes)
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct VideoFrame {
        /// Pixel data (raw or encoded)
        pub pixel_data: Vec<u8>,
        /// Frame width in pixels
        pub width: u32,
        /// Frame height in pixels
        pub height: u32,
        /// Pixel format
        pub format: PixelFormat,
        /// Codec used (None for raw frames)
        pub codec: Option<VideoCodec>,
        /// Sequential frame number
        pub frame_number: u64,
        /// Presentation timestamp in microseconds
        pub timestamp_us: u64,
        /// Keyframe indicator
        pub is_keyframe: bool,
    }

    /// Tensor buffer (standalone struct for nodes)
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct TensorBuffer {
        /// Tensor data (bytes)
        pub data: Vec<u8>,
        /// Tensor shape
        pub shape: Vec<i32>,
        /// Data type
        pub dtype: i32,
    }
}

/// Manifest parsing module
pub mod manifest;

// Error types
mod error;
pub use error::{Error, Result};

// Re-export attribute macros (always available since derive is default)
pub use remotemedia_runtime_core_derive::node_config;
pub use remotemedia_runtime_core_derive::node;

/// Initialize the RemoteMedia runtime core
///
/// This should be called once at startup to initialize logging.
pub fn init() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("RemoteMedia Runtime Core initialized");
    Ok(())
}

// ============================================================================
// Backward Compatibility Re-exports (v0.3 → v0.4 Migration)
// ============================================================================
//
// These re-exports allow existing code to gradually migrate from the old
// monolithic structure to the new decoupled architecture.
//
// Example migration path:
// ```rust
// // Old (v0.3.x):
// use remotemedia_runtime::grpc_service::GrpcServer;
// use remotemedia_runtime::executor::Executor;
//
// // Transitional (v0.4.x with compat):
// use remotemedia_runtime_core::executor::Executor;  // Still works
// // BUT GrpcServer now in: use remotemedia_grpc::GrpcServer;
//
// // New (v0.4.x+):
// use remotemedia_runtime_core::transport::PipelineRunner;
// use remotemedia_grpc::GrpcServer;
// ```
//
// These re-exports will be marked deprecated in v0.5 and removed in v1.0.

/// Backward compatibility: Core execution types remain in runtime-core
///
/// **Migration Note**: Continue using from `remotemedia_runtime_core::executor`
pub mod executor_compat {
    pub use crate::executor::*;
}

/// Backward compatibility: Data types remain in runtime-core
///
/// **Migration Note**: Continue using from `remotemedia_runtime_core::data`
pub mod data_compat {
    pub use crate::data::*;
}

/// Backward compatibility: Manifest types remain in runtime-core
///
/// **Migration Note**: Continue using from `remotemedia_runtime_core::manifest`
pub mod manifest_compat {
    pub use crate::manifest::*;
}

/// Backward compatibility: Node types remain in runtime-core
///
/// **Migration Note**: Continue using from `remotemedia_runtime_core::nodes`
pub mod nodes_compat {
    pub use crate::nodes::*;
}

// NOTE: gRPC-specific types (GrpcServer, StreamingServiceImpl, ExecutionServiceImpl)
// have been moved to the `remotemedia-grpc` crate and are NOT re-exported here.
// Users must update imports:
//   OLD: use remotemedia_runtime::grpc_service::GrpcServer;
//   NEW: use remotemedia_grpc::GrpcServer;

#[cfg(test)]
mod tests {
    use super::*;
    use data::video::{PixelFormat, VideoCodec};

    #[test]
    fn test_init() {
        // Should not panic
        init().ok();
    }

    // T041: Unit tests for RuntimeData::Video validation
    #[test]
    fn test_video_frame_validation_valid() {
        // Valid 720p YUV420P frame
        let frame = data::RuntimeData::Video {
            pixel_data: vec![128u8; 1_382_400],  // 1280*720*1.5
            width: 1280,
            height: 720,
            format: PixelFormat::Yuv420p,
            codec: None,
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: false,
            stream_id: None,
            arrival_ts_us: None,
        };

        assert!(frame.validate_video_frame().is_ok());
    }

    #[test]
    fn test_video_frame_validation_zero_dimensions() {
        let frame = data::RuntimeData::Video {
            pixel_data: vec![],
            width: 0,  // Invalid
            height: 720,
            format: PixelFormat::Yuv420p,
            codec: None,
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: false,
            stream_id: None,
            arrival_ts_us: None,
        };

        let result = frame.validate_video_frame();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("width and height must be > 0"));
    }

    #[test]
    fn test_video_frame_validation_odd_dimensions_yuv() {
        // YUV formats require even dimensions
        let frame = data::RuntimeData::Video {
            pixel_data: vec![128u8; 100],
            width: 1281,  // Odd width
            height: 720,
            format: PixelFormat::Yuv420p,
            codec: None,
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: false,
            stream_id: None,
            arrival_ts_us: None,
        };

        let result = frame.validate_video_frame();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("even dimensions"));
    }

    #[test]
    fn test_video_frame_validation_buffer_size_mismatch() {
        // Buffer size doesn't match format
        let frame = data::RuntimeData::Video {
            pixel_data: vec![128u8; 1000],  // Wrong size for 1280x720 YUV420P
            width: 1280,
            height: 720,
            format: PixelFormat::Yuv420p,
            codec: None,
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: false,
            stream_id: None,
            arrival_ts_us: None,
        };

        let result = frame.validate_video_frame();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Buffer size mismatch"));
    }

    #[test]
    fn test_video_frame_validation_rgb24() {
        // Valid RGB24 frame (odd dimensions OK)
        let frame = data::RuntimeData::Video {
            pixel_data: vec![0u8; 1920 * 1081 * 3],  // Odd height OK for RGB
            width: 1920,
            height: 1081,  // Odd height
            format: PixelFormat::Rgb24,
            codec: None,
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: false,
            stream_id: None,
            arrival_ts_us: None,
        };

        assert!(frame.validate_video_frame().is_ok());
    }

    #[test]
    fn test_video_frame_validation_encoded_variable_size() {
        // Encoded frames have variable size (validation skipped)
        let frame = data::RuntimeData::Video {
            pixel_data: vec![0u8; 5000],  // Variable encoded size
            width: 1280,
            height: 720,
            format: PixelFormat::Encoded,
            codec: Some(VideoCodec::Vp8),
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: true,
            stream_id: None,
            arrival_ts_us: None,
        };

        assert!(frame.validate_video_frame().is_ok());
    }

    // spec 026: Tests for RuntimeData timing methods
    #[test]
    fn test_runtime_data_timing_audio() {
        let audio = data::RuntimeData::Audio {
            samples: vec![0.0; 100],
            sample_rate: 44100,
            channels: 1,
            stream_id: Some("audio_main".to_string()),
            timestamp_us: Some(1_000_000),
            arrival_ts_us: Some(1_001_000),
        };

        let (media_ts, arrival_ts) = audio.timing();
        assert_eq!(media_ts, Some(1_000_000));
        assert_eq!(arrival_ts, Some(1_001_000));
        assert_eq!(audio.stream_id(), Some("audio_main"));
        assert!(audio.is_audio());
        assert!(!audio.is_video());
        assert!(audio.is_timed_media());
    }

    #[test]
    fn test_runtime_data_timing_video() {
        let video = data::RuntimeData::Video {
            pixel_data: vec![0u8; 1000],
            width: 100,
            height: 100,
            format: PixelFormat::Rgb24,
            codec: None,
            frame_number: 0,
            timestamp_us: 2_000_000,
            is_keyframe: true,
            stream_id: Some("video_main".to_string()),
            arrival_ts_us: Some(2_001_000),
        };

        let (media_ts, arrival_ts) = video.timing();
        assert_eq!(media_ts, Some(2_000_000));
        assert_eq!(arrival_ts, Some(2_001_000));
        assert_eq!(video.stream_id(), Some("video_main"));
        assert!(!video.is_audio());
        assert!(video.is_video());
        assert!(video.is_timed_media());
    }

    #[test]
    fn test_runtime_data_timing_non_media() {
        let text = data::RuntimeData::Text("hello".to_string());

        let (media_ts, arrival_ts) = text.timing();
        assert_eq!(media_ts, None);
        assert_eq!(arrival_ts, None);
        assert_eq!(text.stream_id(), None);
        assert!(!text.is_audio());
        assert!(!text.is_video());
        assert!(!text.is_timed_media());
    }

    #[test]
    fn test_runtime_data_set_timestamps() {
        let mut audio = data::RuntimeData::Audio {
            samples: vec![0.0; 100],
            sample_rate: 44100,
            channels: 1,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
        };

        // Set arrival timestamp
        assert!(audio.set_arrival_timestamp(5_000_000));
        let (_, arrival_ts) = audio.timing();
        assert_eq!(arrival_ts, Some(5_000_000));

        // Set media timestamp
        assert!(audio.set_audio_timestamp(4_000_000));
        let (media_ts, _) = audio.timing();
        assert_eq!(media_ts, Some(4_000_000));

        // Non-audio types should return false
        let mut text = data::RuntimeData::Text("hello".to_string());
        assert!(!text.set_arrival_timestamp(1000));
        assert!(!text.set_audio_timestamp(1000));
    }

    // T010-T013: Unit tests for RuntimeData::File (spec 001)
    #[test]
    fn test_file_data_type() {
        let file = data::RuntimeData::File {
            path: "/data/input/video.mp4".to_string(),
            filename: Some("video.mp4".to_string()),
            mime_type: Some("video/mp4".to_string()),
            size: Some(104_857_600),
            offset: None,
            length: None,
            stream_id: None,
        };

        assert_eq!(file.data_type(), "file");
    }

    #[test]
    fn test_file_item_count() {
        let file = data::RuntimeData::File {
            path: "/data/input/video.mp4".to_string(),
            filename: None,
            mime_type: None,
            size: None,
            offset: None,
            length: None,
            stream_id: None,
        };

        assert_eq!(file.item_count(), 1);
    }

    #[test]
    fn test_file_size_bytes() {
        let file = data::RuntimeData::File {
            path: "/data/input/video.mp4".to_string(),
            filename: Some("video.mp4".to_string()),
            mime_type: Some("video/mp4".to_string()),
            size: Some(104_857_600),
            offset: None,
            length: None,
            stream_id: Some("main".to_string()),
        };

        // path(21) + filename(9) + mime_type(9) + stream_id(4) + 24 (3 u64s) = 67
        assert_eq!(file.size_bytes(), 67);
    }

    #[test]
    fn test_file_with_all_fields() {
        let file = data::RuntimeData::File {
            path: "/data/input/video.mp4".to_string(),
            filename: Some("video.mp4".to_string()),
            mime_type: Some("video/mp4".to_string()),
            size: Some(104_857_600),
            offset: Some(1024 * 1024),      // 1 MB offset
            length: Some(64 * 1024),        // 64 KB chunk
            stream_id: Some("video_track".to_string()),
        };

        assert_eq!(file.data_type(), "file");
        assert_eq!(file.item_count(), 1);
    }

    #[test]
    fn test_file_with_only_path() {
        // Minimal file reference with only required field
        let file = data::RuntimeData::File {
            path: "/tmp/output.bin".to_string(),
            filename: None,
            mime_type: None,
            size: None,
            offset: None,
            length: None,
            stream_id: None,
        };

        assert_eq!(file.data_type(), "file");
        assert_eq!(file.item_count(), 1);
        // path(15) + 24 (3 u64s) = 39
        assert_eq!(file.size_bytes(), 39);
    }

    #[test]
    fn test_file_serde_serialization() {
        let file = data::RuntimeData::File {
            path: "/data/input/video.mp4".to_string(),
            filename: Some("video.mp4".to_string()),
            mime_type: Some("video/mp4".to_string()),
            size: Some(104_857_600),
            offset: None,
            length: None,
            stream_id: None,
        };

        // Test serialization
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("File"));
        assert!(json.contains("/data/input/video.mp4"));
        assert!(json.contains("video.mp4"));
        assert!(json.contains("video/mp4"));
        assert!(json.contains("104857600"));

        // Test deserialization roundtrip
        let deserialized: data::RuntimeData = serde_json::from_str(&json).unwrap();
        assert_eq!(file, deserialized);
    }

    #[test]
    fn test_file_serde_skip_none_fields() {
        // File with minimal fields should have compact serialization
        let file = data::RuntimeData::File {
            path: "/tmp/test.txt".to_string(),
            filename: None,
            mime_type: None,
            size: None,
            offset: None,
            length: None,
            stream_id: None,
        };

        let json = serde_json::to_string(&file).unwrap();
        // None fields should be omitted due to skip_serializing_if
        assert!(!json.contains("filename"));
        assert!(!json.contains("mime_type"));
        assert!(!json.contains("offset"));
        assert!(!json.contains("length"));
        assert!(!json.contains("stream_id"));

        // Roundtrip should still work
        let deserialized: data::RuntimeData = serde_json::from_str(&json).unwrap();
        assert_eq!(file, deserialized);
    }

    #[test]
    fn test_file_byte_range_fields() {
        // Test byte range request
        let range_request = data::RuntimeData::File {
            path: "/data/large_file.bin".to_string(),
            filename: None,
            mime_type: None,
            size: Some(1_073_741_824), // 1 GB
            offset: Some(10 * 1024 * 1024), // 10 MB offset
            length: Some(64 * 1024),        // 64 KB chunk
            stream_id: None,
        };

        assert_eq!(range_request.data_type(), "file");

        // Verify serialization includes offset and length
        let json = serde_json::to_string(&range_request).unwrap();
        assert!(json.contains("10485760"));  // offset
        assert!(json.contains("65536"));     // length
    }

    #[test]
    fn test_data_type_hint_file() {
        assert_eq!(data::DataTypeHint::File as i32, 8);
    }
}
