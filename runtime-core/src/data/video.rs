//! Video codec data types and utilities
//!
//! This module provides core video data structures for the RemoteMedia SDK,
//! including pixel formats, video codecs, and frame metadata.
//!
//! See spec 012: Video Codec Support (AV1/VP8/AVC)

use serde::{Deserialize, Serialize};

/// Pixel format for video frames
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PixelFormat {
    /// Unknown/unspecified format
    Unspecified = 0,

    /// YUV 4:2:0 planar (standard codec format)
    /// Layout: Y plane (width*height), U plane (width/2 * height/2), V plane (width/2 * height/2)
    /// Memory: width * height * 3/2 bytes
    /// Use case: Codec input/output, efficient storage
    Yuv420p = 1,

    /// I420 (identical to YUV420P, alternate name for WebRTC compat)
    I420 = 2,

    /// NV12 (semi-planar, Y plane + interleaved UV)
    /// Layout: Y plane (width*height), UV plane (width * height/2)
    /// Memory: width * height * 3/2 bytes
    /// Use case: Hardware acceleration (common GPU format)
    NV12 = 3,

    /// RGB24 (packed 24-bit RGB)
    /// Layout: Packed RGBRGBRGB... (no padding)
    /// Memory: width * height * 3 bytes
    /// Use case: Image processing, display, Python interop (PIL/OpenCV)
    Rgb24 = 4,

    /// RGBA32 (packed 32-bit RGBA with alpha)
    /// Layout: Packed RGBARGBARGBA... (4-byte aligned)
    /// Memory: width * height * 4 bytes
    /// Use case: Compositing, transparency, browser rendering
    Rgba32 = 5,

    /// Encoded bitstream (not raw pixels)
    /// Used when pixel_data contains codec-specific compressed data
    Encoded = 255,
}

impl PixelFormat {
    /// Calculate expected buffer size in bytes
    pub fn buffer_size(&self, width: u32, height: u32) -> usize {
        match self {
            PixelFormat::Yuv420p | PixelFormat::I420 | PixelFormat::NV12 => {
                (width * height * 3 / 2) as usize
            }
            PixelFormat::Rgb24 => (width * height * 3) as usize,
            PixelFormat::Rgba32 => (width * height * 4) as usize,
            PixelFormat::Encoded | PixelFormat::Unspecified => 0, // Variable or unknown
        }
    }

    /// Check if format requires even dimensions (YUV formats)
    pub fn requires_even_dimensions(&self) -> bool {
        matches!(
            self,
            PixelFormat::Yuv420p | PixelFormat::I420 | PixelFormat::NV12
        )
    }
}

/// Video codec type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum VideoCodec {
    /// VP8 (WebM, WebRTC standard)
    /// RFC 6386, royalty-free
    Vp8 = 1,

    /// H.264/AVC (Baseline/Main/High profile)
    /// ITU-T H.264, widely supported
    H264 = 2,

    /// AV1 (next-gen royalty-free codec)
    /// Higher compression than VP8, lower than HEVC
    Av1 = 3,
}

impl VideoCodec {
    /// MIME type for WebRTC/gRPC
    pub fn mime_type(&self) -> &'static str {
        match self {
            VideoCodec::Vp8 => "video/VP8",
            VideoCodec::H264 => "video/H264",
            VideoCodec::Av1 => "video/AV1",
        }
    }

    /// RTP payload type (WebRTC)
    pub fn rtp_payload_type(&self) -> u8 {
        match self {
            VideoCodec::Vp8 => 96,   // Dynamic payload type
            VideoCodec::H264 => 102,
            VideoCodec::Av1 => 104,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format_buffer_size() {
        assert_eq!(PixelFormat::Yuv420p.buffer_size(1280, 720), 1_382_400);
        assert_eq!(PixelFormat::Rgb24.buffer_size(1280, 720), 2_764_800);
        assert_eq!(PixelFormat::Rgba32.buffer_size(1280, 720), 3_686_400);
    }

    #[test]
    fn test_pixel_format_even_dimensions() {
        assert!(PixelFormat::Yuv420p.requires_even_dimensions());
        assert!(PixelFormat::I420.requires_even_dimensions());
        assert!(PixelFormat::NV12.requires_even_dimensions());
        assert!(!PixelFormat::Rgb24.requires_even_dimensions());
        assert!(!PixelFormat::Rgba32.requires_even_dimensions());
    }

    #[test]
    fn test_video_codec_mime_types() {
        assert_eq!(VideoCodec::Vp8.mime_type(), "video/VP8");
        assert_eq!(VideoCodec::H264.mime_type(), "video/H264");
        assert_eq!(VideoCodec::Av1.mime_type(), "video/AV1");
    }

    #[test]
    fn test_video_codec_rtp_payload_types() {
        assert_eq!(VideoCodec::Vp8.rtp_payload_type(), 96);
        assert_eq!(VideoCodec::H264.rtp_payload_type(), 102);
        assert_eq!(VideoCodec::Av1.rtp_payload_type(), 104);
    }
}
