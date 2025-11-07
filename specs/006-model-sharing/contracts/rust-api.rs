// Rust API Contract for Model Registry and Shared Memory Tensors
// This file documents the public API surface for the feature

use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;

// ============================================================================
// Model Registry API
// ============================================================================

/// Trait for models that can be managed by the registry
pub trait InferenceModel: Send + Sync + 'static {
    /// Unique identifier for this model type
    fn model_id(&self) -> &str;
    
    /// Device this model is loaded on
    fn device(&self) -> DeviceType;
    
    /// Memory usage in bytes
    fn memory_usage(&self) -> usize;
    
    /// Perform inference
    async fn infer(&self, input: &TensorBuffer) -> Result<TensorBuffer, Error>;
}

/// Handle to a loaded model with automatic reference counting
pub struct ModelHandle<T: InferenceModel> {
    inner: Arc<T>,
    handle_id: String,
    registry: Arc<ModelRegistry>,
}

impl<T: InferenceModel> ModelHandle<T> {
    /// Get the model for inference
    pub fn model(&self) -> &T {
        &self.inner
    }
    
    /// Get handle identifier
    pub fn handle_id(&self) -> &str {
        &self.handle_id
    }
}

impl<T: InferenceModel> Clone for ModelHandle<T> {
    fn clone(&self) -> Self {
        // Increment reference count
        self.registry.increment_ref(&self.inner.model_id());
        Self {
            inner: self.inner.clone(),
            handle_id: generate_handle_id(),
            registry: self.registry.clone(),
        }
    }
}

impl<T: InferenceModel> Drop for ModelHandle<T> {
    fn drop(&mut self) {
        // Decrement reference count
        self.registry.decrement_ref(&self.inner.model_id());
    }
}

/// Process-local model registry
pub struct ModelRegistry {
    config: RegistryConfig,
    // Internal implementation details hidden
}

impl ModelRegistry {
    /// Create a new registry with configuration
    pub fn new(config: RegistryConfig) -> Self;
    
    /// Get or load a model
    pub async fn get_or_load<T, L>(
        &self,
        key: &str,
        loader: L,
    ) -> Result<ModelHandle<T>, Error>
    where
        T: InferenceModel,
        L: FnOnce() -> Result<T, Error>;
    
    /// Release a model (decrements reference count)
    pub fn release(&self, key: &str);
    
    /// List all loaded models
    pub fn list_models(&self) -> Vec<ModelInfo>;
    
    /// Get registry metrics
    pub fn metrics(&self) -> RegistryMetrics;
    
    /// Force eviction of expired models
    pub fn evict_expired(&self) -> (usize, usize); // (count, freed_bytes)
    
    /// Clear all models (for testing)
    pub fn clear(&self);
}

/// Registry configuration
pub struct RegistryConfig {
    /// Time-to-live for idle models
    pub ttl: Duration,
    
    /// Maximum memory usage
    pub max_memory_bytes: Option<usize>,
    
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
    
    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(30),
            max_memory_bytes: None,
            eviction_policy: EvictionPolicy::Lru,
            enable_metrics: true,
        }
    }
}

pub enum EvictionPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// Time-based only
    Ttl,
    /// Manual only
    Manual,
}

// ============================================================================
// Shared Memory Tensor API
// ============================================================================

/// Enhanced tensor buffer with multiple storage backends
pub struct TensorBuffer {
    storage: TensorStorage,
    shape: Vec<usize>,
    dtype: DataType,
    strides: Vec<usize>,
}

impl TensorBuffer {
    /// Create from heap-allocated data
    pub fn from_vec(data: Vec<u8>, shape: Vec<usize>, dtype: DataType) -> Self;
    
    /// Create from shared memory
    pub fn from_shared_memory(
        region_id: &str,
        offset: usize,
        size: usize,
        shape: Vec<usize>,
        dtype: DataType,
    ) -> Result<Self, Error>;
    
    /// Create zero-copy view from NumPy array (via PyO3)
    #[cfg(feature = "python")]
    pub fn from_numpy(array: &PyArray) -> Result<Self, Error>;
    
    /// Create from DLPack capsule
    #[cfg(feature = "dlpack")]
    pub fn from_dlpack(capsule: *mut DLManagedTensor) -> Result<Self, Error>;
    
    /// Convert to DLPack for zero-copy export
    #[cfg(feature = "dlpack")]
    pub fn to_dlpack(&self) -> *mut DLManagedTensor;
    
    /// Get storage type
    pub fn storage(&self) -> &TensorStorage;
    
    /// Get shape
    pub fn shape(&self) -> &[usize];
    
    /// Get data type
    pub fn dtype(&self) -> DataType;
    
    /// Get raw bytes (may trigger copy from SHM/GPU)
    pub fn as_bytes(&self) -> &[u8];
    
    /// Check if tensor is contiguous
    pub fn is_contiguous(&self) -> bool;
}

