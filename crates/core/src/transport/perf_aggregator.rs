//! Per-session performance aggregator
//!
//! Records dispatch-site I/O timings into HDR histograms and emits a
//! single roll-up snapshot per session per window. The aggregator is
//! the runtime side of the `__perf__` tap channel — see
//! [`crate::data::perf`] for the JSON schema.
//!
//! ## Design
//!
//! - One [`PerfAggregator`] instance per session.
//! - Per `node_id`, two HDR histograms: total latency (every output)
//!   and first-output latency (only the first emission per input).
//! - `record(...)` is sub-microsecond — a single CAS for the
//!   "enabled" flag, a `parking_lot::Mutex::lock()` (uncontested
//!   single CAS) on the per-node slot, two `record_correct()` calls.
//!   No JSON, no allocation, no awaits. Safe to call from the
//!   dispatch hot path.
//! - `flush_snapshot()` builds a [`PerfSnapshot`] and **resets** the
//!   histograms in place (`reset()` keeps capacity). One snapshot
//!   covers exactly `window_ms` of activity. Frontend renders the
//!   latest; sparklines are built from a series.
//! - `enable_perf_tap` flag is set at construction. When `false`,
//!   `record()` returns immediately without acquiring any lock.
//!
//! ## Why not store every event
//!
//! At 50 fps audio + LLM token rate + TTS chunk rate, a busy session
//! emits ~200 events/s. Publishing every one as JSON on the tap
//! channel would dominate runtime cost and saturate the WebSocket.
//! The histogram approach keeps memory bounded (~3 KB per node) and
//! costs ~1 µs per record(). The frontend gets richer information
//! (percentiles, not just a stream of dots) for less work.

use crate::data::perf::{LatencyPercentiles, NodeStats, PerfEventKind, PerfSnapshot};
use hdrhistogram::Histogram;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

/// HDR histogram precision. 3 significant figures + 1 µs to 60 s
/// range covers everything from "fast Rust path" up to "LLM stalled".
/// Memory cost: ~3 KB per histogram.
const HDR_PRECISION: u8 = 3;
const HDR_MIN_US: u64 = 1;
const HDR_MAX_US: u64 = 60_000_000;

/// Per-node slot. One per (session, node_id).
struct NodeBucket {
    /// Inputs received in the current window.
    inputs: u64,
    /// Outputs emitted in the current window.
    outputs: u64,
    /// Latency from input arrival to *each* output (us).
    latency: Histogram<u64>,
    /// Latency from input arrival to the *first* output of that
    /// input (us). One sample per input that produced ≥1 output.
    first_output_latency: Histogram<u64>,
}

impl NodeBucket {
    fn new() -> Self {
        Self {
            inputs: 0,
            outputs: 0,
            latency: Histogram::new_with_bounds(HDR_MIN_US, HDR_MAX_US, HDR_PRECISION)
                .expect("HDR histogram bounds valid"),
            first_output_latency: Histogram::new_with_bounds(
                HDR_MIN_US,
                HDR_MAX_US,
                HDR_PRECISION,
            )
            .expect("HDR histogram bounds valid"),
        }
    }

    fn record_input(&mut self) {
        self.inputs = self.inputs.saturating_add(1);
    }

    fn record_output(&mut self, latency_us: u64, is_first: bool) {
        self.outputs = self.outputs.saturating_add(1);
        // `record_correct` clamps at the histogram's max, so a
        // pathological 60+ s value won't poison the snapshot.
        let _ = self.latency.record(latency_us.clamp(HDR_MIN_US, HDR_MAX_US));
        if is_first {
            let _ = self
                .first_output_latency
                .record(latency_us.clamp(HDR_MIN_US, HDR_MAX_US));
        }
    }

    /// Drain into a snapshot view and reset for the next window.
    fn drain_into(&mut self) -> NodeStats {
        let stats = NodeStats {
            inputs: self.inputs,
            outputs: self.outputs,
            latency_us: percentiles(&self.latency),
            first_output_latency_us: percentiles(&self.first_output_latency),
        };
        self.inputs = 0;
        self.outputs = 0;
        self.latency.reset();
        self.first_output_latency.reset();
        stats
    }
}

