//! IPC protocol messages for model worker communication

use crate::tensor::TensorBuffer;
use std::collections::HashMap;

/// Inference request to model worker
#[derive(Debug, Clone)]
pub struct InferRequest {
    /// Request identifier for tracking
    pub request_id: String,
    /// Input tensor
    pub input: TensorBuffer,
    /// Optional parameters
    pub parameters: HashMap<String, String>,
}

impl InferRequest {
    /// Create a new inference request
    pub fn new(request_id: String, input: TensorBuffer) -> Self {
        Self {
            request_id,
            input,
            parameters: HashMap::new(),
        }
    }
    
    /// Add a parameter
    pub fn with_parameter(mut self, key: String, value: String) -> Self {
        self.parameters.insert(key, value);
        self
    }
}

/// Inference response from model worker
#[derive(Debug, Clone)]
pub struct InferResponse {
    /// Request identifier (matches request)
    pub request_id: String,
    /// Output tensor
    pub output: TensorBuffer,
    /// Inference time in milliseconds
    pub inference_time_ms: u64,
    /// Optional metadata
    pub metadata: HashMap<String, String>,
}

impl InferResponse {
    /// Create a new inference response
    pub fn new(request_id: String, output: TensorBuffer, inference_time_ms: u64) -> Self {
        Self {
            request_id,
            output,
            inference_time_ms,
            metadata: HashMap::new(),
        }
    }
}

/// Worker status information
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerStatus {
    /// Worker is starting up
    Starting,
    /// Worker is ready to accept requests
    Ready,
    /// Worker is processing requests (at capacity)
    Busy,
    /// Worker is shutting down
    Stopping,
    /// Worker has terminated
    Terminated,
}

impl WorkerStatus {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkerStatus::Starting => "starting",
            WorkerStatus::Ready => "ready",
            WorkerStatus::Busy => "busy",
            WorkerStatus::Stopping => "stopping",
            WorkerStatus::Terminated => "terminated",
        }
    }
}

/// Detailed worker status information
#[derive(Debug, Clone)]
pub struct WorkerStatusInfo {
    /// Worker identifier
    pub worker_id: String,
    /// Model identifier this worker serves
    pub model_id: String,
    /// Current status
    pub status: WorkerStatus,
    /// Current number of active requests
    pub current_load: u32,
    /// Maximum batch size
    pub max_batch_size: u32,
    /// Total requests processed
    pub total_requests: u64,
    /// Average latency in milliseconds
    pub average_latency_ms: f64,
}

/// Health check response
#[derive(Debug, Clone)]
pub struct HealthCheckResponse {
    /// Is the worker healthy?
    pub healthy: bool,
    /// Status message
    pub status: String,
}

