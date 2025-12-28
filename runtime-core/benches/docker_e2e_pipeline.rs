//! End-to-End Pipeline Benchmarks: Docker vs Native Multiprocess via IPC
//!
//! Measures real-world performance of complete pipelines including:
//! - Initialization time
//! - IPC channel setup
//! - Data transfer latency
//! - Throughput via iceoryx2
//! - Cleanup overhead
//!
//! Note: These benchmarks require the `multiprocess` feature and optionally `docker` feature.
//! Run with: cargo bench -p remotemedia-runtime-core --features multiprocess,docker

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use remotemedia_runtime_core::data::RuntimeData;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

#[cfg(feature = "docker")]
use remotemedia_runtime_core::python::multiprocess::docker_support::DockerSupport;

#[cfg(feature = "multiprocess")]
use remotemedia_runtime_core::executor::node_executor::{NodeContext, NodeExecutor};

#[cfg(feature = "multiprocess")]
use remotemedia_runtime_core::python::multiprocess::{MultiprocessConfig, MultiprocessExecutor};

#[cfg(feature = "multiprocess")]
use std::collections::HashMap;

/// Helper to check if Docker is available
#[cfg(feature = "docker")]
async fn is_docker_available() -> bool {
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return false;
    }

    match DockerSupport::new().await {
        Ok(docker) => docker.validate_docker_availability().await.is_ok(),
        Err(_) => false,
    }
}

#[cfg(not(feature = "docker"))]
async fn is_docker_available() -> bool {
    false
}

/// Generate test audio data using the main RuntimeData type
fn generate_audio_data(duration_ms: u32, sample_rate: u32) -> RuntimeData {
    let num_samples = (sample_rate as f32 * duration_ms as f32 / 1000.0) as usize;
    let samples: Vec<f32> = (0..num_samples).map(|i| (i as f32 * 0.001).sin()).collect();

    RuntimeData::Audio {
        samples,
        sample_rate,
        channels: 1,
        stream_id: None,
    }
}

/// Benchmark complete pipeline initialization
#[cfg(feature = "multiprocess")]
fn bench_pipeline_init(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Check if Docker is available
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("pipeline_initialization");

    // Native multiprocess initialization
    group.bench_function("native_multiprocess", |b| {
        b.to_async(&rt).iter(|| async {
            let config = MultiprocessConfig::default();
            let mut executor = MultiprocessExecutor::new(config);
            let session_id = format!("bench_{}", uuid::Uuid::new_v4());

            let ctx = NodeContext {
                node_id: "native_node".to_string(),
                node_type: "PassThrough".to_string(),
                params: serde_json::Value::Null,
                session_id: Some(session_id.clone()),
                metadata: HashMap::new(),
            };

            let start = Instant::now();
            let init_result = executor.initialize(&ctx).await;
            let init_time = start.elapsed();

            if init_result.is_ok() {
                let _ = executor.cleanup().await;
            }
            init_time
        });
    });

    // Docker pipeline initialization (if available)
    if docker_available {
        group.bench_function("docker_pipeline", |b| {
            b.to_async(&rt).iter(|| async {
                let config = MultiprocessConfig::default();
                let mut executor = MultiprocessExecutor::new(config);
                let session_id = format!("bench_{}", uuid::Uuid::new_v4());

                let mut metadata = HashMap::new();
                metadata.insert("use_docker".to_string(), serde_json::Value::Bool(true));
                metadata.insert(
                    "docker_config".to_string(),
                    serde_json::json!({
                        "python_version": "3.10",
                        "memory_mb": 512,
                        "cpu_cores": 1.0,
                        "python_packages": ["iceoryx2"],
                    }),
                );

                let ctx = NodeContext {
                    node_id: "docker_node".to_string(),
                    node_type: "PassThrough".to_string(),
                    params: serde_json::Value::Null,
                    session_id: Some(session_id.clone()),
                    metadata,
                };

                let start = Instant::now();
                let init_result = executor.initialize(&ctx).await;
                let init_time = start.elapsed();

                if init_result.is_ok() {
                    let _ = executor.cleanup().await;
                }
                init_time
            });
        });
    }

    group.finish();
}

