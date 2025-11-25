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

/// FFmpeg encoder implementation (ready for ac-ffmpeg integration)
///
/// The implementation creates a functional encoder that returns mock encoded frames.
/// To enable actual encoding, integrate ac-ffmpeg API in the encode() method.
#[cfg(feature = "video")]
pub struct FFmpegEncoder {
    config: VideoEncoderConfig,
    frame_count: u64,
    // TODO: Add ac-ffmpeg encoder state when integrating:
    // encoder: ac_ffmpeg::codec::video::Encoder,
}

#[cfg(feature = "video")]
impl FFmpegEncoder {
    /// Create a new FFmpeg encoder
    ///
    /// # Arguments
    /// * `config` - Encoder configuration
    ///
    /// # Returns
    /// * `Ok(Self)` - Initialized encoder
    /// * `Err(CodecError::NotAvailable)` - FFmpeg not found or codec unavailable
    pub fn new(config: VideoEncoderConfig) -> Result<Self> {
        // TODO: Initialize ac-ffmpeg encoder here
        // Example:
        // let codec_name = match config.codec {
        //     VideoCodec::Vp8 => "libvpx",
        //     VideoCodec::H264 => "libx264",
        //     VideoCodec::Av1 => "libaom-av1",
        // };
        // let encoder = ac_ffmpeg::codec::video::Encoder::builder(codec_name)?
        //     .width(config.width)
        //     .height(config.height)
        //     .bitrate(config.bitrate)
        //     .framerate(config.framerate)
        //     .build()?;

        Ok(Self {
            config,
            frame_count: 0,
        })
    }
}

#[cfg(feature = "video")]
impl VideoEncoderBackend for FFmpegEncoder {
    fn encode(&mut self, input: RuntimeData) -> Result<RuntimeData> {
        // Extract raw video frame
        let (pixel_data, width, height, format, frame_number, timestamp_us) = match input {
            RuntimeData::Video {
                pixel_data,
                width,
                height,
                format,
                codec: None,
                frame_number,
                timestamp_us,
                ..
            } => (pixel_data, width, height, format, frame_number, timestamp_us),
            RuntimeData::Video { codec: Some(_), .. } => {
                return Err(CodecError::InvalidInput("Frame is already encoded".to_string()));
            }
            _ => {
                return Err(CodecError::InvalidInput("Expected video frame".to_string()));
            }
        };

        // Validate pixel format is supported
        if format == PixelFormat::Encoded {
            return Err(CodecError::InvalidInput("Cannot encode already-encoded frame".to_string()));
        }

        // TODO: Complete ac-ffmpeg integration
        //
        // Integration steps:
        //
        // 1. Create VideoFrameMut from pixel_data:
        //    let frame = ac_ffmpeg::codec::video::VideoFrameMut::new(
        //        format_to_ac_ffmpeg(format),
        //        width as usize,
        //        height as usize,
        //    );
        //    frame.planes_mut()[0].copy_from_slice(&pixel_data[...]);  // Y plane
        //    // Copy U and V planes similarly for YUV formats
        //
        // 2. Encode the frame:
        //    let encoder = self.encoder.as_mut().unwrap();
        //    let packets = encoder.encode_frame(&frame)
        //        .map_err(|e| CodecError::EncodingFailed(e.to_string()))?;
        //
        // 3. Extract bitstream from packets:
        //    let mut bitstream = Vec::new();
        //    for packet in packets {
        //        bitstream.extend_from_slice(packet.data());
        //    }
        //
        // 4. Determine if keyframe based on packet flags
        //
        // References:
        // - ac-ffmpeg docs: https://docs.rs/ac-ffmpeg/latest/ac_ffmpeg/codec/video/
        // - GitHub examples: https://github.com/angelcam/rust-ac-ffmpeg/tree/master/examples
        //
        // For now, return mock encoded frame to enable end-to-end testing
        let is_keyframe = self.frame_count % self.config.keyframe_interval as u64 == 0;
        self.frame_count += 1;

        Ok(RuntimeData::Video {
            pixel_data: vec![0x00, 0x01, 0x02],  // Mock bitstream
            width,
            height,
            format: PixelFormat::Encoded,
            codec: Some(self.config.codec),
            frame_number,
            timestamp_us,
            is_keyframe,
        })
    }

    fn codec(&self) -> VideoCodec {
        self.config.codec
    }

    fn reconfigure(&mut self, config: &VideoEncoderConfig) -> Result<()> {
        self.config = config.clone();
        // TODO: Reconfigure ac-ffmpeg encoder with new settings
        Ok(())
    }
}

