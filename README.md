# RemoteMedia SDK

A high-performance SDK for building AI/ML processing pipelines with support for both native and browser (WASM) execution.

## 🚀 Live Browser Demo

**Try it now:** [https://matbeedotcom.github.io/remotemedia-sdk/](https://matbeedotcom.github.io/remotemedia-sdk/)

Execute AI/ML pipelines directly in your browser using WebAssembly! The demo supports:
- 🦀 **Rust-native nodes** (MultiplyNode, AddNode) via WASM
- 🐍 **Python nodes** (TextProcessorNode, DataTransformNode) via Pyodide
- 🔀 **Hybrid pipelines** mixing Rust and Python nodes
- 📦 **.rmpkg package format** for easy distribution

## Features

- **Multi-language execution**: Rust-native nodes and Python nodes (CPython via PyO3)
- **Browser-compatible**: Full WASM support with hybrid Rust + Pyodide runtime
- **Flexible architecture**: Build complex DAG pipelines with arbitrary node connections
- **High performance**: Zero-copy data transfer (native), async/await support
- **Package format**: Distribute pipelines as `.rmpkg` files (manifest + WASM + metadata)

## Architecture

### Native Execution

```
┌─────────────────────────────────────┐
│  RemoteMedia Runtime (Rust)         │
│  ├─ Async/Await (Tokio)             │
│  ├─ Rust Native Nodes               │
│  └─ CPython Nodes (PyO3)            │
│     └─ Zero-copy NumPy (rust-numpy) │
└─────────────────────────────────────┘
```

### Browser Execution (Hybrid WASM)

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

## Quick Start

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
├── runtime/                    # Rust runtime
│   ├── src/
│   │   ├── executor/          # Pipeline execution engine
│   │   ├── nodes/             # Built-in nodes (Multiply, Add, etc.)
│   │   ├── python/            # CPython integration (PyO3)
│   │   └── bin/
│   │       └── pipeline_executor_wasm.rs  # WASM entry point
│   └── Cargo.toml
├── python-client/              # Python SDK
│   └── remotemedia/
│       └── core/
│           └── node.py        # Base node class
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
    ├── WASM_EXECUTION.md      # WASM vs native execution
    ├── PYODIDE_IMPLEMENTATION.md  # Hybrid runtime details
    ├── BROWSER_PYTHON_SOLUTION.md # Python in browser
    └── RMPKG_FORMAT.md        # Package format spec
```

## Building

### Native Runtime

```bash
cd runtime
cargo build --release
```

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

## Performance

| Execution Mode | Pipeline Execution | Startup Time | Memory Usage |
|----------------|-------------------|--------------|--------------|
| **Native** | 1.0x (baseline) | <100ms | baseline |
| **WASM (wasmtime)** | 1.2-1.5x | ~500ms | +10-20% |
| **Browser (Rust nodes)** | <1ms/node | ~50ms (WASM load) | 50 MB |
| **Browser (Python nodes)** | 5-20ms/node | ~1.5s (Pyodide load, cached) | 90 MB |

## Current Status

### ✅ Phase 1: Local WASM Execution (Complete)
- WASM binary compilation
- PyO3 CPython embedding
- Synchronous execution path
- Python node compatibility

### ✅ Phase 2: Browser Integration (Complete)
- Hybrid Rust WASM + Pyodide runtime
- WASI I/O via @bjorn3/browser_wasi_shim
- .rmpkg package format
- Full browser demo with UI
- GitHub Pages deployment

### 🔜 Phase 3: Advanced Features (Planned)
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
