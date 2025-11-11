//! Media tracks for WebRTC
//!
//! Handles RTP track management, encoding, and decoding.

use crate::{Error, Result};
use super::audio::{AudioEncoder, AudioEncoderConfig};
use super::video::{VideoEncoder, VideoDecoder, VideoEncoderConfig, VideoFrame, VideoFormat};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocalWriter;
use webrtc::media::Sample;
use remotemedia_runtime_core::data::RuntimeData;

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

    /// RTP timestamp (in sample units)
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
        let encoder = Arc::new(RwLock::new(AudioEncoder::new(config.clone())?));
        let decoder = Arc::new(RwLock::new(super::audio::AudioDecoder::new(config)?));

        Ok(Self {
            track,
            encoder,
            decoder,
            timestamp: Arc::new(RwLock::new(0)),
        })
    }

    /// Send audio samples over RTP
    ///
    /// # Arguments
    ///
    /// * `samples` - Audio samples as f32 (range -1.0 to 1.0)
    ///
    /// # Note
    ///
    /// This method encodes the audio to Opus and sends it via WebRTC samples.
    /// Opus requires specific frame sizes (2.5, 5, 10, 20, 40, or 60ms).
    /// We chunk the input into 20ms frames (480 samples @ 24kHz, 960 @ 48kHz).
    pub async fn send_audio(&self, samples: Arc<Vec<f32>>) -> Result<()> {
        use tracing::debug;

        // Determine frame size based on sample rate (20ms frame)
        // 24kHz: 20ms = 480 samples
        // 48kHz: 20ms = 960 samples
        let encoder = self.encoder.read().await;
        let sample_rate = encoder.config.sample_rate;
        drop(encoder);

        let frame_size = (sample_rate as usize * 20) / 1000; // 20ms frame
        let frame_duration = Duration::from_millis(20);

        debug!("Chunking {} samples into {}sample frames @ {}Hz",
               samples.len(), frame_size, sample_rate);

        // Process audio in chunks
        for chunk in samples.chunks(frame_size) {
            // Opus requires exact frame sizes - pad last chunk if needed
            let samples_to_encode: Vec<f32> = if chunk.len() < frame_size {
                debug!("Padding last chunk from {} to {} samples", chunk.len(), frame_size);
                let mut padded = chunk.to_vec();
                padded.resize(frame_size, 0.0); // Pad with silence
                padded
            } else {
                chunk.to_vec()
            };

            // Encode this chunk
            let encoded = self.encoder.write().await.encode(&samples_to_encode)?;

            // Update timestamp based on actual samples in this chunk (not padded size)
            let mut ts = self.timestamp.write().await;
            *ts = ts.wrapping_add(chunk.len() as u32);

            // Create WebRTC sample with encoded Opus data
            let sample = Sample {
                data: encoded.into(),
                duration: frame_duration,
                timestamp: std::time::SystemTime::now(),
                ..Default::default()
            };

            // Send sample (handles RTP packetization internally)
            self.track
                .write_sample(&sample)
                .await
                .map_err(|e| Error::MediaTrackError(format!("Failed to write sample: {}", e)))?;
        }

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
        *self.timestamp.read().await
    }
}

/// Video track for WebRTC
///
/// Manages video encoding/decoding and RTP transmission.
pub struct VideoTrack {
    /// Underlying WebRTC track
    track: Arc<TrackLocalStaticSample>,

    /// Video encoder
    encoder: Arc<RwLock<VideoEncoder>>,

    /// Video decoder
    decoder: Arc<RwLock<VideoDecoder>>,

    /// RTP sequence number
    sequence_number: Arc<RwLock<u16>>,

    /// RTP timestamp
    timestamp: Arc<RwLock<u32>>,
}

impl VideoTrack {
    /// Create a new video track
    ///
    /// # Arguments
    ///
    /// * `track` - Underlying WebRTC track
    /// * `config` - Video encoder configuration
    pub fn new(track: Arc<TrackLocalStaticSample>, config: VideoEncoderConfig) -> Result<Self> {
        let encoder = Arc::new(RwLock::new(VideoEncoder::new(config.clone())?));
        let decoder = Arc::new(RwLock::new(VideoDecoder::new(config)?));

        Ok(Self {
            track,
            encoder,
            decoder,
            sequence_number: Arc::new(RwLock::new(0)),
            timestamp: Arc::new(RwLock::new(0)),
        })
    }

