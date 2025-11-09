# Design Document: PyO3 WASM Browser Runtime

## Overview

This document describes the technical design for compiling the RemoteMedia Rust runtime to WebAssembly (WASM) using the `wasm32-wasi` target with embedded CPython via PyO3, enabling Pipeline execution in web browsers.

## Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Browser Environment                         │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │              JavaScript Application Layer                   │ │
│  │  - Load .rmpkg bundle (fetch or file upload)               │ │
│  │  - Instantiate @wasmer/sdk runtime                         │ │
│  │  - Pass manifest JSON via WASI stdin                       │ │
│  │  - Receive results via WASI stdout                         │ │
│  └────────────┬────────────────────────────────────────────────┘ │
│               │ WASI Interface                                   │
│               │ (stdin/stdout, filesystem, env vars)             │
│  ┌────────────▼────────────────────────────────────────────────┐ │
│  │           remotemedia_runtime.wasm Module                   │ │
│  │                                                              │ │
│  │  ┌────────────────────────────────────────────────────────┐ │ │
│  │  │         PyO3 + libpython3.11.a (static-linked)         │ │ │
│  │  │  - Python::with_gil() for GIL management              │ │ │
│  │  │  - Execute Python node code in embedded CPython       │ │ │
│  │  │  - Python stdlib access (limited by WASI)             │ │ │
│  │  └────────────────────────────────────────────────────────┘ │ │
│  │                                                              │ │
│  │  ┌────────────────────────────────────────────────────────┐ │ │
│  │  │              Rust Runtime Core                         │ │ │
│  │  │  - Executor: Pipeline orchestration                   │ │ │
│  │  │  - Manifest parser (serde_json)                       │ │ │
│  │  │  - Node registry (Rust-native + Python nodes)         │ │ │
│  │  │  - Data marshaling (JSON ↔ Python)                    │ │ │
│  │  └────────────────────────────────────────────────────────┘ │ │
│  │                                                              │ │
│  │  ┌────────────────────────────────────────────────────────┐ │ │
│  │  │          RustPython VM (Fallback)                      │ │ │
│  │  │  - Pure-Rust Python interpreter                       │ │ │
│  │  │  - Used when CPython extensions unavailable           │ │ │
│  │  └────────────────────────────────────────────────────────┘ │ │
│  │                                                              │ │
│  │  ┌────────────────────────────────────────────────────────┐ │ │
│  │  │                 WASI Layer                             │ │ │
│  │  │  - File I/O via preopen directories (/usr)            │ │ │
│  │  │  - stdout/stderr for logging and results              │ │ │
│  │  │  - stdin for manifest input                           │ │ │
│  │  │  - Environment variables                              │ │ │
│  │  └────────────────────────────────────────────────────────┘ │ │
│  └─────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────┘
```

### Component Breakdown

#### 1. WASM Binary (`pipeline_executor_wasm.wasm`)

**Purpose**: Standalone WASI Command that executes RemoteMedia pipelines

**Entry Point**: `_start` function (WASI Command standard)

**Execution Flow**:
```rust
fn main() -> PyResult<()> {
    // 1. Initialize PyO3 (prepare_freethreaded_python)
    pyo3::prepare_freethreaded_python();

    // 2. Read manifest from stdin
    let manifest_json = read_stdin();

    // 3. Execute pipeline synchronously
    Python::with_gil(|py| {
        let manifest = parse(&manifest_json)?;
        let executor = Executor::new();
        let result = executor.execute_sync(&manifest)?;

        // 4. Write results to stdout
        println!("{}", serde_json::to_string(&result)?);
        Ok(())
    })
}
```

#### 2. PyO3 + libpython Integration

**Build Configuration**:
- Uses `wlr-libpy` crate to fetch pre-built `libpython3.12.a` for wasm32-wasi
- Statically links libpython, wasi-sysroot, and clang builtins
- PyO3 configured with `abi3-py312` feature for stable ABI (matches local Python 3.12.3)

**Key Dependencies** (from wlr-libpy):
```
libpython3.12.a                    (~5MB)
libwasi-emulated-signal.a          (~50KB)
libwasi-emulated-getpid.a          (~10KB)
libwasi-emulated-process-clocks.a  (~15KB)
libclang_rt.builtins-wasm32.a      (~500KB)
```

**GIL Management**:
```rust
// PyO3 handles GIL automatically in WASM
Python::with_gil(|py| {
    // Python operations here
})
```

#### 3. Data Marshaling (WASM-specific)

**Problem**: `rust-numpy` crate requires CPython C API extensions not available in static libpython.

**Solution**: Conditional compilation with JSON/base64 serialization for WASM:

```rust
// Native path (zero-copy)
#[cfg(not(target_family = "wasm"))]
pub fn numpy_to_json(py: Python, array: &PyAny) -> PyResult<Value> {
    use numpy::PyArrayDyn;
    let arr: &PyArrayDyn<f32> = array.extract()?;
    // Zero-copy via rust-numpy
}

