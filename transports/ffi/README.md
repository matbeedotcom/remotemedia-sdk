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

### Development (Editable Install)

For local development with editable python-client:

```bash
# 1. Install python-client as editable
cd python-client
pip install -e . --no-deps

# 2. Build and link Rust runtime
cd ../transports/ffi
./dev-install.sh
```

The `dev-install.sh` script:
- Builds the Rust extension with maturin
- Creates a symlink in python-client/remotemedia/
- Auto-updates when you rebuild

### Production Install

```bash
# Install python-client normally
pip install python-client/

# Install Rust runtime from wheel
pip install remotemedia_ffi-0.4.0-cp310-abi3-macosx_11_0_arm64.whl
```

Or build the wheel yourself:

```bash
cd transports/ffi
pip install maturin
maturin build --release --features extension-module
# Wheel will be in: ../../target/wheels/
```

## Usage

### Basic Pipeline Execution

```python
import asyncio
import json
from remotemedia.runtime import execute_pipeline, is_available

# Check if Rust runtime is available
if is_available():
    print("Using Rust-accelerated runtime")

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
from remotemedia.runtime import execute_pipeline_with_input

result = await execute_pipeline_with_input(
    manifest_json,
    input_data=["Hello, world!"],
    enable_metrics=True
)
```

### Zero-Copy Numpy Integration

**NEW: Automatic numpy array handling!** Just pass numpy arrays directly - no conversion functions needed!

```python
import numpy as np
from remotemedia.runtime import execute_pipeline_with_input

# Create audio frames (e.g., 20ms at 48kHz = 960 samples)
audio_frame = np.zeros(960, dtype=np.float32)

# Pass numpy array directly - automatically wrapped in RuntimeData::Numpy
result = await execute_pipeline_with_input(manifest_json, [audio_frame])

# Results are automatically converted back to numpy arrays
if isinstance(result, np.ndarray):
    print(f"Received numpy array: {result.shape}")
```

**How it works (zero-copy architecture):**

1. **Python → Rust FFI**: `python_to_runtime_data` detects numpy arrays and wraps them in `RuntimeData::Numpy` (zero-copy via buffer protocol)
2. **Rust Pipeline**: `RuntimeData::Numpy` flows through pipeline without conversion
3. **IPC Boundary**: `to_ipc_runtime_data` serializes **once** to iceoryx2 shared memory format
4. **Multiprocess Node**: Python receives data via zero-copy iceoryx2
5. **Return Path**: `from_ipc_runtime_data` deserializes back to `RuntimeData::Numpy`
6. **Rust → Python FFI**: `runtime_data_to_python` converts back to numpy array

**Performance for streaming audio:**
- **Before**: Serialize on every FFI call (50+ times/sec for 20ms frames) = ~50ms total overhead
- **After**: Serialize **once** at IPC boundary = ~1ms total overhead
- **Speedup**: ~50x reduction in serialization overhead for streaming pipelines!

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

### `numpy_to_audio_dict(arr: np.ndarray, sample_rate: int, channels: int) -> dict`

Convert a numpy array to an audio RuntimeData dictionary for use in pipelines.

**Parameters:**
- `arr`: Numpy array with `float32` dtype containing audio samples
- `sample_rate`: Audio sample rate in Hz (e.g., 16000, 44100, 48000)
- `channels`: Number of audio channels (1 for mono, 2 for stereo, etc.)

**Returns:** Dictionary with keys:
- `type`: "audio"
- `samples`: Audio sample data (list of float32)
- `sample_rate`: Sample rate in Hz
- `channels`: Number of channels

**Example:**
```python
import numpy as np
from remotemedia.runtime import numpy_to_audio_dict

# Create 1 second of 440Hz sine wave
t = np.linspace(0, 1, 48000, dtype=np.float32)
audio = np.sin(2 * np.pi * 440 * t)

# Convert to pipeline format
audio_dict = numpy_to_audio_dict(audio, sample_rate=48000, channels=1)

# Use in pipeline
result = await execute_pipeline_with_input(manifest, [audio_dict])
```

### `audio_dict_to_numpy(audio_dict: dict) -> np.ndarray`

Convert an audio RuntimeData dictionary back to a numpy array.

**Parameters:**
- `audio_dict`: Dictionary with keys: `samples`, `sample_rate`, `channels`

**Returns:** Numpy array with `float32` dtype. Shape is:
- 1D array `(samples,)` for mono audio
- 2D array `(frames, channels)` for multi-channel audio

**Example:**
```python
from remotemedia.runtime import audio_dict_to_numpy

# Get audio from pipeline result
result = await execute_pipeline_with_input(manifest, [audio_dict])

if result.get("type") == "audio":
    # Convert back to numpy for processing
    audio_array = audio_dict_to_numpy(result)
    
    # Now you can use numpy operations
    max_amplitude = np.max(np.abs(audio_array))
    print(f"Max amplitude: {max_amplitude}")
```

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
from remotemedia.runtime import execute_pipeline  # Same API
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
