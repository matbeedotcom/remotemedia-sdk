# WASM Browser Demo Implementation Plan

## Goal
Compile the RemoteMedia Rust runtime (with PyO3 + CPython) to `wasm32-wasi` target to run Pipelines in the browser, following the VMware Labs `wasi-py-rs-pyo3` example.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Browser Environment                       │
│  ┌───────────────────────────────────────────────────────┐  │
│  │         JavaScript/TypeScript Frontend                │  │
│  │  - Load .rmpkg (WASM + manifest + models)            │  │
│  │  - Instantiate Wasmtime/Wasmer WASM runtime          │  │
│  │  - Call pipeline.execute(manifest_json)               │  │
│  └───────────────┬───────────────────────────────────────┘  │
│                  │                                           │
│  ┌───────────────▼───────────────────────────────────────┐  │
│  │         remotemedia_runtime.wasm                      │  │
│  │  ┌─────────────────────────────────────────────────┐  │  │
│  │  │  PyO3 + libpython3.11.a (static-linked)         │  │  │
│  │  │  - Python::with_gil() for GIL management        │  │  │
│  │  │  - Execute Python node code in CPython          │  │  │
│  │  └─────────────────────────────────────────────────┘  │  │
│  │  ┌─────────────────────────────────────────────────┐  │  │
│  │  │  Rust Runtime Core                              │  │  │
│  │  │  - Executor: Pipeline orchestration             │  │  │
│  │  │  - Manifest parser (serde_json)                 │  │  │
│  │  │  - RustPython VM (fallback for pure Python)     │  │  │
│  │  │  - Rust-native nodes (MultiplyNode, AddNode)    │  │  │
│  │  └─────────────────────────────────────────────────┘  │  │
│  │  ┌─────────────────────────────────────────────────┐  │  │
│  │  │  WASI Layer                                     │  │  │
│  │  │  - File I/O via preopen directories            │  │  │
│  │  │  - stdout/stderr for logging                    │  │  │
│  │  │  - No network access (browser provides data)    │  │  │
│  │  └─────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Current State Analysis

### ✅ What Works (Native)
- **PyO3 0.26 FFI**: `execute_pipeline()` and `execute_pipeline_with_input()` (runtime/src/python/ffi.rs)
- **Data marshaling**: Python ↔ Rust JSON conversion (runtime/src/python/marshal.rs)
- **Numpy arrays**: Zero-copy via rust-numpy for PyO3 FFI (runtime/src/python/numpy_marshal.rs)
- **RustPython VM**: Embedded Python interpreter (runtime/src/python/vm.rs)
- **CPython executor**: Native Python node execution via PyO3 (runtime/src/python/cpython_executor.rs)
- **Rust-native nodes**: MultiplyNode, AddNode, RustWhisperNode
- **Async runtime**: Tokio-based async execution

### ❌ What Won't Work in WASM (Needs Adaptation)

#### 1. **rust-numpy dependency**
   - **Problem**: Requires CPython C API extensions not available in static libpython
   - **Solution**: Use JSON/base64 serialization for numpy arrays (already implemented in RustPython path!)
   - **File**: `runtime/src/python/numpy_marshal.rs` - add WASM-specific serialization

#### 2. **Tokio async runtime**
   - **Problem**: WASM doesn't support native threads/async in the same way
   - **Solution**: Options:
     - Use `wasm-bindgen-futures` for browser async
     - Use `async-std` with WASI support
     - Simplify to sync execution for initial demo
   - **Impact**: `executor/mod.rs` needs WASM-compatible async

#### 3. **rwhisper (whisper.cpp)**
   - **Problem**: Uses C++ FFI, requires native compilation
   - **Solution**: Phase 2 - compile whisper.cpp to WASM separately
   - **Workaround**: Disable whisper feature for initial demo, use simpler nodes

#### 4. **File I/O for models**
   - **Problem**: No direct filesystem access in browser
   - **Solution**:
     - Bundle models in .rmpkg
     - Use WASI preopen directories (wasmtime --mapdir)
     - Load from IndexedDB via JavaScript interop

## Implementation Steps

### Phase 1: Minimal WASM Binary (No Whisper)

**Goal**: Get a simple Pipeline (MultiplyNode → AddNode) running in WASM

#### Step 1.1: Add wlr-libpy dependency

**File**: `runtime/Cargo.toml`

