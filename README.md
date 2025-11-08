# RemoteMedia SDK

A high-performance SDK for building AI/ML processing pipelines with **native Rust acceleration** and browser (WASM) execution support.

## What's New in v0.2.1 🎉

**Code Cleanup & Performance Maintained**
- 📦 **54% Less Code**: 50K → 23K lines (archived WASM/browser runtime, NodeExecutor adapter)
- ⚡ **62x Speedup Maintained**: Audio preprocessing remains blazingly fast
- 🎯 **Zero Breaking Changes**: All existing code continues to work
- 🚀 **WebRTC Improved**: Real-time audio latency reduced from 380ms to <10ms
- 📚 **New Documentation**: [Archival Guide](docs/ARCHIVAL_GUIDE.md) for component restoration

See [CHANGELOG.md](CHANGELOG.md) for full details.

## 🚀 Key Features

### Native Rust Acceleration ⚡
- **2-16x faster audio processing** with automatic fallback to Python
- **Built-in metrics** with 29μs overhead (microsecond precision tracking)
- **Transparent runtime selection** - zero code changes, automatic Rust/Python detection
- **Zero-copy data transfer** via rust-numpy (PyO3)
- **Sub-microsecond FFI overhead** for maximum throughput

### Browser-First Execution 🌐
**Live Demo:** [https://matbeedotcom.github.io/remotemedia-sdk/](https://matbeedotcom.github.io/remotemedia-sdk/)

- 🦀 **Rust-native nodes** (MultiplyNode, AddNode) via WASM
- 🐍 **Python nodes** (TextProcessorNode, DataTransformNode) via Pyodide
- 🔀 **Hybrid pipelines** mixing Rust and Python nodes
- 📦 **.rmpkg package format** for easy distribution

### Production-Ready Features
- **Reliable execution**: Exponential backoff retry, circuit breaker (5-failure threshold)
- **Flexible architecture**: Build complex DAG pipelines with arbitrary node connections
- **Async/await**: Non-blocking execution with Tokio runtime
- **Multi-language**: Rust-native nodes and Python nodes (CPython via PyO3)

## Performance Benchmarks

| Feature | Python Baseline | Rust Acceleration | Speedup |
|---------|----------------|-------------------|---------|
| **Audio Resampling** | 344.89ms | 2.84ms | **121.5x faster** ✅ |
| **VAD Processing** | 2.02ms | 2.31ms | 0.87x (Python competitive) |
| **Format Conversion** | 0.32ms | 0.39ms | 0.81x (Python competitive) |
| **Full Audio Pipeline** | 347.26ms | 5.58ms | **62.2x faster** ✅ |
| **Memory Usage** | 141.4 MB | 1.3 MB | **107x less** ✅ |
| **Fast Path vs Standard** | - | 16.3x faster | vs JSON nodes |
| **FFI Overhead** | - | <1μs | Zero-copy transfers |
| **Metrics Overhead** | - | 29μs | 71% under target |

**Runtime Selection**: Automatic detection with graceful Python fallback when Rust unavailable.

## Architecture

### Native Execution with Rust Acceleration

```
┌─────────────────────────────────────────┐
│  Python Application                     │
│  └─ RemoteMedia SDK                     │
│     └─ Runtime Detection                │
│        ├─ Rust Runtime (if available) ✅│
│        │  ├─ FFI Layer (<1μs overhead) │
│        │  ├─ Zero-Copy Transfers       │
│        │  ├─ Built-in Metrics (29μs)   │
│        │  ├─ Async/Await (Tokio)       │
│        │  └─ Rust Native Nodes         │
│        │     └─ Audio: 2-16x faster    │
│        └─ Python Fallback (automatic) 🔄│
│           └─ Pure Python Nodes          │
└─────────────────────────────────────────┘
```

**Features:**
- Automatic Rust/Python runtime selection
- Zero code changes for migration
- Graceful degradation when Rust unavailable
- 15/15 compatibility tests passing

### Browser Execution (WASM)

```
┌──────────────────────────────────────┐
│  Browser (TypeScript)                │
│  ├─ PipelineRunner                   │
│  │   ├─ Rust WASM (~20MB)            │
│  │   │   └─ Rust Nodes               │
│  │   └─ Pyodide WASM (~40MB, cached) │
│  │       └─ Python Nodes             │
│  └─ Package Loader (.rmpkg)          │
└──────────────────────────────────────┘
```

**Try the live demo:** [https://matbeedotcom.github.io/remotemedia-sdk/](https://matbeedotcom.github.io/remotemedia-sdk/)

## Quick Start

### Python SDK with Native Rust Acceleration

**Installation:**
```bash
# Install Python SDK
cd python-client
pip install -e .

# Build Rust runtime (optional - automatic fallback if not built)
cd ../runtime
cargo build --release
```

**Basic Usage:**
```python
from remotemedia import Pipeline

# Create pipeline - automatically uses Rust if available
pipeline = Pipeline.from_yaml("audio_pipeline.yaml")

# Execute with automatic runtime selection
result = await pipeline.run({"audio": audio_data})
```

**With Performance Metrics:**
```python
# Enable built-in metrics (29μs overhead)
pipeline = Pipeline.from_yaml("audio_pipeline.yaml", enable_metrics=True)
result = await pipeline.run({"audio": audio_data})

# Get detailed performance data
metrics = pipeline.get_metrics()
print(f"Total duration: {metrics['total_duration_us']}μs")
print(f"Per-node metrics: {metrics['node_metrics']}")
```

**Runtime Detection:**
```python
from remotemedia import is_rust_runtime_available

if is_rust_runtime_available():
    print("✅ Using Rust acceleration (2-16x faster)")
else:
    print("🔄 Using Python fallback (still works!)")
```

## Examples

### Audio Processing with Rust Acceleration

```python
# examples/audio_pipeline.py
from remotemedia import Pipeline
import numpy as np

# Create audio pipeline (automatically uses Rust if available)
pipeline = Pipeline.from_yaml("configs/audio_processing.yaml", enable_metrics=True)

# Process audio with 2-16x speedup
audio_data = np.random.randn(16000)  # 1 second at 16kHz
result = await pipeline.run({"audio": audio_data})

# Get performance metrics
metrics = pipeline.get_metrics()
print(f"Processing time: {metrics['total_duration_us']}μs")
print(f"Nodes executed: {len(metrics['node_metrics'])}")
```

### Performance Comparison

```python
# examples/benchmark_rust_vs_python.py
from remotemedia import Pipeline, is_rust_runtime_available
import time

async def benchmark():
    # Force Python runtime
    pipeline_python = Pipeline.from_yaml("audio.yaml", runtime_hint="python")
    start = time.perf_counter()
    result_python = await pipeline_python.run({"audio": audio_data})
    python_time = time.perf_counter() - start

    # Force Rust runtime (if available)
    if is_rust_runtime_available():
        pipeline_rust = Pipeline.from_yaml("audio.yaml", runtime_hint="rust")
        start = time.perf_counter()
        result_rust = await pipeline_rust.run({"audio": audio_data})
        rust_time = time.perf_counter() - start
        
        print(f"Python: {python_time*1000:.2f}ms")
        print(f"Rust:   {rust_time*1000:.2f}ms")
        print(f"Speedup: {python_time/rust_time:.2f}x")
```

### Native Runtime

```rust
use remotemedia_runtime::executor::Executor;
use remotemedia_runtime::manifest::Manifest;

#[tokio::main]
async fn main() -> Result<()> {
    let manifest = Manifest::from_file("pipeline.json")?;
    let executor = Executor::new();
    let result = executor.execute(&manifest).await?;
    println!("Result: {:?}", result);
    Ok(())
}
```

### Browser Runtime

```typescript
import { PipelineRunner } from './pipeline-runner';
import { PackageLoader } from './package-loader';

// Load .rmpkg package
const pkg = await PackageLoader.loadFromFile(file);
const runner = new PipelineRunner();

// Load WASM runtime
await runner.loadWasm(pkg.wasmBinary);

// Execute pipeline
const { result } = await runner.execute(pkg.manifest, inputData);
console.log(result);
```

## Project Structure

```
remotemedia-sdk/
├── runtime/                    # Rust runtime with native acceleration
│   ├── src/
│   │   ├── executor/          # Pipeline orchestration (Tokio async)
│   │   ├── nodes/             # Rust-native nodes (audio: resample, VAD)
│   │   ├── python/            # PyO3 FFI bindings (<1μs overhead)
│   │   └── bin/
│   │       └── pipeline_executor_wasm.rs  # WASM entry point
│   ├── tests/                 # Unit & performance tests
│   └── Cargo.toml
├── python-client/              # Python SDK
│   ├── remotemedia/
│   │   ├── core/              # Pipeline, Node base classes
│   │   ├── nodes/             # Python node implementations
│   │   └── __init__.py        # Runtime detection & selection
│   └── tests/
│       └── test_rust_compatibility.py  # 15 compatibility tests
├── examples/                   # Example pipelines
│   ├── audio_pipeline.py      # Audio processing examples
│   ├── rust_runtime/          # 11 Rust acceleration examples
│   └── ...
├── browser-demo/               # Browser demo application
│   ├── src/
│   │   ├── main.ts            # Demo UI
│   │   ├── pipeline-runner.ts # Hybrid WASM executor
│   │   ├── python-executor.ts # Pyodide integration
│   │   └── package-loader.ts  # .rmpkg loader
│   ├── scripts/
│   │   ├── create-package.js  # Package creation tool
│   │   └── test-package.js    # Package validation tool
│   └── examples/              # Example .rmpkg manifests
└── docs/                       # Documentation
    ├── NATIVE_ACCELERATION.md     # Rust acceleration architecture
    ├── PERFORMANCE_TUNING.md      # Optimization strategies
    ├── MIGRATION_GUIDE.md         # v0.1.x → v0.2.0 upgrade
    ├── WASM_EXECUTION.md          # WASM vs native execution
    ├── PYODIDE_IMPLEMENTATION.md  # Hybrid runtime details
    ├── BROWSER_PYTHON_SOLUTION.md # Python in browser
    └── RMPKG_FORMAT.md            # Package format spec
```

## Building

> **📖 Build Guide**: See [QUICK_BUILD_REFERENCE.md](QUICK_BUILD_REFERENCE.md) for fast build commands or [BUILD_CONFIGURATION.md](BUILD_CONFIGURATION.md) for comprehensive documentation.

### Quick Start

**gRPC Server (most common)**:
```bash
cargo run --bin grpc-server --release
```

**gRPC Server + WebRTC Signaling**:
```bash
cargo run --bin grpc-server --release --features webrtc-signaling
```

### Native Runtime with Rust Acceleration

```bash
cd runtime
cargo build --release
```

The compiled library will be automatically detected by the Python SDK. If not built, the SDK gracefully falls back to pure Python execution.

**Build Output:**
- Linux: `runtime/target/release/libremotemedia_runtime.so`
- macOS: `runtime/target/release/libremotemedia_runtime.dylib`
- Windows: `runtime/target/release/remotemedia_runtime.dll`

### Modular Transports

RemoteMedia SDK uses a **modular transport architecture** where each transport is optional:

```bash
# Core library only (no transports)
cargo build -p remotemedia-runtime-core --no-default-features

# gRPC transport only
cargo build -p remotemedia-grpc

# Python FFI transport only
cargo build -p remotemedia-ffi --features extension-module

# WebRTC transport only (when available)
cargo build -p remotemedia-webrtc --features full
```

See [BUILD_CONFIGURATION.md](BUILD_CONFIGURATION.md) for all build options.

### WASM Runtime

```bash
cd runtime
rustup target add wasm32-wasip1
cargo build --target wasm32-wasip1 \
  --bin pipeline_executor_wasm \
  --no-default-features \
  --features wasm \
  --release
```

Output: `runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm` (~20 MB)

### Browser Demo

```bash
cd browser-demo
npm install
npm run dev         # Development server
npm run build       # Production build
```

## Creating Packages

Create distributable `.rmpkg` packages:

```bash
cd browser-demo
npm run package -- \
  --manifest examples/calculator.rmpkg.json \
  --output calculator.rmpkg
```

Validate packages:

```bash
npm run test-package -- calculator.rmpkg
```

## Documentation

### Native Rust Acceleration
- **[Native Acceleration Guide](docs/NATIVE_ACCELERATION.md)** - Architecture, FFI, and data flow
- **[Performance Tuning](docs/PERFORMANCE_TUNING.md)** - Optimization strategies and benchmarks
- **[Migration Guide](docs/MIGRATION_GUIDE.md)** - Upgrading from v0.1.x to v0.2.0

### Browser Execution
- **[WASM Execution Guide](docs/WASM_EXECUTION.md)** - Native vs WASM execution differences
- **[Pyodide Implementation](docs/PYODIDE_IMPLEMENTATION.md)** - Hybrid browser runtime architecture
- **[.rmpkg Format Specification](docs/RMPKG_FORMAT.md)** - Package format details
- **[Browser Demo README](browser-demo/README.md)** - Demo usage and examples

## Examples

### Calculator Pipeline (Rust Nodes)

```json
{
  "version": "v1",
  "metadata": { "name": "calculator" },
  "runtime": { "target": "wasm32-wasi" },
  "nodes": [
    { "id": "multiply", "node_type": "MultiplyNode", "params": { "multiplier": 2 } },
    { "id": "add", "node_type": "AddNode", "params": { "addend": 10 } }
  ],
  "connections": [
    { "from": "multiply", "to": "add" }
  ]
}
```

**Input:** `[5, 7, 3]`
**Output:** `[20, 24, 16]` (5×2+10=20, 7×2+10=24, 3×2+10=16)

### Text Processor (Python Node)

```json
{
  "version": "v1",
  "metadata": { "name": "text-processor" },
  "runtime": { "target": "wasm32-wasi", "features": ["python"] },
  "nodes": [
    { "id": "text1", "node_type": "TextProcessorNode", "params": {} }
  ],
  "connections": []
}
```

**Input:** `[{"text": "Hello WASM", "operations": ["uppercase", "word_count"]}]`

## Performance Comparison

### Native Execution Modes

| Execution Mode | Pipeline Execution | Startup Time | Memory Usage | Notes |
|----------------|-------------------|--------------|--------------|-------|
| **Native Rust** | 0.44ms (audio) | <100ms | baseline | 2-16x faster ✅ |
| **Native Python** | 0.72ms (audio) | <100ms | baseline | Automatic fallback |
| **WASM (wasmtime)** | 1.2-1.5x slower | ~500ms | +10-20% | Server-side only |

### Browser Execution

| Execution Mode | Per-Node Time | Startup Time | Memory Usage |
|----------------|---------------|--------------|--------------|
| **Browser (Rust nodes)** | <1ms/node | ~50ms (WASM load) | 50 MB |
| **Browser (Python nodes)** | 5-20ms/node | ~1.5s (Pyodide, cached) | 90 MB |

## Current Status

### ✅ Native Rust Acceleration (Complete - v0.2.0)

**Phase 1-5: Foundation & Audio Performance**
- Zero-copy data transfer via rust-numpy (PyO3)
- Audio node acceleration: Resample (1.25x), VAD (2.79x), Format conversion
- Fast path execution (16.3x faster than standard nodes)

**Phase 6: Reliable Production Execution**
- Exponential backoff retry with configurable attempts
- Circuit breaker with 5-failure threshold
- Error classification and handling

**Phase 7: Performance Monitoring**
- Built-in metrics with 29μs overhead (71% under 100μs target)
- Microsecond precision tracking
- Per-node execution metrics
- JSON export via FFI

**Phase 8: Runtime Selection Transparency**
- Automatic Rust/Python runtime detection
- Graceful fallback with zero code changes
- 15/15 compatibility tests passing
- Warning system when Rust unavailable

### ✅ Browser Integration (Complete)
- WASM binary compilation
- PyO3 CPython embedding
- Synchronous execution path
- Hybrid Rust WASM + Pyodide runtime
- WASI I/O via @bjorn3/browser_wasi_shim
- .rmpkg package format
- Full browser demo with UI
- GitHub Pages deployment

### 🔜 Advanced Features (Planned)
- Whisper.cpp WASM integration for audio transcription
- Service worker for WASM caching
- WebGPU acceleration for ML models

## Contributing

This project uses [OpenSpec](openspec/) for planning and tracking major changes. See [AGENTS.md](openspec/AGENTS.md) for details.

## License

[Add your license here]

## Links

- **Browser Demo**: [https://matbeedotcom.github.io/remotemedia-sdk/](https://matbeedotcom.github.io/remotemedia-sdk/)
- **GitHub Repository**: [https://github.com/matbeedotcom/remotemedia-sdk](https://github.com/matbeedotcom/remotemedia-sdk)
