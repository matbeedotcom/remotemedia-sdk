//! Benchmarks for StreamingScheduler and DriftMetrics (spec 026)
//!
//! Success criteria from spec:
//! - T088: Scheduler overhead <1% of node execution time
//! - T089: Drift recording <1μs per sample
//! - T090: Health score calculation <10μs
//! - T091: Prometheus export <10ms for 100 streams
//!
//! # Benchmark Categories
//!
//! ## T088 Scheduler Overhead
//!
//! Three measurement modes:
//! - **Absolute overhead**: Fixed cost per call with immediate-return operation
//! - **Relative overhead**: Percentage overhead vs CPU-bound work at various durations
//! - **Fast path**: Minimal overhead path without timeout/metrics
//!
//! ## Contention Benchmarks
//!
//! - Same node contention (hot node, lock contention)
//! - Different nodes (map contention, growth)
//!
//! ## Configuration Comparison
//!
//! - Metrics enabled vs disabled
//! - Cold path (first call) vs warm path (steady state)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use remotemedia_runtime_core::executor::drift_metrics::{DriftMetrics, DriftThresholds};
use remotemedia_runtime_core::executor::streaming_scheduler::{SchedulerConfig, StreamingScheduler};
use remotemedia_runtime_core::Error;
use std::hint::black_box as hint_black_box;
use std::sync::Arc;
use std::time::Duration;

/// CPU-bound work that takes approximately the specified number of iterations.
/// Each iteration does ~1ns of work (simple arithmetic).
///
/// Uses volatile read/write to prevent loop elimination while keeping
/// actual work minimal. The black_box inside the loop ensures the compiler
/// cannot optimize away iterations.
#[inline(never)]
fn cpu_work(iterations: u64) -> u64 {
    let mut result = 0u64;
    for i in 0..iterations {
        // black_box on each iteration prevents loop elimination/vectorization
        result = hint_black_box(result.wrapping_add(i.wrapping_mul(17)));
    }
    result
}

/// Calibrate iterations to approximate microseconds of CPU work.
/// Returns iterations needed for approximately 1µs of work.
///
/// Uses larger loop (10K iterations) and multiple runs taking minimum
/// for more stable calibration.
fn calibrate_iterations() -> u64 {
    const CALIBRATION_ITERS: u64 = 10_000;
    const CALIBRATION_RUNS: usize = 5;

    let mut min_nanos = u64::MAX;

    for _ in 0..CALIBRATION_RUNS {
        let start = std::time::Instant::now();
        let _ = cpu_work(CALIBRATION_ITERS);
        let elapsed = start.elapsed().as_nanos() as u64;
        min_nanos = min_nanos.min(elapsed);
    }

    // Calculate iterations per microsecond
    if min_nanos == 0 {
        1000 // Fallback
    } else {
        // iters_per_us = CALIBRATION_ITERS / (min_nanos / 1000)
        (CALIBRATION_ITERS * 1000) / min_nanos
    }
}

// ============================================================================
// T088: Scheduler Overhead Benchmarks
// ============================================================================

