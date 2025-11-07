//! Model Registry for efficient model sharing across nodes
//!
//! This module provides a process-local registry that maintains single instances
//! of loaded models, enabling multiple nodes to share the same model in memory.

use async_trait::async_trait;
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::RwLock;
use anyhow::Result;

pub mod error;
pub mod handle;
pub mod cache;
pub mod config;
pub mod metrics;

pub use error::ModelRegistryError;
pub use handle::ModelHandle;
pub use cache::{EvictionPolicy, CacheManager};
pub use config::RegistryConfig;
pub use metrics::RegistryMetrics;

use crate::tensor::TensorBuffer;

/// Device types for model placement
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeviceType {
    /// CPU device
    Cpu,
    /// CUDA GPU with device index
    Cuda(u32),
    /// Metal GPU with device index
    Metal(u32),
}

impl DeviceType {
    /// Get device string representation
    pub fn as_str(&self) -> String {
        match self {
            DeviceType::Cpu => "cpu".to_string(),
            DeviceType::Cuda(idx) => format!("cuda:{}", idx),
            DeviceType::Metal(idx) => format!("metal:{}", idx),
        }
    }
}

/// Trait for models that can be managed by the registry
#[async_trait]
pub trait InferenceModel: Send + Sync + 'static {
    /// Unique identifier for this model type
    fn model_id(&self) -> &str;
    
    /// Device this model is loaded on
    fn device(&self) -> DeviceType;
    
    /// Memory usage in bytes
    fn memory_usage(&self) -> usize;
    
    /// Perform inference
    async fn infer(&self, input: &TensorBuffer) -> Result<TensorBuffer>;
}

/// Process-local model registry
pub struct ModelRegistry {
    /// Loaded models indexed by ID
    models: Arc<RwLock<HashMap<String, Arc<dyn InferenceModel>>>>,
    /// Models currently being loaded (to prevent duplicate loads)
    loading: Arc<RwLock<HashSet<String>>>,
    /// Registry configuration
    config: RegistryConfig,
    /// Metrics tracking
    metrics: Arc<RwLock<RegistryMetrics>>,
}

impl ModelRegistry {
    /// Create a new registry with configuration
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            models: Arc::new(RwLock::new(HashMap::new())),
            loading: Arc::new(RwLock::new(HashSet::new())),
            config,
            metrics: Arc::new(RwLock::new(RegistryMetrics::default())),
        }
    }
    
    /// Get or load a model with singleton semantics
    pub async fn get_or_load<T, L>(
        &self,
        key: &str,
        loader: L,
    ) -> Result<ModelHandle<T>>
    where
        T: InferenceModel,
        L: FnOnce() -> Result<T> + Send + 'static,
    {
        // Fast path: check if model is already loaded
        {
            let models = self.models.read().unwrap();
            if let Some(model) = models.get(key) {
                // Record cache hit
                {
                    let mut metrics = self.metrics.write().unwrap();
                    metrics.cache_hits += 1;
                }
                
                // Clone the Arc (increments reference count)
                return Ok(ModelHandle::new(
                    model.clone(),
                    key.to_string(),
                    Arc::clone(&self.models),
                ));
            }
        }
        
        // Check if another task is loading this model
        loop {
            {
                let mut loading = self.loading.write().unwrap();
                if !loading.contains(key) {
                    // Mark as loading
                    loading.insert(key.to_string());
                    break;
                }
            }
            // Wait a bit and check again
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        
        // Load the model
        let model = match tokio::task::spawn_blocking(loader).await {
            Ok(result) => result?,
            Err(e) => {
                // Clean up loading state
                self.loading.write().unwrap().remove(key);
                return Err(anyhow::anyhow!("Failed to load model: {}", e));
            }
        };
        
        // Record cache miss
        {
            let mut metrics = self.metrics.write().unwrap();
            metrics.cache_misses += 1;
            metrics.total_models += 1;
            metrics.total_memory_bytes += model.memory_usage() as u64;
        }
        
        // Store the model
        let model_arc = Arc::new(model) as Arc<dyn InferenceModel>;
        {
            let mut models = self.models.write().unwrap();
            models.insert(key.to_string(), Arc::clone(&model_arc));
        }
        
        // Remove from loading set
        self.loading.write().unwrap().remove(key);
        
        Ok(ModelHandle::new(
            model_arc,
            key.to_string(),
            Arc::clone(&self.models),
        ))
    }
    
    /// Release a model (decrement reference count)
    pub fn release(&self, key: &str) {
        // Check reference count and potentially evict
        let should_evict = {
            let models = self.models.read().unwrap();
            if let Some(model) = models.get(key) {
                Arc::strong_count(model) <= 2 // Only registry and this check hold refs
            } else {
                false
            }
        };
        
        if should_evict {
            // Model is no longer in use, consider for eviction
            // Actual eviction happens via cache manager based on TTL
        }
    }
    
    /// List all loaded models
    pub fn list_models(&self) -> Vec<ModelInfo> {
        let models = self.models.read().unwrap();
        models.iter().map(|(id, model)| {
            ModelInfo {
                model_id: id.clone(),
                device: model.device(),
                memory_bytes: model.memory_usage(),
                reference_count: Arc::strong_count(model) as u32 - 1, // Subtract registry ref
            }
        }).collect()
    }
    
    /// Get registry metrics
    pub fn metrics(&self) -> RegistryMetrics {
        self.metrics.read().unwrap().clone()
    }
    
    /// Force eviction of expired models
    pub fn evict_expired(&self) -> (usize, usize) {
        // TODO: Implement with cache manager
        (0, 0)
    }
    
    /// Clear all models (for testing)
    pub fn clear(&self) {
        let mut models = self.models.write().unwrap();
        models.clear();
        
        let mut metrics = self.metrics.write().unwrap();
        *metrics = RegistryMetrics::default();
    }
}

/// Information about a loaded model
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model identifier
    pub model_id: String,
    /// Device placement
    pub device: DeviceType,
    /// Memory usage in bytes
    pub memory_bytes: usize,
    /// Current reference count
    pub reference_count: u32,
}

use std::collections::HashSet;
