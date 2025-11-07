//! Capability detection for tensor features

/// Tensor capabilities available on this system
#[derive(Debug, Clone)]
pub struct TensorCapabilities {
    /// Shared memory support
    pub shared_memory: bool,
    /// CUDA support
    pub cuda: bool,
    /// Metal support (macOS)
    pub metal: bool,
    /// DLPack support
    pub dlpack: bool,
}

impl TensorCapabilities {
    /// Detect available capabilities
    pub fn detect() -> Self {
        Self {
            shared_memory: Self::detect_shared_memory(),
            cuda: Self::detect_cuda(),
            metal: Self::detect_metal(),
            dlpack: cfg!(feature = "dlpack"),
        }
    }
    
    /// Check if shared memory is available
    fn detect_shared_memory() -> bool {
        #[cfg(feature = "shared-memory")]
        {
            // Try to create a small test region
            match crate::tensor::SharedMemoryRegion::create(4096) {
                Ok(_) => {
                    tracing::debug!("Shared memory available");
                    true
                }
                Err(e) => {
                    tracing::warn!("Shared memory not available: {}", e);
                    false
                }
            }
        }
        
        #[cfg(not(feature = "shared-memory"))]
        {
            false
        }
    }
    
    /// Check if CUDA is available
    fn detect_cuda() -> bool {
        #[cfg(feature = "cuda")]
        {
            // Would check for CUDA runtime
            false
        }
        
        #[cfg(not(feature = "cuda"))]
        {
            false
        }
    }
    
    /// Check if Metal is available (macOS)
    fn detect_metal() -> bool {
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            true
        }
        
        #[cfg(not(all(target_os = "macos", feature = "metal")))]
        {
            false
        }
    }
}

impl Default for TensorCapabilities {
    fn default() -> Self {
        Self::detect()
    }
}

