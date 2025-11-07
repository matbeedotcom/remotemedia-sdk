//! Example gRPC server with model worker service
//!
//! Demonstrates how to run a model worker that serves inference requests
//! via gRPC with shared memory tensor support.

use remotemedia_grpc::model_worker_service::{ModelWorkerServiceImpl, ModelWorkerServiceServer};
use remotemedia_runtime_core::model_registry::{InferenceModel, DeviceType};
use remotemedia_runtime_core::model_worker::{ModelWorker, WorkerConfig};
use remotemedia_runtime_core::tensor::TensorBuffer;
use async_trait::async_trait;
use anyhow::Result;
use tonic::transport::Server;
use std::net::SocketAddr;

/// Example model for demonstration
struct ExampleInferenceModel {
    model_id: String,
    device: DeviceType,
}

impl ExampleInferenceModel {
    fn new(model_id: String, device: DeviceType) -> Self {
        tracing::info!("Creating example model: {} on {:?}", model_id, device);
        Self { model_id, device }
    }
}

#[async_trait]
impl InferenceModel for ExampleInferenceModel {
    fn model_id(&self) -> &str {
        &self.model_id
    }
    
    fn device(&self) -> DeviceType {
        self.device.clone()
    }
    
    fn memory_usage(&self) -> usize {
        100 * 1024 * 1024 // 100MB for example
    }
    
    async fn infer(&self, input: &TensorBuffer) -> Result<TensorBuffer> {
        tracing::info!("Performing inference on {:?}", input.shape());
        
        // Simple echo for demonstration
        // In production, this would run actual model inference
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        Ok(input.clone())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    
    tracing::info!("Starting Model Worker gRPC Server");
    
    // Create example model
    let model = ExampleInferenceModel::new(
        "example-model-v1".to_string(),
        DeviceType::Cpu,
    );
    
    // Create worker configuration
    let worker_config = WorkerConfig {
        max_batch_size: 8,
        batch_timeout_ms: 10,
        max_concurrent_requests: 100,
        health_check_interval_ms: 5000,
    };
    
    // Create model worker
    let worker = ModelWorker::new(
        "worker-1".to_string(),
        model,
        worker_config,
    );
    
    // Create gRPC service adapter
    let service = ModelWorkerServiceImpl::new(worker);
    
    // Configure server address
    let addr: SocketAddr = "0.0.0.0:50052".parse()?;
    
    tracing::info!("Model Worker listening on {}", addr);
    tracing::info!("  Model: example-model-v1");
    tracing::info!("  Device: CPU");
    tracing::info!("  Max batch size: 8");
    tracing::info!("  Endpoint: grpc://localhost:50052");
    
    // Start server
    Server::builder()
        .add_service(ModelWorkerServiceServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
}