// WASM path (base64 serialization)
#[cfg(target_family = "wasm")]
pub fn numpy_to_json(py: Python, array: &PyAny) -> PyResult<Value> {
    let bytes = array.call_method0("tobytes")?;
    let dtype = array.getattr("dtype")?.getattr("name")?.extract()?;
    let shape = array.getattr("shape")?.extract()?;

    json!({
        "__numpy__": true,
        "array": {
            "meta": {"dtype": dtype, "shape": shape},
            "data": base64::encode(bytes)
        }
    })
}
```

**Performance Tradeoff**:
- Native: Zero-copy, microsecond overhead
- WASM: Base64 encoding/decoding, ~2-3x slower for large arrays
- Acceptable for MVP; optimize later if bottleneck

#### 4. Async Execution Strategy

**Problem**: Tokio doesn't fully support WASM (no native threads)

**Solutions** (in order of implementation priority):

**Phase 1 (MVP)**: Synchronous execution using futures executor
```rust
#[cfg(target_family = "wasm")]
pub fn execute_sync(&self, manifest: &Manifest) -> Result<ExecutionResult> {
    use futures::executor::block_on;
    block_on(self.execute(manifest))
}
```

**Phase 2 (Optional)**: Browser-native async using wasm-bindgen-futures
```rust
#[cfg(target_family = "wasm")]
use wasm_bindgen_futures::spawn_local;

#[wasm_bindgen]
pub async fn execute_pipeline_wasm(manifest_json: String) -> Result<JsValue> {
    let manifest = parse(&manifest_json)?;
    let result = executor.execute(&manifest).await?;
    Ok(serde_wasm_bindgen::to_value(&result)?)
}
```

**Decision**: Start with Phase 1 (simpler, works in any WASI runtime), add Phase 2 if async becomes critical.

### Browser Integration Layer

#### TypeScript PipelineRunner

```typescript
import { init, Wasmer } from "@wasmer/sdk";

export class PipelineRunner {
    private wasmModule: Uint8Array;
    private wasmerInstance: Wasmer | null = null;

    async initialize(rmpkgUrl: string) {
        // 1. Load .rmpkg bundle
        const rmpkg = await this.loadRmpkg(rmpkgUrl);

        // 2. Extract WASM module and dependencies
        this.wasmModule = rmpkg.modules.remotemedia_runtime;
        const wasiDeps = rmpkg.wasiDeps;

        // 3. Initialize Wasmer runtime
        await init();
        this.wasmerInstance = await Wasmer.fromFile(this.wasmModule);
    }

    async execute(manifestJson: string): Promise<any> {
        if (!this.wasmerInstance) throw new Error("Not initialized");

        // 1. Create WASI instance with preopen directories
        const instance = await this.wasmerInstance.instantiate({
            mapDirs: {
                "/usr": this.wasiDeps.usr
            }
        });

        // 2. Pass manifest via stdin
        instance.setStdin(new TextEncoder().encode(manifestJson));

        // 3. Execute WASI command
        await instance.start();

        // 4. Read results from stdout
        const stdout = instance.getStdout();
        const result = JSON.parse(new TextDecoder().decode(stdout));

        return result;
    }
}
```

### Package Format (.rmpkg)

**Structure**:
```
voice_pipeline_v1.rmpkg/
├── manifest.json                      # Pipeline definition + metadata
├── modules/
│   └── remotemedia_runtime.wasm      # Compiled WASM binary (~8-10MB)
├── wasi-deps/
│   └── usr/
│       ├── lib/
│       │   └── python3.11/           # Python stdlib (subset)
│       └── share/                     # Additional resources
├── models/                            # Optional: ML models
│   └── whisper-tiny.ggml             # (Phase 3)
└── meta/
    ├── provenance.json                # Build metadata
    └── signature.sig                  # GPG signature (optional)
