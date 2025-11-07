//! Audio codec support (Opus)
//!
//! Note: Opus codec requires CMake and is optional.
//! Enable with the `codecs` feature flag.

use crate::{Error, Result};

/// Audio encoder configuration
#[derive(Debug, Clone)]
pub struct AudioEncoderConfig {
    /// Sample rate in Hz (typically 48000)
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: u16,
    /// Bitrate in bits per second (typically 32000-128000)
    pub bitrate: u32,
    /// Complexity (0-10, higher = better quality but slower)
    pub complexity: u32,
}

impl Default for AudioEncoderConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 1,
            bitrate: 64000,
            complexity: 10,
        }
    }
}

/// Audio encoder (Opus)
pub struct AudioEncoder {
    config: AudioEncoderConfig,
    #[cfg(feature = "codecs")]
    encoder: opus::Encoder,
}

impl AudioEncoder {
    /// Create a new audio encoder
    pub fn new(config: AudioEncoderConfig) -> Result<Self> {
        // Validate configuration
        if config.sample_rate != 48000 && config.sample_rate != 24000 && config.sample_rate != 16000 {
            return Err(Error::InvalidConfig(
                "Opus sample rate must be 48000, 24000, or 16000 Hz".to_string(),
            ));
        }

        if config.channels != 1 && config.channels != 2 {
            return Err(Error::InvalidConfig(
                "Opus supports 1 (mono) or 2 (stereo) channels".to_string(),
            ));
        }

        if config.complexity > 10 {
            return Err(Error::InvalidConfig(
                "Opus complexity must be 0-10".to_string(),
            ));
        }

        #[cfg(feature = "codecs")]
        {
            // Create Opus encoder
            let channels = match config.channels {
                1 => opus::Channels::Mono,
                2 => opus::Channels::Stereo,
                _ => unreachable!(),
            };

            let mut encoder = opus::Encoder::new(
                config.sample_rate,
                channels,
                opus::Application::Voip,
            ).map_err(|e| Error::EncodingError(format!("Failed to create Opus encoder: {:?}", e)))?;

            // Configure encoder
            encoder.set_bitrate(opus::Bitrate::Bits(config.bitrate as i32))
                .map_err(|e| Error::EncodingError(format!("Failed to set bitrate: {:?}", e)))?;

            encoder.set_complexity(config.complexity as i32)
                .map_err(|e| Error::EncodingError(format!("Failed to set complexity: {:?}", e)))?;

            Ok(Self { config, encoder })
        }

        #[cfg(not(feature = "codecs"))]
        Ok(Self { config })
    }

    /// Encode audio samples to Opus format
    ///
    /// # Arguments
    ///
    /// * `samples` - Input audio samples as f32 (range -1.0 to 1.0)
    ///
    /// # Returns
    ///
    /// Encoded Opus packet as bytes (RTP payload)
    #[cfg(not(feature = "codecs"))]
    pub fn encode(&mut self, _samples: &[f32]) -> Result<Vec<u8>> {
        Err(Error::EncodingError(
            "Opus encoding requires the 'codecs' feature flag".to_string(),
        ))
    }

    /// Encode audio samples to Opus format
    ///
    /// # Arguments
    ///
    /// * `samples` - Input audio samples as f32 (range -1.0 to 1.0)
    ///
    /// # Returns
    ///
    /// Encoded Opus packet as bytes (RTP payload)
    #[cfg(feature = "codecs")]
    pub fn encode(&mut self, samples: &[f32]) -> Result<Vec<u8>> {
        // Opus expects samples in range -1.0 to 1.0
        // Allocate buffer for encoded output (max Opus packet size)
        const MAX_PACKET_SIZE: usize = 4000;
        let mut output = vec![0u8; MAX_PACKET_SIZE];

        // Encode the samples
        let len = self.encoder.encode_float(samples, &mut output)
            .map_err(|e| Error::EncodingError(format!("Opus encoding failed: {:?}", e)))?;

        // Truncate to actual encoded size
        output.truncate(len);
        Ok(output)
    }
}

