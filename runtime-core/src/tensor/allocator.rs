//! Shared memory allocator for tensor storage

use super::shared_memory::SharedMemoryRegion;
use super::{TensorBuffer, TensorStorage, DataType, DeviceType};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Configuration for shared memory allocator
#[derive(Debug, Clone)]
pub struct AllocatorConfig {
    /// Maximum total memory in bytes
    pub max_memory_bytes: usize,
    /// Per-session quota (optional)
    pub per_session_quota: Option<usize>,
    /// Cleanup interval
    pub cleanup_interval: Duration,
}

impl Default for AllocatorConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 10 * 1024 * 1024 * 1024, // 10GB
            per_session_quota: None,
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

/// Metadata for allocated regions
#[derive(Debug, Clone)]
struct AllocationMetadata {
    region_id: String,
    size: usize,
    session_id: Option<String>,
    allocated_at: Instant,
    last_accessed: Instant,
}

/// Allocator metrics
#[derive(Debug, Clone, Default)]
pub struct AllocatorMetrics {
    /// Active regions
    pub regions_active: usize,
    /// Bytes currently allocated
    pub bytes_allocated: usize,
    /// Total allocations
    pub allocations_total: u64,
    /// Total frees
    pub frees_total: u64,
}

/// Shared memory allocator for managing tensor storage
pub struct SharedMemoryAllocator {
    /// Configuration
    config: AllocatorConfig,
    /// Active allocations
    allocations: Arc<RwLock<HashMap<String, AllocationMetadata>>>,
    /// Per-session usage tracking
    session_usage: Arc<RwLock<HashMap<String, usize>>>,
    /// Metrics
    metrics: Arc<RwLock<AllocatorMetrics>>,
}

