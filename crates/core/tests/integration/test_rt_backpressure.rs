//! Integration tests for the Phase 1 real-time backpressure work.
//!
//! Covers:
//! - Bounded `tokio::sync::mpsc` block-producer semantics at the
//!   channel level (baseline assumption tests — if these regress, all
//!   the downstream changes regress with them).
//! - `SessionHandle::send_input` back-pressures the caller when the
//!   router is not draining (verified via `PipelineExecutor::create_session`
//!   with an empty manifest and a slow consumer).
//! - `LatencyProbe` / `RtProbeSet` end-to-end probe wiring (records,
//!   snapshots, resets).
//! - `AudioBufferPool` under realistic producer/consumer contention.

use remotemedia_core::data::{AudioBufferPool, RuntimeData};
use remotemedia_core::metrics::{LatencyProbe, RtProbeSet};
use remotemedia_core::transport::{DEFAULT_ROUTER_INPUT_CAPACITY, DEFAULT_ROUTER_OUTPUT_CAPACITY};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

// -----------------------------------------------------------------------
// Baseline: bounded channel block-producer semantics
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_channel_blocks_producer_when_full() {
    // Capacity 2 — send 2 fills it, 3rd should block until receiver drains.
    let (tx, mut rx) = mpsc::channel::<u32>(2);
    tx.send(1).await.unwrap();
    tx.send(2).await.unwrap();

    // Spawn a task that will block trying to send the 3rd.
    let tx_clone = tx.clone();
    let blocker = tokio::spawn(async move {
        let start = Instant::now();
        tx_clone.send(3).await.unwrap();
        start.elapsed()
    });

    // Give the blocker a moment to actually hit the await point.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!blocker.is_finished(), "sender should be parked");

    // Drain one slot → blocker should unblock.
    assert_eq!(rx.recv().await, Some(1));

    let waited = tokio::time::timeout(Duration::from_secs(1), blocker)
        .await
        .expect("blocker did not unblock after drain")
        .unwrap();
    // It waited at least the sleep above; upper bound sanity only.
    assert!(waited >= Duration::from_millis(40));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_channel_try_send_errors_when_full() {
    let (tx, _rx) = mpsc::channel::<u32>(1);
    tx.try_send(1).expect("first try_send should succeed");
    match tx.try_send(2) {
        Err(mpsc::error::TrySendError::Full(2)) => {}
        other => panic!("expected TrySendError::Full(2), got {:?}", other),
    }
}

// -----------------------------------------------------------------------
// SessionRouter ingress backpressure
// -----------------------------------------------------------------------

/// Verifies that `SessionRouter::get_input_sender` returns a bounded
/// `mpsc::Sender<DataPacket>` with:
///   (a) the documented default capacity when no env override is set, and
///   (b) the env-override capacity when `REMOTEMEDIA_ROUTER_INPUT_CAPACITY`
///       is set.
///
/// Combined into one test because `std::env::set_var` is process-global;
/// splitting into two parallel test functions races on the env var.
///
/// This catches regressions that would accidentally re-introduce an
/// unbounded channel on the router's ingress.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_router_input_sender_is_bounded() {
    use remotemedia_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
    use remotemedia_core::nodes::StreamingNodeRegistry;
    use remotemedia_core::transport::{DataPacket, SessionRouter};

    fn make_manifest(name: &str) -> Manifest {
        Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: name.to_string(),
                ..Default::default()
            },
            nodes: vec![NodeManifest {
                id: "a".to_string(),
                node_type: "TestNode".to_string(),
                params: serde_json::json!({}),
                ..Default::default()
            }],
            connections: Vec::<Connection>::new(),
            python_env: None,
        }
    }

    fn make_packet(session: &str, seq: u64, label: &str) -> DataPacket {
        DataPacket {
            data: RuntimeData::Text(label.to_string()),
            from_node: "client".to_string(),
            to_node: None,
            session_id: session.to_string(),
            sequence: seq,
            sub_sequence: 0,
        }
    }

    // Phase 1: default capacity (no env var).
    std::env::remove_var("REMOTEMEDIA_ROUTER_INPUT_CAPACITY");
    {
        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) =
            mpsc::channel::<RuntimeData>(DEFAULT_ROUTER_OUTPUT_CAPACITY);
        let (router, _shutdown_tx) = SessionRouter::new(
            "s1".to_string(),
            Arc::new(make_manifest("test-default")),
            registry,
            output_tx,
        )
        .expect("create router");
        let tx = router.get_input_sender();
        for seq in 0..DEFAULT_ROUTER_INPUT_CAPACITY {
            tx.try_send(make_packet("s1", seq as u64, "p"))
                .expect("send within default capacity must succeed");
        }
        match tx.try_send(make_packet("s1", 999, "overflow")) {
            Err(mpsc::error::TrySendError::Full(_)) => {}
            other => panic!("default: expected Full, got {:?}", other),
        }
    }

    // Phase 2: override via env var. Set → construct → unset immediately
    // to minimize the race window with any other tests in this binary.
    const OVERRIDE: usize = 3;
    std::env::set_var("REMOTEMEDIA_ROUTER_INPUT_CAPACITY", OVERRIDE.to_string());
    let router_env = {
        let registry = Arc::new(StreamingNodeRegistry::new());
        let (output_tx, _output_rx) =
            mpsc::channel::<RuntimeData>(DEFAULT_ROUTER_OUTPUT_CAPACITY);
        let r = SessionRouter::new(
            "s-env".to_string(),
            Arc::new(make_manifest("test-env")),
            registry,
            output_tx,
        )
        .expect("create router");
        std::env::remove_var("REMOTEMEDIA_ROUTER_INPUT_CAPACITY");
        r
    };
    let (router_env, _shutdown_tx) = router_env;
    let tx = router_env.get_input_sender();
    for seq in 0..OVERRIDE {
        tx.try_send(make_packet("s-env", seq as u64, "p"))
            .expect("within override capacity");
    }
    assert!(
        matches!(
            tx.try_send(make_packet("s-env", 999, "overflow")),
            Err(mpsc::error::TrySendError::Full(_))
        ),
        "env override must bound the channel",
    );
}