    /// Send video frame over RTP
    ///
    /// # Arguments
    ///
    /// * `frame` - Video frame to send
    ///
    /// # Note
    ///
    /// This method encodes the video to VP9 and sends it via WebRTC samples.
    /// Requires the `codecs` feature flag for actual encoding.
    pub async fn send_video(&self, frame: &VideoFrame) -> Result<()> {
        // Encode video frame
        let encoded = self.encoder.write().await.encode(frame)?;

        // Update sequence number
        let mut seq = self.sequence_number.write().await;
        *seq = seq.wrapping_add(1);

        // Update timestamp (90kHz clock for video)
        let mut ts = self.timestamp.write().await;
        let timestamp_increment = 90000 / 30; // Assuming 30fps
        *ts = ts.wrapping_add(timestamp_increment);

        // Create WebRTC sample with encoded VP9 data
        // Video frames typically have variable duration based on framerate
        let frame_duration = Duration::from_millis(33); // ~30fps
        let sample = Sample {
            data: encoded.into(),
            duration: frame_duration,
            timestamp: std::time::SystemTime::now(),
            ..Default::default()
        };

        // Send sample (handles RTP packetization internally)
        self.track
            .write_sample(&sample)
            .await
            .map_err(|e| Error::MediaTrackError(format!("Failed to write sample: {}", e)))?;

        Ok(())
    }

    /// Get the underlying WebRTC track
    pub fn track(&self) -> Arc<TrackLocalStaticSample> {
        Arc::clone(&self.track)
    }

    /// Decode received RTP packet to video frame
    ///
    /// # Arguments
    ///
    /// * `payload` - RTP payload (VP9 encoded data)
    ///
    /// # Returns
    ///
    /// Decoded video frame (I420 format)
    ///
    /// # Note
    ///
    /// This method is called when an RTP video packet is received.
    /// Requires the `codecs` feature flag for actual decoding.
    pub async fn on_rtp_packet(&self, payload: &[u8]) -> Result<VideoFrame> {
        self.decoder.write().await.decode(payload)
    }

    /// Get current RTP sequence number
    pub async fn sequence_number(&self) -> u16 {
        *self.sequence_number.read().await
    }

    /// Get current RTP timestamp
    pub async fn timestamp(&self) -> u32 {
        *self.timestamp.read().await
    }

    /// Check if the next frame should be a keyframe
    pub async fn should_force_keyframe(&self) -> bool {
        self.encoder.read().await.should_force_keyframe()
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
                mime_type: "video/VP9".to_string(),
                ..Default::default()
            },
            "video".to_string(),
            "stream".to_string(),
        ));

        let config = VideoEncoderConfig::default();
        let video_track = VideoTrack::new(track, config);
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
        RuntimeData::Audio { samples, sample_rate, .. } => {
            let audio_track = audio_track.ok_or_else(|| {
                Error::MediaTrackError("No audio track available for encoding".to_string())
            })?;

            // Verify sample rate matches encoder config (48kHz expected)
            if *sample_rate != 48000 {
                return Err(Error::InvalidConfig(
                    format!("Audio sample rate must be 48000 Hz, got {}", sample_rate)
                ));
            }

            // Encode the audio samples
            audio_track.encoder.write().await.encode(samples)
        }

        RuntimeData::Video { width, height, pixel_data, format, .. } => {
            let video_track = video_track.ok_or_else(|| {
                Error::MediaTrackError("No video track available for encoding".to_string())
            })?;

            // Convert format i32 to VideoFormat enum
            // 0=unspecified, 1=RGB24, 2=RGBA32, 3=YUV420P (I420)
            let video_format = match format {
                1 => VideoFormat::RGB24,
                3 => VideoFormat::I420,
                _ => return Err(Error::EncodingError(
                    format!("Unsupported video format code: {}", format)
                )),
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

            // Encode the video frame
            video_track.encoder.write().await.encode(&frame)
        }

        _ => Err(Error::MediaTrackError(
            format!("Unsupported RuntimeData type for RTP encoding")
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
        })
    } else {
        let video_track = video_track.ok_or_else(|| {
            Error::MediaTrackError("No video track available for decoding".to_string())
        })?;

        // Decode VP9 to VideoFrame
        let frame = video_track.on_rtp_packet(payload).await?;

        // Convert VideoFormat to format code (0=unspecified, 1=RGB24, 2=RGBA32, 3=YUV420P)
        let format_code = match frame.format {
            VideoFormat::I420 => 3,
            VideoFormat::NV12 => 0, // Map NV12 to unspecified for now
            VideoFormat::RGB24 => 1,
        };

        Ok(RuntimeData::Video {
            pixel_data: frame.data,
            width: frame.width,
            height: frame.height,
            format: format_code,
            frame_number: 0, // Will be set by caller if needed
            timestamp_us: frame.timestamp_us,
        })
    }
}
