//! Video codec backend abstractions
//!
//! Provides traits and implementations for video encoding/decoding backends.
//! Primary backend: ac-ffmpeg (FFmpeg bindings)
//! Optional: rav1e (pure Rust AV1 encoder)
//!
//! ## ac-ffmpeg Integration Status
//!
//! The codec implementations below provide functional mock encoding/decoding
//! that enables end-to-end testing of the video pipeline architecture.
//!
//! To enable actual FFmpeg encoding/decoding, implement the TODOs in:
//! - FFmpegEncoder::encode() - Use ac-ffmpeg VideoEncoder
//! - FFmpegDecoder::decode() - Use ac-ffmpeg VideoDecoder
//!
//! FFmpeg 7.1.1 is installed with libvpx (VP8), libx264 (H.264), and libaom/librav1e (AV1).
//!
//! Key ac-ffmpeg API elements (from source inspection):
//! - VideoDecoder::new(codec_name) or VideoDecoder::builder(codec_name).build()
//! - VideoEncoder::builder(codec_name).pixel_format(fmt).width(w).height(h).time_base(tb).bit_rate(br).build()
//! - PixelFormat: parse from strings "yuv420p", "rgb24", "nv12", etc.
//! - VideoFrameMut::black(format, width, height) - Create frame, use planes_mut()[i].data_mut() to write pixels
//! - decoder.push(packet), decoder.take() -> Option<VideoFrame>
//! - encoder.push(frame), encoder.take() -> Option<Packet>
//! - Packet data access: packet is from demuxer or needs PacketMut construction
//!
//! References:
//! - Source: ~/.cargo/registry/src/.../ac-ffmpeg-0.19.0/src/codec/video/
//! - Docs: https://docs.rs/ac-ffmpeg/0.19.0
//! - Examples: https://github.com/angelcam/rust-ac-ffmpeg/tree/master/examples

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

/// FFmpeg encoder implementation
///
/// Currently returns mock encoded frames. To enable actual encoding:
/// 1. Use VideoEncoder::builder() from ac-ffmpeg
/// 2. Create VideoFrameMut from pixel_data
/// 3. Push frame, take packets
/// 4. Return bitstream as encoded RuntimeData
#[cfg(feature = "video")]
pub struct FFmpegEncoder {
    config: VideoEncoderConfig,
    frame_count: u64,
}

#[cfg(feature = "video")]
impl FFmpegEncoder {
    pub fn new(config: VideoEncoderConfig) -> Result<Self> {
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

        if format == PixelFormat::Encoded {
            return Err(CodecError::InvalidInput("Cannot encode already-encoded frame".to_string()));
        }

        // Mock implementation for testing
        // TODO: Actual ac-ffmpeg integration:
        //
        // 1. Create encoder (lazy init on first frame):
        //    let codec_name = match self.config.codec {
        //        VideoCodec::Vp8 => "libvpx",
        //        VideoCodec::H264 => "libx264",
        //        VideoCodec::Av1 => "libaom-av1",
        //    };
        //    let pixel_format: ac_ffmpeg::codec::video::PixelFormat = "yuv420p".parse()?;
        //    let time_base = ac_ffmpeg::time::TimeBase::new(1, self.config.framerate as i32);
        //    let encoder = ac_ffmpeg::codec::video::VideoEncoder::builder(codec_name)?
        //        .pixel_format(pixel_format)
        //        .width(width as usize)
        //        .height(height as usize)
        //        .time_base(time_base)
        //        .bit_rate(self.config.bitrate as u64)
        //        .build()?;
        //
        // 2. Create VideoFrameMut and copy pixel data:
        //    let mut frame = ac_ffmpeg::codec::video::VideoFrameMut::black(pixel_format, width as usize, height as usize);
        //    let planes_mut = frame.planes_mut();
        //    // For YUV420P: copy Y, U, V planes from pixel_data
        //    planes_mut[0].data_mut().copy_from_slice(&pixel_data[0..y_size]);
        //    planes_mut[1].data_mut().copy_from_slice(&pixel_data[y_size..y_size+uv_size]);
        //    planes_mut[2].data_mut().copy_from_slice(&pixel_data[y_size+uv_size..]);
        //
        // 3. Encode:
        //    encoder.push(frame.freeze())?;
        //    while let Some(packet) = encoder.take()? {
        //        bitstream.extend_from_slice(packet data);
        //        is_keyframe |= packet.is_key();
        //    }

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
        Ok(())
    }
}

/// FFmpeg decoder implementation
///
/// Currently returns mock decoded frames. To enable actual decoding:
/// 1. Use VideoDecoder::new() from ac-ffmpeg
/// 2. Create Packet from bitstream (may need PacketMut)
/// 3. Push packet, take frames
/// 4. Copy frame planes to RuntimeData
#[cfg(feature = "video")]
pub struct FFmpegDecoder {
    config: VideoDecoderConfig,
}

#[cfg(feature = "video")]
impl FFmpegDecoder {
    pub fn new(config: VideoDecoderConfig) -> Result<Self> {
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

        if let Some(expected) = self.config.expected_codec {
            if codec != expected {
                return Err(CodecError::InvalidInput(format!(
                    "Codec mismatch: expected {:?}, got {:?}",
                    expected, codec
                )));
            }
        }

        // Mock implementation for testing
        // TODO: Actual ac-ffmpeg integration:
        //
        // 1. Create decoder (lazy init):
        //    let codec_name = match codec {
        //        VideoCodec::Vp8 => "vp8",
        //        VideoCodec::H264 => "h264",
        //        VideoCodec::Av1 => "av1",
        //    };
        //    let decoder = ac_ffmpeg::codec::video::VideoDecoder::new(codec_name)?;
        //
        // 2. Create packet from bitstream:
        //    // Packets typically come from demuxer, for raw bitstream may need:
        //    let mut packet_mut = ac_ffmpeg::packet::PacketMut::new(pixel_data.len());
        //    packet_mut.data_mut().copy_from_slice(&pixel_data);
        //    // Note: Need to figure out how to convert PacketMut -> Packet
        //
        // 3. Decode:
        //    decoder.push(packet)?;
        //    if let Some(frame) = decoder.take()? {
        //        // Copy frame planes to output
        //        let planes = frame.planes();
        //        for plane in planes {
        //            output_data.extend_from_slice(plane.data());
        //        }
        //    }
        //
        // 4. Convert pixel format if needed using VideoFrameScaler

        let output_size = self.config.output_format.buffer_size(width, height);
        let decoded_pixel_data = vec![128u8; output_size];  // Mock gray frame

        Ok(RuntimeData::Video {
            pixel_data: decoded_pixel_data,
            width,
            height,
            format: self.config.output_format,
            codec: None,
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
