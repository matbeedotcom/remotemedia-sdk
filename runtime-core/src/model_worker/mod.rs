//! Model worker for cross-process model serving
//!
//! This module provides infrastructure for running models in dedicated worker
//! processes, enabling GPU-efficient model sharing across process boundaries.

use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

pub mod protocol;
pub mod client;
pub mod service;
pub mod batch;
pub mod health;
pub mod status;

pub use protocol::{InferRequest, InferResponse, WorkerStatus};
pub use client::ModelWorkerClient;
pub use service::ModelWorkerService;
pub use status::StatusTracker;

use crate::model_registry::InferenceModel;

/// Model worker that owns a model and serves inference requests
pub struct ModelWorker<T: InferenceModel> {
    /// Worker identifier
    worker_id: String,
    /// The model this worker serves
    model: Arc<T>,
    /// Configuration
    config: WorkerConfig,
    /// Status tracker
    status: Arc<RwLock<StatusTracker>>,
}

/// Configuration for model worker
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Maximum batch size for batching requests
    pub max_batch_size: usize,
    /// Timeout for batch accumulation
    pub batch_timeout_ms: u64,
    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,
    /// Health check interval
    pub health_check_interval_ms: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 8,
            batch_timeout_ms: 10,
            max_concurrent_requests: 100,
            health_check_interval_ms: 5000,
        }
    }
}

impl<T: InferenceModel> ModelWorker<T> {
    /// Create a new model worker
    pub fn new(worker_id: String, model: T, config: WorkerConfig) -> Self {
        Self {
            worker_id: worker_id.clone(),
            model: Arc::new(model),
            config,
            status: Arc::new(RwLock::new(StatusTracker::new(worker_id))),
        }
    }
    
    /// Get worker ID
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    
    /// Get model reference
    pub fn model(&self) -> &T {
        &self.model
    }
    
    /// Get configuration
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }
    
    /// Get status tracker
    pub fn status(&self) -> Arc<RwLock<StatusTracker>> {
        Arc::clone(&self.status)
    }
    
    /// Start serving requests on the given endpoint
    pub async fn serve(self, endpoint: &str) -> Result<()> {
        tracing::info!("Starting model worker on {}", endpoint);
        
        // Update status to Ready
        {
            let mut status = self.status.write().await;
            status.set_ready();
        }
        
        // Start gRPC service
        let service = ModelWorkerService::new(
            Arc::clone(&self.model),
            self.config.clone(),
            Arc::clone(&self.status),
        );
        
        service.serve(endpoint).await
    }
}