impl SharedMemoryAllocator {
    /// Create a new allocator
    pub fn new(config: AllocatorConfig) -> Self {
        Self {
            config,
            allocations: Arc::new(RwLock::new(HashMap::new())),
            session_usage: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(AllocatorMetrics::default())),
        }
    }
    
    /// Allocate a tensor in shared memory
    pub fn allocate_tensor(
        &self,
        size: usize,
        session_id: Option<&str>,
    ) -> Result<TensorBuffer> {
        // Check per-session quota
        if let (Some(quota), Some(sid)) = (&self.config.per_session_quota, session_id) {
            let session_usage = self.session_usage.read().unwrap();
            let current_usage = session_usage.get(sid).copied().unwrap_or(0);
            
            if current_usage + size > *quota {
                anyhow::bail!(
                    "Session quota exceeded: {} + {} > {}",
                    current_usage,
                    size,
                    quota
                );
            }
        }
        
        // Check total memory limit
        let metrics = self.metrics.read().unwrap();
        if metrics.bytes_allocated + size > self.config.max_memory_bytes {
            anyhow::bail!(
                "Memory limit exceeded: {} + {} > {}",
                metrics.bytes_allocated,
                size,
                self.config.max_memory_bytes
            );
        }
        drop(metrics);
        
        // Create shared memory region
        let region = SharedMemoryRegion::create(size)?;
        let region_id = region.id().to_string();
        
        // Record allocation
        {
            let mut allocations = self.allocations.write().unwrap();
            let now = Instant::now();
            allocations.insert(
                region_id.clone(),
                AllocationMetadata {
                    region_id: region_id.clone(),
                    size,
                    session_id: session_id.map(|s| s.to_string()),
                    allocated_at: now,
                    last_accessed: now,
                },
            );
        }
        
        // Update session usage
        if let Some(sid) = session_id {
            let mut session_usage = self.session_usage.write().unwrap();
            *session_usage.entry(sid.to_string()).or_insert(0) += size;
        }
        
        // Update metrics
        {
            let mut metrics = self.metrics.write().unwrap();
            metrics.regions_active += 1;
            metrics.bytes_allocated += size;
            metrics.allocations_total += 1;
        }
        
        tracing::debug!(
            "Allocated shared memory tensor: {} bytes (region: {})",
            size,
            region_id
        );
        
        // Create tensor with shared memory storage
        Ok(TensorBuffer {
            storage: TensorStorage::SharedMemory {
                region: Arc::new(region),
                offset: 0,
                size,
            },
            shape: vec![size],  // 1D tensor by default
            dtype: DataType::U8,
            strides: vec![1],
            device: DeviceType::Cpu,
        })
    }
    
    /// Free a shared memory region
    pub fn free(&self, region_id: &str) {
        let mut freed_size = 0;
        let mut session_id = None;
        
        // Remove allocation metadata
        {
            let mut allocations = self.allocations.write().unwrap();
            if let Some(meta) = allocations.remove(region_id) {
                freed_size = meta.size;
                session_id = meta.session_id;
            }
        }
        
        if freed_size > 0 {
            // Update session usage
            if let Some(sid) = session_id {
                let mut session_usage = self.session_usage.write().unwrap();
                if let Some(usage) = session_usage.get_mut(&sid) {
                    *usage = usage.saturating_sub(freed_size);
                }
            }
            
            // Update metrics
            {
                let mut metrics = self.metrics.write().unwrap();
                metrics.regions_active = metrics.regions_active.saturating_sub(1);
                metrics.bytes_allocated = metrics.bytes_allocated.saturating_sub(freed_size);
                metrics.frees_total += 1;
            }
            
            tracing::debug!("Freed shared memory region: {} ({} bytes)", region_id, freed_size);
        }
    }
    
    /// Clean up expired regions based on TTL
    pub fn cleanup_expired(&self, ttl: Duration) -> (usize, usize) {
        let now = Instant::now();
        let mut to_remove = Vec::new();
        
        // Find expired allocations
        {
            let allocations = self.allocations.read().unwrap();
            for (region_id, meta) in allocations.iter() {
                let age = now.duration_since(meta.last_accessed);
                if age > ttl {
                    to_remove.push(region_id.clone());
                }
            }
        }
        
        // Free them
        let count = to_remove.len();
        let mut total_freed = 0;
        
        for region_id in to_remove {
            let size = {
                let allocations = self.allocations.read().unwrap();
                allocations.get(&region_id).map(|m| m.size).unwrap_or(0)
            };
            
            self.free(&region_id);
            total_freed += size;
        }
        
        if count > 0 {
            tracing::info!(
                "Cleaned up {} expired shared memory regions ({} bytes)",
                count,
                total_freed
            );
        }
        
        (count, total_freed)
    }
    
    /// Get allocator metrics
    pub fn metrics(&self) -> AllocatorMetrics {
        self.metrics.read().unwrap().clone()
    }
    
    /// Get per-session usage
    pub fn session_usage(&self, session_id: &str) -> usize {
        self.session_usage.read().unwrap().get(session_id).copied().unwrap_or(0)
    }
}

impl TensorBuffer {
    /// Create a tensor from shared memory
    #[cfg(feature = "shared-memory")]
    pub fn from_shared_memory(
        region_id: &str,
        offset: usize,
        size: usize,
        shape: Vec<usize>,
        dtype: DataType,
    ) -> Result<Self> {
        let region = SharedMemoryRegion::open(region_id, size)?;
        let strides = Self::compute_strides(&shape);
        
        Ok(Self {
            storage: TensorStorage::SharedMemory {
                region: Arc::new(region),
                offset,
                size,
            },
            shape,
            dtype,
            strides,
            device: DeviceType::Cpu,
        })
    }
    
    /// Create a tensor from shared memory (without feature)
    #[cfg(not(feature = "shared-memory"))]
    pub fn from_shared_memory(
        _region_id: &str,
        _offset: usize,
        _size: usize,
        _shape: Vec<usize>,
        _dtype: DataType,
    ) -> Result<Self> {
        anyhow::bail!("Shared memory feature not enabled");
    }
}

