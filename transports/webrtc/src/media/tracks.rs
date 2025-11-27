//! Media tracks for WebRTC
//!
//! Handles RTP track management, encoding, and decoding.

// Phase 4 (US2) media track infrastructure
#![allow(dead_code)]

use super::audio::{AudioEncoder, AudioEncoderConfig};
use super::audio_sender::AudioSender;
use super::video::{VideoFormat, VideoFrame};
use crate::{Error, Result};
use remotemedia_runtime_core::data::video::{PixelFormat, VideoCodec};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::nodes::video::{
    VideoDecoderConfig, VideoDecoderNode, VideoEncoderConfig, VideoEncoderNode,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use webrtc::media::Sample;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

/// Audio track for WebRTC
///
/// Manages audio encoding/decoding and RTP transmission.
pub struct AudioTrack {
    /// Underlying WebRTC track
    track: Arc<TrackLocalStaticSample>,

    /// Audio encoder
    encoder: Arc<RwLock<AudioEncoder>>,

    /// Audio decoder (for receiving audio from remote peer)
    decoder: Arc<RwLock<super::audio::AudioDecoder>>,

    /// Audio sender with ring buffer (for smooth real-time transmission)
    sender: Arc<RwLock<Option<AudioSender>>>,

    /// RTP timestamp (in sample units) - kept for compatibility but sender manages its own
    timestamp: Arc<RwLock<u32>>,
}

impl AudioTrack {
    /// Create a new audio track
    ///
    /// # Arguments
    ///
    /// * `track` - Underlying WebRTC track
    /// * `config` - Audio encoder configuration
    pub fn new(track: Arc<TrackLocalStaticSample>, config: AudioEncoderConfig) -> Result<Self> {
        println!("[AUDIOTRACK] Creating new AudioTrack with ring buffer support!");
        let encoder = Arc::new(RwLock::new(AudioEncoder::new(config.clone())?));
        let decoder = Arc::new(RwLock::new(super::audio::AudioDecoder::new(
            config.clone(),
        )?));

        // Create audio sender with ring buffer
        // Large buffer allows TTS to generate audio in bursts without blocking
        println!("[AUDIOTRACK] About to create AudioSender...");
        let sender = AudioSender::new(Arc::clone(&track), config.ring_buffer_capacity);
        println!("[AUDIOTRACK] AudioSender created!");

        Ok(Self {
            track,
            encoder,
            decoder,
            sender: Arc::new(RwLock::new(Some(sender))),
            timestamp: Arc::new(RwLock::new(0)),
        })
    }

    /// Send audio samples over RTP
    ///
    /// # Arguments
    ///
    /// * `samples` - Audio samples as f32 (range -1.0 to 1.0)
    /// * `sample_rate` - Sample rate of the audio in Hz
    ///
    /// # Note
    ///
    /// This method encodes audio to Opus and enqueues frames into a ring buffer.
    /// A dedicated thread continuously dequeues frames and sends them at real-time pace.
    /// Opus requires specific frame sizes (2.5, 5, 10, 20, 40, or 60ms).
    /// We chunk the input into 20ms frames (320 @ 16kHz, 480 @ 24kHz, 960 @ 48kHz).
    /// The encoder will be recreated if the sample rate changes.
    ///
    /// ARCHITECTURE:
    /// - Production (this method): Encode frames as fast as possible, enqueue to ring buffer
    /// - Transmission (dedicated thread): Dequeue frames and send at real-time pace (20ms intervals)
    /// - This decouples TTS generation speed from playback speed, preventing interruptions
    pub async fn send_audio(&self, samples: Arc<Vec<f32>>, sample_rate: u32) -> Result<()> {
        use tracing::info;

        // Check if encoder needs to be recreated for different sample rate
        {
            let encoder = self.encoder.read().await;
            if encoder.config.sample_rate != sample_rate {
                drop(encoder);
                let old_rate = self.encoder.read().await.config.sample_rate;
                info!(
                    "Sample rate changed from {} to {} Hz, recreating encoder",
                    old_rate, sample_rate
                );

                let mut encoder_write = self.encoder.write().await;
                let new_config = crate::media::audio::AudioEncoderConfig {
                    sample_rate,
                    channels: encoder_write.config.channels,
                    bitrate: encoder_write.config.bitrate,
                    complexity: encoder_write.config.complexity,
                    ring_buffer_capacity: encoder_write.config.ring_buffer_capacity,
                };
                *encoder_write = crate::media::audio::AudioEncoder::new(new_config)?;
                info!("Encoder recreated with sample rate: {} Hz", sample_rate);
            }
        }

        let frame_size = (sample_rate as usize * 20) / 1000; // 20ms frame
        let frame_duration = Duration::from_millis(20);

        info!(
            "AudioTrack: Enqueuing {} samples as {}sample frames @ {}Hz (duration: {:.2}s)",
            samples.len(),
            frame_size,
            sample_rate,
            samples.len() as f64 / sample_rate as f64
        );

        let sender_guard = self.sender.read().await;
        let sender = sender_guard
            .as_ref()
            .ok_or_else(|| Error::MediaTrackError("AudioSender not initialized".to_string()))?;

        let mut frames_enqueued = 0;

        // Process audio in chunks and enqueue frames
        for chunk in samples.chunks(frame_size) {
            // Opus requires exact frame sizes - pad last chunk if needed
            let samples_to_encode: Vec<f32> = if chunk.len() < frame_size {
                let mut padded = chunk.to_vec();
                padded.resize(frame_size, 0.0); // Pad with silence
                padded
            } else {
                chunk.to_vec()
            };

            // Encode this chunk
            let encoded = self.encoder.write().await.encode(&samples_to_encode)?;

            // Enqueue frame into ring buffer (non-blocking)
            sender
                .enqueue_frame(encoded, chunk.len() as u32, frame_duration)
                .await?;
            frames_enqueued += 1;
        }

        info!(
            "AudioTrack: Enqueued {} frames into ring buffer (buffer size: {})",
            frames_enqueued,
            sender.buffer_len()
        );

        Ok(())
    }

    /// Get the underlying WebRTC track
    pub fn track(&self) -> Arc<TrackLocalStaticSample> {
        Arc::clone(&self.track)
    }

    /// Decode received RTP packet to audio samples
    ///
    /// # Arguments
    ///
    /// * `payload` - RTP payload (Opus encoded data)
    ///
    /// # Returns
    ///
    /// Decoded audio samples as f32 (range -1.0 to 1.0) at 48kHz
    ///
    /// # Note
    ///
    /// This method decodes incoming Opus RTP payloads from the remote peer.
    /// Used for bidirectional audio (VAD, STT, etc.).
    pub async fn on_rtp_packet(&self, payload: &[u8]) -> Result<Vec<f32>> {
        use tracing::debug;

        debug!("Decoding RTP packet with {} bytes", payload.len());

        // Decode the Opus payload
        let samples = self.decoder.write().await.decode(payload)?;

        debug!("Decoded {} samples from RTP packet", samples.len());

        Ok(samples)
    }

    /// Get current RTP timestamp
    pub async fn timestamp(&self) -> u32 {
        // Use the sender's timestamp if available (more accurate for ring buffer approach)
        if let Some(sender) = self.sender.read().await.as_ref() {
            sender.timestamp()
        } else {
            *self.timestamp.read().await
        }
    }

    /// Shutdown the audio track and wait for sender thread to complete
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(sender) = self.sender.write().await.take() {
            sender.shutdown().await?;
        }
        Ok(())
    }
}

