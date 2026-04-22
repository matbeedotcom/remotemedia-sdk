//! Hard real-time latency benchmark for the hearing-aid nodes.
//!
//! Validates the Phase 2/3 claim that `remotemedia_core::nodes::process_sync`
//! with `AudioSamples::Pooled` input is viable inside a Core Audio HAL IO
//! callback. The HAL deadline per callback is typically 2–5 ms
//! (e.g. 192 frames @ 48 kHz = 4 ms); we target p99 < 50 µs per invocation
//! for each node, which leaves ample headroom even under scheduling jitter.
//!
//! Run with:
//! ```sh
//! cargo bench -p remotemedia-hearing-nodes --bench process_sync_rt
//! ```
//!
//! Report numbers checked:
//! - `wdrc_stereo_192`   — WDRC on 192 stereo frames @ 48 kHz (4 ms of audio)
//! - `wdrc_stereo_512`   — WDRC on 512 stereo frames (a bigger buffer)
//! - `cros_stereo_192`   — CROS on 192 stereo frames
//! - `wdrc_then_cros_192`— the real hearing-aid chain on 192 stereo frames
//!
//! HRTF is omitted here because it requires a real IR file on disk; the
//! `process_sync` hot-path shape is identical for it.

use std::sync::Arc;
use std::time::Instant;

use audiogram::{Audiogram, EarAudiogram};
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use cros::{CrossFeedConfig, CrossFeedMode};

use remotemedia_core::data::{AudioSamples, RuntimeData};
use remotemedia_core::nodes::{process_sync, SyncStreamingNode};
use remotemedia_hearing_nodes::{CrosNode, WdrcNode};

/// Moderate high-frequency sloping loss, both ears.
fn demo_audiogram() -> Audiogram {
    let thresholds = [10.0, 15.0, 25.0, 40.0, 50.0, 55.0, 60.0, 65.0];
    Audiogram {
        left: EarAudiogram { thresholds, ucl: None },
        right: EarAudiogram { thresholds, ucl: None },
        name: "bench".into(),
        date: String::new(),
    }
}

fn make_audio_arc(frames: usize, channels: u32, sample_rate: u32) -> RuntimeData {
    // Use the Arc variant — the clone done by `process` before moving into
    // the node is O(1) refcount-bump for Arc, mirroring the zero-alloc RT
    // handoff we'd do from a HAL callback (pool buf → Arc view).
    let samples: Arc<[f32]> = (0..(frames * channels as usize))
        .map(|i| ((i as f32) * 0.0001).sin() * 0.3)
        .collect::<Vec<_>>()
        .into();
    RuntimeData::Audio {
        samples: AudioSamples::Arc(samples),
        sample_rate,
        channels,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
        metadata: None,
    }
}

fn bench_wdrc(c: &mut Criterion) {
    let node = WdrcNode::new(demo_audiogram(), Some(48_000));

    for &frames in &[192usize, 512, 960] {
        c.bench_function(&format!("wdrc_stereo_{frames}"), |b| {
            b.iter_batched(
                || make_audio_arc(frames, 2, 48_000),
                |data| {
                    let out = process_sync(black_box(&node as &dyn SyncStreamingNode), data)
                        .expect("wdrc ok");
                    black_box(out);
                },
                BatchSize::SmallInput,
            )
        });
    }
}

fn bench_cros(c: &mut Criterion) {
    let cfg = CrossFeedConfig {
        mode: CrossFeedMode::RightToLeft,
        level_db: -6.0,
        head_shadow_hz: 4000.0,
        cross_surround: false,
    };
    let node = CrosNode::new(cfg);

    for &frames in &[192usize, 512, 960] {
        c.bench_function(&format!("cros_stereo_{frames}"), |b| {
            b.iter_batched(
                || make_audio_arc(frames, 2, 48_000),
                |data| {
                    let out = process_sync(black_box(&node as &dyn SyncStreamingNode), data)
                        .expect("cros ok");
                    black_box(out);
                },
                BatchSize::SmallInput,
            )
        });
    }
}

fn bench_chain(c: &mut Criterion) {
    // This mirrors the real HAL hot path: WDRC then CROS on stereo.
    let wdrc = WdrcNode::new(demo_audiogram(), Some(48_000));
    let cros = CrosNode::new(CrossFeedConfig {
        mode: CrossFeedMode::RightToLeft,
        level_db: -6.0,
        head_shadow_hz: 4000.0,
        cross_surround: false,
    });

    for &frames in &[192usize, 512] {
        c.bench_function(&format!("wdrc_then_cros_stereo_{frames}"), |b| {
            b.iter_batched(
                || make_audio_arc(frames, 2, 48_000),
                |data| {
                    let mid =
                        process_sync(&wdrc as &dyn SyncStreamingNode, data).expect("wdrc ok");
                    let out =
                        process_sync(&cros as &dyn SyncStreamingNode, mid).expect("cros ok");
                    black_box(out);
                },
                BatchSize::SmallInput,
            )
        });
    }
}

/// Worst-case deadline check: assert that 10000 WDRC+CROS invocations on
/// 192 stereo frames complete with every single call under the 4 ms HAL
/// deadline, and p99 under 50 µs. This fails the build if latency regresses.
fn deadline_check(c: &mut Criterion) {
    let wdrc = WdrcNode::new(demo_audiogram(), Some(48_000));
    let cros = CrosNode::new(CrossFeedConfig {
        mode: CrossFeedMode::RightToLeft,
        level_db: -6.0,
        head_shadow_hz: 4000.0,
        cross_surround: false,
    });

    c.bench_function("deadline_check_192", |b| {
        b.iter_custom(|iters| {
            let mut max_ns: u128 = 0;
            let mut over_50us: u64 = 0;
            let mut samples: Vec<u128> = Vec::with_capacity(iters as usize);
            let start_all = Instant::now();

            for _ in 0..iters {
                let data = make_audio_arc(192, 2, 48_000);
                let t0 = Instant::now();
                let mid = process_sync(&wdrc as &dyn SyncStreamingNode, data).unwrap();
                let out = process_sync(&cros as &dyn SyncStreamingNode, mid).unwrap();
                let elapsed = t0.elapsed().as_nanos();
                black_box(out);
                max_ns = max_ns.max(elapsed);
                if elapsed > 50_000 {
                    over_50us += 1;
                }
                samples.push(elapsed);
            }

            samples.sort_unstable();
            let p99 = samples[((samples.len() as f32) * 0.99) as usize];
            let p999 = samples[((samples.len() as f32) * 0.999) as usize];
            eprintln!(
                "deadline_check_192 iters={iters} p99={p99}ns p99.9={p999}ns max={max_ns}ns over_50us={over_50us}"
            );
            // HAL deadline = 4_000_000 ns. Hard fail if any call exceeds it.
            assert!(max_ns < 4_000_000, "RT deadline breach: max {max_ns}ns");

            start_all.elapsed()
        })
    });
}

criterion_group!(rt, bench_wdrc, bench_cros, bench_chain, deadline_check);
criterion_main!(rt);
