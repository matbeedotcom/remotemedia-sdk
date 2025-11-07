# Quick Start: Model Registry and Shared Memory Tensors

This guide demonstrates the three main capabilities: process-local model sharing, cross-process model workers, and shared memory tensors.

## Installation

```bash
# Add to Cargo.toml
[dependencies]
remotemedia-runtime-core = { version = "0.4", features = ["model-registry", "shared-memory"] }

# Python support
pip install remotemedia[ml]
```

## 1. Process-Local Model Sharing (Simplest)

Share a model between multiple nodes in the same process to reduce memory usage.

### Rust Example

```rust
use remotemedia_runtime_core::model_registry::{ModelRegistry, RegistryConfig};
use std::sync::Arc;

// Define your model (implements InferenceModel trait)
struct WhisperModel {
    // Model implementation
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create registry (singleton per process)
    let registry = Arc::new(ModelRegistry::new(RegistryConfig::default()));
    
    // First node loads the model
    let handle1 = registry.get_or_load("whisper-base", || {
        WhisperModel::load("models/whisper-base.onnx")
    }).await?;
    
    // Second node gets the same instance (no reload)
    let handle2 = registry.get_or_load("whisper-base", || {
        WhisperModel::load("models/whisper-base.onnx")
    }).await?;
    
    // Both handles point to the same model in memory
    assert!(Arc::ptr_eq(&handle1.inner, &handle2.inner));
    
    // Use the model
    let output = handle1.model().infer(&input_tensor).await?;
    
    Ok(())
}
```

### Python Example

```python
from remotemedia.core import get_or_load
import torch

def load_whisper():
    """Loader function - called only once"""
    from transformers import WhisperModel
    return WhisperModel.from_pretrained("openai/whisper-base")

# First call loads the model
model1 = get_or_load("whisper-base", load_whisper)

# Second call returns cached instance (instant)
model2 = get_or_load("whisper-base", load_whisper)

# Same model in memory
assert model1 is model2

# Use for inference
output = model1.generate(input_ids)
```

## 2. Shared Memory Tensors (Zero-Copy Transfer)

Transfer large tensors between processes without serialization.

### Rust Example

```rust
use remotemedia_runtime_core::tensor::{TensorBuffer, SharedMemoryAllocator};

// Process A: Create tensor in shared memory
let allocator = SharedMemoryAllocator::new(Default::default());
let tensor = allocator.allocate_tensor(
    1024 * 1024,  // 1MB
    Some("session-123")
)?;

// Fill with data
tensor.as_mut_slice().copy_from_slice(&data);

// Get region ID for sharing
let region_id = tensor.storage().region_id();
println!("Share this ID with other process: {}", region_id);

// Process B: Access the shared tensor
let shared_tensor = TensorBuffer::from_shared_memory(
    &region_id,
    0,  // offset
    1024 * 1024,  // size
    vec![256, 1024],  // shape
    DataType::F32
)?;

// Zero-copy access to data
let data = shared_tensor.as_bytes();
```

### Python Example

```python
import numpy as np
from remotemedia.core import TensorBuffer, SharedMemoryAllocator

# Process A: Create shared tensor
allocator = SharedMemoryAllocator()
data = np.random.randn(256, 1024).astype(np.float32)

# Zero-copy conversion to shared memory
tensor = TensorBuffer.from_numpy(data, zero_copy=True)
region_id = tensor.to_shared_memory()
print(f"Share this ID: {region_id}")

# Process B: Access shared tensor
shared_tensor = TensorBuffer.from_shared_memory(
    region_id=region_id,
    offset=0,
    size=data.nbytes,
    shape=[256, 1024],
    dtype=DataType.F32
)

# Zero-copy to NumPy
shared_array = shared_tensor.to_numpy()
assert np.array_equal(data, shared_array)  # Same data, no copy
```

## 3. Model Worker (Cross-Process GPU Sharing)

Run a model in a dedicated process and serve multiple clients.

### Start Worker Process

```bash
# Start a model worker
remotemedia-model-worker \
    --model-id "llama-7b" \
    --model-path "/models/llama-7b.gguf" \
    --device "cuda:0" \
    --port 50051
```

### Client Example (Rust)

```rust
use remotemedia_runtime_core::model_worker::ModelWorkerClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to worker
    let client = ModelWorkerClient::connect("grpc://localhost:50051").await?;
    
    // Create input tensor
    let input = TensorBuffer::from_vec(
        input_data,
        vec![1, 512],  // shape
        DataType::I32  // token IDs
    );
    
    // Submit inference (tensor sent via shared memory if available)
    let output = client.infer(input, Default::default()).await?;
    
    // Process output
    let tokens = output.as_i32_slice();
    
    Ok(())
}
```

### Client Example (Python)

