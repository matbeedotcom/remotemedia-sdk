//! Metrics collection for the SRT Ingest Gateway
//!
//! Provides basic metrics for monitoring gateway health and performance.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Gateway metrics collector
#[derive(Default)]
pub struct Metrics {
    /// Total sessions created since startup
    sessions_created: AtomicU64,

    /// Total sessions ended since startup
    sessions_ended: AtomicU64,

    /// Total events emitted since startup
    events_emitted: AtomicU64,

    /// Total bytes received since startup
    bytes_received: AtomicU64,

    /// Total packets received since startup
    packets_received: AtomicU64,

    /// Total webhook deliveries attempted
    webhook_attempts: AtomicU64,

    /// Total successful webhook deliveries
    webhook_successes: AtomicU64,

    /// Total failed webhook deliveries
    webhook_failures: AtomicU64,

    /// Current active sessions count
    active_sessions: AtomicU64,

    /// Startup timestamp (unix seconds)
    startup_time: AtomicU64,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            startup_time: AtomicU64::new(now),
            ..Default::default()
        }
    }

    /// Record a session creation
    pub fn session_created(&self) {
        self.sessions_created.fetch_add(1, Ordering::Relaxed);
        self.active_sessions.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a session ending
    pub fn session_ended(&self) {
        self.sessions_ended.fetch_add(1, Ordering::Relaxed);
        self.active_sessions.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record an event being emitted
    pub fn event_emitted(&self) {
        self.events_emitted.fetch_add(1, Ordering::Relaxed);
    }

    /// Record bytes received
    pub fn bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record packets received
    pub fn packets_received(&self, packets: u64) {
        self.packets_received.fetch_add(packets, Ordering::Relaxed);
    }

    /// Record a webhook delivery attempt
    pub fn webhook_attempted(&self) {
        self.webhook_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a successful webhook delivery
    pub fn webhook_succeeded(&self) {
        self.webhook_successes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed webhook delivery
    pub fn webhook_failed(&self) {
        self.webhook_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let uptime_secs = now.saturating_sub(self.startup_time.load(Ordering::Relaxed));

        MetricsSnapshot {
            sessions_created: self.sessions_created.load(Ordering::Relaxed),
            sessions_ended: self.sessions_ended.load(Ordering::Relaxed),
            active_sessions: self.active_sessions.load(Ordering::Relaxed),
            events_emitted: self.events_emitted.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            packets_received: self.packets_received.load(Ordering::Relaxed),
            webhook_attempts: self.webhook_attempts.load(Ordering::Relaxed),
            webhook_successes: self.webhook_successes.load(Ordering::Relaxed),
            webhook_failures: self.webhook_failures.load(Ordering::Relaxed),
            uptime_secs,
        }
    }

    /// Get active session count
    pub fn active_session_count(&self) -> u64 {
        self.active_sessions.load(Ordering::Relaxed)
    }
}

/// Snapshot of current metrics
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    /// Total sessions created
    pub sessions_created: u64,

    /// Total sessions ended
    pub sessions_ended: u64,

    /// Currently active sessions
    pub active_sessions: u64,

    /// Total events emitted
    pub events_emitted: u64,

    /// Total bytes received
    pub bytes_received: u64,

    /// Total packets received
    pub packets_received: u64,

    /// Total webhook attempts
    pub webhook_attempts: u64,

    /// Successful webhook deliveries
    pub webhook_successes: u64,

    /// Failed webhook deliveries
    pub webhook_failures: u64,

    /// Uptime in seconds
    pub uptime_secs: u64,
}

impl MetricsSnapshot {
    /// Calculate events per second
    pub fn events_per_second(&self) -> f64 {
        if self.uptime_secs == 0 {
            0.0
        } else {
            self.events_emitted as f64 / self.uptime_secs as f64
        }
    }

    /// Calculate bytes per second
    pub fn bytes_per_second(&self) -> f64 {
        if self.uptime_secs == 0 {
            0.0
        } else {
            self.bytes_received as f64 / self.uptime_secs as f64
        }
    }

    /// Calculate webhook success rate
    pub fn webhook_success_rate(&self) -> f64 {
        if self.webhook_attempts == 0 {
            1.0
        } else {
            self.webhook_successes as f64 / self.webhook_attempts as f64
        }
    }
}

/// Global metrics instance
static GLOBAL_METRICS: std::sync::OnceLock<Arc<Metrics>> = std::sync::OnceLock::new();

/// Get the global metrics instance
pub fn global_metrics() -> Arc<Metrics> {
    GLOBAL_METRICS
        .get_or_init(|| Arc::new(Metrics::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.sessions_created, 0);
        assert_eq!(snapshot.active_sessions, 0);
        assert_eq!(snapshot.events_emitted, 0);
    }

    #[test]
    fn test_session_tracking() {
        let metrics = Metrics::new();

        metrics.session_created();
        metrics.session_created();
        assert_eq!(metrics.active_session_count(), 2);

        metrics.session_ended();
        assert_eq!(metrics.active_session_count(), 1);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.sessions_created, 2);
        assert_eq!(snapshot.sessions_ended, 1);
        assert_eq!(snapshot.active_sessions, 1);
    }

    #[test]
    fn test_event_tracking() {
        let metrics = Metrics::new();

        for _ in 0..100 {
            metrics.event_emitted();
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.events_emitted, 100);
    }

    #[test]
    fn test_webhook_tracking() {
        let metrics = Metrics::new();

        metrics.webhook_attempted();
        metrics.webhook_succeeded();
        metrics.webhook_attempted();
        metrics.webhook_failed();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.webhook_attempts, 2);
        assert_eq!(snapshot.webhook_successes, 1);
        assert_eq!(snapshot.webhook_failures, 1);
        assert!((snapshot.webhook_success_rate() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_bytes_tracking() {
        let metrics = Metrics::new();

        metrics.bytes_received(1000);
        metrics.bytes_received(2000);
        metrics.packets_received(10);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.bytes_received, 3000);
        assert_eq!(snapshot.packets_received, 10);
    }

    #[test]
    fn test_global_metrics() {
        let m1 = global_metrics();
        let m2 = global_metrics();

        // Should be the same instance
        m1.event_emitted();
        assert_eq!(m2.snapshot().events_emitted, m1.snapshot().events_emitted);
    }
}
