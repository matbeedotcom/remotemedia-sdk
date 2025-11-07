//! Integration tests for process-local model sharing

use remotemedia_runtime_core::model_registry::{
    ModelRegistry, RegistryConfig, InferenceModel, DeviceType,
};
use remotemedia_runtime_core::tensor::TensorBuffer;
use async_trait::async_trait;
use anyhow::Result;
use std::sync::Arc;

/// Mock model for testing
struct MockModel {
    id: String,
    device: DeviceType,
    memory_bytes: usize,
}

#[async_trait]
impl InferenceModel for MockModel {
    fn model_id(&self) -> &str {
        &self.id
    }
    
    fn device(&self) -> DeviceType {
        self.device.clone()
    }
    
    fn memory_usage(&self) -> usize {
        self.memory_bytes
    }
    
    async fn infer(&self, _input: &TensorBuffer) -> Result<TensorBuffer> {
        // Mock inference - return empty tensor
        Ok(TensorBuffer::default())
    }
}

#[tokio::test]
async fn test_model_sharing_single_instance() {
    // Test that two nodes requesting the same model get the same instance
    let registry = Arc::new(ModelRegistry::new(RegistryConfig::default()));
    
    let model_key = "test-model-1";
    let memory_size = 1024 * 1024; // 1MB
    
    // First load
    let handle1 = registry.get_or_load(model_key, || {
        Ok(MockModel {
            id: model_key.to_string(),
            device: DeviceType::Cpu,
            memory_bytes: memory_size,
        })
    }).await.expect("Failed to load model first time");
    
    // Second load (should hit cache)
    let handle2 = registry.get_or_load(model_key, || {
        Ok(MockModel {
            id: model_key.to_string(),
            device: DeviceType::Cpu,
            memory_bytes: memory_size,
        })
    }).await.expect("Failed to load model second time");
    
    // Verify both handles point to the same model
    assert_eq!(handle1.model_id(), handle2.model_id());
    
    // Check metrics
    let metrics = registry.metrics();
    assert_eq!(metrics.cache_hits, 1, "Should have 1 cache hit");
    assert_eq!(metrics.cache_misses, 1, "Should have 1 cache miss");
    assert_eq!(metrics.total_models, 1, "Should have 1 model loaded");
}

#[tokio::test]
async fn test_model_metrics_tracking() {
    let registry = Arc::new(ModelRegistry::new(RegistryConfig::default()));
    
    // Load first model
    let _handle1 = registry.get_or_load("model-a", || {
        Ok(MockModel {
            id: "model-a".to_string(),
            device: DeviceType::Cpu,
            memory_bytes: 512 * 1024,
        })
    }).await.expect("Failed to load model-a");
    
    // Load second model
    let _handle2 = registry.get_or_load("model-b", || {
        Ok(MockModel {
            id: "model-b".to_string(),
            device: DeviceType::Cuda(0),
            memory_bytes: 1024 * 1024,
        })
    }).await.expect("Failed to load model-b");
    
    // Access first model again (cache hit)
    let _handle3 = registry.get_or_load("model-a", || {
        Ok(MockModel {
            id: "model-a".to_string(),
            device: DeviceType::Cpu,
            memory_bytes: 512 * 1024,
        })
    }).await.expect("Failed to reload model-a");
    
    // Verify metrics
    let metrics = registry.metrics();
    assert_eq!(metrics.total_models, 2, "Should have 2 models");
    assert_eq!(metrics.cache_hits, 1, "Should have 1 hit");
    assert_eq!(metrics.cache_misses, 2, "Should have 2 misses");
    assert!(metrics.total_memory_bytes > 0, "Should track memory usage");
}

#[tokio::test]
async fn test_concurrent_model_loading() {
    // Test that concurrent requests for the same model result in single load
    let registry = Arc::new(ModelRegistry::new(RegistryConfig::default()));
    let reg1 = Arc::clone(&registry);
    let reg2 = Arc::clone(&registry);
    
    let model_key = "concurrent-model";
    
    // Launch two concurrent load requests
    let handle1 = tokio::spawn(async move {
        reg1.get_or_load(model_key, || {
            // Simulate slow load
            std::thread::sleep(std::time::Duration::from_millis(100));
            Ok(MockModel {
                id: model_key.to_string(),
                device: DeviceType::Cpu,
                memory_bytes: 1024,
            })
        }).await
    });
    
    let handle2 = tokio::spawn(async move {
        // Start slightly after first request
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        reg2.get_or_load(model_key, || {
            Ok(MockModel {
                id: model_key.to_string(),
                device: DeviceType::Cpu,
                memory_bytes: 1024,
            })
        }).await
    });
    
    // Both should succeed
    let result1 = handle1.await.expect("Task 1 panicked").expect("Load 1 failed");
    let result2 = handle2.await.expect("Task 2 panicked").expect("Load 2 failed");
    
    assert_eq!(result1.model_id(), result2.model_id());
}

#[tokio::test]
async fn test_list_models() {
    let registry = Arc::new(ModelRegistry::new(RegistryConfig::default()));
    
    // Initially empty
    let models = registry.list_models();
    assert_eq!(models.len(), 0, "Registry should start empty");
    
    // Load some models
    let _h1 = registry.get_or_load("model-1", || {
        Ok(MockModel {
            id: "model-1".to_string(),
            device: DeviceType::Cpu,
            memory_bytes: 1024,
        })
    }).await.expect("Failed to load model-1");
    
    let _h2 = registry.get_or_load("model-2", || {
        Ok(MockModel {
            id: "model-2".to_string(),
            device: DeviceType::Cuda(0),
            memory_bytes: 2048,
        })
    }).await.expect("Failed to load model-2");
    
    // List should show both models
    let models = registry.list_models();
    assert_eq!(models.len(), 2, "Should have 2 models");
    
    // Verify model IDs are present
    let model_ids: Vec<String> = models.iter().map(|m| m.model_id.clone()).collect();
    assert!(model_ids.contains(&"model-1".to_string()));
    assert!(model_ids.contains(&"model-2".to_string()));
}

#[tokio::test]
async fn test_registry_clear() {
    let registry = Arc::new(ModelRegistry::new(RegistryConfig::default()));
    
    // Load a model
    let _handle = registry.get_or_load("test-model", || {
        Ok(MockModel {
            id: "test-model".to_string(),
            device: DeviceType::Cpu,
            memory_bytes: 1024,
        })
    }).await.expect("Failed to load model");
    
    // Verify it's loaded
    let models = registry.list_models();
    assert_eq!(models.len(), 1);
    
    // Clear registry
    registry.clear();
    
    // Verify empty
    let models = registry.list_models();
    assert_eq!(models.len(), 0, "Registry should be empty after clear");
    
    let metrics = registry.metrics();
    assert_eq!(metrics.total_models, 0);
    assert_eq!(metrics.total_memory_bytes, 0);
}

