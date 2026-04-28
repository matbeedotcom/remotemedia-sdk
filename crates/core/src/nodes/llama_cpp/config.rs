//! Configuration types for llama.cpp nodes

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared backend config
// ---------------------------------------------------------------------------

/// GPU offload strategy for the llama.cpp backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GpuOffload {
    /// Run entirely on CPU.
    None,
    /// Offload all possible layers to GPU.
    All,
    /// Offload a specific number of layers (0 = CPU only).
    Layers(u16),
}

impl Default for GpuOffload {
    fn default() -> Self {
        Self::None
    }
}

/// llama.cpp backend initialization settings.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct LlamaBackendConfig {
    /// Whether to enable NUMA-aware memory allocation.
    pub numa: bool,
    /// GPU offload strategy.
    pub gpu_offload: GpuOffload,
    /// Flash Attention 2 (reduces memory, may change outputs slightly).
    pub flash_attention: bool,
    /// Number of threads for computation. `0` = auto (all cores).
    pub threads: Option<u32>,
    /// Number of threads for the background I/O thread. `0` = auto.
    pub threads_batch: Option<u32>,
}

impl Default for LlamaBackendConfig {
    fn default() -> Self {
        Self {
            numa: false,
            gpu_offload: GpuOffload::default(),
            flash_attention: false,
            threads: None,
            threads_batch: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Generation config
// ---------------------------------------------------------------------------

/// Configuration for [`super::generation::LlamaCppGenerationNode`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct LlamaCppGenerationConfig {
    /// Path to a GGUF model file (local path or HuggingFace-style repo).
    pub model_path: String,
    /// Backend settings.
    pub backend: LlamaBackendConfig,
    /// Context size (max tokens the model can attend to).
    #[serde(alias = "n_ctx")]
    pub context_size: u32,
    /// Batch size for decoding.
    #[serde(alias = "n_batch")]
    pub batch_size: u32,
    /// Maximum tokens to generate.
    #[serde(alias = "max_tokens")]
    pub max_tokens: u32,
    /// Sampling temperature (0.0 = greedy, 1.0 = max randomness).
    pub temperature: f32,
    /// Top-p nucleus sampling cutoff.
    #[serde(alias = "top_p")]
    pub top_p: f32,
    /// Top-k sampling cutoff (0 = disabled).
    #[serde(alias = "top_k")]
    pub top_k: u32,
    /// Min-p sampling cutoff (0.0 = disabled). Removes tokens with probability
    /// below `min_p * max_probability` before top-k/top-p are applied.
    #[serde(alias = "min_p")]
    pub min_p: f32,
    /// Repeat penalty (1.0 = disabled).
    #[serde(alias = "repeat_penalty")]
    pub repeat_penalty: f32,
    /// System prompt prepended to every generation request.
    #[serde(alias = "system_prompt")]
    pub system_prompt: Option<String>,
    /// Random seed for sampling (0 = random seed each time).
    pub seed: u64,
}

impl Default for LlamaCppGenerationConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            backend: LlamaBackendConfig::default(),
            context_size: 4096,
            batch_size: 512,
            max_tokens: 256,
            temperature: 0.8,
            top_p: 0.95,
            top_k: 40,
            min_p: 0.0,
            repeat_penalty: 1.1,
            system_prompt: None,
            seed: 0,
        }
    }
}