/// T088-A: Absolute per-call scheduler overhead with immediate-return operation.
///
/// This measures the fixed cost of the scheduler wrapper:
/// - Node state lookup (read lock)
/// - Circuit breaker check
/// - Semaphore acquire/release
/// - Timeout wrapper
/// - Metrics recording
fn bench_scheduler_absolute_overhead(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("scheduler_absolute_overhead");
    group.measurement_time(Duration::from_secs(5));

    // Full path with all features
    let scheduler_full = StreamingScheduler::new(SchedulerConfig {
        enable_metrics: true,
        ..Default::default()
    });

    // Pre-warm the node state
    rt.block_on(async {
        let _ = scheduler_full
            .execute_streaming_node("warm_node", || async { Ok::<_, Error>(()) })
            .await;
    });

    group.bench_function("full_path_warm", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(
                scheduler_full
                    .execute_streaming_node("warm_node", || async { Ok::<_, Error>(42) })
                    .await
                    .unwrap()
                    .result,
            )
        });
    });

    // Full path cold (new node each time)
    // Pre-generate node IDs to avoid format! allocation in timed loop
    let scheduler_cold = StreamingScheduler::with_defaults();
    let cold_node_ids: Vec<String> = (0..100_000).map(|i| format!("cold_node_{}", i)).collect();
    let cold_counter = std::sync::atomic::AtomicUsize::new(0);

    group.bench_function("full_path_cold", |b| {
        b.to_async(&rt).iter(|| {
            let idx = cold_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % cold_node_ids.len();
            let node_id = &cold_node_ids[idx];
            let sched = &scheduler_cold;
            async move {
                black_box(
                    sched
                        .execute_streaming_node(node_id, || async { Ok::<_, Error>(42) })
                        .await
                        .unwrap()
                        .result,
                )
            }
        });
    });

    // Fast path with minimal overhead
    let scheduler_fast = StreamingScheduler::with_defaults();
    rt.block_on(async {
        let _ = scheduler_fast
            .execute_streaming_node_fast("fast_warm_node", || async { Ok::<_, Error>(()) })
            .await;
    });

    group.bench_function("fast_path_warm", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(
                scheduler_fast
                    .execute_streaming_node_fast("fast_warm_node", || async { Ok::<_, Error>(42) })
                    .await
                    .unwrap()
                    .result,
            )
        });
    });

    // Metrics disabled path
    let scheduler_no_metrics = StreamingScheduler::new(SchedulerConfig {
        enable_metrics: false,
        ..Default::default()
    });
    rt.block_on(async {
        let _ = scheduler_no_metrics
            .execute_streaming_node("no_metrics_node", || async { Ok::<_, Error>(()) })
            .await;
    });

    group.bench_function("full_path_no_metrics", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(
                scheduler_no_metrics
                    .execute_streaming_node("no_metrics_node", || async { Ok::<_, Error>(42) })
                    .await
                    .unwrap()
                    .result,
            )
        });
    });

    group.finish();
}

/// T088-B: Overhead ratio vs CPU-bound work.
///
/// Measures (scheduler_time - direct_time) / direct_time to get true overhead %.
/// Uses calibrated CPU work at various durations.
///
/// CRITICAL: Baseline uses async runtime to match scheduler measurements.
/// Comparing sync baseline to async scheduler would show async runtime overhead,
/// not scheduler overhead.
fn bench_scheduler_overhead_ratio(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let iters_per_us = calibrate_iterations();

    let mut group = c.benchmark_group("scheduler_overhead_ratio");
    group.measurement_time(Duration::from_secs(5));

    // Test at various work durations (in microseconds)
    for work_us in [10, 100, 1000, 10000] {
        let iterations = work_us * iters_per_us;

        // Baseline: ASYNC direct CPU work (matches scheduler's async context)
        // This measures pure work inside async runtime without scheduler overhead
        // NOTE: We use black_box on iterations to prevent constant propagation optimization
        group.bench_with_input(
            BenchmarkId::new("direct_async", work_us),
            &iterations,
            |b, &iters| {
                b.to_async(&rt).iter(|| async move {
                    black_box(cpu_work(black_box(iters)))
                });
            },
        );

        // With scheduler full path
        let scheduler = StreamingScheduler::with_defaults();
        rt.block_on(async {
            let _ = scheduler
                .execute_streaming_node("bench_node", || async { Ok::<_, Error>(()) })
                .await;
        });

        group.bench_with_input(
            BenchmarkId::new("full_path", work_us),
            &iterations,
            |b, &iters| {
                b.to_async(&rt).iter(|| async {
                    black_box(
                        scheduler
                            .execute_streaming_node("bench_node", || async {
                                Ok::<_, Error>(cpu_work(black_box(iters)))
                            })
                            .await
                            .unwrap()
                            .result,
                    )
                });
            },
        );

        // With scheduler fast path
        let scheduler_fast = StreamingScheduler::with_defaults();
        rt.block_on(async {
            let _ = scheduler_fast
                .execute_streaming_node_fast("fast_bench_node", || async { Ok::<_, Error>(()) })
                .await;
        });

        group.bench_with_input(
            BenchmarkId::new("fast_path", work_us),
            &iterations,
            |b, &iters| {
                b.to_async(&rt).iter(|| async {
                    black_box(
                        scheduler_fast
                            .execute_streaming_node_fast("fast_bench_node", || async {
                                Ok::<_, Error>(cpu_work(black_box(iters)))
                            })
                            .await
                            .unwrap()
                            .result,
                    )
                });
            },
        );
    }

    group.finish();
}

