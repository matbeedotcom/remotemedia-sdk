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
//! ```ignore
//! use remotemedia_runtime_core::transport::PipelineRunner;
//! use remotemedia_runtime_core::transport::TransportData;
//! use remotemedia_runtime_core::data::RuntimeData;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let runner = PipelineRunner::new()?;
//!
//!     let manifest = Arc::new(load_manifest()?);
//!     let input = TransportData::new(RuntimeData::Text("hello".into()));
//!
//!     let output = runner.execute_unary(manifest, input).await?;
//!     println!("Result: {:?}", output.data);
//!     Ok(())
//! }
//! ```

#![warn(clippy::all)]
#![allow(clippy::arc_with_non_send_sync)] // iceoryx2 types are intentionally !Send

// Core execution modules
pub mod audio;
pub mod executor;
pub mod nodes;
pub mod python;
/// Public entrypoint for ergonomic registration macros.
pub mod registration_macros {
    pub use crate::{
        register_python_node, register_python_nodes, register_rust_node, register_rust_node_default,
    };
}

// Manifest
pub use manifest::Manifest;

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
        };

        assert!(frame.validate_video_frame().is_ok());
    }
}