```toml
[dependencies]
# Existing PyO3 - update for WASM compatibility with Python 3.12
pyo3 = { version = "0.26", features = ["abi3-py312"] }  # Match wlr-libpy py312 (local Python 3.12.3)

# WASM-specific: Add wlr-libpy for CPython in WASM
[target.'cfg(target_family = "wasm")'.dependencies]
wlr-libpy = {
    git = "https://github.com/vmware-labs/webassembly-language-runtimes.git",
    default-features = false,
    features = ["py_main", "py312"]  # Use Python 3.12
}

[build-dependencies]
# Existing protobuf build
prost-build = "0.12"
tonic-build = "0.10"

# WASM-specific: wlr-libpy build support with Python 3.12
[target.'cfg(target_family = "wasm")'.build-dependencies]
wlr-libpy = {
    git = "https://github.com/vmware-labs/webassembly-language-runtimes.git",
    default-features = false,
    features = ["build", "py312"]  # Use Python 3.12
}
```

#### Step 1.2: Create build.rs

**File**: `runtime/build.rs`

```rust
fn main() {
    // Existing protobuf build
    build_protos();

    // WASM-specific: Configure static Python libraries
    #[cfg(target_family = "wasm")]
    configure_wasm_libs();
}

fn build_protos() {
    tonic_build::configure()
        .build_server(true)
        .compile(
            &["../remotemedia/protos/execution.proto"],
            &["../remotemedia/protos/"],
        )
        .unwrap();
}

#[cfg(target_family = "wasm")]
fn configure_wasm_libs() {
    use wlr_libpy::bld_cfg::configure_static_libs;
    configure_static_libs().unwrap().emit_link_flags();
}
```

#### Step 1.3: Create WASM binary entry point

**File**: `runtime/src/bin/pipeline_executor_wasm.rs`

```rust
//! WASM binary for executing RemoteMedia pipelines in browser
//!
//! This binary embeds CPython via libpython3.11.a and provides
//! a WASI Command interface for running pipelines.

use pyo3::prelude::*;
use remotemedia_runtime::{executor::Executor, manifest::parse};

fn main() -> PyResult<()> {
    // Initialize PyO3 for WASM (prepare_freethreaded_python)
    pyo3::prepare_freethreaded_python();

    // Read manifest from stdin or args
    let manifest_json = read_manifest_input();

    // Execute pipeline
    Python::with_gil(|py| {
        execute_pipeline_wasm(py, &manifest_json)
    })
}

fn execute_pipeline_wasm(py: Python<'_>, manifest_json: &str) -> PyResult<()> {
    // Parse manifest
    let manifest = parse(manifest_json)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
            format!("Failed to parse manifest: {}", e)
        ))?;

    // Create executor
    let executor = Executor::new();

    // Execute synchronously (no tokio in WASM for now)
    // TODO: Use wasm-bindgen-futures for async
    let result = executor.execute_sync(&manifest)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            format!("Execution failed: {}", e)
        ))?;

    // Print results to stdout (WASI stdio)
    println!("{}", serde_json::to_string_pretty(&result.outputs).unwrap());

    Ok(())
}

fn read_manifest_input() -> String {
    // Read from stdin or command-line args
    use std::io::{self, Read};
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap();
    buffer
}
```

#### Step 1.4: Add synchronous executor path

**File**: `runtime/src/executor/mod.rs`

```rust
impl Executor {
    // Existing async method
    pub async fn execute(&self, manifest: &Manifest) -> Result<ExecutionResult> {
        // ...existing async implementation
    }

    // NEW: Synchronous execution for WASM
    #[cfg(target_family = "wasm")]
    pub fn execute_sync(&self, manifest: &Manifest) -> Result<ExecutionResult> {
        // Use futures executor to run async code synchronously
        use futures::executor::block_on;
        block_on(self.execute(manifest))
    }
}
```

#### Step 1.5: Fix numpy marshaling for WASM

**File**: `runtime/src/python/numpy_marshal.rs`

