//! PipelineExecutor - Unified facade for transport layers
//!
//! This module provides a clean facade replacing PipelineRunner with:
//! - SessionHandle for streaming sessions
//! - PipelineExecutor as unified entry point
//! - Unary and streaming execution modes
//! - Factory registration support
//!
//! # Migration
//!
//! PipelineRunner is deprecated. Use PipelineExecutor instead:
//!
//! ```ignore
//! // Old way (deprecated)
//! let runner = PipelineRunner::new()?;
//! let result = runner.execute_unary(manifest, input).await?;
//!
//! // New way
//! let executor = PipelineExecutor::new()?;
//! let result = executor.execute_unary(manifest, input).await?;
//! ```
//!
//! # Architecture
//!
//! PipelineExecutor wraps SessionRouter with StreamingScheduler to provide:
//! - Production-grade execution with timeout/retry/circuit breaker
//! - DriftMetrics for stream health monitoring
//! - Unified API for all transports (HTTP, gRPC, WebRTC, FFI)
//!
//! # Spec Reference
//!
//! See `/specs/026-streaming-scheduler-migration/` for full specification.

use crate::executor::streaming_scheduler::{SchedulerConfig, StreamingScheduler};
use crate::Result;
use std::sync::Arc;

/// Configuration for PipelineExecutor
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Scheduler configuration
    pub scheduler_config: SchedulerConfig,
    /// Enable drift metrics collection
    pub enable_drift_metrics: bool,
    /// Session ID prefix for generated sessions
    pub session_id_prefix: String,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            scheduler_config: SchedulerConfig::default(),
            enable_drift_metrics: true,
            session_id_prefix: "session".to_string(),
        }
    }
}

/// Unified facade for transport pipeline execution
///
/// PipelineExecutor replaces PipelineRunner with a cleaner API and
/// production-grade execution features.
///
/// # Implementation Note
///
/// Full implementation will be completed in Phase 7 of spec 026.
/// Currently provides the scheduler infrastructure.
pub struct PipelineExecutor {
    /// Configuration
    config: ExecutorConfig,
    /// Streaming scheduler for node execution
    scheduler: Arc<StreamingScheduler>,
    /// Session counter for ID generation
    session_counter: std::sync::atomic::AtomicU64,
}

impl PipelineExecutor {
    /// Create a new PipelineExecutor with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(ExecutorConfig::default())
    }

    /// Create a new PipelineExecutor with custom configuration
    pub fn with_config(config: ExecutorConfig) -> Result<Self> {
        let scheduler = Arc::new(StreamingScheduler::new(config.scheduler_config.clone()));

        Ok(Self {
            config,
            scheduler,
            session_counter: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Get the scheduler reference
    pub fn scheduler(&self) -> &Arc<StreamingScheduler> {
        &self.scheduler
    }

    /// Generate a unique session ID
    pub fn generate_session_id(&self) -> String {
        let count = self
            .session_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("{}_{}", self.config.session_id_prefix, count)
    }

    /// Get scheduler metrics in Prometheus format
    pub async fn prometheus_metrics(&self) -> String {
        self.scheduler.to_prometheus().await
    }

    /// Get scheduler statistics for all nodes
    pub async fn get_node_stats(
        &self,
    ) -> std::collections::HashMap<String, crate::executor::streaming_scheduler::NodeStats> {
        self.scheduler.get_all_node_stats().await
    }
}

impl Default for PipelineExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default PipelineExecutor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_config_default() {
        let config = ExecutorConfig::default();
        assert!(config.enable_drift_metrics);
        assert_eq!(config.session_id_prefix, "session");
    }

    #[test]
    fn test_executor_creation() {
        let executor = PipelineExecutor::new().unwrap();
        assert!(executor.scheduler().config.max_concurrency > 0);
    }

    #[test]
    fn test_session_id_generation() {
        let executor = PipelineExecutor::new().unwrap();
        let id1 = executor.generate_session_id();
        let id2 = executor.generate_session_id();

        assert_ne!(id1, id2);
        assert!(id1.starts_with("session_"));
    }
}