/// Video track for WebRTC
///
/// Manages video encoding/decoding and RTP transmission using runtime-core video nodes.
pub struct VideoTrack {
    /// Underlying WebRTC track
    track: Arc<TrackLocalStaticSample>,

    /// Video encoder (runtime-core VideoEncoderNode for VP8/H.264/AV1)
    encoder: Arc<VideoEncoderNode>,

    /// Video decoder (runtime-core VideoDecoderNode for VP8/H.264/AV1)
    decoder: Arc<VideoDecoderNode>,

    /// RTP sequence number
    sequence_number: Arc<RwLock<u16>>,

    /// RTP timestamp
    timestamp: Arc<RwLock<u32>>,

    /// Video codec being used
    codec: VideoCodec,
}

impl VideoTrack {
    /// Create a new video track with runtime-core encoder/decoder
    ///
    /// # Arguments
    ///
    /// * `track` - Underlying WebRTC track
    /// * `codec` - Video codec to use (VP8, H.264, or AV1)
    /// * `width` - Frame width in pixels
    /// * `height` - Frame height in pixels
    /// * `bitrate` - Target bitrate in bits/second
    /// * `framerate` - Target framerate
    pub fn new(
        track: Arc<TrackLocalStaticSample>,
        codec: VideoCodec,
        _width: u32,
        _height: u32,
        bitrate: u32,
        framerate: u32,
    ) -> Result<Self> {
        // Create runtime-core encoder
        let encoder_config = VideoEncoderConfig {
            codec,
            bitrate,
            framerate,
            keyframe_interval: 60,
            quality_preset: "medium".to_string(),
            hardware_accel: true,
            threads: 0,
        };
        let encoder = VideoEncoderNode::new(encoder_config)
            .map_err(|e| Error::EncodingError(format!("Failed to create video encoder: {}", e)))?;

        // Create runtime-core decoder
        let decoder_config = VideoDecoderConfig {
            expected_codec: Some(codec),
            output_format: PixelFormat::Yuv420p,
            hardware_accel: true,
            threads: 0,
            error_resilience: "lenient".to_string(),
        };
        let decoder = VideoDecoderNode::new(decoder_config)
            .map_err(|e| Error::EncodingError(format!("Failed to create video decoder: {}", e)))?;

        Ok(Self {
            track,
            encoder: Arc::new(encoder),
            decoder: Arc::new(decoder),
            sequence_number: Arc::new(RwLock::new(0)),
            timestamp: Arc::new(RwLock::new(0)),
            codec,
        })
    }

