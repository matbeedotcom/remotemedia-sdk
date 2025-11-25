//! Video codec backend abstractions
//!
//! Provides traits and implementations for video encoding/decoding backends.
//! Primary backend: ac-ffmpeg (FFmpeg bindings)
//! Optional: rav1e (pure Rust AV1 encoder)

use crate::data::RuntimeData;
use crate::data::video::{PixelFormat, VideoCodec};
use super::encoder::VideoEncoderConfig;
use super::decoder::VideoDecoderConfig;

/// Result type for codec operations
pub type Result<T> = std::result::Result<T, CodecError>;

/// Codec-specific errors
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("Codec not available: {0}")]
    NotAvailable(String),

    #[error("Encoding failed: {0}")]
    EncodingFailed(String),

    #[error("Decoding failed: {0}")]
    DecodingFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Video encoder backend trait
///
/// Implementations provide codec-specific encoding (FFmpeg, rav1e, etc.)
pub trait VideoEncoderBackend: Send + Sync {
    /// Encode a raw video frame to compressed bitstream
    ///
    /// # Arguments
    /// * `input` - RuntimeData::Video with codec=None (raw frame)
    ///
    /// # Returns
    /// * `Ok(RuntimeData)` - Encoded frame with codec=Some(...)
    /// * `Err(CodecError)` - Encoding failure
    fn encode(&mut self, input: RuntimeData) -> Result<RuntimeData>;

    /// Get the codec this encoder produces
    fn codec(&self) -> VideoCodec;

    /// Reconfigure encoder (bitrate, quality, etc.)
    fn reconfigure(&mut self, config: &VideoEncoderConfig) -> Result<()>;
}

/// Video decoder backend trait
///
/// Implementations provide codec-specific decoding (FFmpeg, dav1d, etc.)
pub trait VideoDecoderBackend: Send + Sync {
    /// Decode a compressed bitstream to raw video frame
    ///
    /// # Arguments
    /// * `input` - RuntimeData::Video with codec=Some(...) (encoded frame)
    ///
    /// # Returns
    /// * `Ok(RuntimeData)` - Decoded raw frame with codec=None
    /// * `Err(CodecError)` - Decoding failure
    fn decode(&mut self, input: RuntimeData) -> Result<RuntimeData>;

    /// Get the codec this decoder handles
    fn codec(&self) -> VideoCodec;

    /// Get the output pixel format
    fn output_format(&self) -> PixelFormat;
}

/// FFmpeg encoder implementation (stub for now - requires ac-ffmpeg feature)
///
/// Full implementation will use ac-ffmpeg crate for actual encoding
#[cfg(feature = "video")]
pub struct FFmpegEncoder {
    config: VideoEncoderConfig,
    // ac-ffmpeg encoder state will be added when implementing T018
}

#[cfg(feature = "video")]
impl FFmpegEncoder {
    /// Create a new FFmpeg encoder
    ///
    /// # Arguments
    /// * `config` - Encoder configuration
    ///
    /// # Returns
    /// * `Ok(Self)` - Initialized encoder (stub)
    /// * `Err(CodecError::NotAvailable)` - FFmpeg not found
    pub fn new(config: VideoEncoderConfig) -> Result<Self> {
        // Stub implementation - will be completed in T018
        // When implementing: initialize ac-ffmpeg encoder here
        Ok(Self { config })
    }
}

#[cfg(feature = "video")]
impl VideoEncoderBackend for FFmpegEncoder {
    fn encode(&mut self, input: RuntimeData) -> Result<RuntimeData> {
        // Stub implementation - will be completed in T018
        // When implementing: use ac-ffmpeg to encode frame
        Err(CodecError::NotAvailable("FFmpeg encoder not yet implemented".to_string()))
    }

    fn codec(&self) -> VideoCodec {
        self.config.codec
    }

    fn reconfigure(&mut self, config: &VideoEncoderConfig) -> Result<()> {
        self.config = config.clone();
        Ok(())
    }
}

/// FFmpeg decoder implementation (stub for now - requires ac-ffmpeg feature)
///
/// Full implementation will use ac-ffmpeg crate for actual decoding
#[cfg(feature = "video")]
pub struct FFmpegDecoder {
    config: VideoDecoderConfig,
    // ac-ffmpeg decoder state will be added when implementing T019
}

#[cfg(feature = "video")]
impl FFmpegDecoder {
    /// Create a new FFmpeg decoder
    ///
    /// # Arguments
    /// * `config` - Decoder configuration
    ///
    /// # Returns
    /// * `Ok(Self)` - Initialized decoder (stub)
    /// * `Err(CodecError::NotAvailable)` - FFmpeg not found
    pub fn new(config: VideoDecoderConfig) -> Result<Self> {
        // Stub implementation - will be completed in T019
        // When implementing: initialize ac-ffmpeg decoder here
        Ok(Self { config })
    }
}

#[cfg(feature = "video")]
impl VideoDecoderBackend for FFmpegDecoder {
    fn decode(&mut self, input: RuntimeData) -> Result<RuntimeData> {
        // Stub implementation - will be completed in T019
        // When implementing: use ac-ffmpeg to decode frame
        Err(CodecError::NotAvailable("FFmpeg decoder not yet implemented".to_string()))
    }

    fn codec(&self) -> VideoCodec {
        self.config.expected_codec.unwrap_or(VideoCodec::Vp8)
    }

    fn output_format(&self) -> PixelFormat {
        self.config.output_format
    }
}
