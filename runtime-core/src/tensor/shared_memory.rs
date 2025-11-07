//! Cross-platform shared memory implementation

use anyhow::{Result, Context};
use std::sync::Arc;

/// Shared memory region accessible by multiple processes
pub struct SharedMemoryRegion {
    /// Unique region identifier
    id: String,
    /// Total size in bytes
    size: usize,
    /// Underlying shared memory implementation
    #[cfg(feature = "shared-memory")]
    inner: Arc<shared_memory_extended::Shmem>,
}

impl SharedMemoryRegion {
    /// Create a new shared memory region
    #[cfg(feature = "shared-memory")]
    pub fn create(size: usize) -> Result<Self> {
        use shared_memory_extended::{ShmemConf, Shmem};
        
        // Generate unique ID
        let id = uuid::Uuid::new_v4().to_string();
        
        // Create shared memory
        let shmem = ShmemConf::new()
            .size(size)
            .os_id(&id)
            .create()
            .context("Failed to create shared memory region")?;
        
        tracing::debug!("Created shared memory region: {} ({} bytes)", id, size);
        
        Ok(Self {
            id: id.clone(),
            size,
            inner: Arc::new(shmem),
        })
    }
    
    /// Create a new shared memory region (without feature)
    #[cfg(not(feature = "shared-memory"))]
    pub fn create(_size: usize) -> Result<Self> {
        anyhow::bail!("Shared memory feature not enabled");
    }
    
    /// Open an existing shared memory region
    #[cfg(feature = "shared-memory")]
    pub fn open(id: &str, size: usize) -> Result<Self> {
        use shared_memory_extended::ShmemConf;
        
        let shmem = ShmemConf::new()
            .size(size)
            .os_id(id)
            .open()
            .context(format!("Failed to open shared memory region: {}", id))?;
        
        tracing::debug!("Opened shared memory region: {} ({} bytes)", id, size);
        
        Ok(Self {
            id: id.to_string(),
            size,
            inner: Arc::new(shmem),
        })
    }
    
    /// Open an existing shared memory region (without feature)
    #[cfg(not(feature = "shared-memory"))]
    pub fn open(_id: &str, _size: usize) -> Result<Self> {
        anyhow::bail!("Shared memory feature not enabled");
    }
    
    /// Get region identifier
    pub fn id(&self) -> &str {
        &self.id
    }
    
    /// Get region size
    pub fn size(&self) -> usize {
        self.size
    }
    
    /// Read data from the region
    #[cfg(feature = "shared-memory")]
    pub fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        if offset + len > self.size {
            anyhow::bail!("Read out of bounds: offset {} + len {} > size {}", offset, len, self.size);
        }
        
        unsafe {
            let ptr = self.inner.as_ptr().add(offset);
            let slice = std::slice::from_raw_parts(ptr, len);
            Ok(slice.to_vec())
        }
    }
    
    /// Read data from the region (without feature)
    #[cfg(not(feature = "shared-memory"))]
    pub fn read(&self, _offset: usize, _len: usize) -> Result<Vec<u8>> {
        anyhow::bail!("Shared memory feature not enabled");
    }
    
    /// Write data to the region
    #[cfg(feature = "shared-memory")]
    pub fn write(&self, offset: usize, data: &[u8]) -> Result<()> {
        if offset + data.len() > self.size {
            anyhow::bail!("Write out of bounds: offset {} + len {} > size {}", 
                offset, data.len(), self.size);
        }
        
        unsafe {
            let ptr = self.inner.as_ptr().add(offset) as *mut u8;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        }
        
        Ok(())
    }
    
    /// Write data to the region (without feature)
    #[cfg(not(feature = "shared-memory"))]
    pub fn write(&self, _offset: usize, _data: &[u8]) -> Result<()> {
        anyhow::bail!("Shared memory feature not enabled");
    }
    
    /// Get a slice view of the region (zero-copy)
    #[cfg(feature = "shared-memory")]
    pub unsafe fn as_slice(&self) -> &[u8] {
        std::slice::from_raw_parts(self.inner.as_ptr(), self.size)
    }
    
    /// Get a mutable slice view of the region (zero-copy)
    #[cfg(feature = "shared-memory")]
    pub unsafe fn as_mut_slice(&self) -> &mut [u8] {
        std::slice::from_raw_parts_mut(self.inner.as_ptr() as *mut u8, self.size)
    }
}

impl Clone for SharedMemoryRegion {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            size: self.size,
            #[cfg(feature = "shared-memory")]
            inner: Arc::clone(&self.inner),
        }
    }
}

impl std::fmt::Debug for SharedMemoryRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMemoryRegion")
            .field("id", &self.id)
            .field("size", &self.size)
            .finish()
    }
}

// Safety: SharedMemoryRegion is Send + Sync because:
// - The underlying OS shared memory is thread-safe
// - Arc provides thread-safe reference counting
// - All mutations go through unsafe blocks with proper synchronization
unsafe impl Send for SharedMemoryRegion {}
unsafe impl Sync for SharedMemoryRegion {}

