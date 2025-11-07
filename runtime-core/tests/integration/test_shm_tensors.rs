//! Integration tests for shared memory tensors

#[cfg(feature = "shared-memory")]
mod shm_tests {
    use remotemedia_runtime_core::tensor::{
        TensorBuffer, SharedMemoryRegion, SharedMemoryAllocator,
        AllocatorConfig, DataType, TensorCapabilities,
    };
    
    #[test]
    fn test_shared_memory_region_create() {
        let size = 4096; // 4KB
        let region = SharedMemoryRegion::create(size).expect("Failed to create SHM region");
        
        assert_eq!(region.size(), size);
        assert!(!region.id().is_empty());
        
        println!("Created SHM region: {} ({} bytes)", region.id(), region.size());
    }
    
    #[test]
    fn test_shared_memory_write_read() {
        let size = 1024;
        let region = SharedMemoryRegion::create(size).expect("Failed to create region");
        
        // Write data
        let test_data = vec![1u8, 2, 3, 4, 5];
        region.write(0, &test_data).expect("Failed to write");
        
        // Read back
        let read_data = region.read(0, test_data.len()).expect("Failed to read");
        assert_eq!(read_data, test_data);
        
        println!("Write/Read verified for {} bytes", test_data.len());
    }
    
    #[tokio::test]
    async fn test_allocator_basic() {
        let config = AllocatorConfig::default();
        let allocator = SharedMemoryAllocator::new(config);
        
        // Allocate a tensor
        let tensor = allocator.allocate_tensor(1024, None)
            .expect("Failed to allocate tensor");
        
        assert_eq!(tensor.shape(), &[1024]);
        
        let metrics = allocator.metrics();
        assert_eq!(metrics.regions_active, 1);
        assert_eq!(metrics.bytes_allocated, 1024);
        
        println!("Allocated tensor: {:?}", tensor.shape());
    }
    
    #[tokio::test]
    async fn test_allocator_per_session_quota() {
        let config = AllocatorConfig {
            per_session_quota: Some(2048),
            ..Default::default()
        };
        let allocator = SharedMemoryAllocator::new(config);
        
        // First allocation within quota
        let _tensor1 = allocator.allocate_tensor(1024, Some("session-1"))
            .expect("First allocation should succeed");
        
        // Second allocation within quota
        let _tensor2 = allocator.allocate_tensor(512, Some("session-1"))
            .expect("Second allocation should succeed");
        
        // Third allocation exceeds quota
        let result = allocator.allocate_tensor(1024, Some("session-1"));
        assert!(result.is_err(), "Should fail due to quota");
        
        println!("Session quota enforcement working");
    }
    
    #[tokio::test]
    async fn test_allocator_cleanup() {
        use std::time::Duration;
        
        let config = AllocatorConfig::default();
        let allocator = SharedMemoryAllocator::new(config);
        
        // Allocate and immediately free
        let tensor = allocator.allocate_tensor(1024, None)
            .expect("Failed to allocate");
        
        // Get region ID for cleanup
        let region_id = match tensor.storage() {
            remotemedia_runtime_core::tensor::TensorStorage::SharedMemory { region, .. } => {
                region.id().to_string()
            }
            _ => panic!("Expected SharedMemory storage"),
        };
        
        // Free it
        allocator.free(&region_id);
        
        let metrics = allocator.metrics();
        assert_eq!(metrics.regions_active, 0);
        assert_eq!(metrics.frees_total, 1);
        
        println!("Cleanup working correctly");
    }
    
    #[test]
    fn test_capability_detection() {
        let caps = TensorCapabilities::detect();
        
        println!("Tensor Capabilities:");
        println!("  Shared Memory: {}", caps.shared_memory);
        println!("  CUDA: {}", caps.cuda);
        println!("  Metal: {}", caps.metal);
        println!("  DLPack: {}", caps.dlpack);
        
        // Shared memory should be available when feature is enabled
        #[cfg(feature = "shared-memory")]
        assert!(caps.shared_memory, "Shared memory should be available");
    }
    
    #[test]
    fn test_tensor_from_shared_memory() {
        let region = SharedMemoryRegion::create(1024).expect("Failed to create region");
        let region_id = region.id().to_string();
        
        // Write test data
        let test_data: Vec<u8> = (0..100).map(|i| i as u8).collect();
        region.write(0, &test_data).expect("Failed to write");
        
        // Create tensor from shared memory
        let tensor = TensorBuffer::from_shared_memory(
            &region_id,
            0,
            test_data.len(),
            vec![100],
            DataType::U8,
        ).expect("Failed to create tensor from SHM");
        
        // Read back and verify
        let read_data = tensor.as_bytes().expect("Failed to read bytes");
        assert_eq!(read_data, test_data);
        
        println!("Tensor from shared memory verified");
    }
}

#[cfg(not(feature = "shared-memory"))]
mod shm_tests {
    #[test]
    fn test_shared_memory_disabled() {
        println!("Shared memory feature not enabled - skipping SHM tests");
    }
}

