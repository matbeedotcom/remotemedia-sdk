//! End-to-End Pipeline Benchmarks: Docker vs Native Multiprocess via IPC
//!
//! Measures real-world performance of complete pipelines including:
//! - Initialization time
//! - IPC channel setup
//! - Data transfer latency
//! - Throughput via iceoryx2
//! - Cleanup overhead

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use remotemedia_runtime_core::{
    executor::scheduler::ExecutionContext,
    python::multiprocess::{
        data_transfer::RuntimeData, docker_support::DockerSupport,
        multiprocess_executor::MultiprocessExecutor,
    },
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;

/// Helper to check if Docker is available
async fn is_docker_available() -> bool {
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return false;
    }

    match DockerSupport::new().await {
        Ok(docker) => docker.validate_docker_availability().await.is_ok(),
        Err(_) => false,
    }
}

/// Generate test audio data
fn generate_audio_data(duration_ms: u32, sample_rate: u32) -> RuntimeData {
    let num_samples = (sample_rate as f32 * duration_ms as f32 / 1000.0) as usize;
    let samples: Vec<f32> = (0..num_samples).map(|i| (i as f32 * 0.001).sin()).collect();

    RuntimeData::audio(&samples, sample_rate, 1, "bench_session")
}

/// Benchmark complete pipeline initialization
fn bench_pipeline_init(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Check if Docker is available
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("pipeline_initialization");

    // Native multiprocess initialization
    group.bench_function("native_multiprocess", |b| {
        b.to_async(&rt).iter(|| async {
            let executor = MultiprocessExecutor::new();
            let session_id = format!("bench_{}", uuid::Uuid::new_v4());

            let ctx = ExecutionContext {
                node_id: "native_node".to_string(),
                session_id: Some(session_id.clone()),
                metadata: HashMap::new(),
                trace_id: None,
            };

            let start = Instant::now();
            executor.initialize(&ctx, &session_id).await.unwrap();
            let init_time = start.elapsed();

            executor.terminate_session(&session_id).await.ok();
            init_time
        });
    });

    // Docker pipeline initialization (if available)
    if docker_available {
        group.bench_function("docker_pipeline", |b| {
            b.to_async(&rt).iter(|| async {
                let executor = MultiprocessExecutor::new();
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

                let ctx = ExecutionContext {
                    node_id: "docker_node".to_string(),
                    session_id: Some(session_id.clone()),
                    metadata,
                    trace_id: None,
                };

                let start = Instant::now();
                executor.initialize(&ctx, &session_id).await.unwrap();
                let init_time = start.elapsed();

                executor.terminate_session(&session_id).await.ok();
                init_time
            });
        });
    }

    group.finish();
}

