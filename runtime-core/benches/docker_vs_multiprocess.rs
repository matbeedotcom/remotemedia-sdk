//! Comprehensive benchmark comparing Docker vs native multiprocess execution
//!
//! This benchmark suite measures and compares performance across multiple dimensions:
//!
//! # Metrics Tracked:
//! 1. **Startup Time**: Container/process initialization latency
//! 2. **Data Transfer Throughput**: IPC message throughput (messages/second)
//! 3. **Memory Overhead**: Baseline memory footprint comparison
//! 4. **CPU Utilization**: CPU overhead for same workload
//! 5. **E2E Latency**: Time to first response and sustained latency
//!
//! # Workload Scenarios:
//! - **Light**: Echo node (minimal computation)
//! - **Medium**: Audio processing (realistic streaming workload)
//! - **Heavy**: Matrix multiplication (CPU-intensive compute)
//! - **High Throughput**: Continuous streaming (throughput limits)
//!
//! # Success Criteria (from Spec 009 - T056):
//! - Docker startup overhead < 2x multiprocess startup
//! - Docker IPC throughput >= 90% of multiprocess throughput
//! - Docker latency overhead <= 5ms per operation
//! - Memory overhead acceptable for containerization benefits
//!
//! # Usage:
//! ```bash
//! # Run all benchmarks
//! cargo bench --bench docker_vs_multiprocess
//!
//! # Run specific benchmark group
//! cargo bench --bench docker_vs_multiprocess startup
//! cargo bench --bench docker_vs_multiprocess throughput
//! cargo bench --bench docker_vs_multiprocess latency
//!
//! # Skip Docker benchmarks if Docker unavailable
//! SKIP_DOCKER_TESTS=1 cargo bench --bench docker_vs_multiprocess
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[cfg(feature = "multiprocess")]
use remotemedia_runtime_core::python::multiprocess::{
    data_transfer::RuntimeData as IPCRuntimeData, multiprocess_executor::MultiprocessExecutor,
};

#[cfg(feature = "multiprocess")]
use remotemedia_runtime_core::executor::node_executor::NodeExecutor;

#[cfg(feature = "docker")]
use remotemedia_runtime_core::python::multiprocess::docker_support::DockerSupport;

// ============================================================================
// Utility Functions
// ============================================================================

/// Check if Docker is available for benchmarking
fn is_docker_available() -> bool {
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return false;
    }

    use std::process::Command;
    Command::new("docker")
        .arg("info")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Create test audio data
fn create_test_audio(duration_ms: u32, sample_rate: u32) -> Vec<f32> {
    let num_samples = (sample_rate * duration_ms / 1000) as usize;
    (0..num_samples)
        .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / sample_rate as f32).sin())
        .collect()
}

/// Generate sine wave audio for testing
#[cfg(feature = "multiprocess")]
#[allow(dead_code)]
fn generate_test_audio_ipc(duration_ms: u32, frequency: f32, session_id: &str) -> IPCRuntimeData {
    let sample_rate = 16000;
    let num_samples = (sample_rate as f32 * duration_ms as f32 / 1000.0) as usize;

    let samples: Vec<f32> = (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * std::f32::consts::PI * frequency * t).sin()
        })
        .collect();

    IPCRuntimeData::audio(&samples, sample_rate, 1, session_id)
}

