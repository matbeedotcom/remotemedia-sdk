//! Configuration for Whisper speech-to-text node

use serde::{Deserialize, Serialize};

/// Whisper model variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WhisperModel {
    /// Tiny model (~39M parameters)
    Tiny,
    /// Base model (~74M parameters)
    Base,
    /// Small model (~244M parameters)
    Small,
    /// Medium model (~769M parameters)
    Medium,
    /// Large v3 model (~1.5B parameters)
    #[serde(rename = "large-v3")]
    LargeV3,
}

impl WhisperModel {
    /// Get the HuggingFace model ID
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::Tiny => "openai/whisper-tiny",
            Self::Base => "openai/whisper-base",
            Self::Small => "openai/whisper-small",
            Self::Medium => "openai/whisper-medium",
            Self::LargeV3 => "openai/whisper-large-v3",
        }
    }

    /// Get the config filename
    pub fn config_file(&self) -> &'static str {
        "config.json"
    }

    /// Get the model weights filename
    pub fn weights_file(&self) -> &'static str {
        "model.safetensors"
    }

    /// Get the tokenizer filename
    pub fn tokenizer_file(&self) -> &'static str {
        "tokenizer.json"
    }

    /// Get approximate model size in bytes
    pub fn approx_size(&self) -> u64 {
        match self {
            Self::Tiny => 39_000_000,
            Self::Base => 74_000_000,
            Self::Small => 244_000_000,
            Self::Medium => 769_000_000,
            Self::LargeV3 => 1_500_000_000,
        }
    }

    /// Check if model supports multiple languages
    pub fn is_multilingual(&self) -> bool {
        // All standard whisper models are multilingual
        // English-only models would be "tiny.en", "base.en", etc.
        true
    }
}

impl Default for WhisperModel {
    fn default() -> Self {
        Self::Base
    }
}

impl std::fmt::Display for WhisperModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tiny => write!(f, "tiny"),
            Self::Base => write!(f, "base"),
            Self::Small => write!(f, "small"),
            Self::Medium => write!(f, "medium"),
            Self::LargeV3 => write!(f, "large-v3"),
        }
    }
}

impl std::str::FromStr for WhisperModel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tiny" => Ok(Self::Tiny),
            "base" => Ok(Self::Base),
            "small" => Ok(Self::Small),
            "medium" => Ok(Self::Medium),
            "large-v3" | "large" => Ok(Self::LargeV3),
            other => Err(format!("Unknown Whisper model: {}", other)),
        }
    }
}

/// Configuration for WhisperNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperConfig {
    /// Model variant to use
    #[serde(default)]
    pub model: WhisperModel,

    /// Target language code (ISO 639-1) or "auto" for detection
    #[serde(default = "default_language")]
    pub language: String,

    /// Inference device ("auto", "cpu", "cuda", "metal")
    #[serde(default = "default_device")]
    pub device: String,

    /// Enable streaming partial transcriptions
    #[serde(default = "default_streaming")]
    pub streaming: bool,

    /// Task: "transcribe" or "translate"
    #[serde(default = "default_task")]
    pub task: String,
}

fn default_language() -> String {
    "auto".to_string()
}

fn default_device() -> String {
    "auto".to_string()
}

fn default_streaming() -> bool {
    true
}

fn default_task() -> String {
    "transcribe".to_string()
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            model: WhisperModel::default(),
            language: default_language(),
            device: default_device(),
            streaming: default_streaming(),
            task: default_task(),
        }
    }
}

impl WhisperConfig {
    /// Create config from JSON value
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate language code
        if self.language != "auto" && self.language.len() != 2 {
            return Err(format!(
                "Invalid language code: {}. Use 'auto' or ISO 639-1 code (e.g., 'en', 'es')",
                self.language
            ));
        }

        // Validate task
        if self.task != "transcribe" && self.task != "translate" {
            return Err(format!(
                "Invalid task: {}. Use 'transcribe' or 'translate'",
                self.task
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
        let config = WhisperConfig::default();
        assert_eq!(config.model, WhisperModel::Base);
        assert_eq!(config.language, "auto");
        assert!(config.streaming);
    }

    #[test]
    fn test_model_id() {
        assert_eq!(WhisperModel::Base.model_id(), "openai/whisper-base");
        assert_eq!(WhisperModel::LargeV3.model_id(), "openai/whisper-large-v3");
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "model": "small",
            "language": "en",
            "streaming": false
        });
        let config = WhisperConfig::from_json(&json).unwrap();
        assert_eq!(config.model, WhisperModel::Small);
        assert_eq!(config.language, "en");
        assert!(!config.streaming);
    }
}