/// Benchmark IPC data transfer latency
fn bench_ipc_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("ipc_latency");

    // Test with different data sizes
    for size_ms in [10, 50, 100, 500].iter() {
        let test_data = generate_audio_data(*size_ms, 16000);

        // Native multiprocess IPC latency
        group.bench_with_input(
            BenchmarkId::new("native", format!("{}ms", size_ms)),
            size_ms,
            |b, _| {
                b.to_async(&rt).iter_batched_ref(
                    || {
                        // Setup
                        let executor = MultiprocessExecutor::new();
                        let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                        let node_id = "native_node";

                        let ctx = ExecutionContext {
                            node_id: node_id.to_string(),
                            session_id: Some(session_id.clone()),
                            metadata: HashMap::new(),
                            trace_id: None,
                        };

                        rt.block_on(executor.initialize(&ctx, &session_id)).unwrap();
                        (executor, session_id, node_id.to_string())
                    },
                    |(executor, session_id, node_id)| async {
                        // Measure single send/receive cycle
                        let start = Instant::now();
                        executor
                            .send_data_to_node(node_id, session_id, test_data.clone())
                            .await
                            .unwrap();
                        start.elapsed()
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );

        // Docker IPC latency (if available)
        if docker_available {
            group.bench_with_input(
                BenchmarkId::new("docker", format!("{}ms", size_ms)),
                size_ms,
                |b, _| {
                    b.to_async(&rt).iter_batched_ref(
                        || {
                            // Setup
                            let executor = MultiprocessExecutor::new();
                            let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                            let node_id = "docker_node";

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

                            let ctx = ExecutionContext {
                                node_id: node_id.to_string(),
                                session_id: Some(session_id.clone()),
                                metadata,
                                trace_id: None,
                            };

                            rt.block_on(executor.initialize(&ctx, &session_id)).unwrap();
                            (executor, session_id, node_id.to_string())
                        },
                        |(executor, session_id, node_id)| async {
                            // Measure single send/receive cycle
                            let start = Instant::now();
                            executor
                                .send_data_to_node(node_id, session_id, test_data.clone())
                                .await
                                .unwrap();
                            start.elapsed()
                        },
                        criterion::BatchSize::SmallInput,
                    );
                },
            );
        }
    }

    group.finish();
}

/// Benchmark streaming throughput
fn bench_streaming_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("streaming_throughput");

    let chunk_size_ms = 20; // 20ms audio chunks
    let num_chunks = 50; // Total 1 second of audio

    // Native multiprocess throughput
    group.bench_function("native_multiprocess", |b| {
        b.to_async(&rt).iter_batched_ref(
            || {
                // Setup
                let executor = MultiprocessExecutor::new();
                let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                let node_id = "native_node";

                let ctx = ExecutionContext {
                    node_id: node_id.to_string(),
                    session_id: Some(session_id.clone()),
                    metadata: HashMap::new(),
                    trace_id: None,
                };

                rt.block_on(executor.initialize(&ctx, &session_id)).unwrap();

                let chunks: Vec<RuntimeData> = (0..num_chunks)
                    .map(|_| generate_audio_data(chunk_size_ms, 16000))
                    .collect();

                (executor, session_id, node_id.to_string(), chunks)
            },
            |(executor, session_id, node_id, chunks)| async {
                let start = Instant::now();

                for chunk in chunks.iter() {
                    executor
                        .send_data_to_node(node_id, session_id, chunk.clone())
                        .await
                        .unwrap();
                }

                let elapsed = start.elapsed();
                let throughput = chunks.len() as f64 / elapsed.as_secs_f64();
                throughput // Return chunks per second
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Docker streaming throughput (if available)
    if docker_available {
        group.bench_function("docker_pipeline", |b| {
            b.to_async(&rt).iter_batched_ref(
                || {
                    // Setup
                    let executor = MultiprocessExecutor::new();
                    let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                    let node_id = "docker_node";

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

                    let ctx = ExecutionContext {
                        node_id: node_id.to_string(),
                        session_id: Some(session_id.clone()),
                        metadata,
                        trace_id: None,
                    };

                    rt.block_on(executor.initialize(&ctx, &session_id)).unwrap();

                    let chunks: Vec<RuntimeData> = (0..num_chunks)
                        .map(|_| generate_audio_data(chunk_size_ms, 16000))
                        .collect();

                    (executor, session_id, node_id.to_string(), chunks)
                },
                |(executor, session_id, node_id, chunks)| async {
                    let start = Instant::now();

                    for chunk in chunks.iter() {
                        executor
                            .send_data_to_node(node_id, session_id, chunk.clone())
                            .await
                            .unwrap();
                    }

                    let elapsed = start.elapsed();
                    let throughput = chunks.len() as f64 / elapsed.as_secs_f64();
                    throughput // Return chunks per second
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark complete E2E pipeline (init + transfer + cleanup)
fn bench_e2e_pipeline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let docker_available = rt.block_on(is_docker_available());

    let mut group = c.benchmark_group("e2e_pipeline");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20);

    // Native E2E pipeline
    group.bench_function("native_complete", |b| {
        b.to_async(&rt).iter(|| async {
            let executor = MultiprocessExecutor::new();
            let session_id = format!("bench_{}", uuid::Uuid::new_v4());
            let node_id = "native_node";

            // Start timing
            let start = Instant::now();

            // Initialize
            let ctx = ExecutionContext {
                node_id: node_id.to_string(),
                session_id: Some(session_id.clone()),
                metadata: HashMap::new(),
                trace_id: None,
            };
            executor.initialize(&ctx, &session_id).await.unwrap();

            // Send 100ms of audio
            let test_data = generate_audio_data(100, 16000);
            executor
                .send_data_to_node(node_id, &session_id, test_data)
                .await
                .unwrap();

            // Cleanup
            executor.terminate_session(&session_id).await.unwrap();

            start.elapsed()
        });
    });

    // Docker E2E pipeline (if available)
    if docker_available {
        group.bench_function("docker_complete", |b| {
            b.to_async(&rt).iter(|| async {
                let executor = MultiprocessExecutor::new();
                let session_id = format!("bench_{}", uuid::Uuid::new_v4());
                let node_id = "docker_node";

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

                let ctx = ExecutionContext {
                    node_id: node_id.to_string(),
                    session_id: Some(session_id.clone()),
                    metadata,
                    trace_id: None,
                };
                executor.initialize(&ctx, &session_id).await.unwrap();

                // Send 100ms of audio
                let test_data = generate_audio_data(100, 16000);
                executor
                    .send_data_to_node(node_id, &session_id, test_data)
                    .await
                    .unwrap();

                // Cleanup
                executor.terminate_session(&session_id).await.unwrap();

                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Benchmark concurrent pipeline execution
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
                            let executor = MultiprocessExecutor::new();
                            let session_id = format!("bench_{}_{}", i, uuid::Uuid::new_v4());
                            let node_id = format!("native_node_{}", i);

                            let ctx = ExecutionContext {
                                node_id: node_id.clone(),
                                session_id: Some(session_id.clone()),
                                metadata: HashMap::new(),
                                trace_id: None,
                            };

                            executor.initialize(&ctx, &session_id).await.unwrap();

                            let test_data = generate_audio_data(50, 16000);
                            executor
                                .send_data_to_node(&node_id, &session_id, test_data)
                                .await
                                .unwrap();

                            executor.terminate_session(&session_id).await.unwrap();
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.await.unwrap();
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
                                let executor = MultiprocessExecutor::new();
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

                                let ctx = ExecutionContext {
                                    node_id: node_id.clone(),
                                    session_id: Some(session_id.clone()),
                                    metadata,
                                    trace_id: None,
                                };

                                executor.initialize(&ctx, &session_id).await.unwrap();

                                let test_data = generate_audio_data(50, 16000);
                                executor
                                    .send_data_to_node(&node_id, &session_id, test_data)
                                    .await
                                    .unwrap();

                                executor.terminate_session(&session_id).await.unwrap();
                            });
                            handles.push(handle);
                        }

                        for handle in handles {
                            handle.await.unwrap();
                        }

                        start.elapsed()
                    });
                },
            );
        }
    }

    group.finish();
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
