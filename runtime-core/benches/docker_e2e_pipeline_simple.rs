//! Simple E2E Pipeline Benchmark: Docker vs Native Multiprocess
//!
//! This benchmark compares the overall throughput and latency of pipelines
//! running in Docker containers vs native multiprocess execution.

use criterion::{criterion_group, criterion_main, Criterion};
use std::time::{Duration, Instant};

/// Helper to check if Docker is available
fn is_docker_available() -> bool {
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return false;
    }

    // Try docker version command
    std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Benchmark initialization overhead
fn bench_initialization(c: &mut Criterion) {
    let docker_available = is_docker_available();

    let mut group = c.benchmark_group("initialization");
    group.measurement_time(Duration::from_secs(20));
    group.sample_size(10);

    // Native multiprocess initialization
    group.bench_function("native", |b| {
        b.iter(|| {
            let start = Instant::now();

            // Simulate native process startup
            std::process::Command::new("python3")
                .args(&["-c", "import sys; sys.exit(0)"])
                .status()
                .expect("Failed to run Python");

            start.elapsed()
        });
    });

    // Docker initialization (if available)
    if docker_available {
        group.bench_function("docker", |b| {
            b.iter(|| {
                let start = Instant::now();

                // Run a minimal Docker container
                std::process::Command::new("docker")
                    .args(&[
                        "run",
                        "--rm",
                        "python:3.10-slim",
                        "python",
                        "-c",
                        "import sys; sys.exit(0)"
                    ])
                    .status()
                    .expect("Failed to run Docker");

                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Benchmark data transfer throughput
fn bench_throughput(c: &mut Criterion) {
    let docker_available = is_docker_available();

    let mut group = c.benchmark_group("throughput");
    group.throughput(criterion::Throughput::Bytes(1024 * 1024)); // 1MB
    group.measurement_time(Duration::from_secs(10));

    // Native IPC throughput (simulated)
    group.bench_function("native_ipc", |b| {
        let data = vec![0u8; 1024 * 1024]; // 1MB
        b.iter(|| {
            // Simulate IPC transfer
            let _copy = data.clone();
        });
    });

    // Docker volume mount throughput (if available)
    if docker_available {
        group.bench_function("docker_volume", |b| {
            // Create temp file
            let temp_file = "/tmp/bench_data.bin";
            std::fs::write(temp_file, vec![0u8; 1024 * 1024]).ok();

            b.iter(|| {
                // Read through volume mount (simulated)
                let _ = std::fs::read(temp_file);
            });

            std::fs::remove_file(temp_file).ok();
        });
    }

    group.finish();
}

/// Run complete E2E pipeline comparison
fn bench_e2e_pipeline(c: &mut Criterion) {
    if !is_docker_available() {
        println!("Skipping E2E benchmark: Docker not available");
        return;
    }

    let mut group = c.benchmark_group("e2e_pipeline");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(5);

    // Native pipeline
    group.bench_function("native_complete", |b| {
        b.iter(|| {
            let start = Instant::now();

            // Simulate complete native pipeline
            // 1. Start process
            // 2. Transfer data
            // 3. Cleanup
            std::process::Command::new("python3")
                .args(&[
                    "-c",
                    "import time; time.sleep(0.01); print('processed')"
                ])
                .output()
                .expect("Failed to run native pipeline");

            start.elapsed()
        });
    });

    // Docker pipeline
    group.bench_function("docker_complete", |b| {
        b.iter(|| {
            let start = Instant::now();

            // Simulate complete Docker pipeline
            std::process::Command::new("docker")
                .args(&[
                    "run",
                    "--rm",
                    "-m", "512m",
                    "--cpus", "1.0",
                    "python:3.10-slim",
                    "python",
                    "-c",
                    "import time; time.sleep(0.01); print('processed')"
                ])
                .output()
                .expect("Failed to run Docker pipeline");

            start.elapsed()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialization,
    bench_throughput,
    bench_e2e_pipeline
);

criterion_main!(benches);