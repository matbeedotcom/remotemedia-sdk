//! Microbenchmarks for the RT bridge.
//!
//! These approximate the shape of a HAL callback: the benchmark
//! thread acts as the RT producer, pushing a single `RuntimeData::Audio`
//! per iteration and popping the processed result. A real HAL IO proc
//! fires at a fixed audio period (e.g., every 10ms @ 480 samples /
//! 48kHz). What we measure here is the bridge round-trip latency with
//! no syscall overhead — essentially `try_push + worker_yield +
//! try_pop` for an identity node.
//!
//! We are *not* trying to measure node work here; a dedicated `Gain`
//! benchmark shows the per-sample cost added by a realistic DSP node.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use remotemedia_core::data::{AudioSamples, RuntimeData};
use remotemedia_core::nodes::SyncStreamingNode;
use remotemedia_core::Error;
use remotemedia_rt_bridge::{RtBridge, RtBridgeConfig};
use std::time::{Duration, Instant};

struct PassthroughNode;
impl SyncStreamingNode for PassthroughNode {
    fn node_type(&self) -> &str {
        "Passthrough"
    }
    fn process(&self, d: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(d)
    }
}

struct GainNode;
impl SyncStreamingNode for GainNode {
    fn node_type(&self) -> &str {
        "Gain"
    }
    fn process(&self, d: RuntimeData) -> Result<RuntimeData, Error> {
        match d {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                stream_id,
                timestamp_us,
                arrival_ts_us,
                metadata,
            } => {
                // Simulate a per-sample DSP op.
                let scaled: Vec<f32> = samples.iter().map(|s| s * 0.75).collect();
                Ok(RuntimeData::Audio {
                    samples: scaled.into(),
                    sample_rate,
                    channels,
                    stream_id,
                    timestamp_us,
                    arrival_ts_us,
                    metadata,
                })
            }
            other => Ok(other),
        }
    }
}

fn mk_audio(n: usize) -> RuntimeData {
    RuntimeData::Audio {
        samples: AudioSamples::from(vec![0.5_f32; n]),
        sample_rate: 48000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
        metadata: None,
    }
}

fn bench_round_trip_passthrough(c: &mut Criterion) {
    let (_bridge, mut producer, mut consumer) =
        RtBridge::spawn(PassthroughNode, RtBridgeConfig::default()).unwrap();

    c.bench_function("rt_bridge/roundtrip/passthrough/480samples", |b| {
        b.iter_batched(
            || mk_audio(480),
            |audio| {
                producer.try_push(black_box(audio)).unwrap();
                // Spin-wait for the worker to produce the echoed packet.
                // In steady state this is one `yield_now + pop`.
                loop {
                    if let Some(o) = consumer.try_pop() {
                        black_box(o);
                        break;
                    }
                    std::hint::spin_loop();
                }
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_round_trip_gain(c: &mut Criterion) {
    let (_bridge, mut producer, mut consumer) =
        RtBridge::spawn(GainNode, RtBridgeConfig::default()).unwrap();

    c.bench_function("rt_bridge/roundtrip/gain/480samples", |b| {
        b.iter_batched(
            || mk_audio(480),
            |audio| {
                producer.try_push(black_box(audio)).unwrap();
                loop {
                    if let Some(o) = consumer.try_pop() {
                        black_box(o);
                        break;
                    }
                    std::hint::spin_loop();
                }
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_try_push_only(c: &mut Criterion) {
    let (_bridge, mut producer, _consumer) =
        RtBridge::spawn(PassthroughNode, RtBridgeConfig::default()).unwrap();

    c.bench_function("rt_bridge/try_push/480samples", |b| {
        b.iter_batched(
            || mk_audio(480),
            |audio| {
                // Measure just the push side. Some of these will return
                // Full once the worker falls behind; criterion loops run
                // the closure thousands of times in a burst. That's OK
                // for the pure-push-cost measurement.
                let _ = producer.try_push(black_box(audio));
            },
            BatchSize::SmallInput,
        );
        // Let the worker drain.
        std::thread::sleep(Duration::from_millis(10));
    });
}

/// Approximate the audio-period steady-state: 480-sample packets at
/// 48kHz = 10ms period. We post one packet, sleep for 10ms (outside
/// measurement), consume it, repeat. This measures round-trip latency
/// in isolation from "can the worker keep up?"
fn bench_audio_period_simulation(c: &mut Criterion) {
    let (_bridge, mut producer, mut consumer) =
        RtBridge::spawn(GainNode, RtBridgeConfig::default()).unwrap();

    c.bench_function("rt_bridge/audio_period_simulation/10ms_cadence", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let audio = mk_audio(480);
                let t0 = Instant::now();
                producer.try_push(audio).unwrap();
                loop {
                    if consumer.try_pop().is_some() {
                        break;
                    }
                    std::hint::spin_loop();
                }
                total += t0.elapsed();
                // Pretend we just finished a HAL callback. The next
                // one fires 10ms later. This keeps the worker idle
                // between packets, matching steady state.
                std::thread::sleep(Duration::from_millis(10));
            }
            total
        });
    });
}

criterion_group!(
    benches,
    bench_round_trip_passthrough,
    bench_round_trip_gain,
    bench_try_push_only,
    bench_audio_period_simulation,
);
criterion_main!(benches);
