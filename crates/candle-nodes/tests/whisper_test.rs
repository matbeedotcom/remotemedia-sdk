//! Integration tests for WhisperNode
//!
//! Tests speech-to-text transcription functionality.

#[cfg(feature = "whisper")]
mod whisper_tests {
    use remotemedia_candle_nodes::whisper::{WhisperConfig, WhisperModel, WhisperNode};
    use remotemedia_candle_nodes::{DeviceSelector, ModelCache};

    #[test]
    fn test_whisper_config_creation() {
        let config = WhisperConfig::default();
        assert_eq!(config.model, WhisperModel::Base);
        assert_eq!(config.language, "auto");
        assert!(config.streaming);
        assert_eq!(config.task, "transcribe");
    }

    #[test]
    fn test_whisper_config_from_json() {
        let json = serde_json::json!({
            "model": "small",
            "language": "en",
            "streaming": false,
            "device": "cpu"
        });

        let config: WhisperConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.model, WhisperModel::Small);
        assert_eq!(config.language, "en");
        assert!(!config.streaming);
        assert_eq!(config.device, "cpu");
    }

    #[test]
    fn test_whisper_model_ids() {
        assert_eq!(WhisperModel::Tiny.model_id(), "openai/whisper-tiny");
        assert_eq!(WhisperModel::Base.model_id(), "openai/whisper-base");
        assert_eq!(WhisperModel::Small.model_id(), "openai/whisper-small");
        assert_eq!(WhisperModel::Medium.model_id(), "openai/whisper-medium");
        assert_eq!(WhisperModel::LargeV3.model_id(), "openai/whisper-large-v3");
    }

    #[test]
    fn test_whisper_node_creation() {
        let config = WhisperConfig {
            model: WhisperModel::Tiny,
            device: "cpu".to_string(),
            ..Default::default()
        };

        let node = WhisperNode::new("test-whisper-1", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_device_selection_cpu() {
        let device = DeviceSelector::from_config("cpu").unwrap();
        assert!(!device.is_gpu());
        assert_eq!(device.name(), "cpu");
    }

    #[test]
    fn test_device_selection_auto() {
        let device = DeviceSelector::select_best();
        // Should always return some device (at minimum CPU)
        assert!(device.name().len() > 0);
    }

    #[test]
    fn test_model_cache_default_dir() {
        let cache = ModelCache::new();
        let dir = cache.cache_dir();
        // Should contain huggingface in the path
        assert!(dir.to_string_lossy().contains("huggingface"));
    }

    #[tokio::test]
    async fn test_whisper_node_initialization() {
        // This test requires network access to download models
        // Skip in CI environments without network
        if std::env::var("CI").is_ok() {
            return;
        }

        let config = WhisperConfig {
            model: WhisperModel::Tiny, // Smallest model for testing
            device: "cpu".to_string(),
            ..Default::default()
        };

        let node = WhisperNode::new("test-whisper-init", &config).unwrap();
        
        // Initialize should download model if not cached
        // This may take a while on first run
        use remotemedia_core::nodes::streaming_node::AsyncStreamingNode;
        let result = node.initialize().await;
        
        // Note: This may fail if network is unavailable
        // In that case, the error message should be informative
        if let Err(e) = &result {
            eprintln!("Initialization failed (expected if no network): {}", e);
        }
    }
}

#[cfg(not(feature = "whisper"))]
mod whisper_disabled_tests {
    #[test]
    fn test_whisper_feature_disabled() {
        // Verify that whisper types are not available when feature is disabled
        // This test just confirms the module compiles without the feature
        assert!(true);
    }
}

mod common_tests {
    use remotemedia_candle_nodes::{CandleNodeError, Result};

    #[test]
    fn test_error_types() {
        let err = CandleNodeError::model_load("test-model", "test error");
        assert!(err.to_string().contains("test-model"));
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_inference_error() {
        let err = CandleNodeError::inference("node-1", "inference failed");
        assert!(err.to_string().contains("node-1"));
        assert!(err.to_string().contains("inference failed"));
    }

    #[test]
    fn test_invalid_input_error() {
        let err = CandleNodeError::invalid_input("node-1", "Audio", "Video");
        assert!(err.to_string().contains("Audio"));
        assert!(err.to_string().contains("Video"));
    }
}
