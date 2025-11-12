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

    /// Pixel format for video frames
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum PixelFormat {
        /// Unknown format
        Unspecified = 0,
        /// RGB24 (packed)
        Rgb24 = 1,
        /// RGBA32 (packed)
        Rgba32 = 2,
        /// YUV420P (planar)
        Yuv420p = 3,
    }

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
            /// Pixel data
            pixel_data: Vec<u8>,
            /// Frame width
            width: u32,
            /// Frame height
            height: u32,
            /// Pixel format (0=unspecified, 1=RGB24, 2=RGBA32, 3=YUV420P)
            format: i32,
            /// Frame number/sequence
            frame_number: u64,
            /// Timestamp in microseconds
            timestamp_us: u64,
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
                RuntimeData::Json(value) => {
                    serde_json::to_string(value).map(|s| s.len()).unwrap_or(0)
                }
                RuntimeData::Text(s) => s.len(),
                RuntimeData::Binary(b) => b.len(),
                RuntimeData::ControlMessage { segment_id, metadata, .. } => {
                    // Approximate size: type + timestamp + segment_id + metadata
                    let segment_id_size = segment_id.as_ref().map(|s| s.len()).unwrap_or(0);
                    let metadata_size = serde_json::to_string(metadata).map(|s| s.len()).unwrap_or(0);
                    std::mem::size_of::<ControlMessageType>() + 8 + segment_id_size + metadata_size
                }
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
        /// Pixel data
        pub pixel_data: Vec<u8>,
        /// Frame width
        pub width: u32,
        /// Frame height
        pub height: u32,
        /// Pixel format
        pub format: i32,
        /// Frame number
        pub frame_number: u64,
        /// Timestamp in microseconds
        pub timestamp_us: u64,
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

    #[test]
    fn test_init() {
        // Should not panic
        init().ok();
    }
}