/// T088-C: Contention benchmarks.
///
/// Tests scheduler behavior under concurrent access:
/// - Same node (hot node contention)
/// - Different nodes (map contention)
///
/// CRITICAL: Uses pre-spawned tasks waiting on barrier to measure actual
/// contention, not spawn overhead. Previous version measured tokio::spawn
/// cost which dwarfed actual scheduler contention.
fn bench_scheduler_contention(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("scheduler_contention");
    group.measurement_time(Duration::from_secs(5));

    // Pre-generate node IDs to avoid format! in timed path
    let node_ids: Vec<String> = (0..16).map(|i| format!("contention_node_{}", i)).collect();

    // Concurrent tasks on same node - using join_all instead of spawn
    // This measures actual scheduler contention without spawn overhead
    for num_tasks in [2, 4, 8, 16] {
        let scheduler = Arc::new(StreamingScheduler::with_defaults());

        // Pre-warm
        rt.block_on(async {
            let _ = scheduler
                .execute_streaming_node("hot_node", || async { Ok::<_, Error>(()) })
                .await;
        });

        group.bench_with_input(
            BenchmarkId::new("same_node_join", num_tasks),
            &num_tasks,
            |b, &n| {
                b.to_async(&rt).iter(|| {
                    let sched = scheduler.clone();
                    async move {
                        // Use futures::join_all for concurrent execution without spawn overhead
                        let futures: Vec<_> = (0..n)
                            .map(|_| {
                                let s = sched.clone();
                                async move {
                                    s.execute_streaming_node("hot_node", || async {
                                        Ok::<_, Error>(42)
                                    })
                                    .await
                                }
                            })
                            .collect();

                        let results = futures::future::join_all(futures).await;
                        for r in results {
                            let _ = black_box(r);
                        }
                    }
                });
            },
        );

        // Concurrent tasks on different nodes (pre-generated IDs)
        group.bench_with_input(
            BenchmarkId::new("different_nodes_join", num_tasks),
            &num_tasks,
            |b, &n| {
                let ids = &node_ids[..n];
                b.to_async(&rt).iter(|| {
                    let sched = scheduler.clone();
                    async move {
                        let futures: Vec<_> = ids
                            .iter()
                            .map(|node_id| {
                                let s = sched.clone();
                                let id = node_id.clone();
                                async move {
                                    s.execute_streaming_node(&id, || async {
                                        Ok::<_, Error>(42)
                                    })
                                    .await
                                }
                            })
                            .collect();

                        let results = futures::future::join_all(futures).await;
                        for r in results {
                            let _ = black_box(r);
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// T089: Drift Recording Benchmarks
// ============================================================================

/// T089: Benchmark drift recording.
///
/// Measures time to record a single drift sample.
/// Success criteria: <1μs per sample
fn bench_drift_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("drift_recording");
    group.measurement_time(Duration::from_secs(3));

    // Pre-warm metrics with some samples
    let mut metrics = DriftMetrics::with_defaults("bench_stream".to_string());
    for i in 0..100 {
        metrics.record_sample(i * 33_333, i * 33_333, None);
    }

    let mut media_ts = 100 * 33_333u64;
    let mut arrival_ts = 100 * 33_333u64;

    group.bench_function("record_sample", |b| {
        b.iter(|| {
            media_ts += 33_333;
            arrival_ts += 33_333;
            black_box(metrics.record_sample(
                black_box(media_ts),
                black_box(arrival_ts),
                None,
            ))
        });
    });

    // With content hash (freeze detection)
    group.bench_function("record_sample_with_hash", |b| {
        let mut hash = 12345u64;
        b.iter(|| {
            media_ts += 33_333;
            arrival_ts += 33_333;
            hash = hash.wrapping_add(1);
            black_box(metrics.record_sample(
                black_box(media_ts),
                black_box(arrival_ts),
                Some(black_box(hash)),
            ))
        });
    });

    // A/V sample recording
    group.bench_function("record_audio_sample", |b| {
        b.iter(|| {
            media_ts += 33_333;
            arrival_ts += 33_333;
            black_box(metrics.record_audio_sample(
                black_box(media_ts),
                black_box(arrival_ts),
            ))
        });
    });

    group.finish();
}

/// T089-B: Batched drift recording benchmark.
///
/// Measures amortized cost when recording multiple samples.
/// This tests real-world usage where samples arrive in bursts.
fn bench_drift_recording_batched(c: &mut Criterion) {
    let mut group = c.benchmark_group("drift_recording_batched");
    group.measurement_time(Duration::from_secs(3));

    for batch_size in [10, 100, 1000] {
        let mut metrics = DriftMetrics::with_defaults("bench_batch_stream".to_string());
        // Pre-warm
        for i in 0..100 {
            metrics.record_sample(i * 33_333, i * 33_333, None);
        }

        let mut media_ts = 100 * 33_333u64;
        let mut arrival_ts = 100 * 33_333u64;

        group.throughput(Throughput::Elements(batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("batch", batch_size),
            &batch_size,
            |b, &n| {
                b.iter(|| {
                    for _ in 0..n {
                        media_ts += 33_333;
                        arrival_ts += 33_333;
                        black_box(metrics.record_sample(media_ts, arrival_ts, None));
                    }
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// T090: Health Score Calculation Benchmarks
// ============================================================================

/// T090: Benchmark health score calculation.
///
/// Measures time to calculate health score.
/// Success criteria: <10μs
fn bench_health_score(c: &mut Criterion) {
    let mut group = c.benchmark_group("health_score");
    group.measurement_time(Duration::from_secs(3));

    // Create metrics with realistic data
    let mut metrics = DriftMetrics::with_defaults("bench_stream".to_string());
    for i in 0u64..500 {
        let drift = (i % 100) * 100; // Some drift variation
        metrics.record_sample(
            i * 33_333,
            i * 33_333 + drift,
            Some(i % 10), // Occasional freeze
        );
    }

    group.bench_function("health_score", |b| {
        b.iter(|| black_box(metrics.health_score()))
    });

    group.bench_function("cadence_cv", |b| {
        b.iter(|| black_box(metrics.cadence_cv()))
    });

    group.bench_function("alerts", |b| {
        b.iter(|| black_box(metrics.alerts()))
    });

    group.bench_function("is_frozen", |b| {
        b.iter(|| black_box(metrics.is_frozen()))
    });

    group.bench_function("current_lead_us", |b| {
        b.iter(|| black_box(metrics.current_lead_us()))
    });

    group.finish();
}

// ============================================================================
// T091: Prometheus Export Benchmarks
// ============================================================================

/// T091: Benchmark Prometheus export.
///
/// Measures time to export Prometheus metrics for multiple streams.
/// Success criteria: <10ms for 100 streams
fn bench_prometheus_export(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("prometheus_export");
    group.measurement_time(Duration::from_secs(5));

    // Test with different stream counts
    for stream_count in [10, 50, 100, 200] {
        // Create DriftMetrics instances
        let metrics_vec: Vec<DriftMetrics> = (0..stream_count)
            .map(|i| {
                let mut m = DriftMetrics::with_defaults(format!("stream_{}", i));
                // Add some data
                for j in 0..100 {
                    m.record_sample(j * 33_333, j * 33_333, None);
                }
                m
            })
            .collect();

        group.throughput(Throughput::Elements(stream_count as u64));

        group.bench_with_input(
            BenchmarkId::new("drift_metrics", stream_count),
            &metrics_vec,
            |b, metrics| {
                b.iter(|| {
                    let mut output = String::with_capacity(stream_count * 500);
                    for m in metrics {
                        output.push_str(&m.to_prometheus("pipeline"));
                    }
                    black_box(output)
                });
            },
        );

        // Benchmark scheduler Prometheus export
        let scheduler = StreamingScheduler::with_defaults();

        // Pre-populate scheduler with node data
        rt.block_on(async {
            for i in 0..stream_count {
                let node_id = format!("node_{}", i);
                for _ in 0..10 {
                    let _ = scheduler
                        .execute_streaming_node(&node_id, || async { Ok::<_, Error>(()) })
                        .await;
                }
            }
        });

        group.bench_with_input(
            BenchmarkId::new("scheduler", stream_count),
            &stream_count,
            |b, _| {
                b.to_async(&rt).iter(|| async {
                    black_box(scheduler.to_prometheus().await)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Additional Benchmarks
// ============================================================================

/// Benchmark debug JSON export
fn bench_debug_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("debug_json");
    group.measurement_time(Duration::from_secs(3));

    let mut metrics = DriftMetrics::with_defaults("bench_stream".to_string());
    for i in 0..500 {
        metrics.record_sample(i * 33_333, i * 33_333, Some(i as u64));
    }

    group.bench_function("to_debug_json", |b| {
        b.iter(|| black_box(metrics.to_debug_json()))
    });

    group.finish();
}

/// Benchmark scheduler node stats retrieval
fn bench_scheduler_stats(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("scheduler_stats");
    group.measurement_time(Duration::from_secs(3));

    let scheduler = StreamingScheduler::with_defaults();

    // Pre-populate scheduler
    rt.block_on(async {
        for i in 0..50 {
            let node_id = format!("node_{}", i);
            for _ in 0..20 {
                let _ = scheduler
                    .execute_streaming_node(&node_id, || async { Ok::<_, Error>(()) })
                    .await;
            }
        }
    });

    group.bench_function("get_node_stats", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(scheduler.get_node_stats("node_25").await)
        });
    });

    group.bench_function("get_all_node_stats", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(scheduler.get_all_node_stats().await)
        });
    });

    group.bench_function("get_latency_percentiles", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(scheduler.get_latency_percentiles("node_25").await)
        });
    });

    group.finish();
}

/// Benchmark alert hysteresis
fn bench_alert_hysteresis(c: &mut Criterion) {
    let mut group = c.benchmark_group("alert_hysteresis");
    group.measurement_time(Duration::from_secs(3));

    // Create metrics with conditions that trigger alerts
    let thresholds = DriftThresholds {
        slope_threshold_ms_per_s: 1.0,
        samples_to_raise: 3,
        samples_to_clear: 5,
        ..Default::default()
    };
    let mut metrics = DriftMetrics::new("alert_bench".to_string(), thresholds);

    // Prime with samples that create drift
    for i in 0..100 {
        let drift = i * 1000; // Growing drift
        metrics.record_sample(i * 33_333, i * 33_333 + drift, None);
    }

    group.bench_function("update_alerts", |b| {
        let mut ts = 100 * 33_333u64;
        b.iter(|| {
            ts += 33_333;
            // Alternate between conditions to exercise hysteresis
            let drift = if (ts / 33_333) % 10 < 5 { 50_000 } else { 0 };
            black_box(metrics.record_sample(ts, ts + drift, None))
        });
    });

    group.finish();
}

/// Benchmark component isolation: timeout wrapper overhead
fn bench_timeout_wrapper_overhead(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("timeout_wrapper");
    group.measurement_time(Duration::from_secs(3));

    // Direct async call
    group.bench_function("direct_async", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(async { Ok::<_, Error>(42) }.await)
        });
    });

    // With timeout wrapper (very long timeout, just measuring wrapper cost)
    group.bench_function("with_timeout_wrapper", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(
                tokio::time::timeout(Duration::from_secs(30), async { Ok::<_, Error>(42) }).await,
            )
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_scheduler_absolute_overhead,
    bench_scheduler_overhead_ratio,
    bench_scheduler_contention,
    bench_drift_recording,
    bench_drift_recording_batched,
    bench_health_score,
    bench_prometheus_export,
    bench_debug_json,
    bench_scheduler_stats,
    bench_alert_hysteresis,
    bench_timeout_wrapper_overhead,
);
criterion_main!(benches);