    /// Send video frame over RTP (legacy VideoFrame API)
    ///
    /// # Arguments
    ///
    /// * `frame` - WebRTC VideoFrame
    ///
    /// # Note
    ///
    /// Converts VideoFrame to RuntimeData, encodes with runtime-core, sends via RTP.
    /// For new code, use `send_video_runtime_data` directly with RuntimeData::Video.
    pub async fn send_video(&self, frame: &VideoFrame) -> Result<()> {
        // Convert VideoFrame to RuntimeData
        let runtime_data = RuntimeData::Video {
            pixel_data: frame.data.clone(),
            width: frame.width,
            height: frame.height,
            format: match frame.format {
                VideoFormat::I420 => PixelFormat::I420,
                VideoFormat::NV12 => PixelFormat::NV12,
                VideoFormat::RGB24 => PixelFormat::Rgb24,
            },
            codec: None, // Raw frame
            frame_number: 0,
            timestamp_us: frame.timestamp_us,
            is_keyframe: frame.is_keyframe,
            stream_id: None,
        };

        self.send_video_runtime_data(runtime_data).await
    }

    /// Send video frame over RTP (encodes using runtime-core VideoEncoderNode)
    ///
    /// # Arguments
    ///
    /// * `runtime_data` - Raw video frame (RuntimeData::Video with codec=None)
    ///
    /// # Note
    ///
    /// Encodes raw frames using runtime-core VideoEncoderNode (VP8/H.264/AV1 via ac-ffmpeg),
    /// then sends via WebRTC RTP with proper timestamp handling (90kHz clock).
    ///
    /// Phase 5 T067, T078: RTP transmission with 90kHz timestamp mapping
    pub async fn send_video_runtime_data(&self, runtime_data: RuntimeData) -> Result<()> {
        use remotemedia_runtime_core::nodes::streaming_node::AsyncStreamingNode;

        // Encode raw frame using runtime-core VideoEncoderNode
        let encoded = self
            .encoder
            .process(runtime_data)
            .await
            .map_err(|e| Error::EncodingError(format!("Video encoding failed: {}", e)))?;

        // Extract encoded bitstream
        let bitstream = match &encoded {
            RuntimeData::Video {
                pixel_data,
                codec: Some(_),
                ..
            } => pixel_data.clone(),
            _ => {
                return Err(Error::EncodingError(
                    "Expected encoded video frame".to_string(),
                ))
            }
        };

        // Update sequence number
        let mut seq = self.sequence_number.write().await;
        *seq = seq.wrapping_add(1);

        // Update timestamp (90kHz clock for video)
        let mut ts = self.timestamp.write().await;
        let timestamp_increment = 90000 / 30; // Assuming 30fps
        *ts = ts.wrapping_add(timestamp_increment);

        // Create WebRTC sample with encoded bitstream
        let frame_duration = Duration::from_millis(33); // ~30fps
        let sample = Sample {
            data: bitstream.into(),
            duration: frame_duration,
            timestamp: std::time::SystemTime::now(),
            ..Default::default()
        };

        // Send sample (webrtc-rs handles RTP packetization per RFC 7741/6184)
        self.track
            .write_sample(&sample)
            .await
            .map_err(|e| Error::MediaTrackError(format!("Failed to write RTP sample: {}", e)))?;

        Ok(())
    }