// -----------------------------------------------------------------------
// LatencyProbe wiring
// -----------------------------------------------------------------------

#[test]
fn latency_probe_records_and_reports_percentiles() {
    let p = LatencyProbe::new("test-probe");
    // Synthetic distribution: 1000× 10μs, 10× 10 ms (tail).
    for _ in 0..1000 {
        p.record_ns(10_000);
    }
    for _ in 0..10 {
        p.record_ns(10_000_000);
    }
    let snap = p.snapshot();
    assert_eq!(snap.count, 1010);
    // p50 is well under the tail
    assert!(snap.p50_ns < 1_000_000, "p50 too high: {}", snap.p50_ns);
    // p9999 should clearly fall into the tail bucket
    assert!(
        snap.p9999_ns >= 5_000_000,
        "p9999 didn't reach tail: {}",
        snap.p9999_ns
    );
}

#[test]
fn rt_probe_set_snapshot_round_trip() {
    let set = RtProbeSet::new();
    set.ingress.record_ns(1_000);
    set.node_in.record_ns(50_000);
    set.egress.record_ns(2_000);

    let snaps = set.snapshot_all();
    let by_label: std::collections::HashMap<_, _> = snaps.iter().copied().collect();
    assert_eq!(by_label["ingress"].count, 1);
    assert_eq!(by_label["node_in"].count, 1);
    assert_eq!(by_label["egress"].count, 1);
    assert_eq!(by_label["route_in"].count, 0);
    assert_eq!(by_label["node_out"].count, 0);
}

// -----------------------------------------------------------------------
// AudioBufferPool under contention
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn audio_buffer_pool_reuses_buffers_under_contention() {
    const FRAME_SAMPLES: usize = 960; // 48 kHz / 20 ms / mono
    const ITERATIONS: usize = 2_000;
    const PRODUCERS: usize = 4;

    let pool = Arc::new(AudioBufferPool::new(32, FRAME_SAMPLES));

    // Each producer acquires, fills, drops — simulating per-frame churn.
    let mut handles = Vec::with_capacity(PRODUCERS);
    for _ in 0..PRODUCERS {
        let pool = Arc::clone(&pool);
        handles.push(tokio::task::spawn_blocking(move || {
            for _ in 0..ITERATIONS {
                let mut b = pool.acquire();
                b.resize(FRAME_SAMPLES, 0.25);
                // Write pattern to force a touch of the entire buffer.
                for x in b.iter_mut() {
                    *x *= 0.5;
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    // After contention, pool should have reabsorbed at least one
    // buffer — weak lower bound because exact depth is timing-dependent.
    // The real proof of reuse is in the unit test
    // `pool_reuses_capacity_not_contents`; this integration test checks
    // the pool is thread-safe under producer/consumer churn.
    assert!(
        !pool.is_empty(),
        "pool should have reabsorbed at least one buffer (len={})",
        pool.len()
    );
}

#[test]
fn audio_buffer_pool_into_inner_detaches_cleanly() {
    let pool = Arc::new(AudioBufferPool::new(4, 960));
    let mut buf = pool.acquire();
    buf.resize(960, 1.0);
    let v = buf.into_inner();
    assert_eq!(v.len(), 960);
    assert!(v.iter().all(|&x| x == 1.0));
    // Detached buffer did NOT return to pool.
    assert!(pool.is_empty());
}
