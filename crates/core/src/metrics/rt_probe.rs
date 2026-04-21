//! Per-session latency probes for the audio/video data plane.
//!
//! `LatencyProbe` is a lock-protected `hdrhistogram` recording nanosecond
//! durations. Contention is expected to be low — one probe point is hit
//! at most a few times per frame, and the critical section is a single
//! integer insert.
//!
//! `RtProbeSet` groups the five canonical probes we want to watch as the
//! pipeline migrates off tokio:
//!
//! ```text
//!   client ─┐
//!           │  ingress      (transport → router input channel)
//!           ▼
//!     SessionRouter
//!           │  route_in     (router dequeue → node dispatch)
//!           ▼
//!         Node
//!           │  node_in      (node dispatch → node process start)
//!           │  node_out     (node process start → node process end)
//!           ▼
//!     SessionRouter
//!           │  egress       (node output → client output channel)
//!           ▼
//!         client
//! ```
//!
//! For Phase 0 we only wire `ingress` and `egress` at the two ends of
//! the router. The others slot in as the router is decomposed.

use hdrhistogram::Histogram;
use std::sync::Mutex;
use std::time::Instant;

/// Snapshot of a probe's distribution at a point in time.
///
/// All durations are in nanoseconds. `count` is the number of samples
/// observed since the probe was created (or last reset).
#[derive(Debug, Clone, Copy)]
pub struct ProbeSnapshot {
    /// Total number of samples recorded.
    pub count: u64,
    /// 50th percentile latency (nanoseconds).
    pub p50_ns: u64,
    /// 99th percentile latency (nanoseconds).
    pub p99_ns: u64,
    /// 99.9th percentile latency (nanoseconds).
    pub p999_ns: u64,
    /// 99.99th percentile latency (nanoseconds).
    pub p9999_ns: u64,
    /// Maximum recorded latency (nanoseconds).
    pub max_ns: u64,
}

/// A single-point latency probe backed by an HDR histogram.
///
/// Safe to share across threads via `Arc`. Recording takes a
/// `std::sync::Mutex` — fine for control-plane and node-boundary use,
/// but **do not** call this from inside a tight per-sample audio loop.
/// Per-frame granularity (1-10 kHz) is the intended recording rate.
pub struct LatencyProbe {
    hist: Mutex<Histogram<u64>>,
    label: &'static str,
}

impl LatencyProbe {
    /// Create a new probe with the given label.
    ///
    /// The histogram covers 1 ns .. 60 s, 3 significant digits
    /// (~2% relative error).
    pub fn new(label: &'static str) -> Self {
        Self {
            hist: Mutex::new(
                Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3)
                    .expect("valid histogram bounds"),
            ),
            label,
        }
    }

    /// Record a duration in nanoseconds. Values outside the tracked
    /// range are silently clamped by `hdrhistogram`.
    pub fn record_ns(&self, ns: u64) {
        if let Ok(mut h) = self.hist.lock() {
            let _ = h.record(ns.max(1));
        }
    }

    /// Record the elapsed time since `start` in nanoseconds.
    pub fn record_since(&self, start: Instant) {
        let ns = start.elapsed().as_nanos() as u64;
        self.record_ns(ns);
    }

    /// Take a snapshot of the current distribution.
    pub fn snapshot(&self) -> ProbeSnapshot {
        let h = self.hist.lock().expect("probe histogram lock");
        ProbeSnapshot {
            count: h.len(),
            p50_ns: h.value_at_quantile(0.50),
            p99_ns: h.value_at_quantile(0.99),
            p999_ns: h.value_at_quantile(0.999),
            p9999_ns: h.value_at_quantile(0.9999),
            max_ns: h.max(),
        }
    }

    /// Label for this probe (for display / export).
    pub fn label(&self) -> &'static str {
        self.label
    }

    /// Reset the histogram. Useful between test cases.
    pub fn reset(&self) {
        if let Ok(mut h) = self.hist.lock() {
            h.reset();
        }
    }
}

/// The five canonical probe points for a streaming session.
///
/// Only `ingress` and `egress` are wired in Phase 0. The others are
/// defined here so follow-up work doesn't have to edit this type.
pub struct RtProbeSet {
    /// Arrival at the session router (transport → router).
    pub ingress: LatencyProbe,
    /// Router dequeue → node dispatch.
    pub route_in: LatencyProbe,
    /// Node dispatch → node process entry.
    pub node_in: LatencyProbe,
    /// Node process entry → node process return.
    pub node_out: LatencyProbe,
    /// Node output → client output channel.
    pub egress: LatencyProbe,
}

impl RtProbeSet {
    /// Create a fresh probe set.
    pub fn new() -> Self {
        Self {
            ingress: LatencyProbe::new("ingress"),
            route_in: LatencyProbe::new("route_in"),
            node_in: LatencyProbe::new("node_in"),
            node_out: LatencyProbe::new("node_out"),
            egress: LatencyProbe::new("egress"),
        }
    }

    /// Snapshot every probe in declaration order.
    pub fn snapshot_all(&self) -> [(&'static str, ProbeSnapshot); 5] {
        [
            (self.ingress.label(), self.ingress.snapshot()),
            (self.route_in.label(), self.route_in.snapshot()),
            (self.node_in.label(), self.node_in.snapshot()),
            (self.node_out.label(), self.node_out.snapshot()),
            (self.egress.label(), self.egress.snapshot()),
        ]
    }
}

impl Default for RtProbeSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn probe_records_and_snapshots() {
        let p = LatencyProbe::new("test");
        for ns in [100, 200, 300, 400, 500] {
            p.record_ns(ns);
        }
        let snap = p.snapshot();
        assert_eq!(snap.count, 5);
        assert!(snap.p50_ns >= 200 && snap.p50_ns <= 400);
        assert!(snap.max_ns >= 500);
    }

    #[test]
    fn probe_record_since_measures_elapsed() {
        let p = LatencyProbe::new("test");
        let start = Instant::now();
        thread::sleep(Duration::from_micros(100));
        p.record_since(start);
        let snap = p.snapshot();
        assert_eq!(snap.count, 1);
        assert!(snap.max_ns >= 100_000);
    }

    #[test]
    fn probe_set_labels_are_stable() {
        let set = RtProbeSet::new();
        let snaps = set.snapshot_all();
        let labels: Vec<_> = snaps.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            vec!["ingress", "route_in", "node_in", "node_out", "egress"]
        );
    }

    #[test]
    fn probe_reset_clears_history() {
        let p = LatencyProbe::new("test");
        p.record_ns(1000);
        assert_eq!(p.snapshot().count, 1);
        p.reset();
        assert_eq!(p.snapshot().count, 0);
    }
}
