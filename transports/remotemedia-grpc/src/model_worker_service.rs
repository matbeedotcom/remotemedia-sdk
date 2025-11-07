//! gRPC service implementation for model worker

use crate::generated::remotemedia::model_registry::v1::{
    model_worker_service_server::ModelWorkerService as ModelWorkerServiceTrait,
    InferRequest, InferResponse, TensorRef, TensorData,
    HealthCheckRequest, HealthCheckResponse,
    GetStatusRequest, GetStatusResponse,
};
use remotemedia_runtime_core::model_registry::InferenceModel;
use remotemedia_runtime_core::tensor::{TensorBuffer, DataType};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

/// gRPC service adapter for model workers
pub struct ModelWorkerServiceImpl<T: InferenceModel> {
    /// The underlying model worker from runtime-core
    worker: Arc<remotemedia_runtime_core::model_worker::ModelWorker<T>>,
}

impl<T: InferenceModel> ModelWorkerServiceImpl<T> {
    /// Create a new service implementation
    pub fn new(worker: remotemedia_runtime_core::model_worker::ModelWorker<T>) -> Self {
        Self {
            worker: Arc::new(worker),
        }
    }
}

#[tonic::async_trait]
impl<T: InferenceModel + 'static> ModelWorkerServiceTrait for ModelWorkerServiceImpl<T> {
    type InferStreamStream = tokio_stream::wrappers::ReceiverStream<Result<InferResponse, Status>>;
    
    /// Submit inference request to worker
    async fn infer(
        &self,
        request: Request<InferRequest>,
    ) -> Result<Response<InferResponse>, Status> {
        let req = request.into_inner();
        
        // Convert protobuf input to TensorBuffer
        let input_tensor = match req.input {
            Some(input) => match input {
                crate::generated::remotemedia::model_registry::v1::infer_request::Input::TensorRef(tensor_ref) => {
                    // Tensor in shared memory
                    #[cfg(feature = "shared-memory")]
                    {
                        let shape: Vec<usize> = tensor_ref.shape.iter().map(|&s| s as usize).collect();
                        let dtype = parse_dtype(&tensor_ref.dtype)?;
                        
                        TensorBuffer::from_shared_memory(
                            &tensor_ref.region_id,
                            tensor_ref.offset as usize,
                            tensor_ref.size as usize,
                            shape,
                            dtype,
                        ).map_err(|e| Status::internal(format!("Failed to read tensor from SHM: {}", e)))?
                    }
                    
                    #[cfg(not(feature = "shared-memory"))]
                    {
                        return Err(Status::unimplemented("Shared memory not enabled"));
                    }
                }
                crate::generated::remotemedia::model_registry::v1::infer_request::Input::TensorData(tensor_data) => {
                    // Inline tensor data
                    let shape: Vec<usize> = tensor_data.shape.iter().map(|&s| s as usize).collect();
                    let dtype = parse_dtype(&tensor_data.dtype)?;
                    
                    TensorBuffer::from_vec(tensor_data.data, shape, dtype)
                }
            },
            None => return Err(Status::invalid_argument("Missing input tensor")),
        };
        
        // Perform inference using the model
        let output_tensor = self.worker.model()
            .infer(&input_tensor)
            .await
            .map_err(|e| Status::internal(format!("Inference failed: {}", e)))?;
        
        // Convert output back to protobuf
        // For now, use inline data (TODO: use SHM for large tensors)
        let output_bytes = output_tensor.as_bytes()
            .map_err(|e| Status::internal(format!("Failed to read output: {}", e)))?;
        
        let response = InferResponse {
            request_id: req.request_id,
            output: Some(crate::generated::remotemedia::model_registry::v1::infer_response::Output::TensorData(
                TensorData {
                    data: output_bytes,
                    shape: output_tensor.shape().iter().map(|&s| s as i32).collect(),
                    dtype: format_dtype(output_tensor.dtype()),
                }
            )),
            inference_time_ms: 0, // TODO: Measure actual time
            metadata: Default::default(),
        };
        
        Ok(Response::new(response))
    }
    
    /// Submit streaming inference request
    async fn infer_stream(
        &self,
        _request: Request<tonic::Streaming<InferRequest>>,
    ) -> Result<Response<Self::InferStreamStream>, Status> {
        // TODO: Implement streaming
        Err(Status::unimplemented("Streaming not yet implemented"))
    }
    
    /// Health check
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let status = self.worker.status();
        let health = remotemedia_runtime_core::model_worker::health::HealthChecker::new(status);
        let check_result = health.check().await;
        
        Ok(Response::new(HealthCheckResponse {
            healthy: check_result.healthy,
            status: check_result.status,
        }))
    }
    
    /// Get worker status
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        let status_tracker = self.worker.status();
        let status_info = status_tracker.read().await.get_info();
        
        Ok(Response::new(GetStatusResponse {
            worker_id: status_info.worker_id,
            model_id: status_info.model_id,
            status: status_info.status.as_str().to_string(),
            current_load: status_info.current_load,
            max_batch_size: status_info.max_batch_size,
            total_requests: status_info.total_requests,
            average_latency_ms: status_info.average_latency_ms,
        }))
    }
}

/// Parse dtype string to DataType enum
fn parse_dtype(dtype: &str) -> Result<DataType, Status> {
    match dtype {
        "f32" | "float32" => Ok(DataType::F32),
        "f16" | "float16" => Ok(DataType::F16),
        "i32" | "int32" => Ok(DataType::I32),
        "i64" | "int64" => Ok(DataType::I64),
        "u8" | "uint8" => Ok(DataType::U8),
        _ => Err(Status::invalid_argument(format!("Unknown dtype: {}", dtype))),
    }
}

/// Format DataType enum to string
fn format_dtype(dtype: DataType) -> String {
    match dtype {
        DataType::F32 => "f32".to_string(),
        DataType::F16 => "f16".to_string(),
        DataType::I32 => "i32".to_string(),
        DataType::I64 => "i64".to_string(),
        DataType::U8 => "u8".to_string(),
    }
}

// Re-export generated types
pub use crate::generated::remotemedia::model_registry::v1::{
    model_worker_service_server::ModelWorkerServiceServer,
    infer_request, infer_response,
};

