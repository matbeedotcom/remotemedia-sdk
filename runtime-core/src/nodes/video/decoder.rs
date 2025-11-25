//! Video decoder node implementation
//!
//! Decodes compressed bitstreams to raw video frames

use crate::data::video::{PixelFormat, VideoCodec};
use crate::data::RuntimeData;
use crate::nodes::streaming_node::AsyncStreamingNode;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use tracing::warn;

use super::codec::{CodecError, FFmpegDecoder, VideoDecoderBackend};

/// Configuration for video decoding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoDecoderConfig {
    /// Expected codec (for validation)
    /// None means auto-detect from bitstream
    pub expected_codec: Option<VideoCodec>,

    /// Output pixel format
    /// Decoder will convert to this format
    /// Default: Yuv420p (most efficient)
    pub output_format: PixelFormat,

    /// Enable hardware acceleration
    /// Default: true
    pub hardware_accel: bool,

    /// Number of threads for decoding
    /// 0 means auto-detect
    /// Default: 0
    pub threads: u32,

    /// Error resilience mode
    /// - "strict": Fail on any bitstream error
    /// - "lenient": Attempt to decode partial/corrupted frames
    /// Default: "lenient" for real-time streams
    pub error_resilience: String,
}

impl Default for VideoDecoderConfig {
    fn default() -> Self {
        Self {
            expected_codec: None,
            output_format: PixelFormat::Yuv420p,
            hardware_accel: true,
            threads: 0,
            error_resilience: "lenient".to_string(),
        }
    }
}

/// Video decoder node for decoding compressed video streams
///
/// Holds a thread-safe reference to the underlying decoder backend
/// and provides async decoding via `tokio::spawn_blocking`.
pub struct VideoDecoderNode {
    /// Thread-safe decoder backend
    decoder: Arc<Mutex<Box<dyn VideoDecoderBackend>>>,
    /// Decoder configuration
    config: VideoDecoderConfig,
}

impl VideoDecoderNode {
    /// Create a new video decoder node
    ///
    /// # Arguments
    /// * `config` - Decoder configuration
    ///
    /// # Returns
    /// * `Ok(Self)` - Initialized decoder node
    /// * `Err(CodecError)` - Initialization failure
    pub fn new(config: VideoDecoderConfig) -> Result<Self, CodecError> {
        // Create FFmpeg decoder backend
        let decoder = FFmpegDecoder::new(config.clone())?;
        Ok(Self {
            decoder: Arc::new(Mutex::new(Box::new(decoder))),
            config,
        })
    }

    /// Decode a video frame asynchronously
    ///
    /// Uses `tokio::spawn_blocking` to run the expensive decoding operation
    /// on a blocking thread pool, preventing it from blocking the async executor.
    ///
    /// # Arguments
    /// * `input` - Encoded video frame (RuntimeData::Video with codec=Some(...))
    ///
    /// # Returns
    /// * `Ok(RuntimeData)` - Decoded raw frame with codec=None
    /// * `Err(CodecError)` - Decoding failure
    pub async fn decode_frame(&self, input: RuntimeData) -> Result<RuntimeData, CodecError> {
        let decoder = Arc::clone(&self.decoder);
        tokio::task::spawn_blocking(move || {
            let mut dec = decoder.lock().unwrap();
            dec.decode(input)
        })
        .await
        .map_err(|e| CodecError::DecodingFailed(e.to_string()))?
    }
}

#[async_trait]
impl AsyncStreamingNode for VideoDecoderNode {
    fn node_type(&self) -> &str {
        "VideoDecoder"
    }

    async fn process(&self, input: RuntimeData) -> Result<RuntimeData, Error> {
        // Validate input is encoded frame
        match &input {
            RuntimeData::Video { codec: Some(_), .. } => {
                // Expected: encoded frame with codec specified
            }
            RuntimeData::Video { codec: None, .. } => {
                return Err(Error::Execution(
                    "Expected encoded video frame for decoding (codec must be specified)".to_string(),
                ));
            }
            _ => {
                return Err(Error::Execution(
                    "Expected encoded video frame for decoding".to_string(),
                ));
            }
        }

        // Decode frame with error resilience
        match self.decode_frame(input).await {
            Ok(frame) => Ok(frame),
            Err(e) if self.config.error_resilience == "lenient" => {
                warn!("Dropped corrupted frame: {}", e);
                Ok(RuntimeData::Video {
                    pixel_data: vec![],
                    width: 0,
                    height: 0,
                    format: PixelFormat::Unspecified,
                    codec: None,
                    frame_number: 0,
                    timestamp_us: 0,
                    is_keyframe: false,
                })
            }
            Err(e) => Err(Error::Execution(format!("Decoding failed: {}", e))),
        }
    }

    async fn process_multi(
        &self,
        mut inputs: std::collections::HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
        // Extract first input and process
        if let Some((_name, data)) = inputs.into_iter().next() {
            self.process(data).await
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    fn is_multi_input(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "decoder_tests.rs"]
mod tests;
