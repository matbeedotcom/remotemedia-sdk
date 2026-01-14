//! Configuration for LLM text generation nodes

use serde::{Deserialize, Serialize};

/// LLM model variants
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LlmModel {
    #[default]
    Phi2,
    Phi3Mini,
    Llama32_1b,
    Llama32_3b,
}

/// Quantization level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quantization {
    F32,
    #[default]
    F16,
    Q8_0,
    Q5_1,
    Q5_0,
    Q4_1,
    Q4_0,
}

/// Generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    #[serde(default = "default_top_k")]
    pub top_k: u32,
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
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

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            top_k: default_top_k(),
            repeat_penalty: default_repeat_penalty(),
            stop_sequences: Vec::new(),
        }
    }
}

/// LLM node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub model: LlmModel,
    #[serde(default)]
    pub quantization: Quantization,
    #[serde(default = "default_device")]
    pub device: String,
    #[serde(default)]
    pub generation: GenerationConfig,
    #[serde(default)]
    pub system_prompt: String,
}

fn default_device() -> String {
    "auto".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model: LlmModel::default(),
            quantization: Quantization::default(),
            device: default_device(),
            generation: GenerationConfig::default(),
            system_prompt: String::new(),
        }
    }
}
