//! Worker status tracking

use super::protocol::{WorkerStatus, WorkerStatusInfo};
use std::time::Instant;

/// Tracks worker status and performance metrics
pub struct StatusTracker {
    worker_id: String,
    model_id: String,
    status: WorkerStatus,
    current_load: u32,
    max_batch_size: u32,
    total_requests: u64,
    total_latency_ms: u64,
    started_at: Instant,
}

impl StatusTracker {
    /// Create a new status tracker
    pub fn new(worker_id: String) -> Self {
        Self {
            worker_id,
            model_id: String::new(),
            status: WorkerStatus::Starting,
            current_load: 0,
            max_batch_size: 8,
            total_requests: 0,
            total_latency_ms: 0,
            started_at: Instant::now(),
        }
    }
    
    /// Set model ID
    pub fn set_model_id(&mut self, model_id: String) {
        self.model_id = model_id;
    }
    
    /// Set max batch size
    pub fn set_max_batch_size(&mut self, size: u32) {
        self.max_batch_size = size;
    }
    
    /// Mark worker as ready
    pub fn set_ready(&mut self) {
        self.status = WorkerStatus::Ready;
        tracing::info!("Worker {} is ready", self.worker_id);
    }
    
    /// Mark worker as busy
    pub fn set_busy(&mut self) {
        self.status = WorkerStatus::Busy;
    }
    
    /// Mark worker as stopping
    pub fn set_stopping(&mut self) {
        self.status = WorkerStatus::Stopping;
    }
    
    /// Increment current load
    pub fn increment_load(&mut self) {
        self.current_load += 1;
        if self.current_load >= self.max_batch_size {
            self.status = WorkerStatus::Busy;
        }
    }
    
    /// Decrement current load
    pub fn decrement_load(&mut self) {
        if self.current_load > 0 {
            self.current_load -= 1;
        }
        if self.current_load < self.max_batch_size && self.status == WorkerStatus::Busy {
            self.status = WorkerStatus::Ready;
        }
    }
    
    /// Record completed request
    pub fn record_request(&mut self, latency_ms: u64) {
        self.total_requests += 1;
        self.total_latency_ms += latency_ms;
    }
    
    /// Get average latency
    pub fn average_latency_ms(&self) -> f64 {
        if self.total_requests > 0 {
            self.total_latency_ms as f64 / self.total_requests as f64
        } else {
            0.0
        }
    }
    
    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
    
    /// Get current status info
    pub fn get_info(&self) -> WorkerStatusInfo {
        WorkerStatusInfo {
            worker_id: self.worker_id.clone(),
            model_id: self.model_id.clone(),
            status: self.status.clone(),
            current_load: self.current_load,
            max_batch_size: self.max_batch_size,
            total_requests: self.total_requests,
            average_latency_ms: self.average_latency_ms(),
        }
    }
    
    /// Check if worker is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self.status, WorkerStatus::Ready | WorkerStatus::Busy)
    }
}

