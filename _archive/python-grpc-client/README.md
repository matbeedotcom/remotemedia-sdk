# Python gRPC Client for Rust RemoteMedia Service

Modern Python client for the Rust gRPC service (003-rust-grpc-service).

## Features

- **ExecutePipeline**: Unary RPC for batch audio processing
- **GetVersion**: Service version and compatibility check
- **StreamPipeline**: Bidirectional streaming for real-time audio
- Type-safe proto-generated stubs
- Async/await support with asyncio

## Installation

```bash
# Install dependencies
pip install -r requirements.txt

# Generate proto stubs
python generate_protos.py
```

## Quick Start

```python
import asyncio
from remotemedia_client import RemoteMediaClient, AudioBuffer, AudioFormat

async def main():
    # Connect to service
    async with RemoteMediaClient("localhost:50051") as client:
        # Check version
        version = await client.get_version()
        print(f"Service version: {version.protocol_version}")
        
        # Create pipeline manifest
        manifest = {
            "version": "v1",
            "metadata": {
                "name": "simple_calculator",
                "description": "Test pipeline",
                "created_at": "2025-10-28T00:00:00Z"
            },
            "nodes": [
                {
                    "id": "calc",
                    "node_type": "CalculatorNode",
                    "params": "{\"operation\": \"add\", \"value\": 5.0}",
                    "is_streaming": False
                }
            ],
            "connections": []
        }
        
        # Execute pipeline
        result = await client.execute_pipeline(
            manifest=manifest,
            audio_inputs={},
            data_inputs={"calc": '{"value": 10.0}'}
        )
        
        print(f"Result: {result.data_outputs['calc']}")

asyncio.run(main())
```

## Streaming Example

```python
import asyncio
from remotemedia_client import RemoteMediaClient, AudioBuffer, AudioFormat
import struct

async def stream_audio():
    async with RemoteMediaClient("localhost:50051") as client:
        manifest = {
            "version": "v1",
            "metadata": {"name": "stream_test"},
            "nodes": [{"id": "source", "node_type": "PassThrough", "params": "{}"}],
            "connections": []
        }
        
        # Generate audio chunks
        async def audio_generator():
            for i in range(10):
                # Generate 1600 samples of sine wave
                samples = [struct.pack('<f', 0.5) for _ in range(1600)]
                buffer = AudioBuffer(
                    samples=b''.join(samples),
                    sample_rate=16000,
                    channels=1,
                    format=AudioFormat.F32,
                    num_samples=1600
                )
                yield ("source", buffer, i)
        
        # Stream pipeline
        async for result in client.stream_pipeline(manifest, audio_generator()):
            print(f"Chunk {result.sequence}: {result.processing_time_ms}ms")

asyncio.run(stream_audio())
```

## API Compatibility

This client is compatible with:
- Rust gRPC service v0.2.1+
- Protocol version: v1
- Features: 003-rust-grpc-service (Phases 1-5)

**Not compatible** with old Python RemoteExecutionService API.

## Architecture

```
python-grpc-client/
├── remotemedia_client.py     # Main client implementation
├── generate_protos.py        # Proto generation script
├── requirements.txt          # Dependencies
├── examples/                 # Usage examples
│   ├── simple_execution.py
│   └── streaming_audio.py
└── generated/                # Generated proto stubs (auto-created)
    ├── common_pb2.py
    ├── execution_pb2.py
    ├── execution_pb2_grpc.py
    ├── streaming_pb2.py
    └── streaming_pb2_grpc.py
```

## Testing

```bash
# Ensure Rust server is running
cd runtime
cargo run --bin grpc_server --features grpc-transport

# Run examples (in another terminal)
cd python-grpc-client
python examples/simple_execution.py
python examples/streaming_audio.py
```

## Development

### Regenerate Protos

When proto definitions change:

```bash
python generate_protos.py
```

### Type Hints

Generated stubs include full type hints for IDE support. Clean imports available:

```python
# Instead of common_pb2, execution_pb2, etc.
from generated import AudioBuffer, PipelineManifest, ExecuteRequest
from generated import PipelineExecutionServiceStub
```

## Performance

Measured against Rust server on localhost:

- **GetVersion**: ~1-2ms
- **ExecutePipeline**: ~3-5ms (simple operations)
- **StreamPipeline**: ~0.04ms per chunk

## Error Handling

```python
from remotemedia_client import RemoteMediaClient, RemoteMediaError

try:
    result = await client.execute_pipeline(manifest, {}, {})
except RemoteMediaError as e:
    print(f"Error type: {e.error_type}")
    print(f"Message: {e.message}")
    print(f"Node: {e.failing_node_id}")
    print(f"Context: {e.context}")
```

## License

Same as parent project (remotemedia-sdk).