/// Benchmark IPC data transfer latency
#[cfg(feature = "multiprocess")]
fn bench_ipc_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("ipc_latency");

    // Test with different data sizes
    for size_ms in [10, 50, 100, 500].iter() {
        let size = *size_ms;

        // Native multiprocess IPC latency
        group.bench_with_input(
            BenchmarkId::new("native", format!("{}ms", size_ms)),
            size_ms,
            |b, _| {
                b.to_async(&rt).iter(|| async {
                    // Setup per iteration (includes init cost in measurement)
                    let config = MultiprocessConfig::default();
                    let mut executor = MultiprocessExecutor::new(config);
                    let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                    let node_id = "native_node".to_string();

                    let ctx = NodeContext {
                        node_id: node_id.clone(),
                        node_type: "PassThrough".to_string(),
                        params: serde_json::Value::Null,
                        session_id: Some(session_id.clone()),
                        metadata: HashMap::new(),
                    };

                    let _ = executor.initialize(&ctx).await;
                    let test_data = generate_audio_data(size, 16000);

                    // Measure single send/receive cycle
                    let start = Instant::now();
                    let _ = executor
                        .send_data_to_node(&node_id, &session_id, test_data)
                        .await;
                    let elapsed = start.elapsed();

                    let _ = executor.cleanup().await;
                    elapsed
                });
            },
        );

        // Docker IPC latency (if available)
        if docker_available {
            group.bench_with_input(
                BenchmarkId::new("docker", format!("{}ms", size_ms)),
                size_ms,
                |b, _| {
                    b.to_async(&rt).iter(|| async {
                        // Setup per iteration
                        let config = MultiprocessConfig::default();
                        let mut executor = MultiprocessExecutor::new(config);
                        let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                        let node_id = "docker_node".to_string();

                        let mut metadata = HashMap::new();
                        metadata
                            .insert("use_docker".to_string(), serde_json::Value::Bool(true));
                        metadata.insert(
                            "docker_config".to_string(),
                            serde_json::json!({
                                "python_version": "3.10",
                                "memory_mb": 512,
                                "cpu_cores": 1.0,
                                "python_packages": ["iceoryx2"],
                            }),
                        );

                        let ctx = NodeContext {
                            node_id: node_id.clone(),
                            node_type: "PassThrough".to_string(),
                            params: serde_json::Value::Null,
                            session_id: Some(session_id.clone()),
                            metadata,
                        };

                        let _ = executor.initialize(&ctx).await;
                        let test_data = generate_audio_data(size, 16000);

                        // Measure single send/receive cycle
                        let start = Instant::now();
                        let _ = executor
                            .send_data_to_node(&node_id, &session_id, test_data)
                            .await;
                        let elapsed = start.elapsed();

                        let _ = executor.cleanup().await;
                        elapsed
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark streaming throughput
#[cfg(feature = "multiprocess")]
fn bench_streaming_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("streaming_throughput");

    const CHUNK_SIZE_MS: u32 = 20; // 20ms audio chunks
    const NUM_CHUNKS: usize = 50; // Total 1 second of audio

    // Native multiprocess throughput
    group.bench_function("native_multiprocess", |b| {
        b.to_async(&rt).iter(|| async {
            // Setup
            let config = MultiprocessConfig::default();
            let mut executor = MultiprocessExecutor::new(config);
            let session_id = format!("bench_{}", uuid::Uuid::new_v4());
            let node_id = "native_node".to_string();

            let ctx = NodeContext {
                node_id: node_id.clone(),
                node_type: "PassThrough".to_string(),
                params: serde_json::Value::Null,
                session_id: Some(session_id.clone()),
                metadata: HashMap::new(),
            };

            let _ = executor.initialize(&ctx).await;

            let chunks: Vec<RuntimeData> = (0..NUM_CHUNKS)
                .map(|_| generate_audio_data(CHUNK_SIZE_MS, 16000))
                .collect();

            let start = Instant::now();

            for chunk in chunks.iter() {
                let _ = executor
                    .send_data_to_node(&node_id, &session_id, chunk.clone())
                    .await;
            }

            let elapsed = start.elapsed();
            let throughput = chunks.len() as f64 / elapsed.as_secs_f64();

            let _ = executor.cleanup().await;
            throughput // Return chunks per second
        });
    });

    // Docker streaming throughput (if available)
    if docker_available {
        group.bench_function("docker_pipeline", |b| {
            b.to_async(&rt).iter(|| async {
                // Setup
                let config = MultiprocessConfig::default();
                let mut executor = MultiprocessExecutor::new(config);
                let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                let node_id = "docker_node".to_string();

                let mut metadata = HashMap::new();
                metadata.insert("use_docker".to_string(), serde_json::Value::Bool(true));
                metadata.insert(
                    "docker_config".to_string(),
                    serde_json::json!({
                        "python_version": "3.10",
                        "memory_mb": 512,
                        "cpu_cores": 1.0,
                        "python_packages": ["iceoryx2"],
                    }),
                );

                let ctx = NodeContext {
                    node_id: node_id.clone(),
                    node_type: "PassThrough".to_string(),
                    params: serde_json::Value::Null,
                    session_id: Some(session_id.clone()),
                    metadata,
                };

                let _ = executor.initialize(&ctx).await;

                let chunks: Vec<RuntimeData> = (0..NUM_CHUNKS)
                    .map(|_| generate_audio_data(CHUNK_SIZE_MS, 16000))
                    .collect();

                let start = Instant::now();

                for chunk in chunks.iter() {
                    let _ = executor
                        .send_data_to_node(&node_id, &session_id, chunk.clone())
                        .await;
                }

                let elapsed = start.elapsed();
                let throughput = chunks.len() as f64 / elapsed.as_secs_f64();

                let _ = executor.cleanup().await;
                throughput // Return chunks per second
            });
        });
    }

    group.finish();
}

/// Benchmark complete E2E pipeline (init + transfer + cleanup)
#[cfg(feature = "multiprocess")]
fn bench_e2e_pipeline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("e2e_pipeline");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20);

    // Native E2E pipeline
    group.bench_function("native_complete", |b| {
        b.to_async(&rt).iter(|| async {
            let config = MultiprocessConfig::default();
            let mut executor = MultiprocessExecutor::new(config);
            let session_id = format!("bench_{}", uuid::Uuid::new_v4());
            let node_id = "native_node".to_string();

            // Start timing
            let start = Instant::now();

            // Initialize
            let ctx = NodeContext {
                node_id: node_id.clone(),
                node_type: "PassThrough".to_string(),
                params: serde_json::Value::Null,
                session_id: Some(session_id.clone()),
                metadata: HashMap::new(),
            };
            let _ = executor.initialize(&ctx).await;

            // Send 100ms of audio
            let test_data = generate_audio_data(100, 16000);
            let _ = executor
                .send_data_to_node(&node_id, &session_id, test_data)
                .await;

            // Cleanup
            let _ = executor.cleanup().await;

            start.elapsed()
        });
    });

    // Docker E2E pipeline (if available)
    if docker_available {
        group.bench_function("docker_complete", |b| {
            b.to_async(&rt).iter(|| async {
                let config = MultiprocessConfig::default();
                let mut executor = MultiprocessExecutor::new(config);
                let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                let node_id = "docker_node".to_string();

                // Start timing
                let start = Instant::now();

                // Initialize with Docker
                let mut metadata = HashMap::new();
                metadata.insert("use_docker".to_string(), serde_json::Value::Bool(true));
                metadata.insert(
                    "docker_config".to_string(),
                    serde_json::json!({
                        "python_version": "3.10",
                        "memory_mb": 512,
                        "cpu_cores": 1.0,
                        "python_packages": ["iceoryx2"],
                    }),
                );

                let ctx = NodeContext {
                    node_id: node_id.clone(),
                    node_type: "PassThrough".to_string(),
                    params: serde_json::Value::Null,
                    session_id: Some(session_id.clone()),
                    metadata,
                };
                let _ = executor.initialize(&ctx).await;

                // Send 100ms of audio
                let test_data = generate_audio_data(100, 16000);
                let _ = executor
                    .send_data_to_node(&node_id, &session_id, test_data)
                    .await;

                // Cleanup
                let _ = executor.cleanup().await;

                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Benchmark concurrent pipeline execution
#[cfg(feature = "multiprocess")]
fn bench_concurrent_pipelines(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("concurrent_pipelines");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(10);

    for num_concurrent in [1, 2, 4, 8].iter() {
        // Native concurrent pipelines
        group.bench_with_input(
            BenchmarkId::new("native", num_concurrent),
            num_concurrent,
            |b, &num| {
                b.to_async(&rt).iter(|| async move {
                    let start = Instant::now();

                    let mut handles = vec![];
                    for i in 0..num {
                        let handle = tokio::spawn(async move {
                            let config = MultiprocessConfig::default();
                            let mut executor = MultiprocessExecutor::new(config);
                            let session_id = format!("bench_{}_{}", i, uuid::Uuid::new_v4());
                            let node_id = format!("native_node_{}", i);

                            let ctx = NodeContext {
                                node_id: node_id.clone(),
                                node_type: "PassThrough".to_string(),
                                params: serde_json::Value::Null,
                                session_id: Some(session_id.clone()),
                                metadata: HashMap::new(),
                            };

                            let _ = executor.initialize(&ctx).await;

                            let test_data = generate_audio_data(50, 16000);
                            let _ = executor
                                .send_data_to_node(&node_id, &session_id, test_data)
                                .await;

                            let _ = executor.cleanup().await;
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        let _ = handle.await;
                    }

                    start.elapsed()
                });
            },
        );

        // Docker concurrent pipelines (if available)
        if docker_available {
            group.bench_with_input(
                BenchmarkId::new("docker", num_concurrent),
                num_concurrent,
                |b, &num| {
                    b.to_async(&rt).iter(|| async move {
                        let start = Instant::now();

                        let mut handles = vec![];
                        for i in 0..num {
                            let handle = tokio::spawn(async move {
                                let config = MultiprocessConfig::default();
                                let mut executor = MultiprocessExecutor::new(config);
                                let session_id = format!("bench_{}_{}", i, uuid::Uuid::new_v4());
                                let node_id = format!("docker_node_{}", i);

                                let mut metadata = HashMap::new();
                                metadata.insert(
                                    "use_docker".to_string(),
                                    serde_json::Value::Bool(true),
                                );
                                metadata.insert(
                                    "docker_config".to_string(),
                                    serde_json::json!({
                                        "python_version": "3.10",
                                        "memory_mb": 256,
                                        "cpu_cores": 0.5,
                                        "python_packages": ["iceoryx2"],
                                    }),
                                );

                                let ctx = NodeContext {
                                    node_id: node_id.clone(),
                                    node_type: "PassThrough".to_string(),
                                    params: serde_json::Value::Null,
                                    session_id: Some(session_id.clone()),
                                    metadata,
                                };

                                let _ = executor.initialize(&ctx).await;

                                let test_data = generate_audio_data(50, 16000);
                                let _ = executor
                                    .send_data_to_node(&node_id, &session_id, test_data)
                                    .await;

                                let _ = executor.cleanup().await;
                            });
                            handles.push(handle);
                        }

                        for handle in handles {
                            let _ = handle.await;
                        }

                        start.elapsed()
                    });
                },
            );
        }
    }

    group.finish();
}

// Stub implementations when multiprocess feature is not enabled
#[cfg(not(feature = "multiprocess"))]
fn bench_pipeline_init(_c: &mut Criterion) {
    // No-op when multiprocess feature is disabled
}

#[cfg(not(feature = "multiprocess"))]
fn bench_ipc_latency(_c: &mut Criterion) {
    // No-op when multiprocess feature is disabled
}

#[cfg(not(feature = "multiprocess"))]
fn bench_streaming_throughput(_c: &mut Criterion) {
    // No-op when multiprocess feature is disabled
}

#[cfg(not(feature = "multiprocess"))]
fn bench_e2e_pipeline(_c: &mut Criterion) {
    // No-op when multiprocess feature is disabled
}

#[cfg(not(feature = "multiprocess"))]
fn bench_concurrent_pipelines(_c: &mut Criterion) {
    // No-op when multiprocess feature is disabled
}

criterion_group!(
    benches,
    bench_pipeline_init,
    bench_ipc_latency,
    bench_streaming_throughput,
    bench_e2e_pipeline,
    bench_concurrent_pipelines
);

criterion_main!(benches);