```python
import asyncio
from remotemedia.core import ModelWorkerClient, TensorBuffer

async def main():
    # Connect to worker
    client = ModelWorkerClient("grpc://localhost:50051")
    await client.connect()
    
    # Prepare input
    input_ids = [101, 2023, 2003, 1037, 3231, 102]  # Token IDs
    input_tensor = TensorBuffer.from_numpy(
        np.array(input_ids, dtype=np.int32)
    )
    
    # Submit inference (uses shared memory automatically)
    output_tensor = await client.infer(input_tensor)
    
    # Get results
    output_tokens = output_tensor.to_numpy()
    print(f"Generated: {output_tokens}")
    
    await client.close()

asyncio.run(main())
```

## 4. Complete Pipeline Example

Combine all three features in a real pipeline.

```python
import asyncio
import numpy as np
from remotemedia.core import (
    get_or_load,
    ModelWorkerClient,
    TensorBuffer,
    SharedMemoryAllocator
)

async def audio_pipeline(audio_path: str):
    """Process audio through multiple models efficiently"""
    
    # 1. Load VAD model (process-local sharing)
    vad_model = get_or_load("silero-vad", load_vad_model)
    
    # 2. Load audio and detect speech (in-process)
    audio = load_audio(audio_path)
    speech_segments = vad_model.detect_speech(audio)
    
    # 3. Prepare audio chunks in shared memory
    allocator = SharedMemoryAllocator()
    chunks = []
    
    for segment in speech_segments:
        # Allocate in shared memory for zero-copy transfer
        chunk_tensor = allocator.allocate_tensor(
            size=segment.nbytes,
            session_id="audio-pipeline"
        )
        chunk_tensor.from_numpy(segment, zero_copy=True)
        chunks.append(chunk_tensor)
    
    # 4. Send to Whisper worker (GPU process)
    whisper_client = ModelWorkerClient("grpc://whisper-worker:50051")
    await whisper_client.connect()
    
    transcripts = []
    for chunk in chunks:
        # Zero-copy transfer via shared memory
        result = await whisper_client.infer(chunk)
        text = decode_whisper_output(result)
        transcripts.append(text)
    
    # 5. Send to LLM worker for processing
    llm_client = ModelWorkerClient("grpc://llm-worker:50052")
    await llm_client.connect()
    
    prompt = f"Summarize: {' '.join(transcripts)}"
    prompt_tensor = text_to_tensor(prompt)
    
    summary_tensor = await llm_client.infer(prompt_tensor)
    summary = decode_llm_output(summary_tensor)
    
    # Cleanup
    await whisper_client.close()
    await llm_client.close()
    
    return summary

# Run pipeline
result = asyncio.run(audio_pipeline("meeting.wav"))
print(f"Summary: {result}")
```

## Configuration

### Registry Configuration

```python
from remotemedia.core import RegistryConfig, EvictionPolicy

config = RegistryConfig(
    ttl_seconds=60.0,  # Evict after 60s idle
    max_memory_bytes=10 * 1024**3,  # 10GB limit
    eviction_policy=EvictionPolicy.LRU,
    enable_metrics=True
)

registry = ModelRegistry(config)
```

### Shared Memory Limits

```python
from remotemedia.core import AllocatorConfig

config = AllocatorConfig(
    max_memory_bytes=5 * 1024**3,  # 5GB total
    per_session_quota=512 * 1024**2,  # 512MB per session
    cleanup_interval_seconds=30.0
)

allocator = SharedMemoryAllocator(config)
```

## Monitoring

```python
# Get registry metrics
metrics = registry.metrics()
print(f"Cache hit rate: {metrics.hit_rate:.2%}")
print(f"Memory used: {metrics.total_memory_bytes / 1024**3:.1f}GB")
print(f"Models loaded: {metrics.total_models}")

# List loaded models
for model_info in registry.list_models():
    print(f"{model_info.model_id}: {model_info.memory_bytes / 1024**2:.1f}MB")
```

## Best Practices

1. **Start Simple**: Use process-local sharing first
2. **Profile Memory**: Monitor actual memory savings
3. **Handle Failures**: Always have fallback to serialization
4. **Clean Up**: Use context managers for automatic cleanup
5. **Batch Requests**: Aggregate requests to model workers
6. **Monitor Metrics**: Track cache hits and memory usage

## Troubleshooting

### Model Not Sharing
- Check that model IDs match exactly
- Verify registry is singleton (same instance)
- Look at reference counts in metrics

### Shared Memory Errors
- Check system limits: `ipcs -lm` (Linux)
- Verify cleanup is happening (no orphaned regions)
- Fall back to heap allocation if SHM unavailable

### Worker Connection Failed
- Verify worker process is running
- Check firewall/network settings
- Test health endpoint: `curl http://localhost:50051/health`

## Next Steps

- See [data-model.md](data-model.md) for detailed entity descriptions
- See [contracts/](contracts/) for full API specifications
- See [research.md](research.md) for design decisions and benchmarks