/// Audio decoder (Opus)
pub struct AudioDecoder {
    config: AudioEncoderConfig,
    #[cfg(feature = "codecs")]
    decoder: opus::Decoder,
}

impl AudioDecoder {
    /// Create a new audio decoder
    pub fn new(config: AudioEncoderConfig) -> Result<Self> {
        // Validate configuration
        if config.sample_rate != 48000 && config.sample_rate != 24000 && config.sample_rate != 16000 {
            return Err(Error::InvalidConfig(
                "Opus sample rate must be 48000, 24000, or 16000 Hz".to_string(),
            ));
        }

        if config.channels != 1 && config.channels != 2 {
            return Err(Error::InvalidConfig(
                "Opus supports 1 (mono) or 2 (stereo) channels".to_string(),
            ));
        }

        #[cfg(feature = "codecs")]
        {
            // Create Opus decoder
            let channels = match config.channels {
                1 => opus::Channels::Mono,
                2 => opus::Channels::Stereo,
                _ => unreachable!(),
            };

            let decoder = opus::Decoder::new(config.sample_rate, channels)
                .map_err(|e| Error::EncodingError(format!("Failed to create Opus decoder: {:?}", e)))?;

            Ok(Self { config, decoder })
        }

        #[cfg(not(feature = "codecs"))]
        Ok(Self { config })
    }

    /// Decode Opus packet to audio samples
    ///
    /// # Arguments
    ///
    /// * `payload` - Encoded Opus packet (RTP payload)
    ///
    /// # Returns
    ///
    /// Decoded audio samples as f32 (range -1.0 to 1.0) at 48kHz
    #[cfg(not(feature = "codecs"))]
    pub fn decode(&mut self, _payload: &[u8]) -> Result<Vec<f32>> {
        Err(Error::EncodingError(
            "Opus decoding requires the 'codecs' feature flag".to_string(),
        ))
    }

    /// Decode Opus packet to audio samples
    ///
    /// # Arguments
    ///
    /// * `payload` - Encoded Opus packet (RTP payload)
    ///
    /// # Returns
    ///
    /// Decoded audio samples as f32 (range -1.0 to 1.0) at 48kHz
    #[cfg(feature = "codecs")]
    pub fn decode(&mut self, payload: &[u8]) -> Result<Vec<f32>> {
        // Opus frame size: typically 2.5, 5, 10, 20, 40, or 60 ms
        // At 48kHz, 20ms = 960 samples per channel
        // Max frame size for Opus is 120ms @ 48kHz = 5760 samples per channel
        const MAX_FRAME_SIZE: usize = 5760;
        let mut output = vec![0f32; MAX_FRAME_SIZE * self.config.channels as usize];

        // Decode the packet
        let len = self.decoder.decode_float(payload, &mut output, false)
            .map_err(|e| Error::EncodingError(format!("Opus decoding failed: {:?}", e)))?;

        // Truncate to actual decoded size
        output.truncate(len * self.config.channels as usize);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_encoder_config_default() {
        let config = AudioEncoderConfig::default();
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.channels, 1);
        assert_eq!(config.bitrate, 64000);
    }

    #[test]
    fn test_audio_encoder_creation() {
        let config = AudioEncoderConfig::default();
        let encoder = AudioEncoder::new(config);
        assert!(encoder.is_ok());
    }

    #[test]
    fn test_audio_encoder_invalid_sample_rate() {
        let config = AudioEncoderConfig {
            sample_rate: 44100, // Not supported by Opus
            ..Default::default()
        };
        let encoder = AudioEncoder::new(config);
        assert!(encoder.is_err());
    }

    #[test]
    fn test_audio_decoder_creation() {
        let config = AudioEncoderConfig::default();
        let decoder = AudioDecoder::new(config);
        assert!(decoder.is_ok());
    }
}
