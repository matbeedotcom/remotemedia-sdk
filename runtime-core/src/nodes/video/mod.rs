//! Video processing nodes
//!
//! This module provides video encoding, decoding, and processing nodes
//! for the RemoteMedia SDK pipeline architecture.
//!
//! See spec 012: Video Codec Support (AV1/VP8/AVC)

pub mod codec;
pub mod encoder;
pub mod decoder;
pub mod scaler;
pub mod format_converter;

// Re-export encoder and decoder nodes (T020-T027 complete)
pub use encoder::{VideoEncoderConfig, VideoEncoderNode};
pub use decoder::{VideoDecoderConfig, VideoDecoderNode};

// Phase 6: Video processing nodes (T084-T095)
pub use scaler::{VideoScalerNode, VideoScalerConfig};
// pub use format_converter::{VideoFormatConverterNode, VideoFormatConverterConfig};
