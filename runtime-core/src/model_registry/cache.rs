//! Cache management for model registry

use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Eviction policies for model cache
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// Time-based only (TTL)
    Ttl,
    /// Manual eviction only
    Manual,
}

/// Metadata for cached models
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Model identifier
    pub model_id: String,
    /// Last access timestamp
    pub last_accessed: Instant,
    /// Load timestamp
    pub loaded_at: Instant,
    /// Access count (for LFU)
    pub access_count: u64,
}

/// Cache manager for model eviction
pub struct CacheManager {
    /// Eviction policy
    policy: EvictionPolicy,
    /// TTL duration
    ttl: Duration,
    /// Cache entries metadata
    entries: HashMap<String, CacheEntry>,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new(policy: EvictionPolicy, ttl: Duration) -> Self {
        Self {
            policy,
            ttl,
            entries: HashMap::new(),
        }
    }
    
    /// Record model access
    pub fn record_access(&mut self, model_id: &str) {
        if let Some(entry) = self.entries.get_mut(model_id) {
            entry.last_accessed = Instant::now();
            entry.access_count += 1;
        } else {
            // New entry
            let now = Instant::now();
            self.entries.insert(model_id.to_string(), CacheEntry {
                model_id: model_id.to_string(),
                last_accessed: now,
                loaded_at: now,
                access_count: 1,
            });
        }
    }
    
    /// Get models eligible for eviction based on policy
    pub fn get_eviction_candidates(&self, current_refs: &HashMap<String, usize>) -> Vec<String> {
        let now = Instant::now();
        let mut candidates = Vec::new();
        
        for (model_id, entry) in &self.entries {
            // Check if model has no external references
            let ref_count = current_refs.get(model_id).copied().unwrap_or(0);
            if ref_count > 1 {
                // Model still in use, skip
                continue;
            }
            
            // Check based on policy
            match self.policy {
                EvictionPolicy::Ttl => {
                    let age = now.duration_since(entry.last_accessed);
                    if age > self.ttl {
                        candidates.push(model_id.clone());
                    }
                }
                EvictionPolicy::Manual => {
                    // No automatic eviction
                }
                EvictionPolicy::Lru | EvictionPolicy::Lfu => {
                    // For LRU/LFU, also check TTL first
                    let age = now.duration_since(entry.last_accessed);
                    if age > self.ttl {
                        candidates.push(model_id.clone());
                    }
                }
            }
        }
        
        // Sort candidates based on policy
        match self.policy {
            EvictionPolicy::Lru => {
                // Sort by last accessed (oldest first)
                candidates.sort_by_key(|id| {
                    self.entries.get(id).map(|e| e.last_accessed).unwrap_or(Instant::now())
                });
            }
            EvictionPolicy::Lfu => {
                // Sort by access count (least accessed first)
                candidates.sort_by_key(|id| {
                    self.entries.get(id).map(|e| e.access_count).unwrap_or(0)
                });
            }
            _ => {}
        }
        
        candidates
    }
    
    /// Remove an entry from cache tracking
    pub fn remove_entry(&mut self, model_id: &str) {
        self.entries.remove(model_id);
    }
    
    /// Get all tracked entries
    pub fn entries(&self) -> &HashMap<String, CacheEntry> {
        &self.entries
    }
}
