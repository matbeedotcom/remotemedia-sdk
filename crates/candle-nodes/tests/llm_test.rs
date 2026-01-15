//! Integration tests for LLM nodes (Phi and LLaMA)

use remotemedia_candle_nodes::{DeviceSelector, ModelCache};

mod common_tests {
    use super::*;

    #[test]
    fn test_device_from_config() {
        let device = DeviceSelector::from_config("auto").unwrap();
        assert!(!device.name().is_empty());
    }

    #[test]
    fn test_model_cache_stats() {
        let cache = ModelCache::new();
        let stats = cache.stats();
        assert!(stats.is_ok());
    }
}

#[cfg(feature = "llm")]
mod llm_tests {
    use remotemedia_candle_nodes::llm::{
        GenerationConfig, LlamaConfig, LlamaModel, LlmConfig, PhiConfig, PhiModel,
        PhiNode, LlamaNode, Quantization,
    };

    #[test]
    fn test_phi_config_default() {
        let config = PhiConfig::default();
        assert_eq!(config.model, PhiModel::Phi2);
        assert_eq!(config.llm.quantization, Quantization::F16);
    }

    #[test]
    fn test_llama_config_default() {
        let config = LlamaConfig::default();
        assert_eq!(config.model, LlamaModel::Llama32_1b);
    }

    #[test]
    fn test_generation_config_default() {
        let config = GenerationConfig::default();
        assert_eq!(config.max_tokens, 256);
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.top_p, 0.9);
        assert_eq!(config.top_k, 40);
        assert!(config.streaming);
    }

    #[test]
    fn test_generation_config_validation() {
        let mut config = GenerationConfig::default();
        assert!(config.validate().is_ok());

        config.temperature = -1.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_phi_model_ids() {
        assert_eq!(PhiModel::Phi2.model_id(), "microsoft/phi-2");
        assert!(PhiModel::Phi3Mini.model_id().contains("Phi-3"));
    }

    #[test]
    fn test_llama_model_ids() {
        assert!(LlamaModel::Llama32_1b.model_id().contains("Llama-3.2"));
        assert!(LlamaModel::Llama32_3b.model_id().contains("3B"));
    }

    #[test]
    fn test_quantization_memory_factor() {
        assert_eq!(Quantization::F32.memory_factor(), 1.0);
        assert_eq!(Quantization::F16.memory_factor(), 0.5);
        assert!(Quantization::Q4_0.memory_factor() < Quantization::Q8_0.memory_factor());
    }

    #[test]
    fn test_phi_node_creation() {
        let config = PhiConfig::default();
        let node = PhiNode::new("test-phi", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_llama_node_creation() {
        let config = LlamaConfig::default();
        let node = LlamaNode::new("test-llama", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "model": "phi-2",
            "quantization": "f16",
            "generation": {
                "max_tokens": 128,
                "temperature": 0.5
            }
        });

        let config = PhiConfig::from_json(&json);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.llm.generation.max_tokens, 128);
    }

    #[test]
    fn test_llm_config_validation() {
        let config = LlmConfig::default();
        assert!(config.validate().is_ok());
    }
}

#[cfg(not(feature = "llm"))]
mod llm_disabled_tests {
    #[test]
    fn test_llm_feature_disabled() {
        // LLM feature not enabled - verify compilation works
        assert!(true);
    }
}

mod sampling_tests {
    #[cfg(feature = "llm")]
    use remotemedia_candle_nodes::llm::sampling::Sampler;

    #[cfg(feature = "llm")]
    #[test]
    fn test_sampler_creation() {
        let sampler = Sampler::new(0.7, 0.9, 40, 1.1);
        // Sampler should be created successfully
        let _ = sampler;
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_greedy_sampling() {
        let sampler = Sampler::new(0.0, 1.0, 0, 1.0);
        let mut logits = vec![1.0, 2.0, 5.0, 0.5];
        let token = sampler.sample(&mut logits, &[]);
        assert_eq!(token, 2); // Highest logit at index 2
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_repeat_penalty_applied() {
        let sampler = Sampler::new(0.0, 1.0, 0, 2.0);
        let mut logits1 = vec![1.0, 2.0, 3.0];
        let mut logits2 = vec![1.0, 2.0, 3.0];

        let _ = sampler.sample(&mut logits1, &[]);
        let _ = sampler.sample(&mut logits2, &[2]);

        // With repeat penalty on token 2, logits2[2] should be reduced
        assert!(logits2[2] < logits1[2]);
    }
}