fn percentiles(h: &Histogram<u64>) -> LatencyPercentiles {
    if h.is_empty() {
        return LatencyPercentiles::default();
    }
    LatencyPercentiles {
        p50_us: h.value_at_quantile(0.50),
        p95_us: h.value_at_quantile(0.95),
        p99_us: h.value_at_quantile(0.99),
        max_us: h.max(),
    }
}

/// Per-session performance aggregator.
///
/// Construct one and share `Arc<PerfAggregator>` between the session
/// router (which calls [`Self::record_input`] / [`Self::record_output`]
/// from `spawn_node_pipeline`) and the periodic flush task.
pub struct PerfAggregator {
    session_id: String,
    enabled: AtomicBool,
    /// `node_id → NodeBucket`. Hot path locks a single slot, not the
    /// outer map (`DashMap` would need DashMap; `Mutex<HashMap>` is
    /// fine here because slot creation is rare and reads are short).
    buckets: Mutex<HashMap<String, NodeBucket>>,
    /// Window length used in emitted snapshots. Set once at
    /// construction; aggregator does not enforce — the flush task
    /// owns the timer.
    window_ms: u32,
}

impl PerfAggregator {
    pub fn new(session_id: String, enabled: bool, window_ms: u32) -> Self {
        Self {
            session_id,
            enabled: AtomicBool::new(enabled),
            buckets: Mutex::new(HashMap::new()),
            window_ms,
        }
    }

    /// Returns `true` if the aggregator is recording. Hot path uses
    /// this to skip the per-record lock entirely when disabled.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Enable/disable at runtime (e.g., from a control-bus toggle).
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    /// Record that `node_id` accepted an input. Cheap when disabled.
    #[inline]
    pub fn record_input(&self, node_id: &str) {
        if !self.is_enabled() {
            return;
        }
        let mut buckets = self.buckets.lock();
        buckets
            .entry(node_id.to_string())
            .or_insert_with(NodeBucket::new)
            .record_input();
    }

    /// Record that `node_id` emitted an output `latency_us`
    /// microseconds after its input arrived. `is_first` flags the
    /// first emission for that input (used for first-output
    /// percentiles).
    #[inline]
    pub fn record_output(&self, node_id: &str, latency_us: u64, is_first: bool) {
        if !self.is_enabled() {
            return;
        }
        let mut buckets = self.buckets.lock();
        buckets
            .entry(node_id.to_string())
            .or_insert_with(NodeBucket::new)
            .record_output(latency_us, is_first);
    }

    /// Drain all node buckets into a [`PerfSnapshot`] and reset
    /// histograms. Called by the periodic flush task; safe to call
    /// from any thread.
    pub fn flush_snapshot(&self) -> PerfSnapshot {
        let ts_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut buckets = self.buckets.lock();
        let nodes: HashMap<String, NodeStats> = buckets
            .iter_mut()
            .map(|(id, bucket)| (id.clone(), bucket.drain_into()))
            .collect();

        PerfSnapshot {
            kind: PerfEventKind::PerfSnapshot,
            session_id: self.session_id.clone(),
            ts_ms,
            window_ms: self.window_ms,
            nodes,
        }
    }

    /// Returns `true` if the latest window had any activity. Used
    /// by the flush task to skip a publish when nothing happened
    /// (silent steady state — no point spamming the tap).
    pub fn has_activity(&self) -> bool {
        let buckets = self.buckets.lock();
        buckets
            .values()
            .any(|b| b.inputs > 0 || b.outputs > 0)
    }
}

impl PerfAggregator {
    /// Read the perf-tap enable flag from the environment. Set
    /// `REMOTEMEDIA_PERF_TAP=1` to opt in. Off by default so
    /// production sessions pay zero overhead.
    pub fn enabled_from_env() -> bool {
        std::env::var("REMOTEMEDIA_PERF_TAP")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
            .unwrap_or(false)
    }

