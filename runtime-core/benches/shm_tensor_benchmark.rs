//! Benchmark for shared memory tensor transfers using our actual iceoryx2 implementation
//!
//! This measures the REAL performance of our zero-copy IPC system.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::time::Duration;

#[cfg(feature = "shared-memory")]
use remotemedia_runtime_core::tensor::{
    SharedMemoryRegion, TensorBuffer, DataType, SharedMemoryAllocator, AllocatorConfig,
};

/// Benchmark serialization baseline (pickle-like)
fn benchmark_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("tensor_serialization");
    
    for size_mb in [1, 10, 100] {
        let num_elements = (size_mb * 1024 * 1024) / 4; // float32
        let data: Vec<f32> = (0..num_elements).map(|i| i as f32).collect();
        let bytes: Vec<u8> = data.iter().flat_map(|&f| f.to_le_bytes()).collect();
        
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        
        group.bench_with_input(
            BenchmarkId::new("bincode_serialize", format!("{}MB", size_mb)),
            &bytes,
            |b, bytes| {
                b.iter(|| {
                    let serialized = bincode::serialize(&bytes).unwrap();
                    let _deserialized: Vec<u8> = bincode::deserialize(&serialized).unwrap();
                    black_box(_deserialized);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark our shared memory implementation
#[cfg(feature = "shared-memory")]
fn benchmark_shared_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("tensor_shared_memory");
    
    for size_mb in [1, 10, 100] {
        let num_elements = (size_mb * 1024 * 1024) / 4; // float32
        let data: Vec<f32> = (0..num_elements).map(|i| i as f32).collect();
        let bytes: Vec<u8> = data.iter().flat_map(|&f| f.to_le_bytes()).collect();
        
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        
        group.bench_with_input(
            BenchmarkId::new("shm_create_write_read", format!("{}MB", size_mb)),
            &bytes,
            |b, bytes| {
                b.iter(|| {
                    // Create shared memory region
                    let region = SharedMemoryRegion::create(bytes.len()).unwrap();
                    
                    // Write data
                    region.write(0, bytes).unwrap();
                    
                    // Read data back
                    let read_bytes = region.read(0, bytes.len()).unwrap();
                    
                    black_box(read_bytes);
                    // Region automatically cleaned up on drop
                });
            },
        );
        
        // Benchmark just the read (zero-copy scenario)
        group.bench_with_input(
            BenchmarkId::new("shm_read_only", format!("{}MB", size_mb)),
            &bytes,
            |b, bytes| {
                // Pre-create and populate region
                let region = SharedMemoryRegion::create(bytes.len()).unwrap();
                region.write(0, bytes).unwrap();
                
                b.iter(|| {
                    // Just read (zero-copy in real scenario)
                    let read_bytes = region.read(0, bytes.len()).unwrap();
                    black_box(read_bytes);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark SharedMemoryAllocator
#[cfg(feature = "shared-memory")]
fn benchmark_allocator(c: &mut Criterion) {
    let mut group = c.benchmark_group("tensor_allocator");
    
    let config = AllocatorConfig::default();
    let allocator = SharedMemoryAllocator::new(config);
    
    for size_mb in [1, 10, 100] {
        let size_bytes = size_mb * 1024 * 1024;
        
        group.throughput(Throughput::Bytes(size_bytes as u64));
        
        group.bench_with_input(
            BenchmarkId::new("allocate_tensor", format!("{}MB", size_mb)),
            &size_bytes,
            |b, &size| {
                b.iter(|| {
                    let tensor = allocator.allocate_tensor(size, None).unwrap();
                    black_box(tensor);
                    // Tensor dropped, region cleaned up
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark TensorBuffer creation patterns
fn benchmark_tensor_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("tensor_creation");
    
    for size_mb in [1, 10, 100] {
        let num_elements = (size_mb * 1024 * 1024) / 4;
        let data: Vec<u8> = vec![0u8; num_elements * 4];
        
        group.throughput(Throughput::Bytes(data.len() as u64));
        
        // Heap allocation
        group.bench_with_input(
            BenchmarkId::new("from_vec_heap", format!("{}MB", size_mb)),
            &data,
            |b, data| {
                b.iter(|| {
                    let tensor = TensorBuffer::from_vec(
                        data.clone(),
                        vec![num_elements],
                        DataType::F32,
                    );
                    black_box(tensor);
                });
            },
        );
    }
    
    group.finish();
}

#[cfg(feature = "shared-memory")]
criterion_group!(
    benches,
    benchmark_serialization,
    benchmark_shared_memory,
    benchmark_allocator,
    benchmark_tensor_creation
);

#[cfg(not(feature = "shared-memory"))]
criterion_group!(
    benches,
    benchmark_serialization,
    benchmark_tensor_creation
);

criterion_main!(benches);

