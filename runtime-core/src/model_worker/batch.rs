//! Request batching for model workers

use super::protocol::InferRequest;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use std::sync::Arc;

/// Batch of inference requests
pub struct RequestBatch {
    /// Requests in this batch
    pub requests: Vec<InferRequest>,
    /// When this batch was created
    pub created_at: Instant,
}

/// Batches inference requests for efficient processing
pub struct RequestBatcher {
    /// Pending requests
    pending: Arc<Mutex<Vec<InferRequest>>>,
    /// Maximum batch size
    max_batch_size: usize,
    /// Batch timeout
    batch_timeout: Duration,
}

impl RequestBatcher {
    /// Create a new request batcher
    pub fn new(max_batch_size: usize, batch_timeout_ms: u64) -> Self {
        Self {
            pending: Arc::new(Mutex::new(Vec::new())),
            max_batch_size,
            batch_timeout: Duration::from_millis(batch_timeout_ms),
        }
    }
    
    /// Add a request to the batch
    pub async fn add_request(&self, request: InferRequest) -> Option<RequestBatch> {
        let mut pending = self.pending.lock().await;
        pending.push(request);
        
        // Check if batch is full
        if pending.len() >= self.max_batch_size {
            let requests = std::mem::take(&mut *pending);
            return Some(RequestBatch {
                requests,
                created_at: Instant::now(),
            });
        }
        
        None
    }
    
    /// Flush pending requests (called on timeout)
    pub async fn flush(&self) -> Option<RequestBatch> {
        let mut pending = self.pending.lock().await;
        if pending.is_empty() {
            return None;
        }
        
        let requests = std::mem::take(&mut *pending);
        Some(RequestBatch {
            requests,
            created_at: Instant::now(),
        })
    }
    
    /// Get batch timeout
    pub fn timeout(&self) -> Duration {
        self.batch_timeout
    }
    
    /// Get pending request count
    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }
}

