//! Configuration types for emotion vector nodes

use serde::{Deserialize, Serialize};

/// How to pool token-level activations into a single vector per prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PoolingMode {
    /// Use only the last token's activation (standard for instruction
    /// models — the last token is the assistant-generation header).
    #[default]
    LastToken,
    /// Mean-pool across all non-special tokens.
    MeanPool,
    /// Use the [0] (CLS-equivalent) position.
    FirstToken,
}

/// Metadata attached to every extracted emotion vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionVectorMetadata {
    /// Model identifier (e.g. "meta-llama/Llama-3.1-8B-Instruct")
    pub model: String,
    /// Layer index where the vector was extracted
    pub layer: usize,
    /// Hidden dimension (shape[0] of the vector)
    pub hidden_size: usize,
    /// Emotion label (e.g. "happy", "sad", "neutral")
    pub emotion: String,
    /// Pooling mode used
    pub pooling: PoolingMode,
    /// Number of positive (emotion) samples
    pub n_positive: usize,
    /// Number of neutral baseline samples
    pub n_neutral: usize,
    /// L2 norm of the raw (pre-normalization) difference vector
    pub raw_norm: f32,
    /// SHA-256 of the prompt dataset (for reproducibility)
    #[serde(default)]
    pub dataset_hash: String,
    /// System prompt used during extraction
    #[serde(default)]
    pub system_prompt: String,
    /// Whether the vector is L2-normalized (unit vector)
    #[serde(default = "default_true")]
    pub normalized: bool,
}

fn default_true() -> bool {
    true
}

/// Configuration for [`EmotionExtractorNode`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct EmotionExtractConfig {
    /// Model to extract from (HuggingFace repo or local path).
    /// Used only for metadata — the node accepts pre-captured
    /// activations as `RuntimeData::Tensor` input.
    pub model: String,

    /// Target layer(s) to extract at.
    /// When multiple layers are specified, one vector is emitted
    /// per layer.
    pub layers: Vec<usize>,

    /// Hidden dimension of the model (for shape validation).
    pub hidden_size: usize,

    /// Pooling strategy for token-level activations.
    pub pooling: PoolingMode,

    /// System prompt applied during extraction (for metadata).
    #[serde(default)]
    pub system_prompt: String,

    /// Whether to L2-normalize the output vector.
    #[serde(default = "default_true")]
    pub normalize: bool,
}

impl Default for EmotionExtractConfig {
    fn default() -> Self {
        Self {
            model: "meta-llama/Llama-3.1-8B-Instruct".to_string(),
            layers: vec![21],
            hidden_size: 4096,
            pooling: PoolingMode::LastToken,
            system_prompt: String::new(),
            normalize: true,
        }
    }
}

impl EmotionExtractConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.layers.is_empty() {
            return Err("layers must contain at least one layer index".to_string());
        }
        if self.hidden_size == 0 {
            return Err("hidden_size must be > 0".to_string());
        }
        Ok(())
    }
}

/// Per-emotion steering configuration.
///
/// Vectors are received as `RuntimeData::Tensor` from the pipeline
/// (e.g., from `EmotionExtractorNode`). The `emotion` label is
/// carried in the tensor's metadata field. The `coefficient` here
/// is the initial value, overridable at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SteeringVectorConfig {
    /// Emotion label (e.g. "happy", "sad")
    pub emotion: String,
    /// Initial coefficient (fraction of layer norm).
    /// Can be overridden at runtime via aux port.
    #[serde(default = "default_zero")]
    pub coefficient: f32,
}

fn default_zero() -> f32 {
    0.0
}

/// Configuration for [`EmotionSteeringNode`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct EmotionSteerConfig {
    /// Model identifier (for metadata matching).
    pub model: String,

    /// Target layer for steering injection.
    pub layer: usize,

    /// Layer norm at the target layer (computed offline).
    /// Used to scale coefficients: actual_delta = coef × layer_norm × vector.
    pub layer_norm: f32,

    /// Emotion labels and their initial coefficients.
    /// Vectors themselves arrive as `RuntimeData::Tensor` from the pipeline.
    pub vectors: Vec<SteeringVectorConfig>,

    /// Maximum allowed coefficient magnitude (safety cap).
    #[serde(default = "default_max_coef")]
    pub max_coefficient: f32,

    /// Generation parameters (passed to the underlying LLM).
    #[serde(default)]
    pub generation: GenerationParams,

    /// System prompt for the LLM.
    #[serde(default)]
    pub system_prompt: String,
}

fn default_max_coef() -> f32 {
    1.0
}

/// Generation parameters for the underlying LLM.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct GenerationParams {
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
        }
    }
}

impl Default for EmotionSteerConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            layer: 21,
            layer_norm: 14.7,
            vectors: Vec::new(),
            max_coefficient: default_max_coef(),
            generation: GenerationParams::default(),
            system_prompt: String::new(),
        }
    }
}

impl EmotionSteerConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.vectors.is_empty() {
            return Err("at least one steering vector must be configured".to_string());
        }
        if self.layer_norm <= 0.0 {
            return Err("layer_norm must be > 0".to_string());
        }
        if self.max_coefficient <= 0.0 {
            return Err("max_coefficient must be > 0".to_string());
        }
        for v in &self.vectors {
            if v.emotion.is_empty() {
                return Err("each vector must have a non-empty emotion label".to_string());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_config_default() {
        let cfg = EmotionExtractConfig::default();
        assert_eq!(cfg.layers, vec![21]);
        assert_eq!(cfg.hidden_size, 4096);
        assert!(cfg.normalize);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_extract_config_rejects_empty_layers() {
        let mut cfg = EmotionExtractConfig::default();
        cfg.layers = vec![];
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_steer_config_default() {
        let cfg = EmotionSteerConfig::default();
        assert_eq!(cfg.layer, 21);
        assert_eq!(cfg.layer_norm, 14.7);
        assert_eq!(cfg.max_coefficient, 1.0);
    }

    #[test]
    fn test_steer_config_rejects_no_vectors() {
        let cfg = EmotionSteerConfig::default();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_steer_config_with_vectors() {
        let mut cfg = EmotionSteerConfig::default();
        cfg.vectors.push(SteeringVectorConfig {
            emotion: "happy".to_string(),
            coefficient: 0.5,
        });
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_pooling_mode_serde() {
        let json = serde_json::json!({"pooling": "last_token"});
        let cfg: EmotionExtractConfig = serde_json::from_value(json).unwrap();
        assert_eq!(cfg.pooling, PoolingMode::LastToken);

        let json2 = serde_json::json!({"pooling": "mean_pool"});
        let cfg2: EmotionExtractConfig = serde_json::from_value(json2).unwrap();
        assert_eq!(cfg2.pooling, PoolingMode::MeanPool);
    }
}
