//! Metrics tracking for model registry

/// Registry performance metrics
#[derive(Debug, Clone, Default)]
pub struct RegistryMetrics {
    /// Total models currently loaded
    pub total_models: usize,
    /// Total memory used in bytes
    pub total_memory_bytes: u64,
    /// Cache hits
    pub cache_hits: u64,
    /// Cache misses
    pub cache_misses: u64,
    /// Number of evictions
    pub evictions: u64,
}

impl RegistryMetrics {
    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        let hits = self.cache_hits as f64;
        let misses = self.cache_misses as f64;
        let total = hits + misses;
        if total > 0.0 {
            hits / total
        } else {
            0.0
        }
    }
    
    /// Increment cache hits
    pub fn increment_hits(&mut self) {
        self.cache_hits += 1;
    }
    
    /// Increment cache misses
    pub fn increment_misses(&mut self) {
        self.cache_misses += 1;
    }
    
    /// Increment evictions
    pub fn increment_evictions(&mut self) {
        self.evictions += 1;
    }
    
    /// Update model count and memory
    pub fn update_model_stats(&mut self, models: usize, memory: u64) {
        self.total_models = models;
        self.total_memory_bytes = memory;
    }
}
