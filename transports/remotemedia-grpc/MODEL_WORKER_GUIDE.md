# Model Worker Guide: gRPC Transport Integration

This guide shows how to use the model worker service through the gRPC transport.

## Quick Start

### 1. Start Model Worker Server

```bash
cd transports/remotemedia-grpc
cargo run --example model_worker_server --features model-registry
```

Server listens on `0.0.0.0:50052`

### 2. Connect Client

```bash
# In another terminal
cargo run --example model_worker_client --features model-registry
```

## Architecture

```
┌─────────────────┐         ┌─────────────────┐         ┌─────────────────┐
│  Client 1       │         │  Client 2       │         │  Client 3       │
│  (Python/Rust)  │         │  (Python/Rust)  │         │  (Python/Rust)  │
└────────┬────────┘         └────────┬────────┘         └────────┬────────┘
         │                           │                           │
         │              gRPC (port 50052)                        │
         └───────────────────────────┼───────────────────────────┘
                                     │
                          ┌──────────▼──────────┐
                          │   gRPC Server       │
                          │  ModelWorkerService │
                          └──────────┬──────────┘
                                     │
                          ┌──────────▼──────────┐
                          │   ModelWorker       │
                          │   (runtime-core)    │
                          └──────────┬──────────┘
                                     │
                          ┌──────────▼──────────┐
                          │  InferenceModel     │
                          │   (Your ML Model)   │
                          └─────────────────────┘
```

## Features

### 1. Model Serving
- Single model instance serves multiple clients
- Automatic batching for efficiency
- Health checks for Kubernetes/Docker

### 2. Tensor Transfer
- **Inline**: Small tensors sent directly in proto
- **Shared Memory**: Large tensors via SHM references (when enabled)

### 3. Monitoring
- Health check endpoint
- Status endpoint with metrics
- Per-request timing

## Usage with Custom Models

### Create Your Model

```rust
use remotemedia_runtime_core::model_registry::InferenceModel;
use remotemedia_runtime_core::tensor::TensorBuffer;
use async_trait::async_trait;

struct MyModel {
    // Your model fields
}

#[async_trait]
impl InferenceModel for MyModel {
    fn model_id(&self) -> &str {
        "my-model-v1"
    }
    
    fn device(&self) -> DeviceType {
        DeviceType::Cuda(0)
    }
    
    fn memory_usage(&self) -> usize {
        // Your model size in bytes
        1_500_000_000 // 1.5GB
    }
    
    async fn infer(&self, input: &TensorBuffer) -> Result<TensorBuffer> {
        // Your inference logic
        todo!()
    }
}
```

### Start Server with Your Model

```rust
let model = MyModel::load()?;
let worker = ModelWorker::new("worker-1".to_string(), model, WorkerConfig::default());
let service = ModelWorkerServiceImpl::new(worker);

Server::builder()
    .add_service(ModelWorkerServiceServer::new(service))
    .serve("0.0.0.0:50052".parse()?)
    .await?;
```

## Integration with Main Server

To add model worker to the main gRPC server alongside pipeline execution:

```rust
use remotemedia_grpc::{GrpcServer, ServiceConfig};
use remotemedia_grpc::model_worker_service::ModelWorkerServiceServer;

// Create main server
let server = GrpcServer::new(config, runner)?;

// Add model worker service (optional)
#[cfg(feature = "model-registry")]
{
    let worker = create_your_worker()?;
    let worker_service = ModelWorkerServiceImpl::new(worker);
    
    // Server would need to be extended to support adding additional services
    // Current implementation uses serve() which builds the server internally
}
```

## Python Client

```python
import grpc
from generated import model_worker_pb2, model_worker_pb2_grpc

# Connect
channel = grpc.insecure_channel('localhost:50052')
stub = model_worker_pb2_grpc.ModelWorkerServiceStub(channel)

# Health check
health = stub.HealthCheck(model_worker_pb2.HealthCheckRequest())
print(f"Healthy: {health.healthy}")

# Inference
request = model_worker_pb2.InferRequest(
    model_id="my-model",
    input=model_worker_pb2.TensorData(
        data=tensor_bytes,
        shape=[10, 224, 224],
        dtype="f32"
    ),
    request_id="req-123"
)

response = stub.Infer(request)
print(f"Inference time: {response.inference_time_ms}ms")
```

## Shared Memory Optimization

When `shared-memory` feature is enabled:

### Server Side
```rust
#[cfg(feature = "shared-memory")]
{
    use remotemedia_runtime_core::tensor::SharedMemoryAllocator;
    
    let allocator = SharedMemoryAllocator::new(Default::default());
    let output_tensor = allocator.allocate_tensor(size, None)?;
    
    // Write inference results to SHM
    // ...
    
    // Return tensor reference instead of data
    InferResponse {
        output: Some(Output::TensorRef(TensorRef {
            region_id: output_tensor.region_id(),
            offset: 0,
            size: tensor_size,
            shape: output_shape,
            dtype: "f32".to_string(),
        })),
        ...
    }
}
```

### Client Side
```rust
if let Output::TensorRef(tensor_ref) = response.output {
    // Read from shared memory (zero-copy)
    let tensor = TensorBuffer::from_shared_memory(
        &tensor_ref.region_id,
        tensor_ref.offset as usize,
        tensor_ref.size as usize,
        tensor_ref.shape.iter().map(|&s| s as usize).collect(),
        parse_dtype(&tensor_ref.dtype)?
    )?;
}
```

## Performance

With model registry and shared memory enabled:

- **Memory**: Single model instance (vs N instances)
- **Latency**: Sub-millisecond for cached models
- **Throughput**: 5.4 GB/s for tensor transfers
- **Concurrency**: 100+ requests per worker

## Examples

Run the examples:

```bash
# Terminal 1: Start server
cargo run --example model_worker_server --features model-registry

# Terminal 2: Run client
cargo run --example model_worker_client --features model-registry
```

## Production Deployment

For production, integrate with the main gRPC server or run as separate service:

```yaml
# docker-compose.yml
services:
  model-worker:
    image: remotemedia/model-worker:latest
    ports:
      - "50052:50052"
    environment:
      - MODEL_ID=whisper-base
      - DEVICE=cuda:0
    deploy:
      resources:
        reservations:
          devices:
            - capabilities: [gpu]
```

## See Also

- [Model Registry Guide](../../runtime-core/src/model_registry/README.md)
- [Shared Memory Guide](../../runtime-core/src/tensor/README.md)
- [gRPC Transport README](README.md)