    /// Get the underlying WebRTC track
    pub fn track(&self) -> Arc<TrackLocalStaticSample> {
        Arc::clone(&self.track)
    }

    /// Decode received RTP packet to RuntimeData (uses runtime-core VideoDecoderNode)
    ///
    /// # Arguments
    ///
    /// * `payload` - RTP payload (encoded video bitstream)
    ///
    /// # Returns
    ///
    /// Decoded RuntimeData::Video frame
    ///
    /// # Note
    ///
    /// Decodes RTP payload using runtime-core VideoDecoderNode (VP8/H.264/AV1 via ac-ffmpeg).
    /// Phase 5 T068: RTP depacketization and decoding
    pub async fn decode_rtp_payload(&self, payload: &[u8]) -> Result<RuntimeData> {
        use remotemedia_runtime_core::nodes::streaming_node::AsyncStreamingNode;

        // Create encoded RuntimeData from RTP payload
        let encoded_data = RuntimeData::Video {
            pixel_data: payload.to_vec(),
            width: 1280, // Will be overridden by decoder
            height: 720,
            format: PixelFormat::Encoded,
            codec: Some(self.codec),
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: false,
            stream_id: None,
        };

        // Decode using runtime-core VideoDecoderNode
        self.decoder
            .process(encoded_data)
            .await
            .map_err(|e| Error::EncodingError(format!("Video decoding failed: {}", e)))
    }

    /// Get current RTP sequence number
    pub async fn sequence_number(&self) -> u16 {
        *self.sequence_number.read().await
    }

    /// Get current RTP timestamp
    pub async fn timestamp(&self) -> u32 {
        *self.timestamp.read().await
    }

    /// Get the video codec being used
    pub fn codec(&self) -> VideoCodec {
        self.codec
    }

    /// Check if the next frame should be a keyframe
    pub async fn should_force_keyframe(&self) -> bool {
        // Keyframe every 60 frames (matches encoder config)
        let seq = *self.sequence_number.read().await;
        seq % 60 == 0
    }

    /// Legacy method: Decode received RTP packet to VideoFrame
    ///
    /// For new code, use `decode_rtp_payload` which returns RuntimeData::Video
    pub async fn on_rtp_packet(&self, payload: &[u8]) -> Result<VideoFrame> {
        let runtime_data = self.decode_rtp_payload(payload).await?;

        // Convert RuntimeData::Video back to VideoFrame for backward compatibility
        match runtime_data {
            RuntimeData::Video {
                pixel_data,
                width,
                height,
                format,
                ..
            } => {
                let video_format = match format {
                    PixelFormat::I420 | PixelFormat::Yuv420p => VideoFormat::I420,
                    PixelFormat::NV12 => VideoFormat::NV12,
                    PixelFormat::Rgb24 => VideoFormat::RGB24,
                    _ => VideoFormat::I420,
                };

                Ok(VideoFrame {
                    width,
                    height,
                    format: video_format,
                    data: pixel_data,
                    timestamp_us: 0,
                    is_keyframe: false,
                })
            }
            _ => Err(Error::EncodingError("Expected video frame".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;

    #[tokio::test]
    async fn test_audio_track_creation() {
        let track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "audio/opus".to_string(),
                ..Default::default()
            },
            "audio".to_string(),
            "stream".to_string(),
        ));

        let config = AudioEncoderConfig::default();
        let audio_track = AudioTrack::new(track, config);
        assert!(audio_track.is_ok());
    }

    #[tokio::test]
    async fn test_video_track_creation() {
        let track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/VP8".to_string(),
                ..Default::default()
            },
            "video".to_string(),
            "stream".to_string(),
        ));

        let video_track = VideoTrack::new(track, VideoCodec::Vp8, 1280, 720, 2_000_000, 30);
        assert!(video_track.is_ok());
    }
}

