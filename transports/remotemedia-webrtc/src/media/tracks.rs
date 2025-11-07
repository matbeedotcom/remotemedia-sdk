//! Media tracks for WebRTC
//!
//! Handles RTP track management, encoding, and decoding.

use crate::{Error, Result};
use super::audio::{AudioEncoder, AudioDecoder, AudioEncoderConfig};
use super::video::{VideoEncoder, VideoDecoder, VideoEncoderConfig, VideoFrame};
use std::sync::Arc;
use tokio::sync::RwLock;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};

/// Audio track for WebRTC
///
/// Manages audio encoding/decoding and RTP transmission.
pub struct AudioTrack {
    /// Underlying WebRTC track
    track: Arc<TrackLocalStaticRTP>,

    /// Audio encoder
    encoder: Arc<RwLock<AudioEncoder>>,

    /// Audio decoder
    decoder: Arc<RwLock<AudioDecoder>>,

    /// RTP sequence number
    sequence_number: Arc<RwLock<u16>>,

    /// RTP timestamp
    timestamp: Arc<RwLock<u32>>,
}

impl AudioTrack {
    /// Create a new audio track
    ///
    /// # Arguments
    ///
    /// * `track` - Underlying WebRTC track
    /// * `config` - Audio encoder configuration
    pub fn new(track: Arc<TrackLocalStaticRTP>, config: AudioEncoderConfig) -> Result<Self> {
        let encoder = Arc::new(RwLock::new(AudioEncoder::new(config.clone())?));
        let decoder = Arc::new(RwLock::new(AudioDecoder::new(config)?));

        Ok(Self {
            track,
            encoder,
            decoder,
            sequence_number: Arc::new(RwLock::new(0)),
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
    /// This method encodes the audio to Opus and sends it over RTP.
    /// Requires the `codecs` feature flag for actual encoding.
    pub async fn send_audio(&self, samples: Arc<Vec<f32>>) -> Result<()> {
        // Encode audio samples
        let encoded = self.encoder.write().await.encode(&samples)?;

        // Update sequence number
        let mut seq = self.sequence_number.write().await;
        *seq = seq.wrapping_add(1);

        // Update timestamp (assuming 48kHz, 20ms frames = 960 samples)
        let mut ts = self.timestamp.write().await;
        *ts = ts.wrapping_add(960);

        // Send RTP packet (write raw encoded bytes)
        self.track
            .as_ref()
            .write(&encoded)
            .await
            .map_err(|e| Error::MediaTrackError(format!("Failed to write RTP packet: {}", e)))?;

        Ok(())
    }

    /// Get the underlying WebRTC track
    pub fn track(&self) -> Arc<TrackLocalStaticRTP> {
        Arc::clone(&self.track)
    }
}

/// Video track for WebRTC
///
/// Manages video encoding/decoding and RTP transmission.
pub struct VideoTrack {
    /// Underlying WebRTC track
    track: Arc<TrackLocalStaticRTP>,

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
    pub fn new(track: Arc<TrackLocalStaticRTP>, config: VideoEncoderConfig) -> Result<Self> {
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
    /// This method encodes the video to VP9 and sends it over RTP.
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

        // Send RTP packet (write raw encoded bytes)
        self.track
            .as_ref()
            .write(&encoded)
            .await
            .map_err(|e| Error::MediaTrackError(format!("Failed to write RTP packet: {}", e)))?;

        Ok(())
    }

    /// Get the underlying WebRTC track
    pub fn track(&self) -> Arc<TrackLocalStaticRTP> {
        Arc::clone(&self.track)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;

    #[tokio::test]
    async fn test_audio_track_creation() {
        let track = Arc::new(TrackLocalStaticRTP::new(
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
        let track = Arc::new(TrackLocalStaticRTP::new(
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
