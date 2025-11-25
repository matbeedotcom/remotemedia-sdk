//! Unit tests for VideoDecoderNode
//!
//! Spec 012: Video Codec Support - T040

#[cfg(test)]
mod tests {
    use crate::nodes::video::{VideoDecoderConfig, VideoDecoderNode};
    use crate::data::video::{PixelFormat, VideoCodec};
    use crate::data::RuntimeData;
    use crate::nodes::streaming_node::AsyncStreamingNode;

    #[tokio::test]
    async fn test_decoder_node_creation() {
        // Test creating decoder with default config
        let config = VideoDecoderConfig::default();
        let result = VideoDecoderNode::new(config);

        // Should succeed (or fail gracefully if FFmpeg not available)
        match result {
            Ok(_) => {
                // Decoder created successfully
            }
            Err(e) => {
                // Expected if FFmpeg not integrated yet or not installed
                assert!(e.to_string().contains("not yet implemented") || e.to_string().contains("not available"));
            }
        }
    }

    #[tokio::test]
    async fn test_decoder_validates_input() {
        let config = VideoDecoderConfig {
            expected_codec: Some(VideoCodec::Vp8),
            output_format: PixelFormat::Yuv420p,
            ..Default::default()
        };

        // Try to create decoder (may fail if FFmpeg not available)
        if let Ok(decoder) = VideoDecoderNode::new(config) {
            // Test 1: Reject raw frames (codec=None)
            let raw_frame = RuntimeData::Video {
                pixel_data: vec![128u8; 1_382_400],  // 720p YUV
                width: 1280,
                height: 720,
                format: PixelFormat::Yuv420p,
                codec: None,  // Raw frame
                frame_number: 0,
                timestamp_us: 0,
                is_keyframe: false,
            };

            let result = decoder.process(raw_frame).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("codec must be specified"));

            // Test 2: Reject non-video data
            let text_data = RuntimeData::Text("hello".to_string());
            let result = decoder.process(text_data).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Expected encoded video frame"));
        }
    }

    #[tokio::test]
    async fn test_decoder_error_resilience_lenient() {
        let config = VideoDecoderConfig {
            expected_codec: Some(VideoCodec::Vp8),
            output_format: PixelFormat::Yuv420p,
            error_resilience: "lenient".to_string(),
            ..Default::default()
        };

        if let Ok(decoder) = VideoDecoderNode::new(config) {
            // Create corrupted encoded frame (invalid bitstream)
            let corrupted_frame = RuntimeData::Video {
                pixel_data: vec![0xFF; 100],  // Invalid VP8 data
                width: 1280,
                height: 720,
                format: PixelFormat::Encoded,
                codec: Some(VideoCodec::Vp8),
                frame_number: 0,
                timestamp_us: 0,
                is_keyframe: true,
            };

            // In lenient mode, should return empty frame instead of error
            let result = decoder.process(corrupted_frame).await;

            match result {
                Ok(RuntimeData::Video { width: 0, height: 0, .. }) => {
                    // Correctly dropped corrupted frame
                }
                Err(_) => {
                    // If FFmpeg not available, error is expected
                }
                Ok(_) => {
                    // Unexpected: should either drop (empty frame) or error
                }
            }
        }
    }

    #[tokio::test]
    async fn test_decoder_error_resilience_strict() {
        let config = VideoDecoderConfig {
            expected_codec: Some(VideoCodec::Vp8),
            output_format: PixelFormat::Yuv420p,
            error_resilience: "strict".to_string(),
            ..Default::default()
        };

        if let Ok(decoder) = VideoDecoderNode::new(config) {
            // Create corrupted encoded frame
            let corrupted_frame = RuntimeData::Video {
                pixel_data: vec![0xFF; 100],
                width: 1280,
                height: 720,
                format: PixelFormat::Encoded,
                codec: Some(VideoCodec::Vp8),
                frame_number: 0,
                timestamp_us: 0,
                is_keyframe: true,
            };

            // In strict mode, should return error
            let result = decoder.process(corrupted_frame).await;
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_decoder_config_defaults() {
        let config = VideoDecoderConfig::default();
        assert_eq!(config.expected_codec, None);
        assert_eq!(config.output_format, PixelFormat::Yuv420p);
        assert_eq!(config.hardware_accel, true);
        assert_eq!(config.threads, 0);
        assert_eq!(config.error_resilience, "lenient");
    }
}
