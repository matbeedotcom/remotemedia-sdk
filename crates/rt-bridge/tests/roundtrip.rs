//! Integration tests for the RT bridge.
//!
//! These exercise the public API end-to-end without actually enabling
//! an RT priority thread (the `realtime` feature is off by default
//! and these tests are expected to run in unprivileged CI).

use remotemedia_core::data::{AudioSamples, RuntimeData};
use remotemedia_core::nodes::SyncStreamingNode;
use remotemedia_core::Error;
use remotemedia_rt_bridge::{RtBridge, RtBridgeConfig, TryPushError};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Identity node: returns the input unchanged. Ideal for measuring
/// bridge overhead in isolation from node work.
struct PassthroughNode {
    calls: Arc<AtomicU32>,
}

impl SyncStreamingNode for PassthroughNode {
    fn node_type(&self) -> &str {
        "Passthrough"
    }
    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(data)
    }
}

/// Gain node: scales audio samples by a fixed factor. Exercises the
/// common "read-every-sample, write-every-sample" pattern.
struct GainNode {
    gain: f32,
}

impl SyncStreamingNode for GainNode {
    fn node_type(&self) -> &str {
        "Gain"
    }
    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        match data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                stream_id,
                timestamp_us,
                arrival_ts_us,
                metadata,
            } => {
                let scaled: Vec<f32> =
                    samples.iter().map(|s| s * self.gain).collect();
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

    fn process_multi(
        &self,
        _inputs: HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
        Err(Error::Execution("not multi-input".into()))
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

/// Wait until the bridge has produced at least `n` outputs or `timeout`
/// elapses. Returns the actual count observed.
fn wait_for_outputs(
    consumer: &mut remotemedia_rt_bridge::RtOutputConsumer,
    sink: &mut Vec<RuntimeData>,
    want: usize,
    timeout: Duration,
) -> usize {
    let deadline = Instant::now() + timeout;
    while sink.len() < want && Instant::now() < deadline {
        while let Some(o) = consumer.try_pop() {
            sink.push(o);
        }
        if sink.len() < want {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    sink.len()
}

#[test]
fn passthrough_roundtrip() {
    let calls = Arc::new(AtomicU32::new(0));
    let node = PassthroughNode {
        calls: Arc::clone(&calls),
    };
    let (bridge, mut producer, mut consumer) =
        RtBridge::spawn(node, RtBridgeConfig::default()).expect("spawn");

    for i in 0..8 {
        producer.try_push(mk_audio(128 + i)).expect("push");
    }

    let mut outputs = Vec::new();
    let n = wait_for_outputs(&mut consumer, &mut outputs, 8, Duration::from_secs(2));
    assert_eq!(n, 8, "expected 8 passthrough outputs, got {n}");
    assert_eq!(calls.load(Ordering::Relaxed), 8);

    let stats = bridge.stats();
    assert_eq!(stats.processed, 8);
    assert_eq!(stats.process_errors, 0);
    assert_eq!(stats.output_overflows, 0);
}

#[test]
fn gain_node_modifies_samples() {
    let (bridge, mut producer, mut consumer) =
        RtBridge::spawn(GainNode { gain: 2.0 }, RtBridgeConfig::default())
            .expect("spawn");

    producer.try_push(mk_audio(64)).expect("push");

    let mut outputs = Vec::new();
    wait_for_outputs(&mut consumer, &mut outputs, 1, Duration::from_secs(2));

    let out = outputs.into_iter().next().expect("one output");
    match out {
        RuntimeData::Audio { samples, .. } => {
            // Input was 0.5 everywhere; gain 2.0 ⇒ 1.0 everywhere.
            assert!(samples.iter().all(|&s| (s - 1.0).abs() < 1e-6));
            assert_eq!(samples.len(), 64);
        }
        _ => panic!("expected Audio"),
    }
    drop(bridge);
}

#[test]
fn try_push_returns_full_when_ring_saturated() {
    // Tiny rings. Choke the worker by giving it real work per packet
    // and flood faster than it can drain.
    let config = RtBridgeConfig {
        input_capacity: 4,
        output_capacity: 4,
        thread_name: Some("rt-bridge-full-test".into()),
        #[cfg(feature = "realtime")]
        priority: None,
        #[cfg(feature = "realtime")]
        core_id: None,
    };
    // Slow node so the producer can overtake.
    struct SlowNode;
    impl SyncStreamingNode for SlowNode {
        fn node_type(&self) -> &str { "Slow" }
        fn process(&self, d: RuntimeData) -> Result<RuntimeData, Error> {
            std::thread::sleep(Duration::from_millis(20));
            Ok(d)
        }
    }
    let (_bridge, mut producer, _consumer) =
        RtBridge::spawn(SlowNode, config).expect("spawn");

    // Push more than capacity at once. At least one push should fail
    // with Full — the worker is sleeping on the first packet.
    let mut saw_full = false;
    for i in 0..32 {
        match producer.try_push(mk_audio(16 + i)) {
            Ok(()) => {}
            Err(TryPushError::Full(_)) => {
                saw_full = true;
                break;
            }
        }
    }
    assert!(saw_full, "expected ring to saturate under slow node");
}

#[test]
fn shutdown_joins_cleanly() {
    let (bridge, mut producer, mut consumer) =
        RtBridge::spawn(
            PassthroughNode { calls: Arc::new(AtomicU32::new(0)) },
            RtBridgeConfig::default(),
        )
        .expect("spawn");

    for _ in 0..4 {
        producer.try_push(mk_audio(64)).ok();
    }

    let mut outs = Vec::new();
    wait_for_outputs(&mut consumer, &mut outs, 4, Duration::from_secs(2));

    // Explicit shutdown; should not hang.
    bridge.shutdown();
}

#[test]
fn drop_bridge_shuts_down_worker() {
    // If Drop fails to join, the test will hang on the handle's drop.
    // A timeout on the worker should ensure this finishes promptly.
    let (bridge, _producer, _consumer) = RtBridge::spawn(
        PassthroughNode { calls: Arc::new(AtomicU32::new(0)) },
        RtBridgeConfig::default(),
    )
    .expect("spawn");
    drop(bridge);
}

#[test]
fn stats_reflect_activity() {
    let (bridge, mut producer, mut consumer) = RtBridge::spawn(
        PassthroughNode { calls: Arc::new(AtomicU32::new(0)) },
        RtBridgeConfig::default(),
    )
    .expect("spawn");
    for _ in 0..10 {
        producer.try_push(mk_audio(32)).ok();
    }
    let mut out = Vec::new();
    wait_for_outputs(&mut consumer, &mut out, 10, Duration::from_secs(2));
    let s = bridge.stats();
    assert_eq!(s.processed, 10);
    assert_eq!(s.process_errors, 0);
}

#[test]
fn handles_capacity_reports_ring_size() {
    let (_bridge, producer, consumer) = RtBridge::spawn(
        PassthroughNode { calls: Arc::new(AtomicU32::new(0)) },
        RtBridgeConfig {
            input_capacity: 16,
            output_capacity: 32,
            thread_name: None,
            #[cfg(feature = "realtime")]
            priority: None,
            #[cfg(feature = "realtime")]
            core_id: None,
        },
    )
    .expect("spawn");

    // rtrb may round capacity up internally; the handle should
    // report at least the requested size.
    assert!(producer.capacity() >= 16);
    assert!(consumer.capacity() >= 32);
}
