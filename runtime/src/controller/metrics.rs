//! Metrics for the LLM Pipeline Controller

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::ActionOutcome;

/// Metrics collected by the controller
///
/// These metrics help monitor the controller's own performance
/// and effectiveness, separate from the pipeline metrics.
pub struct ControllerMetrics {
    /// Total observations collected
    observations_total: AtomicU64,

    /// Total LLM calls made
    llm_calls_total: AtomicU64,

    /// Total LLM latency in microseconds
    llm_latency_total_us: AtomicU64,

    /// Total actions taken (all types)
    actions_total: AtomicU64,

    /// Successful actions
    actions_success: AtomicU64,

    /// Failed actions
    actions_failed: AtomicU64,

    /// Rejected actions (policy violations)
    actions_rejected: AtomicU64,

    /// Circuit breaker activations
    circuit_breaker_activations: AtomicU64,
}

impl ControllerMetrics {
    pub fn new() -> Self {
        Self {
            observations_total: AtomicU64::new(0),
            llm_calls_total: AtomicU64::new(0),
            llm_latency_total_us: AtomicU64::new(0),
            actions_total: AtomicU64::new(0),
            actions_success: AtomicU64::new(0),
            actions_failed: AtomicU64::new(0),
            actions_rejected: AtomicU64::new(0),
            circuit_breaker_activations: AtomicU64::new(0),
        }
    }

