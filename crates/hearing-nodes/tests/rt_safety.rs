//! Integration tests that drive the hearing-aid DSP nodes through
//! [`remotemedia_rt_bridge::RtBridge`]. These are the closest we can
//! get to a CI-executable assertion that the nodes produce correct
//! output without panicking under the RT-path contract
//! (single-consumer Mutex, pre-sized scratch, in-place DSP).

use audiogram::{Audiogram, EarAudiogram};
use cros::{CrossFeedConfig, CrossFeedMode};
use remotemedia_core::data::{AudioSamples, RuntimeData};
use remotemedia_hearing_nodes::{CrosNode, WdrcNode};
use remotemedia_rt_bridge::{RtBridge, RtBridgeConfig, RtOutputConsumer};
use std::time::{Duration, Instant};

const SAMPLE_RATE: u32 = 48_000;

fn flat_audiogram() -> Audiogram {
    Audiogram {
        left: EarAudiogram {
            thresholds: [25.0; 8],
            ucl: None,
        },
        right: EarAudiogram {
            thresholds: [25.0; 8],
            ucl: None,
        },
        name: "rt-test".into(),
        date: String::new(),
    }
}

fn mk_audio(frames: usize, channels: u32) -> RuntimeData {
    RuntimeData::Audio {
        samples: AudioSamples::from(vec![0.25_f32; frames * channels as usize]),
        sample_rate: SAMPLE_RATE,
        channels,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
        metadata: None,
    }
}

fn wait_for(
    consumer: &mut RtOutputConsumer,
    n: usize,
    timeout: Duration,
) -> Vec<RuntimeData> {
    let deadline = Instant::now() + timeout;
    let mut sink = Vec::with_capacity(n);
    while sink.len() < n && Instant::now() < deadline {
        while let Some(o) = consumer.try_pop() {
            sink.push(o);
        }
        if sink.len() < n {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    sink
}

/// Drive `N` packets through the node via rt-bridge in a
/// push-then-drain loop that mirrors a HAL IO callback cadence: push
/// one, spin-wait for the corresponding output, collect. If the
/// steady-state RT contract is violated (node panics, allocates
/// unboundedly, or the bridge drops packets) this will either panic
/// or time out.
fn run_roundtrip<N>(node: N, packets: usize, frames: usize, channels: u32)
where
    N: remotemedia_core::nodes::SyncStreamingNode + 'static,
{
    let (bridge, mut producer, mut consumer) =
        RtBridge::spawn(node, RtBridgeConfig::default()).expect("spawn");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut sent = 0usize;
    let mut got = 0usize;
    while got < packets {
        // Try to push a fresh packet if we haven't fulfilled the quota
        // yet. If the ring is full, back off briefly — the worker is
        // catching up. This mirrors the `TryPushError::Full` path a
        // HAL callback would take (drop on overflow).
        if sent < packets {
            match producer.try_push(mk_audio(frames, channels)) {
                Ok(()) => sent += 1,
                Err(_) => {
                    // Ring saturated; drain first to free slots.
                }
            }
        }
        // Drain all currently-available outputs.
        while let Some(out) = consumer.try_pop() {
            match &out {
                RuntimeData::Audio {
                    samples,
                    channels: och,
                    sample_rate,
                    ..
                } => {
                    assert_eq!(*och, channels);
                    assert_eq!(*sample_rate, SAMPLE_RATE);
                    assert_eq!(samples.len(), frames * channels as usize);
                }
                other => panic!("expected Audio, got {:?}", other.data_type()),
            }
            got += 1;
        }
        if Instant::now() > deadline {
            panic!(
                "roundtrip deadline hit: sent {sent} got {got}/{packets}; bridge stats = {:?}",
                bridge.stats()
            );
        }
        // Tiny yield so the worker gets CPU instead of us busy-pushing.
        std::thread::yield_now();
    }
    // Drain any residual outputs still in flight.
    let _extra = wait_for(&mut consumer, 0, Duration::from_millis(50));

    let stats = bridge.stats();
    assert!(stats.processed >= packets as u64, "stats: {:?}", stats);
    assert_eq!(stats.process_errors, 0, "stats: {:?}", stats);
}

/// WDRC: 1000 stereo 10ms packets through rt-bridge.
#[test]
fn wdrc_handles_1000_packets_through_rt_bridge() {
    let node = WdrcNode::new(flat_audiogram(), Some(SAMPLE_RATE));
    run_roundtrip(node, 1000, 480, 2);
}

/// CROS: same shape.
#[test]
fn cros_handles_1000_packets_through_rt_bridge() {
    let cfg = CrossFeedConfig {
        mode: CrossFeedMode::RightToLeft,
        level_db: -6.0,
        head_shadow_hz: 4000.0,
        cross_surround: false,
    };
    let node = CrosNode::new(cfg);
    run_roundtrip(node, 1000, 480, 2);
}

/// Verify that input carried as `AudioSamples::Pooled` (zero-alloc
/// variant) round-trips through the bridge without the worker
/// producing errors and without any pool-recycling issues.
#[test]
fn wdrc_accepts_pooled_input() {
    use remotemedia_core::data::AudioBufferPool;
    use std::sync::Arc;

    let pool = Arc::new(AudioBufferPool::new(32, 480 * 2));
    let node = WdrcNode::new(flat_audiogram(), Some(SAMPLE_RATE));
    let (bridge, mut producer, mut consumer) =
        RtBridge::spawn(node, RtBridgeConfig::default()).expect("spawn");

    // Push-and-drain loop; emulates HAL callback cadence. Backs off
    // on a full input ring instead of panicking.
    let want = 16usize;
    let mut sent = 0usize;
    let mut got = 0usize;
    let deadline = Instant::now() + Duration::from_secs(10);
    while got < want {
        if sent < want {
            let mut buf = pool.acquire();
            buf.extend_from_slice(&[0.1_f32; 480 * 2]);
            let data = RuntimeData::Audio {
                samples: AudioSamples::from(buf),
                sample_rate: SAMPLE_RATE,
                channels: 2,
                stream_id: None,
                timestamp_us: None,
                arrival_ts_us: None,
                metadata: None,
            };
            if producer.try_push(data).is_ok() {
                sent += 1;
            }
        }
        while consumer.try_pop().is_some() {
            got += 1;
        }
        if Instant::now() > deadline {
            panic!("pooled round-trip deadline: sent {sent} got {got}/{want}");
        }
        std::thread::yield_now();
    }

    let stats = bridge.stats();
    assert!(stats.processed >= want as u64);
    assert_eq!(stats.process_errors, 0);
}

/// Mono input — WDRC should still run (using left-ear params).
#[test]
fn wdrc_handles_mono_input() {
    let node = WdrcNode::new(flat_audiogram(), Some(SAMPLE_RATE));
    let (_bridge, mut producer, mut consumer) =
        RtBridge::spawn(node, RtBridgeConfig::default()).expect("spawn");

    producer.try_push(mk_audio(480, 1)).expect("push mono");
    let outs = wait_for(&mut consumer, 1, Duration::from_secs(2));
    assert_eq!(outs.len(), 1);
    match &outs[0] {
        RuntimeData::Audio { channels, samples, .. } => {
            assert_eq!(*channels, 1);
            assert_eq!(samples.len(), 480);
        }
        _ => panic!("expected Audio"),
    }
}
