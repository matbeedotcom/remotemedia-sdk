//! Error types for tensor and shared memory operations

use thiserror::Error;

/// Tensor and shared memory errors
#[derive(Debug, Error)]
pub enum TensorError {
    /// Shared memory creation failed
    #[error("Failed to create shared memory region: {0}")]
    SharedMemoryCreationFailed(String),
    
    /// Shared memory mapping failed
    #[error("Failed to map shared memory: {0}")]
    SharedMemoryMappingFailed(String),
    
    /// Shared memory region not found
    #[error("Shared memory region not found: {0}")]
    SharedMemoryNotFound(String),
    
    /// Invalid tensor shape
    #[error("Invalid tensor shape: {0:?}")]
    InvalidShape(Vec<usize>),
    
    /// Invalid tensor data type
    #[error("Invalid tensor data type")]
    InvalidDataType,
    
    /// Tensor size mismatch
    #[error("Tensor size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { 
        /// Expected size
        expected: usize, 
        /// Actual size
        actual: usize 
    },
    
    /// Out of shared memory
    #[error("Out of shared memory: needed {needed} bytes, available {available}")]
    OutOfSharedMemory { 
        /// Bytes needed
        needed: usize, 
        /// Bytes available
        available: usize 
    },
    
    /// Session quota exceeded
    #[error("Session {0} exceeded shared memory quota")]
    QuotaExceeded(String),
    
    /// Platform not supported
    #[error("Platform not supported for shared memory: {0}")]
    PlatformNotSupported(String),
    
    /// GPU memory error
    #[error("GPU memory error: {0}")]
    GpuMemoryError(String),
    
    /// Zero-copy not available
    #[error("Zero-copy not available: {0}")]
    ZeroCopyUnavailable(String),
    
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Other error
    #[error("Tensor error: {0}")]
    Other(String),
}