/// Convert RuntimeData to RTP-ready format (T072)
///
/// # Arguments
///
/// * `data` - Runtime data (Audio or Video)
/// * `audio_track` - Optional audio track for encoding
/// * `video_track` - Optional video track for encoding
///
/// # Returns
///
/// RTP payload bytes ready to send
///
/// # Note
///
/// This function encodes RuntimeData to the appropriate codec format (Opus for audio, VP9 for video).
pub async fn runtime_data_to_rtp(
    data: &RuntimeData,
    audio_track: Option<&AudioTrack>,
    video_track: Option<&VideoTrack>,
) -> Result<Vec<u8>> {
    match data {
        RuntimeData::Audio {
            samples,
            sample_rate,
            ..
        } => {
            let audio_track = audio_track.ok_or_else(|| {
                Error::MediaTrackError("No audio track available for encoding".to_string())
            })?;

            // Verify sample rate matches encoder config (48kHz expected)
            if *sample_rate != 48000 {
                return Err(Error::InvalidConfig(format!(
                    "Audio sample rate must be 48000 Hz, got {}",
                    sample_rate
                )));
            }

            // Encode the audio samples
            audio_track.encoder.write().await.encode(samples)
        }

        RuntimeData::Video {
            width,
            height,
            pixel_data,
            format,
            ..
        } => {
            let video_track = video_track.ok_or_else(|| {
                Error::MediaTrackError("No video track available for encoding".to_string())
            })?;

            // Convert PixelFormat enum to VideoFormat enum
            let video_format = match format {
                PixelFormat::Rgb24 => VideoFormat::RGB24,
                PixelFormat::Yuv420p | PixelFormat::I420 => VideoFormat::I420,
                _ => {
                    return Err(Error::EncodingError(format!(
                        "Unsupported video format: {:?}",
                        format
                    )))
                }
            };

            // Create VideoFrame
            let frame = VideoFrame {
                width: *width,
                height: *height,
                format: video_format,
                data: pixel_data.clone(),
                timestamp_us: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
                is_keyframe: video_track.should_force_keyframe().await,
            };

            // Encode using VideoTrack's send_video method
            video_track.send_video(&frame).await?;
            Ok(vec![]) // Return empty Vec since data is sent via track
        }

        _ => Err(Error::MediaTrackError(
            "Unsupported RuntimeData type for RTP encoding".to_string(),
        )),
    }
}

/// Convert RTP payload to RuntimeData (T073)
///
/// # Arguments
///
/// * `payload` - RTP payload bytes
/// * `is_audio` - True for audio (Opus), false for video (VP9)
/// * `audio_track` - Optional audio track for decoding
/// * `video_track` - Optional video track for decoding
///
/// # Returns
///
/// Decoded RuntimeData (Audio or Video)
///
/// # Note
///
/// This function decodes RTP payloads (Opus or VP9) back to RuntimeData format.
pub async fn rtp_to_runtime_data(
    payload: &[u8],
    is_audio: bool,
    audio_track: Option<&AudioTrack>,
    video_track: Option<&VideoTrack>,
) -> Result<RuntimeData> {
    if is_audio {
        let audio_track = audio_track.ok_or_else(|| {
            Error::MediaTrackError("No audio track available for decoding".to_string())
        })?;

        // Decode Opus to f32 samples
        let samples = audio_track.on_rtp_packet(payload).await?;

        Ok(RuntimeData::Audio {
            samples,
            sample_rate: 48000, // Opus always decodes to 48kHz
            channels: 1,        // Assuming mono for now
            stream_id: None,
        })
    } else {
        let video_track = video_track.ok_or_else(|| {
            Error::MediaTrackError("No video track available for decoding".to_string())
        })?;

        // Decode VP9 to VideoFrame
        let frame = video_track.on_rtp_packet(payload).await?;

        // Convert VideoFormat to PixelFormat enum
        let format = match frame.format {
            VideoFormat::I420 => PixelFormat::I420,
            VideoFormat::NV12 => PixelFormat::NV12,
            VideoFormat::RGB24 => PixelFormat::Rgb24,
        };

        Ok(RuntimeData::Video {
            pixel_data: frame.data,
            width: frame.width,
            height: frame.height,
            format,
            codec: None,     // Raw frame from WebRTC
            frame_number: 0, // Will be set by caller if needed
            timestamp_us: frame.timestamp_us,
            is_keyframe: false, // Will be set by encoder
            stream_id: None,
        })
    }
}