/// FFmpeg decoder implementation (ready for ac-ffmpeg integration)
///
/// The implementation creates a functional decoder that returns mock decoded frames.
/// To enable actual decoding, integrate ac-ffmpeg API in the decode() method.
#[cfg(feature = "video")]
pub struct FFmpegDecoder {
    config: VideoDecoderConfig,
    // TODO: Add ac-ffmpeg decoder state when integrating:
    // decoder: ac_ffmpeg::codec::video::Decoder,
}

#[cfg(feature = "video")]
impl FFmpegDecoder {
    /// Create a new FFmpeg decoder
    ///
    /// # Arguments
    /// * `config` - Decoder configuration
    ///
    /// # Returns
    /// * `Ok(Self)` - Initialized decoder
    /// * `Err(CodecError::NotAvailable)` - FFmpeg not found or codec unavailable
    pub fn new(config: VideoDecoderConfig) -> Result<Self> {
        // TODO: Initialize ac-ffmpeg decoder here
        // Example:
        // let codec_name = match config.expected_codec {
        //     Some(VideoCodec::Vp8) => "vp8",
        //     Some(VideoCodec::H264) => "h264",
        //     Some(VideoCodec::Av1) => "av1",
        //     None => "vp8",  // Auto-detect
        // };
        // let decoder = ac_ffmpeg::codec::video::Decoder::new(codec_name)?;

        Ok(Self { config })
    }
}

#[cfg(feature = "video")]
impl VideoDecoderBackend for FFmpegDecoder {
    fn decode(&mut self, input: RuntimeData) -> Result<RuntimeData> {
        // Extract encoded video frame
        let (pixel_data, width, height, codec, frame_number, timestamp_us) = match input {
            RuntimeData::Video {
                pixel_data,
                width,
                height,
                codec: Some(codec),
                frame_number,
                timestamp_us,
                ..
            } => (pixel_data, width, height, codec, frame_number, timestamp_us),
            RuntimeData::Video { codec: None, .. } => {
                return Err(CodecError::InvalidInput("Frame is not encoded".to_string()));
            }
            _ => {
                return Err(CodecError::InvalidInput("Expected video frame".to_string()));
            }
        };

        // Validate codec matches expected
        if let Some(expected) = self.config.expected_codec {
            if codec != expected {
                return Err(CodecError::InvalidInput(format!(
                    "Codec mismatch: expected {:?}, got {:?}",
                    expected, codec
                )));
            }
        }

        // TODO: Complete ac-ffmpeg integration
        //
        // Integration steps:
        //
        // 1. Create packet from bitstream:
        //    let packet = ac_ffmpeg::codec::Packet::new(&pixel_data);
        //
        // 2. Decode the packet:
        //    let decoder = self.decoder.as_mut().unwrap();
        //    let frames = decoder.decode_packet(&packet)
        //        .map_err(|e| CodecError::DecodingFailed(e.to_string()))?;
        //
        // 3. Extract first frame:
        //    let frame = frames.into_iter().next()
        //        .ok_or_else(|| CodecError::DecodingFailed("No frame decoded".to_string()))?;
        //
        // 4. Convert frame to output pixel format if needed:
        //    let scaler = VideoFrameScaler::builder()
        //        .source_pixel_format(frame.pixel_format())
        //        .target_pixel_format(format_to_ac_ffmpeg(self.config.output_format))
        //        .build()?;
        //    let scaled_frame = scaler.scale(&frame)?;
        //
        // 5. Copy pixel data from frame planes:
        //    let mut pixel_data = Vec::new();
        //    for plane in scaled_frame.planes() {
        //        pixel_data.extend_from_slice(plane);
        //    }
        //
        // References:
        // - ac-ffmpeg docs: https://docs.rs/ac-ffmpeg/latest/ac_ffmpeg/codec/video/
        // - GitHub: https://github.com/angelcam/rust-ac-ffmpeg
        //
        // For now, return mock decoded frame to enable end-to-end testing
        let output_size = self.config.output_format.buffer_size(width, height);
        let decoded_pixel_data = vec![128u8; output_size];  // Mock gray frame

        Ok(RuntimeData::Video {
            pixel_data: decoded_pixel_data,
            width,
            height,
            format: self.config.output_format,
            codec: None,  // Decoded = raw frame
            frame_number,
            timestamp_us,
            is_keyframe: false,
        })
    }

    fn codec(&self) -> VideoCodec {
        self.config.expected_codec.unwrap_or(VideoCodec::Vp8)
    }

    fn output_format(&self) -> PixelFormat {
        self.config.output_format
    }
}
