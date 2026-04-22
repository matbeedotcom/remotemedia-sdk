//! Criterion benchmarks for Phase 0/1 of the RT migration.
//!
//! Measures:
//! - Per-call overhead of [`LatencyProbe::record_ns`] (expected: tens of ns).
//! - Per-call overhead of [`RtProbeSet::snapshot_all`] (expected: μs-range).
//! - Cost of a bounded vs unbounded `tokio::sync::mpsc` send/recv round-trip
//!   on a single-threaded runtime — the key micro-cost we're paying in
//!   exchange for backpressure.
//! - [`AudioBufferPool`] acquire + fill + drop cycle on the hot path
//!   (pool-hit), compared against a straight `Vec::with_capacity` + drop
//!   (pool-miss / baseline).
//!
//! Run with:
//!   cargo bench -p remotemedia-core --bench bench_rt_backpressure
//!
//! These are micro-benchmarks; their absolute values are less important
//! than catching regressions across future commits. Criterion persists
//! baselines — after the first run, subsequent runs highlight shifts.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use remotemedia_core::data::AudioBufferPool;
use remotemedia_core::metrics::{LatencyProbe, RtProbeSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

// -----------------------------------------------------------------------
// LatencyProbe
// -----------------------------------------------------------------------

fn bench_probe_record_ns(c: &mut Criterion) {
    let p = LatencyProbe::new("bench");
    c.bench_function("probe_record_ns", |b| {
        b.iter(|| {
            p.record_ns(black_box(1_234));
        });
    });
}

fn bench_probe_record_since(c: &mut Criterion) {
    let p = LatencyProbe::new("bench");
    c.bench_function("probe_record_since", |b| {
        b.iter(|| {
            let t = Instant::now();
            p.record_since(black_box(t));
        });
    });
}

fn bench_probe_snapshot(c: &mut Criterion) {
    let p = LatencyProbe::new("bench");
    // Populate histogram with a realistic spread.
    for ns in (0..10_000).map(|i| 1_000 + i * 10) {
        p.record_ns(ns as u64);
    }
    c.bench_function("probe_snapshot", |b| {
        b.iter(|| {
            let s = p.snapshot();
            black_box(s);
        });
    });
}

fn bench_rt_probe_set_snapshot_all(c: &mut Criterion) {
    let set = RtProbeSet::new();
    // Touch each probe so snapshot has non-trivial state.
    for _ in 0..10_000 {
        set.ingress.record_ns(1_000);
        set.route_in.record_ns(500);
        set.node_in.record_ns(800);
        set.node_out.record_ns(50_000);
        set.egress.record_ns(900);
    }
    c.bench_function("rt_probe_set_snapshot_all", |b| {
        b.iter(|| {
            let s = set.snapshot_all();
            black_box(s);
        });
    });
}

// -----------------------------------------------------------------------
// Bounded vs unbounded channel round-trip
// -----------------------------------------------------------------------

fn bench_channel_bounded_round_trip(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("channel_bounded_round_trip_cap8", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = mpsc::channel::<u64>(8);
                tx.send(black_box(42)).await.unwrap();
                let v = rx.recv().await.unwrap();
                black_box(v)
            })
        });
    });
}

fn bench_channel_unbounded_round_trip(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("channel_unbounded_round_trip", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = mpsc::unbounded_channel::<u64>();
                tx.send(black_box(42)).unwrap();
                let v = rx.recv().await.unwrap();
                black_box(v)
            })
        });
    });
}

fn bench_channel_bounded_batch_8(c: &mut Criterion) {
    // Send a full batch up to capacity, then drain — more realistic
    // than round-trip for audio-frame cadence.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("channel_bounded_batch_8", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = mpsc::channel::<u64>(8);
                for i in 0..8u64 {
                    tx.send(i).await.unwrap();
                }
                for _ in 0..8 {
                    let v = rx.recv().await.unwrap();
                    black_box(v);
                }
            })
        });
    });
}

// -----------------------------------------------------------------------
// AudioBufferPool
// -----------------------------------------------------------------------

fn bench_pool_acquire_fill_drop(c: &mut Criterion) {
    const FRAME: usize = 960;
    let pool = Arc::new(AudioBufferPool::new(16, FRAME));
    // Warm up so the first bench iter is a pool hit.
    {
        let mut b = pool.acquire();
        b.resize(FRAME, 0.0);
    }

    c.bench_function("pool_acquire_fill_drop_frame960", |b| {
        b.iter(|| {
            let mut buf = pool.acquire();
            buf.resize(FRAME, black_box(0.25));
            black_box(buf.len());
        });
    });
}

fn bench_vec_alloc_fill_drop(c: &mut Criterion) {
    const FRAME: usize = 960;
    c.bench_function("vec_alloc_fill_drop_frame960", |b| {
        b.iter(|| {
            let mut v: Vec<f32> = Vec::with_capacity(FRAME);
            v.resize(FRAME, black_box(0.25));
            black_box(v.len());
        });
    });
}

criterion_group!(
    benches,
    bench_probe_record_ns,
    bench_probe_record_since,
    bench_probe_snapshot,
    bench_rt_probe_set_snapshot_all,
    bench_channel_bounded_round_trip,
    bench_channel_unbounded_round_trip,
    bench_channel_bounded_batch_8,
    bench_pool_acquire_fill_drop,
    bench_vec_alloc_fill_drop,
);
criterion_main!(benches);