/// Tensor storage backend
pub enum TensorStorage {
    /// Standard heap allocation
    Heap(Vec<u8>),
    
    /// Shared memory region
    SharedMemory {
        region: Arc<SharedMemoryRegion>,
        offset: usize,
        size: usize,
    },
    
    /// GPU memory
    #[cfg(feature = "cuda")]
    GpuMemory {
        device_ptr: *mut std::ffi::c_void,
        device_id: u32,
        size: usize,
    },
}

/// Shared memory region accessible by multiple processes
pub struct SharedMemoryRegion {
    id: String,
    size: usize,
    // Platform-specific handle hidden
}

impl SharedMemoryRegion {
    /// Create a new shared memory region
    pub fn create(size: usize) -> Result<Self, Error>;
    
    /// Open an existing shared memory region
    pub fn open(id: &str) -> Result<Self, Error>;
    
    /// Get region identifier
    pub fn id(&self) -> &str;
    
    /// Get region size
    pub fn size(&self) -> usize;
    
    /// Map region into process memory
    pub fn map(&self, offset: usize, len: usize) -> Result<&[u8], Error>;
    
    /// Map region as mutable
    pub fn map_mut(&self, offset: usize, len: usize) -> Result<&mut [u8], Error>;
}

/// Shared memory allocator for managing regions
pub struct SharedMemoryAllocator {
    config: AllocatorConfig,
}

impl SharedMemoryAllocator {
    /// Create a new allocator
    pub fn new(config: AllocatorConfig) -> Self;
    
    /// Allocate a tensor in shared memory
    pub fn allocate_tensor(
        &self,
        size: usize,
        session_id: Option<&str>,
    ) -> Result<TensorBuffer, Error>;
    
    /// Free a shared memory region
    pub fn free(&self, region_id: &str);
    
    /// Get allocator metrics
    pub fn metrics(&self) -> AllocatorMetrics;
}

// ============================================================================
// Model Worker API
// ============================================================================

/// Client for connecting to model worker processes
pub struct ModelWorkerClient {
    endpoint: String,
    // Internal connection details hidden
}

impl ModelWorkerClient {
    /// Connect to a model worker
    pub async fn connect(endpoint: &str) -> Result<Self, Error>;
    
    /// Submit inference request
    pub async fn infer(
        &self,
        input: TensorBuffer,
        parameters: HashMap<String, String>,
    ) -> Result<TensorBuffer, Error>;
    
    /// Submit streaming inference request
    pub fn infer_stream(
        &self,
        inputs: impl Stream<Item = TensorBuffer>,
    ) -> impl Stream<Item = Result<TensorBuffer, Error>>;
    
    /// Check worker health
    pub async fn health_check(&self) -> Result<bool, Error>;
    
    /// Get worker status
    pub async fn status(&self) -> Result<WorkerStatus, Error>;
}

/// Model worker process
pub struct ModelWorker<T: InferenceModel> {
    model: Arc<T>,
    config: WorkerConfig,
}

impl<T: InferenceModel> ModelWorker<T> {
    /// Create a new worker
    pub fn new(model: T, config: WorkerConfig) -> Self;
    
    /// Start serving requests
    pub async fn serve(self, endpoint: &str) -> Result<(), Error>;
}

// ============================================================================
// Supporting Types
// ============================================================================

pub enum DeviceType {
    Cpu,
    Cuda(u32),  // GPU index
    Metal(u32), // Metal device index
}

pub enum DataType {
    F32,
    F16,
    I32,
    I64,
    U8,
}

pub struct ModelInfo {
    pub model_id: String,
    pub device: DeviceType,
    pub memory_bytes: usize,
    pub reference_count: u32,
    pub loaded_at: std::time::SystemTime,
    pub last_accessed: std::time::SystemTime,
}

pub struct RegistryMetrics {
    pub total_models: usize,
    pub total_memory_bytes: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
}

pub struct AllocatorConfig {
    pub max_memory_bytes: usize,
    pub per_session_quota: Option<usize>,
    pub cleanup_interval: Duration,
}

pub struct AllocatorMetrics {
    pub regions_active: usize,
    pub bytes_allocated: usize,
    pub allocations_total: u64,
    pub frees_total: u64,
}

pub struct WorkerConfig {
    pub max_batch_size: usize,
    pub batch_timeout: Duration,
    pub max_concurrent_requests: usize,
}

pub struct WorkerStatus {
    pub worker_id: String,
    pub model_id: String,
    pub status: String,
    pub current_load: u32,
    pub total_requests: u64,
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    
    #[error("Out of memory: needed {needed} bytes, available {available}")]
    OutOfMemory { needed: usize, available: usize },
    
    #[error("Shared memory error: {0}")]
    SharedMemoryError(String),
    
    #[error("Worker error: {0}")]
    WorkerError(String),
    
    #[error("Invalid tensor shape or type")]
    InvalidTensor,
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