```rust
// Add conditional compilation
#[cfg(not(target_family = "wasm"))]
use numpy::{PyArray1, PyArrayDyn};

// Existing zero-copy path for native
#[cfg(not(target_family = "wasm"))]
pub fn numpy_to_json(py: Python, array: &PyAny) -> PyResult<serde_json::Value> {
    // ... existing rust-numpy zero-copy implementation
}

// NEW: Base64 serialization for WASM (matches RustPython approach)
#[cfg(target_family = "wasm")]
pub fn numpy_to_json(py: Python, array: &PyAny) -> PyResult<serde_json::Value> {
    // Use Python's numpy.ndarray.tobytes() + base64 encoding
    use pyo3::types::PyBytes;

    let tobytes = array.call_method0("tobytes")?;
    let bytes: &PyBytes = tobytes.downcast()?;
    let encoded = base64::encode(bytes.as_bytes());

    let dtype: String = array.getattr("dtype")?.getattr("name")?.extract()?;
    let shape: Vec<usize> = array.getattr("shape")?.extract()?;

    Ok(serde_json::json!({
        "__numpy__": true,
        "array": {
            "meta": {
                "dtype": dtype,
                "shape": shape,
            },
            "data": encoded,
        }
    }))
}

// Similar approach for json_to_numpy
#[cfg(target_family = "wasm")]
pub fn json_to_numpy(py: Python, value: &serde_json::Value) -> PyResult<Py<PyAny>> {
    // Deserialize base64 → create numpy array via np.frombuffer()
    // ... implementation matches whisper.rs extract_from_numpy_format logic
}
```

#### Step 1.6: Build for wasm32-wasi

**Commands**:
```bash
# Install target
rustup target add wasm32-wasi

# Build WASM binary
cd runtime
cargo build --target wasm32-wasi --bin pipeline_executor_wasm --release

# Output: target/wasm32-wasi/release/pipeline_executor_wasm.wasm
```

#### Step 1.7: Test locally with wasmtime

```bash
# Install wasmtime
curl https://wasmtime.dev/install.sh -sSf | bash

# Create test manifest
cat > test_manifest.json <<EOF
{
  "version": "v1",
  "nodes": [
    {"id": "multiply", "type": "MultiplyNode", "params": {"factor": 2}},
    {"id": "add", "type": "AddNode", "params": {"value": 10}}
  ],
  "edges": [
    {"from": "multiply", "to": "add"}
  ]
}
EOF

# Run with wasmtime (map /usr for libpython deps)
wasmtime \
  --mapdir /usr::target/wasm32-wasi/wasi-deps/usr \
  target/wasm32-wasi/release/pipeline_executor_wasm.wasm \
  < test_manifest.json
```

**Expected output**:
```json
{
  "outputs": [12, 14, 16],  // (1*2)+10, (2*2)+10, (3*2)+10
  "execution_time_ms": 45
}
```

### Phase 2: Browser Integration

#### Step 2.1: Choose browser WASM runtime

**Options**:
1. **@wasmer/sdk** (wasmer.io) - Full WASI support, IndexedDB filesystem
2. **wasmtime-js** - Official Wasmtime bindings for browser
3. **wasm-workers-server** - Cloudflare Workers-style runtime

**Recommended**: `@wasmer/sdk` for better WASI filesystem support

#### Step 2.2: Create browser runner

**File**: `browser-demo/src/pipeline-runner.ts`

```typescript
import { init, Wasmer } from "@wasmer/sdk";

export async function runPipeline(manifestJson: string, wasmModule: Uint8Array) {
  // Initialize Wasmer
  await init();

  // Create WASI instance with preopen directories
  const wasmer = await Wasmer.fromFile(wasmModule);

  // Map /usr directory for libpython deps
  const instance = await wasmer.instantiate({
    mapDirs: {
      "/usr": "/wasi-deps/usr",
    },
  });

  // Pass manifest via stdin
  const stdin = new TextEncoder().encode(manifestJson);
  instance.setStdin(stdin);

  // Run WASI command (_start)
  await instance.start();

  // Read results from stdout
  const stdout = instance.getStdout();
  const result = JSON.parse(new TextDecoder().decode(stdout));

  return result;
}
```

#### Step 2.3: Bundle as .rmpkg

**Structure**:
```
voice_pipeline_v1.rmpkg/
├── manifest.json              # Pipeline definition
├── modules/
│   └── remotemedia_runtime.wasm  # Compiled WASM binary
├── wasi-deps/
│   └── usr/                   # libpython + wasi-sysroot
├── models/
│   └── whisper-tiny.ggml      # (Phase 3: Whisper WASM)
└── meta/
    ├── provenance.json        # Build metadata
    └── signature.sig          # GPG signature
```

### Phase 3: Whisper in WASM (Advanced)

#### Challenge: whisper.cpp → WASM

**Option A: Emscripten compilation**
```bash
# Clone whisper.cpp
git clone https://github.com/ggerganov/whisper.cpp
cd whisper.cpp

# Build with Emscripten
emcc -O3 -s WASM=1 -s EXPORTED_FUNCTIONS='["_whisper_init"]' \
  whisper.cpp -o whisper.wasm
```