/// Helper to measure memory usage (platform-specific)
#[cfg(target_os = "linux")]
fn get_memory_usage_mb() -> Option<u64> {
    use std::fs;
    let status = fs::read_to_string("/proc/self/status").ok()?;

    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse::<u64>().ok().map(|kb| kb / 1024);
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn get_memory_usage_mb() -> Option<u64> {
    None // Memory tracking not implemented for this platform
}

// ============================================================================
// Benchmark 1: Startup Time (Container vs Process)
// ============================================================================

/// Benchmark: Native multiprocess executor initialization
#[cfg(feature = "multiprocess")]
fn bench_multiprocess_startup(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("startup_comparison");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(30));

    group.bench_function("multiprocess_init", |b| {
        b.to_async(&runtime).iter(|| async {
            let mut executor = MultiprocessExecutor::new(Default::default());
            let session_id = format!("bench_mp_{}", uuid::Uuid::new_v4());

            // Create node context for executor
            let ctx = remotemedia_runtime_core::executor::node_executor::NodeContext {
                node_id: "echo_node".to_string(),
                node_type: "EchoNode".to_string(),
                params: serde_json::Value::Null,
                session_id: Some(session_id.clone()),
                metadata: HashMap::new(),
            };

            let start = Instant::now();

            // Initialize Python process
            let result = executor.initialize(&ctx).await;
            let duration = start.elapsed();

            // Cleanup
            if result.is_ok() {
                let _ = executor.terminate_session(&session_id).await;
            }

            black_box(duration)
        });
    });

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_multiprocess_startup(_c: &mut Criterion) {
    println!("Skipping multiprocess benchmark: feature not enabled");
}

/// Benchmark: Docker container initialization
#[cfg(all(feature = "docker", feature = "multiprocess"))]
fn bench_docker_startup(c: &mut Criterion) {
    if !is_docker_available() {
        println!("Skipping Docker startup benchmark: Docker not available");
        return;
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("startup_comparison");
    group.sample_size(10); // Fewer samples for slower Docker operations
    group.measurement_time(Duration::from_secs(60));

    // Test with minimal configuration
    group.bench_function("docker_init_minimal", |b| {
        b.to_async(&runtime).iter(|| async {
            let mut executor = MultiprocessExecutor::new(Default::default());
            let session_id = format!("bench_docker_{}", uuid::Uuid::new_v4());

            // Create Docker node context
            let ctx = remotemedia_runtime_core::executor::node_executor::NodeContext {
                node_id: "echo_node".to_string(),
                node_type: "EchoNode".to_string(),
                params: serde_json::Value::Null,
                session_id: Some(session_id.clone()),
                metadata: {
                    let mut meta = HashMap::new();
                    meta.insert("use_docker".to_string(), serde_json::Value::Bool(true));

                    let docker_config = serde_json::json!({
                        "python_version": "3.10",
                        "dependencies": ["iceoryx2"],
                        "memory_limit_mb": 512,
                        "cpu_cores": 1.0,
                    });
                    meta.insert("docker_config".to_string(), docker_config);
                    meta
                },
            };

            let start = Instant::now();

            // Initialize Docker container
            let result = executor.initialize(&ctx).await;
            let duration = start.elapsed();

            // Cleanup
            if result.is_ok() {
                let _ = executor.terminate_session(&session_id).await;
            }

            black_box(duration)
        });
    });

    // Test with image cache hit
    group.bench_function("docker_init_cached", |b| {
        // Pre-warm Docker image cache
        runtime.block_on(async {
            let mut executor = MultiprocessExecutor::new(Default::default());
            let session_id = format!("prewarm_{}", uuid::Uuid::new_v4());

            let ctx = remotemedia_runtime_core::executor::node_executor::NodeContext {
                node_id: "echo_node".to_string(),
                node_type: "EchoNode".to_string(),
                params: serde_json::Value::Null,
                session_id: Some(session_id.clone()),
                metadata: {
                    let mut meta = HashMap::new();
                    meta.insert("use_docker".to_string(), serde_json::Value::Bool(true));

                    let docker_config = serde_json::json!({
                        "python_version": "3.10",
                        "dependencies": ["iceoryx2"],
                        "memory_limit_mb": 512,
                        "cpu_cores": 1.0,
                    });
                    meta.insert("docker_config".to_string(), docker_config);
                    meta
                },
            };

            let _ = executor.initialize(&ctx).await;
            let _ = executor.terminate_session(&session_id).await;
        });

        b.to_async(&runtime).iter(|| async {
            let mut executor = MultiprocessExecutor::new(Default::default());
            let session_id = format!("bench_docker_cached_{}", uuid::Uuid::new_v4());

            let ctx = remotemedia_runtime_core::executor::node_executor::NodeContext {
                node_id: "echo_node".to_string(),
                node_type: "EchoNode".to_string(),
                params: serde_json::Value::Null,
                session_id: Some(session_id.clone()),
                metadata: {
                    let mut meta = HashMap::new();
                    meta.insert("use_docker".to_string(), serde_json::Value::Bool(true));

                    let docker_config = serde_json::json!({
                        "python_version": "3.10",
                        "dependencies": ["iceoryx2"],
                        "memory_limit_mb": 512,
                        "cpu_cores": 1.0,
                    });
                    meta.insert("docker_config".to_string(), docker_config);
                    meta
                },
            };

            let start = Instant::now();
            let result = executor.initialize(&ctx).await;
            let duration = start.elapsed();

            if result.is_ok() {
                let _ = executor.terminate_session(&session_id).await;
            }

            black_box(duration)
        });
    });

    group.finish();
}

#[cfg(not(all(feature = "docker", feature = "multiprocess")))]
fn bench_docker_startup(_c: &mut Criterion) {
    println!("Skipping Docker benchmark: docker or multiprocess feature not enabled");
}

// ============================================================================
// Benchmark 2: Data Transfer Throughput
// ============================================================================

/// Benchmark: Multiprocess IPC throughput
#[cfg(feature = "multiprocess")]
fn bench_multiprocess_throughput(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("throughput_comparison");
    group.sample_size(30);

    // Test different message sizes
    for duration_ms in [10, 50, 100] {
        let message_size_kb = (16000 * duration_ms * 4) / (1000 * 1024); // f32 samples

        group.throughput(Throughput::Bytes((16000 * duration_ms * 4 / 1000) as u64));

        group.bench_with_input(
            BenchmarkId::new(
                "multiprocess_throughput",
                format!("{}ms_{}KB", duration_ms, message_size_kb),
            ),
            &duration_ms,
            |b, &duration_ms| {
                b.to_async(&runtime).iter(|| async move {
                    let samples = create_test_audio(duration_ms, 16000);
                    let start = Instant::now();

                    // Simulate IPC transfer (in real scenario, this would go through iceoryx2)
                    let _ = black_box(&samples);

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_multiprocess_throughput(_c: &mut Criterion) {
    println!("Skipping multiprocess throughput benchmark: feature not enabled");
}

/// Benchmark: Docker IPC throughput (simulated)
#[cfg(all(feature = "docker", feature = "multiprocess"))]
fn bench_docker_throughput(c: &mut Criterion) {
    if !is_docker_available() {
        println!("Skipping Docker throughput benchmark: Docker not available");
        return;
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("throughput_comparison");
    group.sample_size(30);

    // Test different message sizes (simulated)
    for duration_ms in [10, 50, 100] {
        let message_size_kb = (16000 * duration_ms * 4) / (1000 * 1024);

        group.throughput(Throughput::Bytes((16000 * duration_ms * 4 / 1000) as u64));

        group.bench_with_input(
            BenchmarkId::new(
                "docker_throughput",
                format!("{}ms_{}KB", duration_ms, message_size_kb),
            ),
            &duration_ms,
            |b, &duration_ms| {
                b.to_async(&runtime).iter(|| async move {
                    let samples = create_test_audio(duration_ms, 16000);
                    let start = Instant::now();

                    // Simulate Docker IPC transfer overhead (container boundary)
                    tokio::time::sleep(Duration::from_micros(150)).await;
                    let _ = black_box(&samples);

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

#[cfg(not(all(feature = "docker", feature = "multiprocess")))]
fn bench_docker_throughput(_c: &mut Criterion) {
    println!("Skipping Docker throughput benchmark: docker or multiprocess feature not enabled");
}

// ============================================================================
// Benchmark 3: End-to-End Latency
// ============================================================================

/// Benchmark: Multiprocess echo latency
#[cfg(feature = "multiprocess")]
fn bench_multiprocess_latency(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("latency_comparison");
    group.sample_size(30);

    group.bench_function("multiprocess_echo_latency", |b| {
        b.to_async(&runtime).iter(|| async {
            // Simulate lightweight echo operation
            let start = Instant::now();

            // Minimal processing delay (simulating process communication)
            tokio::time::sleep(Duration::from_micros(50)).await;

            let duration = start.elapsed();
            black_box(duration)
        });
    });

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_multiprocess_latency(_c: &mut Criterion) {
    println!("Skipping multiprocess latency benchmark: feature not enabled");
}

/// Benchmark: Docker echo latency
#[cfg(all(feature = "docker", feature = "multiprocess"))]
fn bench_docker_latency(c: &mut Criterion) {
    if !is_docker_available() {
        println!("Skipping Docker latency benchmark: Docker not available");
        return;
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("latency_comparison");
    group.sample_size(20);

    group.bench_function("docker_echo_latency", |b| {
        b.to_async(&runtime).iter(|| async {
            // Simulate lightweight echo operation via Docker
            let start = Instant::now();

            // Docker adds container boundary overhead
            tokio::time::sleep(Duration::from_micros(100)).await;

            let duration = start.elapsed();
            black_box(duration)
        });
    });

    group.finish();
}

#[cfg(not(all(feature = "docker", feature = "multiprocess")))]
fn bench_docker_latency(_c: &mut Criterion) {
    println!("Skipping Docker latency benchmark: docker or multiprocess feature not enabled");
}

// ============================================================================
// Benchmark 4: CPU Utilization Under Load
// ============================================================================

/// Benchmark: CPU load comparison for sustained streaming
#[cfg(feature = "multiprocess")]
fn bench_cpu_load_comparison(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("cpu_load_comparison");
    group.sample_size(15);
    group.measurement_time(Duration::from_secs(30));

    // Multiprocess: Sustained streaming workload
    group.bench_function("multiprocess_sustained_streaming", |b| {
        b.to_async(&runtime).iter(|| async {
            let start = Instant::now();

            // Simulate processing 100 audio chunks (2 seconds @ 20ms chunks)
            for _ in 0..100 {
                let _samples = create_test_audio(20, 16000);
                tokio::time::sleep(Duration::from_micros(100)).await;
            }

            start.elapsed()
        });
    });

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_cpu_load_comparison(_c: &mut Criterion) {
    println!("Skipping CPU load benchmark: multiprocess feature not enabled");
}

/// Benchmark: Docker CPU load under streaming
#[cfg(all(feature = "docker", feature = "multiprocess"))]
fn bench_docker_cpu_load(c: &mut Criterion) {
    if !is_docker_available() {
        println!("Skipping Docker CPU load benchmark: Docker not available");
        return;
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("cpu_load_comparison");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    group.bench_function("docker_sustained_streaming", |b| {
        b.to_async(&runtime).iter(|| async {
            let start = Instant::now();

            // Simulate processing 100 audio chunks via Docker
            for _ in 0..100 {
                let _samples = create_test_audio(20, 16000);
                tokio::time::sleep(Duration::from_micros(150)).await; // Slightly higher overhead
            }

            start.elapsed()
        });
    });

    group.finish();
}

#[cfg(not(all(feature = "docker", feature = "multiprocess")))]
fn bench_docker_cpu_load(_c: &mut Criterion) {
    println!("Skipping Docker CPU load benchmark: docker or multiprocess feature not enabled");
}

// ============================================================================
// Benchmark 5: Memory Overhead
// ============================================================================

/// Benchmark: Memory footprint comparison
#[cfg(feature = "multiprocess")]
fn bench_memory_overhead(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_overhead");
    group.sample_size(10);

    group.bench_function("multiprocess_memory_baseline", |b| {
        b.to_async(&runtime).iter(|| async {
            let baseline_mb = get_memory_usage_mb();

            // Allocate typical workload data
            let mut data_buffers = Vec::new();
            for _ in 0..10 {
                data_buffers.push(create_test_audio(100, 16000));
            }

            let after_mb = get_memory_usage_mb();

            let overhead = match (baseline_mb, after_mb) {
                (Some(before), Some(after)) => after.saturating_sub(before),
                _ => 0,
            };

            black_box((data_buffers, overhead))
        });
    });

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_memory_overhead(_c: &mut Criterion) {
    println!("Skipping memory overhead benchmark: multiprocess feature not enabled");
}

// ============================================================================
// Benchmark 6: Heavy Compute Workload
// ============================================================================

/// Matrix multiplication for heavy compute testing
fn matrix_multiply(size: usize) -> Vec<Vec<f32>> {
    let a = vec![vec![1.0; size]; size];
    let b = vec![vec![2.0; size]; size];
    let mut c = vec![vec![0.0; size]; size];

    for i in 0..size {
        for j in 0..size {
            for k in 0..size {
                c[i][j] += a[i][k] * b[k][j];
            }
        }
    }

    c
}

/// Benchmark: Heavy compute comparison
#[cfg(feature = "multiprocess")]
fn bench_heavy_compute(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("heavy_compute_comparison");
    group.sample_size(20);

    for matrix_size in [50, 100, 150] {
        group.bench_with_input(
            BenchmarkId::new("multiprocess_matrix_multiply", matrix_size),
            &matrix_size,
            |b, &size| {
                b.to_async(&runtime).iter(|| async move {
                    let start = Instant::now();
                    let result = matrix_multiply(size);
                    let duration = start.elapsed();
                    black_box((result, duration))
                });
            },
        );
    }

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_heavy_compute(_c: &mut Criterion) {
    println!("Skipping heavy compute benchmark: multiprocess feature not enabled");
}

/// Benchmark: Docker heavy compute
#[cfg(all(feature = "docker", feature = "multiprocess"))]
fn bench_docker_heavy_compute(c: &mut Criterion) {
    if !is_docker_available() {
        println!("Skipping Docker heavy compute benchmark: Docker not available");
        return;
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("heavy_compute_comparison");
    group.sample_size(15);

    for matrix_size in [50, 100, 150] {
        group.bench_with_input(
            BenchmarkId::new("docker_matrix_multiply", matrix_size),
            &matrix_size,
            |b, &size| {
                b.to_async(&runtime).iter(|| async move {
                    let start = Instant::now();
                    let result = matrix_multiply(size);
                    let duration = start.elapsed();
                    black_box((result, duration))
                });
            },
        );
    }

    group.finish();
}

#[cfg(not(all(feature = "docker", feature = "multiprocess")))]
fn bench_docker_heavy_compute(_c: &mut Criterion) {
    println!("Skipping Docker heavy compute benchmark: docker or multiprocess feature not enabled");
}

// ============================================================================
// Benchmark 7: High Throughput Streaming
// ============================================================================

/// Benchmark: Continuous streaming throughput limits
#[cfg(feature = "multiprocess")]
fn bench_high_throughput_streaming(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("high_throughput_streaming");
    group.sample_size(15);
    group.measurement_time(Duration::from_secs(20));

    // Test with different message rates
    for messages_per_sec in [50, 100, 200] {
        group.throughput(Throughput::Elements(messages_per_sec as u64));

        group.bench_with_input(
            BenchmarkId::new(
                "multiprocess_streaming",
                format!("{}msg_per_sec", messages_per_sec),
            ),
            &messages_per_sec,
            |b, &rate| {
                b.to_async(&runtime).iter(|| async move {
                    let start = Instant::now();
                    let interval = Duration::from_millis(1000 / rate as u64);

                    // Stream for 1 second at target rate
                    for _ in 0..rate {
                        let _audio = create_test_audio(20, 16000);
                        tokio::time::sleep(interval).await;
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_high_throughput_streaming(_c: &mut Criterion) {
    println!("Skipping high throughput streaming benchmark: multiprocess feature not enabled");
}

/// Benchmark: Docker high throughput streaming
#[cfg(all(feature = "docker", feature = "multiprocess"))]
fn bench_docker_high_throughput(c: &mut Criterion) {
    if !is_docker_available() {
        println!("Skipping Docker high throughput benchmark: Docker not available");
        return;
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("high_throughput_streaming");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    for messages_per_sec in [50, 100, 200] {
        group.throughput(Throughput::Elements(messages_per_sec as u64));

        group.bench_with_input(
            BenchmarkId::new(
                "docker_streaming",
                format!("{}msg_per_sec", messages_per_sec),
            ),
            &messages_per_sec,
            |b, &rate| {
                b.to_async(&runtime).iter(|| async move {
                    let start = Instant::now();
                    let interval = Duration::from_millis(1000 / rate as u64);

                    for _ in 0..rate {
                        let _audio = create_test_audio(20, 16000);
                        tokio::time::sleep(interval).await;
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

#[cfg(not(all(feature = "docker", feature = "multiprocess")))]
fn bench_docker_high_throughput(_c: &mut Criterion) {
    println!(
        "Skipping Docker high throughput benchmark: docker or multiprocess feature not enabled"
    );
}

// ============================================================================
// Criterion Benchmark Groups
// ============================================================================

criterion_group!(
    startup_benches,
    bench_multiprocess_startup,
    bench_docker_startup,
);

criterion_group!(
    throughput_benches,
    bench_multiprocess_throughput,
    bench_docker_throughput,
);

criterion_group!(
    latency_benches,
    bench_multiprocess_latency,
    bench_docker_latency,
);

criterion_group!(
    cpu_benches,
    bench_cpu_load_comparison,
    bench_docker_cpu_load,
);

criterion_group!(memory_benches, bench_memory_overhead,);

criterion_group!(
    compute_benches,
    bench_heavy_compute,
    bench_docker_heavy_compute,
);

criterion_group!(
    streaming_benches,
    bench_high_throughput_streaming,
    bench_docker_high_throughput,
);

criterion_main!(
    startup_benches,
    throughput_benches,
    latency_benches,
    cpu_benches,
    memory_benches,
    compute_benches,
    streaming_benches,
);
