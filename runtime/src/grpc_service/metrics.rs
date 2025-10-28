//! Prometheus metrics collection for gRPC service
//!
//! Tracks request counters, latency histograms, active connections gauge.
//! Metrics exposed at /metrics HTTP endpoint.

#![cfg(feature = "grpc-transport")]

use prometheus::{
    CounterVec, HistogramOpts, HistogramVec, IntGauge, IntGaugeVec, Opts, Registry,
};
use std::sync::Arc;

/// Prometheus metrics for gRPC service
#[derive(Clone)]
pub struct ServiceMetrics {
    /// Total requests processed (labeled by RPC method and status)
    pub requests_total: CounterVec,

    /// Request latency distribution in seconds (labeled by RPC method)
    pub request_duration_seconds: HistogramVec,

    /// Active connections gauge
    pub active_connections: IntGauge,

    /// Active concurrent executions gauge
    pub active_executions: IntGauge,

    /// Pipeline execution errors (labeled by error type)
    pub execution_errors_total: CounterVec,

    /// Audio samples processed
    pub audio_samples_processed: CounterVec,

    /// Memory usage gauge per execution (labeled by execution ID)
    pub execution_memory_bytes: IntGaugeVec,

    /// Prometheus registry
    pub registry: Arc<Registry>,
}

impl ServiceMetrics {
    /// Create new metrics with a custom registry
    pub fn new(registry: Registry) -> Result<Self, prometheus::Error> {
        let requests_total = CounterVec::new(
            Opts::new(
                "remotemedia_grpc_requests_total",
                "Total number of gRPC requests processed",
            ),
            &["method", "status"],
        )?;

        let request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "remotemedia_grpc_request_duration_seconds",
                "Request latency distribution in seconds",
            )
            .buckets(vec![
                0.001, // 1ms
                0.0025, 0.005, // 2.5ms, 5ms (target: <5ms p50)
                0.01, 0.025, 0.05, // 10ms, 25ms, 50ms
                0.1, 0.25, 0.5, 1.0, // 100ms, 250ms, 500ms, 1s
            ]),
            &["method"],
        )?;

        let active_connections = IntGauge::new(
            "remotemedia_grpc_active_connections",
            "Number of active gRPC connections",
        )?;

        let active_executions = IntGauge::new(
            "remotemedia_grpc_active_executions",
            "Number of concurrent pipeline executions",
        )?;

        let execution_errors_total = CounterVec::new(
            Opts::new(
                "remotemedia_execution_errors_total",
                "Total pipeline execution errors",
            ),
            &["error_type"],
        )?;

        let audio_samples_processed = CounterVec::new(
            Opts::new(
                "remotemedia_audio_samples_processed_total",
                "Total audio samples processed",
            ),
            &["node_type"],
        )?;

        let execution_memory_bytes = IntGaugeVec::new(
            Opts::new(
                "remotemedia_execution_memory_bytes",
                "Current memory usage per execution",
            ),
            &["execution_id"],
        )?;

        // Register all metrics
        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(request_duration_seconds.clone()))?;
        registry.register(Box::new(active_connections.clone()))?;
        registry.register(Box::new(active_executions.clone()))?;
        registry.register(Box::new(execution_errors_total.clone()))?;
        registry.register(Box::new(audio_samples_processed.clone()))?;
        registry.register(Box::new(execution_memory_bytes.clone()))?;

        Ok(Self {
            requests_total,
            request_duration_seconds,
            active_connections,
            active_executions,
            execution_errors_total,
            audio_samples_processed,
            execution_memory_bytes,
            registry: Arc::new(registry),
        })
    }

    /// Create with default registry
    pub fn with_default_registry() -> Result<Self, prometheus::Error> {
        Self::new(Registry::new())
    }

    /// Record RPC request start (returns start time for duration calculation)
    pub fn record_request_start(&self, method: &str) -> std::time::Instant {
        self.active_executions.inc();
        std::time::Instant::now()
    }

    /// Record RPC request completion
    pub fn record_request_end(&self, method: &str, status: &str, start: std::time::Instant) {
        let duration = start.elapsed();
        self.requests_total
            .with_label_values(&[method, status])
            .inc();
        self.request_duration_seconds
            .with_label_values(&[method])
            .observe(duration.as_secs_f64());
        self.active_executions.dec();
    }

    /// Record execution error
    pub fn record_error(&self, error_type: &str) {
        self.execution_errors_total
            .with_label_values(&[error_type])
            .inc();
    }

    /// Record audio samples processed
    pub fn record_samples_processed(&self, node_type: &str, samples: u64) {
        self.audio_samples_processed
            .with_label_values(&[node_type])
            .inc_by(samples as f64);
    }

    /// Update execution memory usage
    pub fn set_execution_memory(&self, execution_id: &str, bytes: i64) {
        self.execution_memory_bytes
            .with_label_values(&[execution_id])
            .set(bytes);
    }

    /// Remove execution memory tracking
    pub fn clear_execution_memory(&self, execution_id: &str) {
        let _ = self.execution_memory_bytes.remove_label_values(&[execution_id]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = ServiceMetrics::with_default_registry();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_request_metrics() {
        let metrics = ServiceMetrics::with_default_registry().unwrap();

        let start = metrics.record_request_start("ExecutePipeline");
        std::thread::sleep(std::time::Duration::from_millis(10));
        metrics.record_request_end("ExecutePipeline", "success", start);

        // Verify counter incremented
        let samples = metrics.requests_total.collect();
        assert!(!samples.is_empty());
    }

    #[test]
    fn test_error_metrics() {
        let metrics = ServiceMetrics::with_default_registry().unwrap();

        metrics.record_error("validation");
        metrics.record_error("execution");

        let samples = metrics.execution_errors_total.collect();
        assert!(!samples.is_empty());
    }

    #[test]
    fn test_audio_samples_metrics() {
        let metrics = ServiceMetrics::with_default_registry().unwrap();

        metrics.record_samples_processed("AudioResample", 44100);
        metrics.record_samples_processed("VAD", 16000);

        let samples = metrics.audio_samples_processed.collect();
        assert!(!samples.is_empty());
    }

    #[test]
    fn test_execution_memory_tracking() {
        let metrics = ServiceMetrics::with_default_registry().unwrap();

        metrics.set_execution_memory("exec-123", 10_000_000); // 10MB
        metrics.set_execution_memory("exec-456", 5_000_000); // 5MB
        metrics.clear_execution_memory("exec-123");

        let samples = metrics.execution_memory_bytes.collect();
        assert!(!samples.is_empty());
    }

    #[test]
    fn test_active_connections() {
        let metrics = ServiceMetrics::with_default_registry().unwrap();

        metrics.active_connections.inc();
        metrics.active_connections.inc();
        assert_eq!(metrics.active_connections.get(), 2);

        metrics.active_connections.dec();
        assert_eq!(metrics.active_connections.get(), 1);
    }
}

