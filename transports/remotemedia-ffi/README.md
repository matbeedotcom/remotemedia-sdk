# RemoteMedia FFI Transport

Python FFI (Foreign Function Interface) transport for RemoteMedia pipelines, providing fast Rust-accelerated pipeline execution for Python applications.

## Overview

This crate provides Python bindings to the `remotemedia-runtime-core`, enabling:
- **Fast execution**: Native Rust performance for media processing
- **Zero-copy**: Direct numpy array integration for audio/video data
- **Async support**: Full Python asyncio integration via PyO3
- **Independent deployment**: Can be updated without touching runtime-core

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Python Application                                 │
│  ↓                                                   │
│  remotemedia_ffi (PyO3 module)                      │
│  ├─ execute_pipeline()                              │
│  ├─ execute_pipeline_with_input()                   │
│  └─ marshal.py ↔ RuntimeData conversion             │
│     ↓                                                │
│  remotemedia-runtime-core (PipelineRunner)          │
│  ├─ Transport-agnostic execution                    │
│  ├─ Node registry                                   │
│  └─ Audio/video processing                          │
└─────────────────────────────────────────────────────┘
```

## Installation

### From Source

```bash
cd transports/remotemedia-ffi
pip install maturin
maturin develop --release
```

### For Python Package

```bash
# In python-client/
pip install -e .
```

## Usage

### Basic Pipeline Execution

```python
import asyncio
from remotemedia_ffi import execute_pipeline

async def main():
    manifest = {
        "version": "v1",
        "metadata": {"name": "audio_processing"},
        "nodes": [
            {
                "id": "resample",
                "node_type": "AudioResample",
                "params": {"target_rate": 16000}
            }
        ],
        "connections": []
    }

    manifest_json = json.dumps(manifest)
    result = await execute_pipeline(manifest_json)
    print(result)

asyncio.run(main())
```

### With Input Data

```python
result = await execute_pipeline_with_input(
    manifest_json,
    input_data=["Hello, world!"],
    enable_metrics=True
)
```

### Zero-Copy Numpy Integration

```python
import numpy as np
from remotemedia_ffi import numpy_to_audio

# Convert numpy array to audio data (zero-copy)
audio_samples = np.random.randn(16000).astype(np.float32)
audio_data = numpy_to_audio(audio_samples, sample_rate=16000, channels=1)

# Process through pipeline
result = await execute_pipeline_with_input(manifest_json, [audio_data])
```

## API Reference

### `execute_pipeline(manifest_json: str, enable_metrics: bool = False) -> Any`

Execute a pipeline from a JSON manifest.

**Parameters:**
- `manifest_json`: JSON string containing pipeline definition
- `enable_metrics`: If True, include execution metrics in response

**Returns:** Pipeline execution results (format depends on pipeline output)

### `execute_pipeline_with_input(manifest_json: str, input_data: List[Any], enable_metrics: bool = False) -> Any`

Execute a pipeline with input data.

**Parameters:**
- `manifest_json`: JSON string containing pipeline definition
- `input_data`: List of input items to process
- `enable_metrics`: If True, include execution metrics in response

**Returns:** Pipeline execution results

### `get_runtime_version() -> str`

Get the version of the FFI transport.

### `is_available() -> bool`

Check if Rust runtime is available (always returns `True`).

## Development

### Building

```bash
# Debug build
maturin develop

# Release build with optimizations
maturin develop --release

# Build wheel
maturin build --release
```

### Testing

```bash
# Run Rust tests
cargo test

# Run Python tests
pytest python/tests/
```

### Type Stubs

For better IDE support, generate type stubs:

```bash
maturin develop --release
stubgen -p remotemedia_ffi -o stubs/
```

## Performance Benefits

Compared to pure Python execution:
- **Audio processing**: 2-16x faster (depending on operation)
- **VAD (Voice Activity Detection)**: 8-12x faster
- **Resampling**: 4-6x faster
- **Zero-copy**: No data copying between Python and Rust

## Migration from v0.3

```python
# OLD (v0.3.x):
from remotemedia_runtime import execute_pipeline

# NEW (v0.4.x):
from remotemedia_ffi import execute_pipeline  # Same API
```

The API remains the same, but execution now goes through the decoupled `PipelineRunner`.

## Environment Variables

- `RUST_LOG`: Control logging level (default: "info")
  ```bash
  RUST_LOG=debug python my_app.py
  ```

## Troubleshooting

### Import Error

If you see `ModuleNotFoundError: No module named 'remotemedia_ffi'`:
1. Ensure maturin is installed: `pip install maturin`
2. Build the module: `maturin develop --release`
3. Check Python can find the module: `python -c "import remotemedia_ffi; print(remotemedia_ffi.__version__)"`

### Performance Issues

For maximum performance:
1. Use release builds: `maturin develop --release`
2. Enable CPU optimizations: `RUSTFLAGS="-C target-cpu=native" maturin develop --release`
3. Use zero-copy numpy integration where possible

## See Also

- [Runtime Core](../../runtime-core/README.md) - Core execution engine
- [gRPC Transport](../remotemedia-grpc/README.md) - gRPC service transport
- [Python Client](../../python-client/README.md) - Python SDK documentation
- [Transport Decoupling Spec](../../specs/003-transport-decoupling/spec.md) - Architecture details
