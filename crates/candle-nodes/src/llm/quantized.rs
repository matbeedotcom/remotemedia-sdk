//! GGUF quantized model loading utilities
//!
//! Provides support for loading quantized GGUF models for memory-efficient inference.

use crate::error::{CandleNodeError, Result};
use std::path::Path;

/// Supported GGUF quantization types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GgufQuantType {
    /// 4-bit quantization (Q4_0)
    Q4_0,
    /// 4-bit quantization with improved accuracy (Q4_1)
    Q4_1,
    /// 5-bit quantization (Q5_0)
    Q5_0,
    /// 5-bit quantization with improved accuracy (Q5_1)
    Q5_1,
    /// 8-bit quantization
    Q8_0,
    /// Half precision (F16)
    F16,
    /// Full precision (F32)
    F32,
}

impl GgufQuantType {
    /// Get file suffix for this quantization type
    pub fn file_suffix(&self) -> &'static str {
        match self {
            Self::Q4_0 => "q4_0",
            Self::Q4_1 => "q4_1",
            Self::Q5_0 => "q5_0",
            Self::Q5_1 => "q5_1",
            Self::Q8_0 => "q8_0",
            Self::F16 => "f16",
            Self::F32 => "f32",
        }
    }

    /// Get approximate memory reduction factor compared to F32
    pub fn memory_factor(&self) -> f32 {
        match self {
            Self::F32 => 1.0,
            Self::F16 => 0.5,
            Self::Q8_0 => 0.25,
            Self::Q5_0 | Self::Q5_1 => 0.16,
            Self::Q4_0 | Self::Q4_1 => 0.125,
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "q4_0" | "q4-0" => Some(Self::Q4_0),
            "q4_1" | "q4-1" => Some(Self::Q4_1),
            "q5_0" | "q5-0" => Some(Self::Q5_0),
            "q5_1" | "q5-1" => Some(Self::Q5_1),
            "q8_0" | "q8-0" | "q8" => Some(Self::Q8_0),
            "f16" | "fp16" => Some(Self::F16),
            "f32" | "fp32" => Some(Self::F32),
            _ => None,
        }
    }
}

/// GGUF model metadata
#[derive(Debug, Clone)]
pub struct GgufMetadata {
    /// Model architecture (e.g., "llama", "phi")
    pub architecture: String,
    /// Quantization type
    pub quant_type: GgufQuantType,
    /// Number of parameters
    pub num_params: u64,
    /// Context length
    pub context_length: u32,
    /// Embedding dimension
    pub embedding_dim: u32,
    /// Number of attention heads
    pub num_heads: u32,
    /// Number of layers
    pub num_layers: u32,
}

/// GGUF model loader
pub struct GgufLoader;

impl GgufLoader {
    /// Check if a file is a GGUF file
    pub fn is_gguf_file(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        path.extension()
            .map(|ext| ext.to_string_lossy().to_lowercase() == "gguf")
            .unwrap_or(false)
    }

    /// Load GGUF model metadata from file
    #[cfg(feature = "llm")]
    pub fn load_metadata(path: impl AsRef<Path>) -> Result<GgufMetadata> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Err(CandleNodeError::model_load(
                path.display().to_string(),
                "File not found",
            ));
        }

        if !Self::is_gguf_file(path) {
            return Err(CandleNodeError::model_load(
                path.display().to_string(),
                "Not a GGUF file",
            ));
        }

        // TODO: Parse actual GGUF header
        // For now, return placeholder metadata
        Ok(GgufMetadata {
            architecture: "unknown".to_string(),
            quant_type: GgufQuantType::Q4_0,
            num_params: 0,
            context_length: 4096,
            embedding_dim: 4096,
            num_heads: 32,
            num_layers: 32,
        })
    }

    #[cfg(not(feature = "llm"))]
    pub fn load_metadata(_path: impl AsRef<Path>) -> Result<GgufMetadata> {
        Err(CandleNodeError::configuration(
            "gguf",
            "LLM feature not enabled",
        ))
    }

    /// Estimate memory usage for a GGUF model
    pub fn estimate_memory(num_params: u64, quant_type: GgufQuantType) -> u64 {
        let base_bytes = num_params * 4; // F32 baseline
        (base_bytes as f32 * quant_type.memory_factor()) as u64
    }

    /// Get recommended quantization for available memory
    pub fn recommend_quantization(num_params: u64, available_memory_gb: f32) -> GgufQuantType {
        let available_bytes = (available_memory_gb * 1024.0 * 1024.0 * 1024.0) as u64;
        let base_size = num_params * 4;

        // Try quantization levels from highest quality to lowest
        let quant_types = [
            GgufQuantType::F16,
            GgufQuantType::Q8_0,
            GgufQuantType::Q5_1,
            GgufQuantType::Q5_0,
            GgufQuantType::Q4_1,
            GgufQuantType::Q4_0,
        ];

        for quant in quant_types {
            let estimated = (base_size as f32 * quant.memory_factor()) as u64;
            // Add 20% overhead for KV cache and other buffers
            if estimated * 12 / 10 <= available_bytes {
                return quant;
            }
        }

        // Default to smallest if nothing fits
        GgufQuantType::Q4_0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quant_type_memory_factor() {
        assert_eq!(GgufQuantType::F32.memory_factor(), 1.0);
        assert_eq!(GgufQuantType::F16.memory_factor(), 0.5);
        assert!(GgufQuantType::Q4_0.memory_factor() < GgufQuantType::Q8_0.memory_factor());
    }

    #[test]
    fn test_quant_type_from_str() {
        assert_eq!(GgufQuantType::from_str("q4_0"), Some(GgufQuantType::Q4_0));
        assert_eq!(GgufQuantType::from_str("F16"), Some(GgufQuantType::F16));
        assert_eq!(GgufQuantType::from_str("invalid"), None);
    }

    #[test]
    fn test_is_gguf_file() {
        assert!(GgufLoader::is_gguf_file("model.gguf"));
        assert!(!GgufLoader::is_gguf_file("model.safetensors"));
    }

    #[test]
    fn test_estimate_memory() {
        let params_7b = 7_000_000_000u64;
        let f32_size = GgufLoader::estimate_memory(params_7b, GgufQuantType::F32);
        let q4_size = GgufLoader::estimate_memory(params_7b, GgufQuantType::Q4_0);
        assert!(q4_size < f32_size);
        assert!(q4_size < f32_size / 4);
    }

    #[test]
    fn test_recommend_quantization() {
        // 8GB should fit Q4_0 for 7B model
        let quant = GgufLoader::recommend_quantization(7_000_000_000, 8.0);
        assert!(quant.memory_factor() <= 0.5);
    }
}
