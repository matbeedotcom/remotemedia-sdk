//! Model handle with reference counting

use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use super::InferenceModel;

/// Handle to a loaded model with automatic reference counting
pub struct ModelHandle<T: InferenceModel> {
    /// Inner model reference (type-erased to dyn InferenceModel)
    inner: Arc<dyn InferenceModel>,
    /// Model ID for registry operations
    model_id: String,
    /// Registry models map for cleanup
    registry_models: Arc<RwLock<HashMap<String, Arc<dyn InferenceModel>>>>,
    /// Phantom data to maintain type parameter
    _phantom: std::marker::PhantomData<T>,
}

impl<T: InferenceModel> ModelHandle<T> {
    /// Create a new handle (internal use only)
    pub(crate) fn new(
        inner: Arc<dyn InferenceModel>,
        model_id: String,
        registry_models: Arc<RwLock<HashMap<String, Arc<dyn InferenceModel>>>>,
    ) -> Self {
        Self {
            inner,
            model_id,
            registry_models,
            _phantom: std::marker::PhantomData,
        }
    }
    
    /// Get the model for inference
    pub fn model(&self) -> &dyn InferenceModel {
        &*self.inner
    }
    
    /// Get model identifier
    pub fn model_id(&self) -> &str {
        &self.model_id
    }
}

impl<T: InferenceModel> Clone for ModelHandle<T> {
    fn clone(&self) -> Self {
        // Arc::clone increments the reference count automatically
        Self {
            inner: Arc::clone(&self.inner),
            model_id: self.model_id.clone(),
            registry_models: Arc::clone(&self.registry_models),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T: InferenceModel> Drop for ModelHandle<T> {
    fn drop(&mut self) {
        // When handle is dropped, Arc reference count decreases automatically
        // Check if this is the last non-registry reference
        let ref_count = Arc::strong_count(&self.inner);
        
        // If only the registry holds a reference (count = 1 or 2 depending on timing)
        // the cache manager will handle eviction based on TTL
        if ref_count <= 2 {
            // Model is now eligible for eviction
            tracing::debug!(
                "Model {} now eligible for eviction (ref_count: {})",
                self.model_id,
                ref_count
            );
        }
    }
}