```

**manifest.json**:
```json
{
  "version": "v1",
  "runtime_target": "wasm32-wasi",
  "nodes": [...],
  "edges": [...],
  "metadata": {
    "build_timestamp": "2025-01-24T10:00:00Z",
    "wasm_binary": "modules/remotemedia_runtime.wasm",
    "dependencies": {
      "libpython": "3.11.3",
      "wasi_sdk": "19.0"
    }
  }
}
```

## Key Design Decisions

### Decision 1: wasm32-wasi vs Emscripten

**Chosen**: `wasm32-wasi`

**Rationale**:
- More portable (works in Node.js, Deno, Cloudflare Workers, browser)
- Standard WASI interface (filesystem, stdio) easier to reason about
- Better tooling support (wasmtime, wasmer)
- Emscripten would lock us into browser-specific APIs

**Tradeoff**: Slightly larger binary size, but better long-term portability

### Decision 2: PyO3 abi3 vs py311-specific

**Chosen**: `abi3-py311`

**Rationale**:
- Stable ABI allows static linking of libpython
- Forward compatibility with future Python versions
- Required for wlr-libpy integration

**Tradeoff**: Slightly slower FFI calls, but negligible in WASM context

### Decision 3: Sync vs Async Execution

**Chosen**: Synchronous execution (Phase 1), async optional (Phase 2)

**Rationale**:
- Tokio doesn't support WASM threads yet
- Sync execution simpler to implement and debug
- Browser async can be handled at JavaScript layer
- futures::executor::block_on works for single-threaded execution

**Tradeoff**: Cannot leverage Tokio's parallelism, but WASM is single-threaded anyway

### Decision 4: Numpy Serialization

**Chosen**: JSON + base64 for WASM, zero-copy for native

**Rationale**:
- rust-numpy unavailable in WASM (C extension dependencies)
- Base64 already proven in RustPython path
- Conditional compilation keeps native performance

**Tradeoff**: ~2-3x slower for large arrays in WASM, acceptable for MVP

### Decision 5: Browser WASM Runtime

**Chosen**: `@wasmer/sdk` (primary), `wasmtime-js` (fallback)

**Rationale**:
- @wasmer/sdk has better WASI filesystem support (IndexedDB-backed)
- Good documentation and active maintenance
- wasmtime-js as fallback for compatibility

**Tradeoff**: Adds ~2MB JavaScript bundle, but necessary for WASI support

## Implementation Phases

### Phase 1: MVP (8-10 hours)
**Goal**: Get WASM binary running locally in wasmtime

**Deliverables**:
- Build configuration (Cargo.toml, build.rs)
- WASM binary entry point
- Sync executor
- WASM-compatible numpy marshaling
- Local wasmtime test

**Success Metric**: `cargo build --target wasm32-wasi` succeeds, pipeline executes in wasmtime

### Phase 2: Browser Integration (6-8 hours)
**Goal**: Run WASM in browser via @wasmer/sdk

**Deliverables**:
- .rmpkg packaging format
- TypeScript PipelineRunner
- Browser demo HTML/CSS/JS
- Deployment to GitHub Pages

**Success Metric**: Live browser demo executing pipelines from .rmpkg bundles

### Phase 3: Whisper WASM (8-12 hours, Optional)
**Goal**: Add Whisper transcription in browser

**Deliverables**:
- whisper.cpp WASM integration
- RustWhisperNode WASM adaptation
- Full audio pipeline demo

**Success Metric**: Browser transcription with RTF < 2.0 (real-time capable)

## Performance Considerations

### Expected Overhead

| Component | Native | WASM | Overhead Factor |
|-----------|--------|------|-----------------|
| Pipeline parsing | ~1ms | ~2ms | 2x |
| Node execution (Rust) | baseline | 1.2-1.5x | 1.2-1.5x |
| Node execution (Python) | baseline | 1.5-2x | 1.5-2x |
| Numpy marshaling | <1ms (zero-copy) | ~5-10ms (base64) | 5-10x |
| Total pipeline | baseline | 1.5-2x | 1.5-2x |

**Mitigation**:
- Use Rust-native nodes where possible (no Python overhead)
- Batch numpy operations to amortize serialization cost
- Use wasm-opt for binary size and performance optimization

### Bundle Size

| Component | Size |
|-----------|------|
| remotemedia_runtime.wasm | ~8-10MB |
| wasi-deps (Python stdlib subset) | ~5-7MB |
| Models (Whisper tiny) | ~75MB (optional) |
| @wasmer/sdk JavaScript | ~2MB |
| **Total (no models)** | **~15-20MB** |
| **Total (with Whisper)** | **~90-95MB** |

**Mitigation**:
- Lazy load models (separate fetch after initial load)
- Use brotli/gzip compression (50-60% reduction)
- Offer "lite" version without Whisper for smaller use cases

## Security Considerations

### WASM Sandbox
- ✅ Memory isolation (WASM linear memory)
- ✅ No direct access to host filesystem (only preopen directories)
- ✅ No network access (browser must provide data)
- ✅ Limited syscalls (WASI capability-based security)

### Untrusted Code Execution
- ⚠️ Python node code runs in embedded CPython (not sandboxed)
- ✅ RustPython VM can be used for untrusted Python code
- ⚠️ .rmpkg bundles should be signed and verified before execution

**Recommendation**: Add signature verification in Phase 2 (GPG or similar)

## Testing Strategy

### Unit Tests
- Numpy marshaling round-trip (base64 encode/decode)
- Manifest parsing in WASM
- Sync executor behavior

### Integration Tests
- Full pipeline: Python → serialize → WASM → execute
- Browser integration: load .rmpkg → execute → results
- Error propagation across WASM boundary

### Performance Tests
- WASM vs native execution time
- Bundle size optimization
- Cold start time (first load)

## Rollout Plan

### Phase 1: Internal Testing
- Developers test locally with wasmtime
- Validate WASM build on CI/CD
- Performance benchmarking

### Phase 2: Public Demo
- Deploy browser demo to GitHub Pages
- Share demo link for feedback
- Gather performance metrics from real users

### Phase 3: Production (Future)
- Add to main RemoteMedia SDK documentation
- Publish .rmpkg packages to registry
- Support in `remotemedia build --target wasm32-wasi`

## Monitoring and Observability

### Metrics to Track
- WASM binary size over time
- Execution time vs native runtime
- Browser compatibility issues (bug reports)
- User adoption (demo page views, .rmpkg downloads)

### Logging
- WASM execution logs via WASI stderr
- JavaScript console for browser integration
- Performance marks for debugging

## Open Issues and Future Work

### Known Limitations
1. **Single-threaded execution** - WASM threads are experimental
2. **No GPU acceleration** - WebGPU API integration needed
3. **Large bundle size** - Optimization ongoing
4. **Limited Python stdlib** - Only subset available in WASM

### Future Enhancements
1. **WebGPU integration** for GPU-accelerated models
2. **Streaming execution** with Web Workers
3. **IndexedDB caching** for models and dependencies
4. **Service Worker** for offline execution
5. **WebRTC transport** for peer-to-peer pipelines in browser

## References

- VMware Labs PyO3 WASM: https://github.com/vmware-labs/webassembly-language-runtimes
- PyO3 Documentation: https://pyo3.rs/v0.26.0/
- WASI Specification: https://github.com/WebAssembly/WASI
- @wasmer/sdk: https://wasmer.io/
- Wasmtime: https://wasmtime.dev/

---

**Document Version**: 1.0
**Last Updated**: 2025-01-24
**Status**: Pending Review