impl LlamaCppGenerationConfig {
    /// Validate configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.model_path.is_empty() {
            return Err("model_path must not be empty".to_string());
        }
        if self.context_size == 0 {
            return Err("context_size must be > 0".to_string());
        }
        if self.batch_size == 0 {
            return Err("batch_size must be > 0".to_string());
        }
        if self.temperature < 0.0 {
            return Err("temperature must be >= 0".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Embedding config
// ---------------------------------------------------------------------------

/// Configuration for [`super::embedding::LlamaCppEmbeddingNode`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct LlamaCppEmbeddingConfig {
    /// Path to a GGUF model file.
    pub model_path: String,
    /// Backend settings.
    pub backend: LlamaBackendConfig,
    /// Context size.
    #[serde(alias = "n_ctx")]
    pub context_size: u32,
    /// Batch size.
    #[serde(alias = "n_batch")]
    pub batch_size: u32,
    /// Pooling strategy.
    pub pooling: EmbeddingPooling,
    /// Whether to L2-normalize the output vector.
    #[serde(alias = "normalize")]
    pub l2_normalize: bool,
}

/// How to pool token-level embeddings into a single vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingPooling {
    /// Mean-pool across all tokens.
    Mean,
    /// Use the last token's embedding.
    LastToken,
    /// Use the first token's embedding.
    FirstToken,
    /// Use the [0] (CLS-equivalent) position.
    Cls,
}

impl Default for EmbeddingPooling {
    fn default() -> Self {
        Self::Mean
    }
}

impl Default for LlamaCppEmbeddingConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            backend: LlamaBackendConfig::default(),
            context_size: 512,
            batch_size: 256,
            pooling: EmbeddingPooling::Mean,
            l2_normalize: true,
        }
    }
}

impl LlamaCppEmbeddingConfig {
    /// Validate configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.model_path.is_empty() {
            return Err("model_path must not be empty".to_string());
        }
        if self.context_size == 0 {
            return Err("context_size must be > 0".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Activation config
// ---------------------------------------------------------------------------

/// Configuration for [`super::activation::LlamaCppActivationNode`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct LlamaCppActivationConfig {
    /// Path to a GGUF model file.
    pub model_path: String,
    /// Backend settings.
    pub backend: LlamaBackendConfig,
    /// Context size.
    #[serde(alias = "n_ctx")]
    pub context_size: u32,
    /// Batch size.
    #[serde(alias = "n_batch")]
    pub batch_size: u32,
    /// Layer indices to capture activations at (0-indexed from the first
    /// transformer layer). E.g. `[21]` captures layer 21.
    pub layers: Vec<usize>,
    /// Pooling strategy for token-level activations.
    pub pooling: ActivationPooling,
    /// Whether to L2-normalize the output vector.
    pub normalize: bool,
    /// System prompt (prepended when using chat templates).
    #[serde(alias = "system_prompt")]
    pub system_prompt: Option<String>,
}

/// How to pool token-level activations into a single vector per prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPooling {
    /// Mean-pool across all tokens.
    Mean,
    /// Use the last token's activation.
    LastToken,
    /// Use the first token's activation.
    FirstToken,
}

impl Default for ActivationPooling {
    fn default() -> Self {
        Self::LastToken
    }
}

impl Default for LlamaCppActivationConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            backend: LlamaBackendConfig::default(),
            context_size: 4096,
            batch_size: 512,
            layers: vec![21],
            pooling: ActivationPooling::LastToken,
            normalize: true,
            system_prompt: None,
        }
    }
}