**Option B: Use existing whisper.wasm**
- Project: https://github.com/ggerganov/whisper.cpp/tree/master/examples/whisper.wasm
- Already provides browser-ready Whisper

**Integration**:
```rust
// In RustWhisperNode::initialize() for WASM
#[cfg(target_family = "wasm")]
{
    // Use wasm-bindgen to call JavaScript whisper.wasm
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        fn whisper_init(model_path: &str) -> i32;
        fn whisper_transcribe(audio: &[f32]) -> String;
    }

    // Initialize from bundled model in .rmpkg
    whisper_init("/models/whisper-tiny.ggml");
}
```

## Dependencies Summary

### Cargo.toml Changes

```toml
[package]
# ... existing

[lib]
crate-type = ["cdylib", "rlib"]  # Keep both for FFI + WASM

[[bin]]
name = "pipeline_executor_wasm"
path = "src/bin/pipeline_executor_wasm.rs"
required-features = ["wasm"]

[features]
default = ["webrtc-transport"]
wasm = []  # Enable WASM-specific code paths

[dependencies]
# Update PyO3 for abi3 static linking
pyo3 = { version = "0.26", features = ["abi3-py311"] }

# WASM-specific async
[target.'cfg(target_family = "wasm")'.dependencies]
wlr-libpy = { git = "https://github.com/vmware-labs/webassembly-language-runtimes.git", features = ["py_main"] }
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"

[build-dependencies]
[target.'cfg(target_family = "wasm")'.build-dependencies]
wlr-libpy = { git = "https://github.com/vmware-labs/webassembly-language-runtimes.git", features = ["build"] }
```

## Estimated Effort

| Phase | Task | Lines of Code | Time |
|-------|------|---------------|------|
| 1.1 | Cargo.toml + build.rs | ~50 | 1 hour |
| 1.2 | WASM binary entry point | ~150 | 2 hours |
| 1.3 | Sync executor path | ~30 | 1 hour |
| 1.4 | Numpy WASM marshaling | ~200 | 3 hours |
| 1.5 | Build + local test | - | 2 hours |
| 2.1 | Browser integration | ~300 | 4 hours |
| 2.2 | .rmpkg packaging | ~150 | 2 hours |
| 3.1 | Whisper WASM | ~500 | 8+ hours |
| **Total** | | **~1380 lines** | **~23 hours** |

## Success Criteria

### Phase 1 (Minimal WASM)
- ✅ Build succeeds: `cargo build --target wasm32-wasi`
- ✅ Runs in wasmtime: Simple math pipeline (Multiply → Add)
- ✅ Output matches native Rust runtime results

### Phase 2 (Browser)
- ✅ Loads in browser via @wasmer/sdk
- ✅ Executes pipeline from .rmpkg bundle
- ✅ Returns results to JavaScript
- ✅ Demonstrates in live browser demo

### Phase 3 (Whisper)
- ✅ Whisper transcription works in WASM
- ✅ Comparable performance to native (<2x slower)
- ✅ Full audio pipeline: Audio → Resample → Whisper → Output

## Known Limitations

1. **No GPU acceleration** - WebGPU API needed for GPU Whisper
2. **Single-threaded** - WASM threads are experimental
3. **Large bundle size** - libpython3.11.a + models = ~50MB+
4. **No network I/O** - Browser must provide data via JavaScript
5. **Async limitations** - Tokio doesn't fully support WASM yet

## Next Steps

1. **Start with Step 1.1** - Add wlr-libpy and build.rs
2. **Create minimal binary** - Just execute manifest, no nodes
3. **Add Rust-native nodes** - MultiplyNode, AddNode (no Python)
4. **Test RustPython in WASM** - Verify embedded VM works
5. **Add CPython nodes** - Test PyO3 + libpython integration
6. **Browser integration** - @wasmer/sdk + TypeScript
7. **Whisper WASM** - Advanced feature, separate workstream

## References

- VMware Labs PyO3 WASM Example: https://github.com/vmware-labs/webassembly-language-runtimes/tree/main/python/examples/embedding/wasi-py-rs-pyo3
- PyO3 Documentation: https://pyo3.rs/v0.26.0/
- Wasmtime: https://wasmtime.dev/
- Wasmer SDK: https://wasmer.io/
- Whisper.cpp WASM: https://github.com/ggerganov/whisper.cpp/tree/master/examples/whisper.wasm
