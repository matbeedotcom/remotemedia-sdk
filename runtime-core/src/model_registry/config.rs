//! Configuration for model registry

use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Model eviction policies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvictionPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// Time-based only
    Ttl,
    /// Manual eviction only
    Manual,
}

impl Default for EvictionPolicy {
    fn default() -> Self {
        EvictionPolicy::Lru
    }
}

/// Registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Time-to-live for idle models
    pub ttl: Duration,
    
    /// Maximum memory usage in bytes
    pub max_memory_bytes: Option<usize>,
    
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
    
    /// Enable metrics collection
    pub enable_metrics: bool,
    
    /// Maximum number of models
    pub max_models: Option<usize>,
    
    /// Enable automatic cleanup
    pub auto_cleanup: bool,
    
    /// Cleanup interval
    pub cleanup_interval: Duration,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(30),
            max_memory_bytes: None,
            eviction_policy: EvictionPolicy::Lru,
            enable_metrics: true,
            max_models: None,
            auto_cleanup: true,
            cleanup_interval: Duration::from_secs(10),
        }
    }
}

impl RegistryConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set TTL for idle models
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }
    
    /// Set maximum memory usage
    pub fn with_max_memory(mut self, bytes: usize) -> Self {
        self.max_memory_bytes = Some(bytes);
        self
    }
    
    /// Set eviction policy
    pub fn with_eviction_policy(mut self, policy: EvictionPolicy) -> Self {
        self.eviction_policy = policy;
        self
    }
    
    /// Set maximum number of models
    pub fn with_max_models(mut self, count: usize) -> Self {
        self.max_models = Some(count);
        self
    }
}
