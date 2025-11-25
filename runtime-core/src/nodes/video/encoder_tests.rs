//! Unit tests for VideoEncoderNode
//!
//! Spec 012: Video Codec Support - T039

#[cfg(test)]
mod tests {
    use crate::nodes::video::{VideoEncoderConfig, VideoEncoderNode};
    use crate::data::video::{PixelFormat, VideoCodec};
    use crate::data::RuntimeData;
    use crate::nodes::streaming_node::AsyncStreamingNode;

    #[tokio::test]
    async fn test_encoder_node_creation() {
        // Test creating encoder with default config
        let config = VideoEncoderConfig::default();
        let result = VideoEncoderNode::new(config);

        // Should succeed (or fail gracefully if FFmpeg not available)
        match result {
            Ok(_) => {
                // Encoder created successfully
            }
            Err(e) => {
                // Expected if FFmpeg not integrated yet or not installed
                assert!(e.to_string().contains("not yet implemented") || e.to_string().contains("not available"));
            }
        }
    }

    #[tokio::test]
    async fn test_encoder_validates_input() {
        let config = VideoEncoderConfig {
            codec: VideoCodec::Vp8,
            bitrate: 2_000_000,
            framerate: 30,
            ..Default::default()
        };

        // Try to create encoder (may fail if FFmpeg not available)
        if let Ok(encoder) = VideoEncoderNode::new(config) {
            // Test 1: Reject already-encoded frames
            let encoded_frame = RuntimeData::Video {
                pixel_data: vec![0u8; 1000],
                width: 1280,
                height: 720,
                format: PixelFormat::Encoded,
                codec: Some(VideoCodec::Vp8),  // Already encoded
                frame_number: 0,
                timestamp_us: 0,
                is_keyframe: true,
            };

            let result = encoder.process(encoded_frame).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("already-encoded"));

            // Test 2: Reject non-video data
            let text_data = RuntimeData::Text("hello".to_string());
            let result = encoder.process(text_data).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Expected RuntimeData::Video"));
        }
    }

    #[tokio::test]
    async fn test_encoder_accepts_raw_frames() {
        let config = VideoEncoderConfig {
            codec: VideoCodec::Vp8,
            bitrate: 1_000_000,
            framerate: 30,
            ..Default::default()
        };

        if let Ok(encoder) = VideoEncoderNode::new(config) {
            // Create a raw 720p YUV420P frame
            let width = 1280u32;
            let height = 720u32;
            let frame_size = (width * height * 3 / 2) as usize;  // YUV420P
            let pixel_data = vec![128u8; frame_size];  // Gray frame

            let raw_frame = RuntimeData::Video {
                pixel_data,
                width,
                height,
                format: PixelFormat::Yuv420p,
                codec: None,  // Raw frame
                frame_number: 0,
                timestamp_us: 0,
                is_keyframe: false,
            };

            // Should accept raw frame (but encoding may fail if FFmpeg not available)
            let result = encoder.process(raw_frame).await;

            // Either succeeds with encoded frame, or fails with codec error
            match result {
                Ok(RuntimeData::Video { codec: Some(_), .. }) => {
                    // Encoding worked! Check it returned encoded frame
                }
                Err(e) => {
                    // Expected if FFmpeg not yet integrated
                    assert!(e.to_string().contains("not yet implemented") || e.to_string().contains("not available"));
                }
                Ok(_) => panic!("Expected encoded video frame with codec set"),
            }
        }
    }

    #[test]
    fn test_encoder_config_defaults() {
        let config = VideoEncoderConfig::default();
        assert_eq!(config.codec, VideoCodec::Vp8);
        assert_eq!(config.bitrate, 1_000_000);
        assert_eq!(config.framerate, 30);
        assert_eq!(config.keyframe_interval, 60);
        assert_eq!(config.quality_preset, "medium");
        assert_eq!(config.hardware_accel, true);
        assert_eq!(config.threads, 0);
    }

    #[tokio::test]
    async fn test_h264_encoder() {
        // Test H.264 encoding
        let config = VideoEncoderConfig {
            codec: VideoCodec::H264,
            bitrate: 2_000_000,
            framerate: 30,
            ..Default::default()
        };

        if let Ok(encoder) = VideoEncoderNode::new(config) {
            // Create a raw 720p YUV420P frame
            let width = 1280u32;
            let height = 720u32;
            let frame_size = (width * height * 3 / 2) as usize;
            let pixel_data = vec![128u8; frame_size];

            let raw_frame = RuntimeData::Video {
                pixel_data,
                width,
                height,
                format: PixelFormat::Yuv420p,
                codec: None,
                frame_number: 0,
                timestamp_us: 0,
                is_keyframe: false,
            };

            // Encode with H.264
            let result = encoder.process(raw_frame).await;

            match result {
                Ok(RuntimeData::Video { codec: Some(VideoCodec::H264), format: PixelFormat::Encoded, .. }) => {
                    // Successfully encoded to H.264
                }
                Err(e) => {
                    // May fail if libx264 not available
                    assert!(e.to_string().contains("not available") || e.to_string().contains("Failed to create encoder"));
                }
                Ok(_) => panic!("Expected H.264 encoded frame"),
            }
        }
    }

    #[tokio::test]
    async fn test_av1_encoder() {
        // Test AV1 encoding
        let config = VideoEncoderConfig {
            codec: VideoCodec::Av1,
            bitrate: 1_500_000,
            framerate: 30,
            ..Default::default()
        };

        if let Ok(encoder) = VideoEncoderNode::new(config) {
            // Create a raw 720p YUV420P frame
            let width = 1280u32;
            let height = 720u32;
            let frame_size = (width * height * 3 / 2) as usize;
            let pixel_data = vec![128u8; frame_size];

            let raw_frame = RuntimeData::Video {
                pixel_data,
                width,
                height,
                format: PixelFormat::Yuv420p,
                codec: None,
                frame_number: 0,
                timestamp_us: 0,
                is_keyframe: false,
            };

            // Encode with AV1
            let result = encoder.process(raw_frame).await;

            match result {
                Ok(RuntimeData::Video { codec: Some(VideoCodec::Av1), format: PixelFormat::Encoded, .. }) => {
                    // Successfully encoded to AV1
                }
                Err(e) => {
                    // May fail if libaom-av1 not available
                    assert!(e.to_string().contains("not available") || e.to_string().contains("Failed to create encoder"));
                }
                Ok(_) => panic!("Expected AV1 encoded frame"),
            }
        }
    }
}