impl LlamaCppActivationConfig {
    /// Validate configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.model_path.is_empty() {
            return Err("model_path must not be empty".to_string());
        }
        if self.layers.is_empty() {
            return Err("layers must contain at least one layer index".to_string());
        }
        if self.context_size == 0 {
            return Err("context_size must be > 0".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Steering config
// ---------------------------------------------------------------------------

/// Per-emotion (or per-direction) steering configuration.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LlamaCppSteerVector {
    /// Label for this steering direction (e.g. "happy", "sad").
    pub label: String,
    /// Initial coefficient (fraction of layer norm).
    /// Can be overridden at runtime via JSON input.
    #[serde(default = "default_zero")]
    pub coefficient: f32,
}

fn default_zero() -> f32 {
    0.0
}

/// Configuration for [`super::steer::LlamaCppSteerNode`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct LlamaCppSteerConfig {
    /// Path to a GGUF model file (must match the activation extraction model).
    pub model_path: String,
    /// Backend settings.
    pub backend: LlamaBackendConfig,
    /// Context size.
    #[serde(alias = "n_ctx")]
    pub context_size: u32,
    /// Batch size.
    #[serde(alias = "n_batch")]
    pub batch_size: u32,
    /// Target layer for steering injection.
    pub layer: usize,
    /// Layer norm at the target layer (computed offline during extraction).
    #[serde(alias = "layer_norm")]
    pub layer_norm_value: f32,
    /// Steering directions and their initial coefficients.
    /// Vectors themselves arrive as `RuntimeData::Tensor` from the pipeline.
    pub vectors: Vec<LlamaCppSteerVector>,
    /// Maximum allowed coefficient magnitude (safety cap).
    #[serde(alias = "max_coefficient")]
    pub max_coefficient: f32,
    /// Generation parameters.
    pub generation: LlamaCppGenerationConfig,
    /// System prompt.
    #[serde(alias = "system_prompt")]
    pub system_prompt: Option<String>,
}

impl Default for LlamaCppSteerConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            backend: LlamaBackendConfig::default(),
            context_size: 4096,
            batch_size: 512,
            layer: 21,
            layer_norm_value: 14.7,
            vectors: Vec::new(),
            max_coefficient: 1.0,
            generation: LlamaCppGenerationConfig::default(),
            system_prompt: None,
        }
    }
}

impl LlamaCppSteerConfig {
    /// Validate configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.model_path.is_empty() {
            return Err("model_path must not be empty".to_string());
        }
        if self.vectors.is_empty() {
            return Err("at least one steering vector must be configured".to_string());
        }
        if self.layer_norm_value <= 0.0 {
            return Err("layer_norm_value must be > 0".to_string());
        }
        if self.max_coefficient <= 0.0 {
            return Err("max_coefficient must be > 0".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unified config enum (for factory dispatch)
// ---------------------------------------------------------------------------

/// Unified configuration envelope for all llama.cpp nodes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum LlamaCppConfig {
    /// Text generation mode.
    Generation(LlamaCppGenerationConfig),
    /// Embedding mode.
    Embedding(LlamaCppEmbeddingConfig),
    /// Activation extraction mode.
    Activation(LlamaCppActivationConfig),
    /// Steering mode.
    Steer(LlamaCppSteerConfig),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_config_default() {
        let cfg = LlamaCppGenerationConfig::default();
        assert_eq!(cfg.context_size, 4096);
        assert_eq!(cfg.temperature, 0.8);
        assert!(cfg.validate().is_err()); // model_path empty
    }

    #[test]
    fn test_generation_config_valid() {
        let mut cfg = LlamaCppGenerationConfig::default();
        cfg.model_path = "/path/to/model.gguf".to_string();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_embedding_config_default() {
        let cfg = LlamaCppEmbeddingConfig::default();
        assert!(cfg.l2_normalize);
        assert_eq!(cfg.pooling, EmbeddingPooling::Mean);
    }

    #[test]
    fn test_activation_config_default() {
        let cfg = LlamaCppActivationConfig::default();
        assert_eq!(cfg.layers, vec![21]);
        assert_eq!(cfg.pooling, ActivationPooling::LastToken);
        assert!(cfg.normalize);
    }

    #[test]
    fn test_steer_config_default() {
        let cfg = LlamaCppSteerConfig::default();
        assert_eq!(cfg.layer, 21);
        assert_eq!(cfg.max_coefficient, 1.0);
        assert!(cfg.validate().is_err()); // no vectors
    }

    #[test]
    fn test_gpu_offload_serde() {
        // GpuOffload::Layers serialized as object with snake_case key
        let offload = GpuOffload::Layers(32);
        let serialized = serde_json::to_value(&offload).unwrap();
        assert_eq!(serialized, serde_json::json!({"layers": 32}));
    }
}
