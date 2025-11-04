use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use remotemedia_runtime::python::multiprocess::{
    ipc_channel::ChannelRegistry,
    data_transfer::RuntimeData,
};
use std::sync::Arc;
use std::time::Duration;

/// Benchmark IPC latency for various payload sizes
fn bench_ipc_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("ipc_latency");

    // Test payload sizes: 1KB, 10KB, 100KB, 1MB, 10MB
    let sizes = vec![
        1024,           // 1KB
        10 * 1024,      // 10KB
        100 * 1024,     // 100KB
        1024 * 1024,    // 1MB
        10 * 1024 * 1024, // 10MB
    ];

    for size in sizes {
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}bytes", size)),
            &size,
            |b, &size| {
                // Setup tokio runtime
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.to_async(&rt).iter(|| async {
                    // Create test data
                    let payload = vec![0u8; size];
                    let data = RuntimeData {
                        data_type: remotemedia_runtime::python::multiprocess::data_transfer::DataType::Audio,
                        session_id: "bench_session".to_string(),
                        timestamp: 12345,
                        payload,
                    };

                    // Serialize and deserialize to measure roundtrip
                    let bytes = data.to_bytes();
                    let recovered = RuntimeData::from_bytes(&bytes).unwrap();

                    black_box(recovered);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark channel creation overhead
fn bench_channel_creation(c: &mut Criterion) {
    c.bench_function("channel_creation", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.to_async(&rt).iter(|| async {
            #[cfg(feature = "multiprocess")]
            {
                let mut registry = ChannelRegistry::new();
                registry.initialize().unwrap();

                let channel = registry.create_channel(
                    "bench_channel",
                    100,
                    true,
                ).await.unwrap();

                registry.destroy_channel(channel).await.unwrap();
            }

            #[cfg(not(feature = "multiprocess"))]
            {
                // No-op for non-multiprocess builds
            }
        });
    });
}

/// Benchmark publish-subscribe roundtrip (without channel creation overhead)
#[cfg(feature = "multiprocess")]
fn bench_pub_sub_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("pub_sub_roundtrip");
    group.measurement_time(Duration::from_secs(10));

    // Test with small (1KB), medium (100KB), and large (1MB, 5MB) payloads
    // Note: 10MB would exceed MAX_SLICE_LEN after adding RuntimeData headers
    let sizes = vec![1024, 100 * 1024, 1024 * 1024, 5 * 1024 * 1024];

    for size in sizes {
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}bytes", size)),
            &size,
            |b, &size| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                // Benchmark with setup phase
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        // Setup phase: Create channel, publisher, and subscriber ONCE
                        let mut registry = ChannelRegistry::new();
                        registry.initialize().unwrap();

                        let channel_name = format!("bench_pubsub_{}", size);
                        let channel = registry.create_channel(
                            &channel_name,
                            100,
                            false,
                        ).await.unwrap();

                        let publisher = registry.create_publisher(&channel_name).await.unwrap();
                        let subscriber = registry.create_subscriber(&channel_name).await.unwrap();

                        // Measurement phase: Only measure publish + receive latency
                        let start = std::time::Instant::now();

                        for _ in 0..iters {
                            // Create test data
                            let payload = vec![0u8; size];
                            let data = RuntimeData {
                                data_type: remotemedia_runtime::python::multiprocess::data_transfer::DataType::Audio,
                                session_id: "bench_session".to_string(),
                                timestamp: 12345,
                                payload,
                            };

                            // Publish
                            publisher.publish(data).await.unwrap();

                            // Receive (iceoryx2 is fast enough that no delay is needed)
                            let received = subscriber.receive().await.unwrap();
                            black_box(received);
                        }

                        let elapsed = start.elapsed();

                        // Cleanup phase
                        registry.destroy_channel(channel).await.unwrap();

                        elapsed
                    })
                });
            },
        );
    }

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_pub_sub_roundtrip(_c: &mut Criterion) {
    // No-op for non-multiprocess builds
}

criterion_group!(
    benches,
    bench_ipc_latency,
    bench_channel_creation,
    bench_pub_sub_roundtrip
);
criterion_main!(benches);
