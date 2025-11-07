//! Model worker binary
//!
//! Runs a model in a dedicated process for cross-process sharing

use remotemedia_runtime_core::model_worker::{ModelWorker, WorkerConfig};
use remotemedia_runtime_core::model_registry::{InferenceModel, DeviceType};
use remotemedia_runtime_core::tensor::TensorBuffer;
use async_trait::async_trait;
use anyhow::Result;
use clap::Parser;

/// Example model implementation for demonstration
struct ExampleModel {
    model_id: String,
    device: DeviceType,
}

#[async_trait]
impl InferenceModel for ExampleModel {
    fn model_id(&self) -> &str {
        &self.model_id
    }
    
    fn device(&self) -> DeviceType {
        self.device.clone()
    }
    
    fn memory_usage(&self) -> usize {
        100 * 1024 * 1024 // 100MB example
    }
    
    async fn infer(&self, input: &TensorBuffer) -> Result<TensorBuffer> {
        // Simple echo for demonstration
        Ok(input.clone())
    }
}

#[derive(Parser, Debug)]
#[command(name = "model-worker")]
#[command(about = "Model worker process for cross-process model sharing")]
struct Args {
    /// Worker ID
    #[arg(long, default_value = "worker-1")]
    worker_id: String,
    
    /// Model ID to serve
    #[arg(long, default_value = "example-model")]
    model_id: String,
    
    /// Device (cpu, cuda:0, etc.)
    #[arg(long, default_value = "cpu")]
    device: String,
    
    /// Endpoint to listen on
    #[arg(long, default_value = "0.0.0.0:50051")]
    endpoint: String,
    
    /// Maximum batch size
    #[arg(long, default_value_t = 8)]
    max_batch_size: usize,
    
    /// Batch timeout in milliseconds
    #[arg(long, default_value_t = 10)]
    batch_timeout_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    
    let args = Args::parse();
    
    tracing::info!("Starting model worker");
    tracing::info!("  Worker ID: {}", args.worker_id);
    tracing::info!("  Model ID: {}", args.model_id);
    tracing::info!("  Device: {}", args.device);
    tracing::info!("  Endpoint: {}", args.endpoint);
    
    // Parse device
    let device = if args.device.starts_with("cuda:") {
        let idx: u32 = args.device[5..].parse()?;
        DeviceType::Cuda(idx)
    } else if args.device.starts_with("metal:") {
        let idx: u32 = args.device[6..].parse()?;
        DeviceType::Metal(idx)
    } else {
        DeviceType::Cpu
    };
    
    // Create example model
    let model = ExampleModel {
        model_id: args.model_id.clone(),
        device,
    };
    
    // Create worker config
    let config = WorkerConfig {
        max_batch_size: args.max_batch_size,
        batch_timeout_ms: args.batch_timeout_ms,
        max_concurrent_requests: 100,
        health_check_interval_ms: 5000,
    };
    
    // Create and start worker
    let worker = ModelWorker::new(args.worker_id, model, config);
    
    tracing::info!("Worker ready, serving on {}", args.endpoint);
    
    // Start serving
    worker.serve(&args.endpoint).await?;
    
    Ok(())
}