    /// Record an observation being collected
    pub fn record_observation(&self) {
        self.observations_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an LLM call and its latency
    pub fn record_llm_call(&self, latency: Duration) {
        self.llm_calls_total.fetch_add(1, Ordering::Relaxed);
        self.llm_latency_total_us
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
    }

    /// Record an action and its outcome
    pub fn record_action(&self, outcome: &ActionOutcome) {
        self.actions_total.fetch_add(1, Ordering::Relaxed);

        match outcome {
            ActionOutcome::Success { .. } => {
                self.actions_success.fetch_add(1, Ordering::Relaxed);
            }
            ActionOutcome::Failed { .. } => {
                self.actions_failed.fetch_add(1, Ordering::Relaxed);
            }
            ActionOutcome::Rejected { .. } => {
                self.actions_rejected.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Record a circuit breaker activation
    pub fn record_circuit_breaker(&self) {
        self.circuit_breaker_activations.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of current metrics
    pub fn snapshot(&self) -> ControllerMetricsSnapshot {
        let observations = self.observations_total.load(Ordering::Relaxed);
        let llm_calls = self.llm_calls_total.load(Ordering::Relaxed);
        let llm_latency_us = self.llm_latency_total_us.load(Ordering::Relaxed);
        let actions = self.actions_total.load(Ordering::Relaxed);
        let success = self.actions_success.load(Ordering::Relaxed);
        let failed = self.actions_failed.load(Ordering::Relaxed);
        let rejected = self.actions_rejected.load(Ordering::Relaxed);
        let circuit_breaker = self.circuit_breaker_activations.load(Ordering::Relaxed);

        ControllerMetricsSnapshot {
            observations_total: observations,
            llm_calls_total: llm_calls,
            llm_avg_latency_ms: if llm_calls > 0 {
                (llm_latency_us as f64 / llm_calls as f64) / 1000.0
            } else {
                0.0
            },
            actions_total: actions,
            actions_success: success,
            actions_failed: failed,
            actions_rejected: rejected,
            action_success_rate: if actions > 0 {
                success as f64 / actions as f64
            } else {
                1.0
            },
            circuit_breaker_activations: circuit_breaker,
        }
    }

    /// Reset all metrics (for testing)
    pub fn reset(&self) {
        self.observations_total.store(0, Ordering::Relaxed);
        self.llm_calls_total.store(0, Ordering::Relaxed);
        self.llm_latency_total_us.store(0, Ordering::Relaxed);
        self.actions_total.store(0, Ordering::Relaxed);
        self.actions_success.store(0, Ordering::Relaxed);
        self.actions_failed.store(0, Ordering::Relaxed);
        self.actions_rejected.store(0, Ordering::Relaxed);
        self.circuit_breaker_activations.store(0, Ordering::Relaxed);
    }
}

impl Default for ControllerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// A point-in-time snapshot of controller metrics
#[derive(Debug, Clone, serde::Serialize)]
pub struct ControllerMetricsSnapshot {
    /// Total observations collected
    pub observations_total: u64,

    /// Total LLM calls made
    pub llm_calls_total: u64,

    /// Average LLM latency in milliseconds
    pub llm_avg_latency_ms: f64,

    /// Total actions taken
    pub actions_total: u64,

    /// Successful actions
    pub actions_success: u64,

    /// Failed actions
    pub actions_failed: u64,

    /// Rejected actions
    pub actions_rejected: u64,

    /// Success rate (0.0 - 1.0)
    pub action_success_rate: f64,

    /// Circuit breaker activations
    pub circuit_breaker_activations: u64,
}

impl ControllerMetricsSnapshot {
    /// Format as a human-readable string
    pub fn to_summary(&self) -> String {
        format!(
            "Controller: {} obs, {} LLM calls (avg {:.1}ms), {} actions ({:.0}% success), {} circuit breaker",
            self.observations_total,
            self.llm_calls_total,
            self.llm_avg_latency_ms,
            self.actions_total,
            self.action_success_rate * 100.0,
            self.circuit_breaker_activations
        )
    }
}

/// Histogram for tracking latency distributions
pub struct LatencyHistogram {
    /// Buckets: <1ms, <5ms, <10ms, <25ms, <50ms, <100ms, <250ms, <500ms, <1000ms, >=1000ms
    buckets: [AtomicU64; 10],
    sum_us: AtomicU64,
    count: AtomicU64,
}

impl LatencyHistogram {
    pub fn new() -> Self {
        Self {
            buckets: Default::default(),
            sum_us: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    pub fn record(&self, duration: Duration) {
        let ms = duration.as_millis() as u64;

        let bucket = match ms {
            0 => 0,
            1..=4 => 1,
            5..=9 => 2,
            10..=24 => 3,
            25..=49 => 4,
            50..=99 => 5,
            100..=249 => 6,
            250..=499 => 7,
            500..=999 => 8,
            _ => 9,
        };

        self.buckets[bucket].fetch_add(1, Ordering::Relaxed);
        self.sum_us.fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn mean_ms(&self) -> f64 {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }
        let sum_us = self.sum_us.load(Ordering::Relaxed);
        (sum_us as f64 / count as f64) / 1000.0
    }

    /// Estimate percentile (approximate, based on buckets)
    pub fn percentile(&self, p: f64) -> f64 {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }

        let target = (count as f64 * p) as u64;
        let mut cumulative = 0u64;

        let bucket_upper_bounds = [1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2000.0];

        for (i, bucket) in self.buckets.iter().enumerate() {
            cumulative += bucket.load(Ordering::Relaxed);
            if cumulative >= target {
                return bucket_upper_bounds[i];
            }
        }

        bucket_upper_bounds[9]
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_metrics() {
        let metrics = ControllerMetrics::new();

        metrics.record_observation();
        metrics.record_observation();
        metrics.record_llm_call(Duration::from_millis(50));
        metrics.record_llm_call(Duration::from_millis(100));
        metrics.record_action(&ActionOutcome::Success {
            latency_impact_ms: Some(-5.0),
            details: None,
        });
        metrics.record_action(&ActionOutcome::Failed {
            error: "test".to_string(),
        });

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.observations_total, 2);
        assert_eq!(snapshot.llm_calls_total, 2);
        assert!((snapshot.llm_avg_latency_ms - 75.0).abs() < 0.1);
        assert_eq!(snapshot.actions_total, 2);
        assert_eq!(snapshot.actions_success, 1);
        assert_eq!(snapshot.actions_failed, 1);
        assert!((snapshot.action_success_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_latency_histogram() {
        let hist = LatencyHistogram::new();

        hist.record(Duration::from_micros(500)); // <1ms bucket
        hist.record(Duration::from_millis(3)); // <5ms bucket
        hist.record(Duration::from_millis(50)); // <100ms bucket

        assert!((hist.mean_ms() - 17.83).abs() < 0.1);
        assert_eq!(hist.percentile(0.5), 5.0); // 50th percentile
    }
}
