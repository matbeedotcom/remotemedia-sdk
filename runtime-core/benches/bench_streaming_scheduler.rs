//! Benchmarks for StreamingScheduler and DriftMetrics (spec 026)
//!
//! Success criteria from spec:
//! - T088: Scheduler overhead <1% of node execution time
//! - T089: Drift recording <1μs per sample
//! - T090: Health score calculation <10μs
//! - T091: Prometheus export <10ms for 100 streams

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use remotemedia_runtime_core::executor::drift_metrics::{DriftMetrics, DriftThresholds};
use remotemedia_runtime_core::executor::streaming_scheduler::StreamingScheduler;
use remotemedia_runtime_core::Error;
use std::time::Duration;

/// T088: Benchmark scheduler overhead
///
/// Measures the overhead of the scheduler wrapper compared to direct execution.
/// Success criteria: overhead <1% of node execution time
fn bench_scheduler_overhead(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("scheduler_overhead");
    group.measurement_time(Duration::from_secs(5));

    // Test with different simulated node execution times
    for node_duration_us in [100, 1000, 10000] {
        let scheduler = StreamingScheduler::with_defaults();

        // Baseline: direct execution without scheduler
        group.bench_with_input(
            BenchmarkId::new("direct", node_duration_us),
            &node_duration_us,
            |b, &duration_us| {
                b.to_async(&rt).iter(|| async {
                    // Simulate node work
                    tokio::time::sleep(Duration::from_micros(duration_us)).await;
                    black_box(42)
                });
            },
        );

        // With scheduler
        group.bench_with_input(
            BenchmarkId::new("with_scheduler", node_duration_us),
            &node_duration_us,
            |b, &duration_us| {
                b.to_async(&rt).iter(|| async {
                    scheduler
                        .execute_streaming_node("bench_node", || async {
                            tokio::time::sleep(Duration::from_micros(duration_us)).await;
                            Ok::<_, Error>(42)
                        })
                        .await
                        .unwrap()
                        .result
                });
            },
        );
    }

    group.finish();
}

/// T089: Benchmark drift recording
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

/// T090: Benchmark health score calculation
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

/// T091: Benchmark Prometheus export
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

criterion_group!(
    benches,
    bench_scheduler_overhead,
    bench_drift_recording,
    bench_health_score,
    bench_prometheus_export,
    bench_debug_json,
    bench_scheduler_stats,
    bench_alert_hysteresis,
);
criterion_main!(benches);
