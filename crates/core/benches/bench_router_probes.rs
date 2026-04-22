//! End-to-end probe harness: drives a multi-node sync pipeline through
//! the `SessionRouter` and prints the probe distributions after each
//! criterion run. This is the quantitative counterpart to the
//! per-primitive benches in `bench_rt_backpressure.rs`.
//!
//! What it measures:
//! - `router/passthrough_chain/Nnodes` — round-trip latency of one
//!   `RuntimeData::Audio` packet through an N-node sync passthrough
//!   pipeline (router ingress → node dispatch × N → egress → client
//!   output channel).
//!
//! What it reports (post-run, via `eprintln!`):
//! - `ingress`, `node_out`, `egress` latency probes: p50 / p99 / p999
//!   / max / sample count.
//! - `spawn_count` (core router should stay at 0 — any non-zero means
//!   a regression that re-introduced a per-packet `tokio::spawn`).
//! - `loopback_depth` (gauge sample at the top of the router loop;
//!   stable "current" value, not a histogram).
//!
//! Use this to eyeball phase-over-phase wins:
//! ```sh
//! cargo bench -p remotemedia-core --bench bench_router_probes -- --save-baseline before
//! # ... apply phase change ...
//! cargo bench -p remotemedia-core --bench bench_router_probes -- --baseline before
//! ```
//!
//! Note: criterion's own numbers capture end-to-end round-trip time.
//! The probe snapshots capture the internal distribution of `node_out`
//! (scheduler dispatch latency) and `egress` (router → client) which
//! are the pieces the RT migration targets.

use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use serde_json::Value;
use tokio::sync::mpsc;

use remotemedia_core::data::{AudioSamples, RuntimeData};
use remotemedia_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use remotemedia_core::nodes::{
    StreamingNode, StreamingNodeFactory, StreamingNodeRegistry, SyncNodeWrapper, SyncStreamingNode,
};
use remotemedia_core::transport::{
    DataPacket, SessionRouter, DEFAULT_ROUTER_OUTPUT_CAPACITY,
};
use remotemedia_core::Error;

// ---------------------------------------------------------------------------
// Local sync passthrough node + factory
// ---------------------------------------------------------------------------

struct BenchPassthrough;

impl SyncStreamingNode for BenchPassthrough {
    fn node_type(&self) -> &str {
        "BenchPassthrough"
    }
    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

struct BenchPassthroughFactory;

impl StreamingNodeFactory for BenchPassthroughFactory {
    fn create(
        &self,
        _node_id: String,
        _params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        Ok(Box::new(SyncNodeWrapper(BenchPassthrough)))
    }
    fn node_type(&self) -> &str {
        "BenchPassthrough"
    }
}

// ---------------------------------------------------------------------------
// Pipeline + router construction
// ---------------------------------------------------------------------------

fn chain_manifest(n: usize) -> Manifest {
    assert!(n >= 1);
    let nodes = (0..n)
        .map(|i| NodeManifest {
            id: format!("n{i}"),
            node_type: "BenchPassthrough".to_string(),
            params: serde_json::json!({}),
            ..Default::default()
        })
        .collect();
    let connections = (0..n.saturating_sub(1))
        .map(|i| Connection {
            from: format!("n{i}"),
            to: format!("n{}", i + 1),
        })
        .collect();
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: format!("bench-passthrough-chain-{n}"),
            ..Default::default()
        },
        nodes,
        connections,
        python_env: None,
    }
}

fn audio_packet(session: &str, seq: u64, samples: usize) -> DataPacket {
    DataPacket {
        data: RuntimeData::Audio {
            samples: AudioSamples::from(vec![0.25_f32; samples]),
            sample_rate: 48_000,
            channels: 1,
            stream_id: Some(session.to_string()),
            timestamp_us: None,
            arrival_ts_us: None,
            metadata: None,
        },
        from_node: "client".to_string(),
        to_node: Some("n0".to_string()),
        session_id: session.to_string(),
        sequence: seq,
        sub_sequence: 0,
    }
}

struct RunningRouter {
    input_tx: mpsc::Sender<DataPacket>,
    output_rx: mpsc::Receiver<RuntimeData>,
    probes: Arc<remotemedia_core::metrics::RtProbeSet>,
    _shutdown_tx: mpsc::Sender<()>,
    _handle: tokio::task::JoinHandle<()>,
}

fn start_router(rt: &tokio::runtime::Runtime, n_nodes: usize) -> RunningRouter {
    rt.block_on(async move {
        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(BenchPassthroughFactory));

        let manifest = Arc::new(chain_manifest(n_nodes));
        let (output_tx, output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

        let (router, shutdown_tx) =
            SessionRouter::new("bench-session".to_string(), manifest, Arc::new(registry), output_tx)
                .expect("router construction");

        let input_tx = router.get_input_sender();
        let probes = router.probes();
        let handle = router.start();

        RunningRouter {
            input_tx,
            output_rx,
            probes,
            _shutdown_tx: shutdown_tx,
            _handle: handle,
        }
    })
}

// ---------------------------------------------------------------------------
// Probe reporting
// ---------------------------------------------------------------------------

fn print_probe_report(label: &str, probes: &remotemedia_core::metrics::RtProbeSet) {
    let snaps = probes.snapshot_all();
    let op = probes.operational_snapshot();
    eprintln!("\n=== probe report: {label} ===");
    eprintln!(
        "{:<10} {:>10} {:>10} {:>12} {:>12} {:>12}",
        "probe", "count", "p50 (ns)", "p99 (ns)", "p999 (ns)", "max (ns)"
    );
    for (label, snap) in snaps {
        eprintln!(
            "{:<10} {:>10} {:>10} {:>12} {:>12} {:>12}",
            label, snap.count, snap.p50_ns, snap.p99_ns, snap.p999_ns, snap.max_ns
        );
    }
    eprintln!(
        "operational: spawn_count={} loopback_depth={}",
        op.spawn_count, op.loopback_depth
    );
}

// ---------------------------------------------------------------------------
// Benches
// ---------------------------------------------------------------------------

fn bench_router_chain(c: &mut Criterion, n_nodes: usize) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut router = start_router(&rt, n_nodes);

    let bench_name = format!("router/passthrough_chain/{n_nodes}nodes");
    let mut seq: u64 = 0;
    const FRAME_SAMPLES: usize = 480; // 10 ms @ 48 kHz, mono

    c.bench_function(&bench_name, |b| {
        b.iter_batched(
            || {
                seq += 1;
                audio_packet("bench-session", seq, FRAME_SAMPLES)
            },
            |pkt| {
                rt.block_on(async {
                    router.input_tx.send(black_box(pkt)).await.expect("ingress");
                    let out = router.output_rx.recv().await.expect("egress");
                    black_box(out);
                });
            },
            BatchSize::SmallInput,
        );
    });

    print_probe_report(&bench_name, &router.probes);
}

fn bench_router_chain_1(c: &mut Criterion) {
    bench_router_chain(c, 1);
}
fn bench_router_chain_4(c: &mut Criterion) {
    bench_router_chain(c, 4);
}
fn bench_router_chain_12(c: &mut Criterion) {
    bench_router_chain(c, 12);
}

criterion_group!(
    benches,
    bench_router_chain_1,
    bench_router_chain_4,
    bench_router_chain_12,
);
criterion_main!(benches);
