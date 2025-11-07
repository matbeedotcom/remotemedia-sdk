//! gRPC service implementation for model workers

use super::protocol::{InferRequest, InferResponse, WorkerStatusInfo};
use super::batch::RequestBatcher;
use super::health::HealthChecker;
use super::status::StatusTracker;
use crate::model_registry::InferenceModel;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Instant;

/// Model worker gRPC service
pub struct ModelWorkerService<T: InferenceModel> {
    /// The model this service wraps
    model: Arc<T>,
    /// Request batcher
    batcher: Arc<RequestBatcher>,
    /// Status tracker
    status: Arc<RwLock<StatusTracker>>,
    /// Health checker
    health: HealthChecker,
}

impl<T: InferenceModel> ModelWorkerService<T> {
    /// Create a new service
    pub fn new(
        model: Arc<T>,
        config: super::WorkerConfig,
        status: Arc<RwLock<StatusTracker>>,
    ) -> Self {
        let batcher = Arc::new(RequestBatcher::new(
            config.max_batch_size,
            config.batch_timeout_ms,
        ));
        
        let health = HealthChecker::new(Arc::clone(&status));
        
        // Update status with model info
        tokio::spawn({
            let status = Arc::clone(&status);
            let model_id = model.model_id().to_string();
            let max_batch = config.max_batch_size as u32;
            async move {
                let mut s = status.write().await;
                s.set_model_id(model_id);
                s.set_max_batch_size(max_batch);
            }
        });
        
        Self {
            model,
            batcher,
            status,
            health,
        }
    }
    
    /// Handle an inference request
    pub async fn infer(&self, request: InferRequest) -> Result<InferResponse> {
        let start = Instant::now();
        
        // Increment load
        {
            let mut status = self.status.write().await;
            status.increment_load();
        }
        
        // Perform inference
        let output = self.model.infer(&request.input).await?;
        
        let elapsed_ms = start.elapsed().as_millis() as u64;
        
        // Record metrics and decrement load
        {
            let mut status = self.status.write().await;
            status.record_request(elapsed_ms);
            status.decrement_load();
        }
        
        Ok(InferResponse::new(
            request.request_id,
            output,
            elapsed_ms,
        ))
    }
    
    /// Get worker status
    pub async fn get_status(&self) -> WorkerStatusInfo {
        let status = self.status.read().await;
        status.get_info()
    }
    
    /// Perform health check
    pub async fn health_check(&self) -> super::protocol::HealthCheckResponse {
        self.health.check().await
    }
    
    /// Start serving (simplified - actual gRPC implementation would go here)
    pub async fn serve(self, endpoint: &str) -> Result<()> {
        tracing::info!("Model worker service listening on {}", endpoint);
        
        // In a full implementation, this would:
        // 1. Set up gRPC server using tonic
        // 2. Register service handlers
        // 3. Start accepting connections
        // 4. Handle graceful shutdown
        
        // For now, this is a simplified implementation
        // The actual gRPC integration would be added when integrating with
        // the transports/remotemedia-grpc crate
        
        tracing::warn!(
            "Service.serve() is a placeholder - integrate with remotemedia-grpc for full gRPC support"
        );
        
        Ok(())
    }
}

