//! Configuration for Silero VAD (Voice Activity Detection) node

use serde::{Deserialize, Serialize};

/// Supported sample rates for Silero VAD
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VadSampleRate {
    /// 8kHz sample rate (frame_size=256, context_size=32)
    #[serde(rename = "8000")]
    Sr8k,
    /// 16kHz sample rate (frame_size=512, context_size=64)
    #[serde(rename = "16000")]
    Sr16k,
}

impl VadSampleRate {
    /// Get the sample rate in Hz
    pub fn hz(&self) -> u32 {
        match self {
            Self::Sr8k => 8000,
            Self::Sr16k => 16000,
        }
    }

    /// Get the frame size in samples
    pub fn frame_size(&self) -> usize {
        match self {
            Self::Sr8k => 256,
            Self::Sr16k => 512,
        }
    }

    /// Get the context size in samples
    pub fn context_size(&self) -> usize {
        match self {
            Self::Sr8k => 32,
            Self::Sr16k => 64,
        }
    }
}

impl Default for VadSampleRate {
    fn default() -> Self {
        Self::Sr16k
    }
}

impl std::fmt::Display for VadSampleRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sr8k => write!(f, "8000"),
            Self::Sr16k => write!(f, "16000"),
        }
    }
}

/// Configuration for SileroVadNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VadConfig {
    /// Sample rate for VAD processing
    #[serde(default)]
    pub sample_rate: VadSampleRate,

    /// Speech probability threshold (0.0 - 1.0)
    /// Probabilities above this are considered speech
    #[serde(default = "default_threshold")]
    pub threshold: f32,

    /// Minimum speech duration in milliseconds
    /// Speech segments shorter than this are ignored
    #[serde(default = "default_min_speech_duration")]
    pub min_speech_duration_ms: u32,

    /// Minimum silence duration in milliseconds
    /// Silence segments shorter than this don't end speech
    #[serde(default = "default_min_silence_duration")]
    pub min_silence_duration_ms: u32,

    /// Inference device ("auto", "cpu", "cuda", "metal")
    #[serde(default = "default_device")]
    pub device: String,

    /// Whether to output speech segments or raw probabilities
    #[serde(default = "default_output_segments")]
    pub output_segments: bool,
}

fn default_threshold() -> f32 {
    0.5
}

fn default_min_speech_duration() -> u32 {
    250
}

fn default_min_silence_duration() -> u32 {
    100
}

fn default_device() -> String {
    "auto".to_string()
}

fn default_output_segments() -> bool {
    true
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sample_rate: VadSampleRate::default(),
            threshold: default_threshold(),
            min_speech_duration_ms: default_min_speech_duration(),
            min_silence_duration_ms: default_min_silence_duration(),
            device: default_device(),
            output_segments: default_output_segments(),
        }
    }
}

impl VadConfig {
    /// Create config from JSON value
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.threshold < 0.0 || self.threshold > 1.0 {
            return Err(format!(
                "Invalid threshold: {}. Must be between 0.0 and 1.0",
                self.threshold
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = VadConfig::default();
        assert_eq!(config.sample_rate, VadSampleRate::Sr16k);
        assert_eq!(config.threshold, 0.5);
        assert!(config.output_segments);
    }

    #[test]
    fn test_sample_rate_params() {
        assert_eq!(VadSampleRate::Sr8k.hz(), 8000);
        assert_eq!(VadSampleRate::Sr8k.frame_size(), 256);
        assert_eq!(VadSampleRate::Sr8k.context_size(), 32);

        assert_eq!(VadSampleRate::Sr16k.hz(), 16000);
        assert_eq!(VadSampleRate::Sr16k.frame_size(), 512);
        assert_eq!(VadSampleRate::Sr16k.context_size(), 64);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "sample_rate": "8000",
            "threshold": 0.7,
            "output_segments": false
        });
        let config = VadConfig::from_json(&json).unwrap();
        assert_eq!(config.sample_rate, VadSampleRate::Sr8k);
        assert_eq!(config.threshold, 0.7);
        assert!(!config.output_segments);
    }
}
