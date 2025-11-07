//! Example client for model worker service
//!
//! Demonstrates how to connect to a model worker and submit inference requests.

use remotemedia_grpc::generated::remotemedia::model_registry::v1::{
    model_worker_service_client::ModelWorkerServiceClient,
    InferRequest, TensorData,
    HealthCheckRequest, GetStatusRequest,
};
use anyhow::Result;
use tonic::Request;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    
    tracing::info!("Model Worker Client Example");
    
    // Connect to worker
    let endpoint = "http://localhost:50052";
    tracing::info!("Connecting to worker at {}", endpoint);
    
    let mut client = ModelWorkerServiceClient::connect(endpoint).await?;
    
    tracing::info!("Connected successfully");
    
    // Health check
    tracing::info!("Performing health check...");
    let health_response = client
        .health_check(Request::new(HealthCheckRequest {}))
        .await?;
    
    let health = health_response.into_inner();
    tracing::info!("Health: {} - {}", health.healthy, health.status);
    
    // Get worker status
    tracing::info!("Getting worker status...");
    let status_response = client
        .get_status(Request::new(GetStatusRequest {}))
        .await?;
    
    let status = status_response.into_inner();
    tracing::info!("Worker Status:");
    tracing::info!("  Worker ID: {}", status.worker_id);
    tracing::info!("  Model ID: {}", status.model_id);
    tracing::info!("  Status: {}", status.status);
    tracing::info!("  Current Load: {}", status.current_load);
    tracing::info!("  Total Requests: {}", status.total_requests);
    
    // Submit inference request
    tracing::info!("Submitting inference request...");
    
    // Create test tensor (10 float32 values)
    let test_data: Vec<f32> = (0..10).map(|i| i as f32).collect();
    let tensor_bytes: Vec<u8> = test_data
        .iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect();
    
    let infer_request = InferRequest {
        model_id: "example-model-v1".to_string(),
        input: Some(remotemedia_grpc::generated::remotemedia::model_registry::v1::infer_request::Input::TensorData(
            TensorData {
                data: tensor_bytes,
                shape: vec![10],
                dtype: "f32".to_string(),
            }
        )),
        parameters: Default::default(),
        request_id: "test-request-1".to_string(),
    };
    
    let infer_response = client.infer(Request::new(infer_request)).await?;
    
    let response = infer_response.into_inner();
    tracing::info!("Inference complete:");
    tracing::info!("  Request ID: {}", response.request_id);
    tracing::info!("  Inference time: {}ms", response.inference_time_ms);
    
    if let Some(output) = response.output {
        match output {
            remotemedia_grpc::generated::remotemedia::model_registry::v1::infer_response::Output::TensorData(tensor) => {
                tracing::info!("  Output shape: {:?}", tensor.shape);
                tracing::info!("  Output dtype: {}", tensor.dtype);
                tracing::info!("  Output size: {} bytes", tensor.data.len());
            }
            remotemedia_grpc::generated::remotemedia::model_registry::v1::infer_response::Output::TensorRef(tensor_ref) => {
                tracing::info!("  Output in shared memory:");
                tracing::info!("    Region ID: {}", tensor_ref.region_id);
                tracing::info!("    Shape: {:?}", tensor_ref.shape);
            }
        }
    }
    
    tracing::info!("Example complete!");
    
    Ok(())
}

