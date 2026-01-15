//! Configuration for LLM text generation nodes

use serde::{Deserialize, Serialize};

/// Phi model variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PhiModel {
    /// Phi-2 (2.7B parameters)
    #[default]
    Phi2,
    /// Phi-3 Mini (3.8B parameters)
    Phi3Mini,
    /// Phi-3 Mini 128k context
    Phi3Mini128k,
}

impl PhiModel {
    /// Get HuggingFace model ID
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::Phi2 => "microsoft/phi-2",
            Self::Phi3Mini => "microsoft/Phi-3-mini-4k-instruct",
            Self::Phi3Mini128k => "microsoft/Phi-3-mini-128k-instruct",
        }
    }

    /// Get model weights filename
    pub fn weights_file(&self) -> &'static str {
        "model.safetensors"
    }

    /// Get tokenizer filename
    pub fn tokenizer_file(&self) -> &'static str {
        "tokenizer.json"
    }

    /// Get approximate model size in bytes
    pub fn approx_size(&self) -> u64 {
        match self {
            Self::Phi2 => 5_600_000_000,
            Self::Phi3Mini | Self::Phi3Mini128k => 7_600_000_000,
        }
    }
}

impl std::fmt::Display for PhiModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Phi2 => write!(f, "phi-2"),
            Self::Phi3Mini => write!(f, "phi-3-mini"),
            Self::Phi3Mini128k => write!(f, "phi-3-mini-128k"),
        }
    }
}

/// LLaMA model variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LlamaModel {
    /// LLaMA 3.2 1B parameters
    #[default]
    Llama32_1b,
    /// LLaMA 3.2 3B parameters
    Llama32_3b,
    /// LLaMA 3.1 8B parameters
    Llama31_8b,
}

impl LlamaModel {
    /// Get HuggingFace model ID
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::Llama32_1b => "meta-llama/Llama-3.2-1B-Instruct",
            Self::Llama32_3b => "meta-llama/Llama-3.2-3B-Instruct",
            Self::Llama31_8b => "meta-llama/Meta-Llama-3.1-8B-Instruct",
        }
    }

    /// Get model weights filename
    pub fn weights_file(&self) -> &'static str {
        "model.safetensors"
    }

    /// Get tokenizer filename
    pub fn tokenizer_file(&self) -> &'static str {
        "tokenizer.json"
    }

    /// Get approximate model size in bytes
    pub fn approx_size(&self) -> u64 {
        match self {
            Self::Llama32_1b => 2_500_000_000,
            Self::Llama32_3b => 6_500_000_000,
            Self::Llama31_8b => 16_000_000_000,
        }
    }
}

impl std::fmt::Display for LlamaModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Llama32_1b => write!(f, "llama-3.2-1b"),
            Self::Llama32_3b => write!(f, "llama-3.2-3b"),
            Self::Llama31_8b => write!(f, "llama-3.1-8b"),
        }
    }
}

/// Quantization level for GGUF models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quantization {
    /// Full precision (float32)
    F32,
    /// Half precision (float16)
    #[default]
    F16,
    /// 8-bit quantization
    Q8_0,
    /// 5-bit quantization (variant 1)
    Q5_1,
    /// 5-bit quantization (variant 0)
    Q5_0,
    /// 4-bit quantization (variant 1)
    Q4_1,
    /// 4-bit quantization (variant 0)
    Q4_0,
}

impl Quantization {
    /// Get memory reduction factor compared to F32
    pub fn memory_factor(&self) -> f32 {
        match self {
            Self::F32 => 1.0,
            Self::F16 => 0.5,
            Self::Q8_0 => 0.25,
            Self::Q5_1 | Self::Q5_0 => 0.16,
            Self::Q4_1 | Self::Q4_0 => 0.125,
        }
    }
}

/// Generation configuration for LLM inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Sampling temperature (0.0 = deterministic, higher = more random)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p (nucleus) sampling threshold
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Top-k sampling limit
    #[serde(default = "default_top_k")]
    pub top_k: u32,

    /// Repetition penalty (1.0 = no penalty)
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,

    /// Stop sequences to end generation
    #[serde(default)]
    pub stop_sequences: Vec<String>,

    /// Enable streaming token output
    #[serde(default = "default_true")]
    pub streaming: bool,
}

fn default_max_tokens() -> u32 {
    256
}

fn default_temperature() -> f32 {
    0.7
}

fn default_top_p() -> f32 {
    0.9
}

fn default_top_k() -> u32 {
    40
}

fn default_repeat_penalty() -> f32 {
    1.1
}

fn default_true() -> bool {
    true
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            top_k: default_top_k(),
            repeat_penalty: default_repeat_penalty(),
            stop_sequences: Vec::new(),
            streaming: true,
        }
    }
}

impl GenerationConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.temperature < 0.0 {
            return Err("temperature must be >= 0.0".to_string());
        }
        if self.top_p < 0.0 || self.top_p > 1.0 {
            return Err("top_p must be 0.0-1.0".to_string());
        }
        if self.repeat_penalty < 1.0 {
            return Err("repeat_penalty must be >= 1.0".to_string());
        }
        Ok(())
    }
}

/// LLM node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Quantization level
    #[serde(default)]
    pub quantization: Quantization,

    /// Inference device ("auto", "cpu", "cuda", "metal")
    #[serde(default = "default_device")]
    pub device: String,

    /// Generation parameters
    #[serde(default)]
    pub generation: GenerationConfig,

    /// System prompt for chat models
    #[serde(default)]
    pub system_prompt: String,
}

fn default_device() -> String {
    "auto".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            quantization: Quantization::default(),
            device: default_device(),
            generation: GenerationConfig::default(),
            system_prompt: String::new(),
        }
    }
}

impl LlmConfig {
    /// Create config from JSON value
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        self.generation.validate()
    }
}

/// Phi-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhiConfig {
    /// Phi model variant
    #[serde(default)]
    pub model: PhiModel,

    /// Base LLM configuration
    #[serde(flatten)]
    pub llm: LlmConfig,
}

impl Default for PhiConfig {
    fn default() -> Self {
        Self {
            model: PhiModel::default(),
            llm: LlmConfig::default(),
        }
    }
}

impl PhiConfig {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.llm.validate()
    }
}

/// LLaMA-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaConfig {
    /// LLaMA model variant
    #[serde(default)]
    pub model: LlamaModel,

    /// Base LLM configuration
    #[serde(flatten)]
    pub llm: LlmConfig,
}

impl Default for LlamaConfig {
    fn default() -> Self {
        Self {
            model: LlamaModel::default(),
            llm: LlmConfig::default(),
        }
    }
}

impl LlamaConfig {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.llm.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phi_model_id() {
        assert_eq!(PhiModel::Phi2.model_id(), "microsoft/phi-2");
    }

    #[test]
    fn test_llama_model_id() {
        assert_eq!(LlamaModel::Llama32_1b.model_id(), "meta-llama/Llama-3.2-1B-Instruct");
    }

    #[test]
    fn test_quantization_factor() {
        assert_eq!(Quantization::F16.memory_factor(), 0.5);
        assert_eq!(Quantization::Q4_0.memory_factor(), 0.125);
    }

    #[test]
    fn test_generation_config_default() {
        let config = GenerationConfig::default();
        assert_eq!(config.max_tokens, 256);
        assert!(config.streaming);
    }
}