    /// Read the snapshot window length (ms) from the environment.
    /// Falls back to 1000 ms (1 Hz). Clamped to `[100, 10_000]`.
    pub fn window_ms_from_env() -> u32 {
        std::env::var("REMOTEMEDIA_PERF_WINDOW_MS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .map(|v| v.clamp(100, 10_000))
            .unwrap_or(1000)
    }
}

/// Spawn the periodic flush task. Returns a `JoinHandle` so the
/// session can await teardown. The task exits when the `shutdown`
/// `Notify` fires.
pub fn spawn_flush_task<P>(
    aggregator: Arc<PerfAggregator>,
    publish: P,
    shutdown: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()>
where
    P: Fn(PerfSnapshot) + Send + Sync + 'static,
{
    let window = std::time::Duration::from_millis(aggregator.window_ms as u64);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(window);
        // First tick fires immediately; skip it so the first
        // snapshot represents a real window of activity.
        ticker.tick().await;
        loop {
            tokio::select! {
                biased;
                _ = shutdown.notified() => break,
                _ = ticker.tick() => {
                    if !aggregator.is_enabled() || !aggregator.has_activity() {
                        continue;
                    }
                    let snapshot = aggregator.flush_snapshot();
                    publish(snapshot);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_aggregator_records_nothing() {
        let agg = PerfAggregator::new("s".into(), false, 1000);
        agg.record_input("n");
        agg.record_output("n", 1234, true);
        let snap = agg.flush_snapshot();
        assert!(snap.nodes.is_empty(), "disabled aggregator must skip slot creation");
    }

    #[test]
    fn enabled_aggregator_records_and_resets() {
        let agg = PerfAggregator::new("s".into(), true, 1000);
        agg.record_input("n1");
        agg.record_output("n1", 100, true);
        agg.record_output("n1", 200, false);
        agg.record_input("n2");
        agg.record_output("n2", 50, true);

        let snap = agg.flush_snapshot();
        let n1 = snap.nodes.get("n1").expect("n1 stats");
        assert_eq!(n1.inputs, 1);
        assert_eq!(n1.outputs, 2);
        assert!(n1.latency_us.p50_us > 0);
        assert!(n1.first_output_latency_us.p50_us > 0);

        let n2 = snap.nodes.get("n2").expect("n2 stats");
        assert_eq!(n2.inputs, 1);
        assert_eq!(n2.outputs, 1);

        // Second flush after no activity → empty stats but slot
        // remains.
        let snap2 = agg.flush_snapshot();
        let n1b = snap2.nodes.get("n1").expect("slot persists across flush");
        assert_eq!(n1b.inputs, 0);
        assert_eq!(n1b.outputs, 0);
        assert_eq!(n1b.latency_us.p50_us, 0);
    }

    #[test]
    fn first_output_latency_only_records_first() {
        let agg = PerfAggregator::new("s".into(), true, 1000);
        agg.record_input("n");
        agg.record_output("n", 100, true);
        agg.record_output("n", 999_000, false);
        let snap = agg.flush_snapshot();
        let stats = snap.nodes.get("n").expect("stats");
        // first_output histogram has exactly the 100 µs sample —
        // p99 of one sample = the sample itself.
        assert_eq!(stats.first_output_latency_us.max_us, 100);
        // Total latency histogram has both samples; max should be
        // the slow one (clamped + bucketed but ~999000).
        assert!(stats.latency_us.max_us > 100_000);
    }

    #[test]
    fn has_activity_reports_correctly() {
        let agg = PerfAggregator::new("s".into(), true, 1000);
        assert!(!agg.has_activity());
        agg.record_input("n");
        assert!(agg.has_activity());
        let _ = agg.flush_snapshot();
        assert!(!agg.has_activity(), "flush resets activity counters");
    }
}
