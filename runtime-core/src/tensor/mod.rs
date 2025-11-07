//! Tensor module for shared memory and zero-copy tensor operations
//!
//! This module provides tensor abstractions with multiple storage backends
//! including heap allocation, shared memory, and GPU memory.

use std::sync::Arc;
use anyhow::Result;

pub mod error;
pub mod capabilities;
#[cfg(feature = "shared-memory")]
pub mod shared_memory;
#[cfg(feature = "shared-memory")]
pub mod allocator;

pub use error::TensorError;
pub use capabilities::TensorCapabilities;

#[cfg(feature = "shared-memory")]
pub use shared_memory::SharedMemoryRegion;
#[cfg(feature = "shared-memory")]
pub use allocator::{SharedMemoryAllocator, AllocatorConfig, AllocatorMetrics};

/// Data types for tensors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    /// 32-bit floating point
    F32,
    /// 16-bit floating point
    F16,
    /// 32-bit signed integer
    I32,
    /// 64-bit signed integer
    I64,
    /// 8-bit unsigned integer
    U8,
}

impl DataType {
    /// Get size in bytes for this data type
    pub fn size_bytes(&self) -> usize {
        match self {
            DataType::F32 => 4,
            DataType::F16 => 2,
            DataType::I32 => 4,
            DataType::I64 => 8,
            DataType::U8 => 1,
        }
    }
}

/// Device types for tensor placement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    /// CPU device
    Cpu,
    /// CUDA GPU with device index
    Cuda(u32),
    /// Metal GPU with device index
    Metal(u32),
}

/// Storage backend for tensors
#[derive(Debug, Clone)]
pub enum TensorStorage {
    /// Standard heap allocation
    Heap(Vec<u8>),
    
    /// Shared memory region
    #[cfg(feature = "shared-memory")]
    SharedMemory {
        /// Shared memory region
        region: Arc<shared_memory::SharedMemoryRegion>,
        /// Offset within region
        offset: usize,
        /// Size in bytes
        size: usize,
    },
    
    /// GPU memory (future enhancement)
    #[cfg(feature = "cuda")]
    GpuMemory {
        /// Device pointer
        device_ptr: *mut std::ffi::c_void,
        /// GPU device index
        device_id: u32,
        /// Size in bytes
        size: usize,
    },
}

// Mark TensorStorage as Send + Sync (carefully!)
unsafe impl Send for TensorStorage {}
unsafe impl Sync for TensorStorage {}

/// Enhanced tensor buffer with multiple storage backends
#[derive(Debug, Clone)]
pub struct TensorBuffer {
    /// Storage backend
    storage: TensorStorage,
    /// Tensor shape
    shape: Vec<usize>,
    /// Data type
    dtype: DataType,
    /// Memory strides
    strides: Vec<usize>,
    /// Device placement
    device: DeviceType,
}

impl Default for TensorBuffer {
    fn default() -> Self {
        Self {
            storage: TensorStorage::Heap(Vec::new()),
            shape: vec![0],
            dtype: DataType::F32,
            strides: vec![1],
            device: DeviceType::Cpu,
        }
    }
}

impl TensorBuffer {
    /// Create from heap-allocated data
    pub fn from_vec(data: Vec<u8>, shape: Vec<usize>, dtype: DataType) -> Self {
        let strides = Self::compute_strides(&shape);
        Self {
            storage: TensorStorage::Heap(data),
            shape,
            dtype,
            strides,
            device: DeviceType::Cpu,
        }
    }
    
    /// Get storage type
    pub fn storage(&self) -> &TensorStorage {
        &self.storage
    }
    
    /// Get shape
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }
    
    /// Get data type
    pub fn dtype(&self) -> DataType {
        self.dtype
    }
    
    /// Get device
    pub fn device(&self) -> DeviceType {
        self.device
    }
    
    /// Get raw bytes (may trigger copy from SHM/GPU)
    pub fn as_bytes(&self) -> Result<Vec<u8>> {
        match &self.storage {
            TensorStorage::Heap(data) => Ok(data.clone()),
            #[cfg(feature = "shared-memory")]
            TensorStorage::SharedMemory { region, offset, size } => {
                region.read(*offset, *size)
            }
            #[cfg(feature = "cuda")]
            TensorStorage::GpuMemory { .. } => {
                anyhow::bail!("GPU memory read not yet implemented")
            }
        }
    }
    
    /// Check if tensor is contiguous
    pub fn is_contiguous(&self) -> bool {
        // Check if strides match expected contiguous layout
        let expected_strides = Self::compute_strides(&self.shape);
        self.strides == expected_strides
    }
    
    /// Compute strides for contiguous tensor
    fn compute_strides(shape: &[usize]) -> Vec<usize> {
        let mut strides = vec![1; shape.len()];
        for i in (0..shape.len() - 1).rev() {
            strides[i] = strides[i + 1] * shape[i + 1];
        }
        strides
    }
}

/// Placeholder for shared memory module when feature is disabled
#[cfg(not(feature = "shared-memory"))]
pub mod shared_memory {
    use super::*;
    
    pub struct SharedMemoryRegion;
}
