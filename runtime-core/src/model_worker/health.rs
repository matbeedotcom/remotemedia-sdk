//! Health check functionality for model workers

use super::protocol::HealthCheckResponse;
use super::status::StatusTracker;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Health checker for model workers
pub struct HealthChecker {
    status: Arc<RwLock<StatusTracker>>,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(status: Arc<RwLock<StatusTracker>>) -> Self {
        Self { status }
    }
    
    /// Perform health check
    pub async fn check(&self) -> HealthCheckResponse {
        let status = self.status.read().await;
        
        let healthy = status.is_healthy();
        let status_str = if healthy {
            format!(
                "Healthy - {} requests processed, {:.1}ms avg latency",
                status.get_info().total_requests,
                status.average_latency_ms()
            )
        } else {
            format!("Unhealthy - Status: {:?}", status.get_info().status)
        };
        
        HealthCheckResponse {
            healthy,
            status: status_str,
        }
    }
}

