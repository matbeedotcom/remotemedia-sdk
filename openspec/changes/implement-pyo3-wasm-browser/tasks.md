# Implementation Tasks

## Phase 1: Local WASM Execution (MVP)

### 1.1 Build Configuration
- [ ] 1.1.1 Add `wlr-libpy` dependency with `py312` feature to `runtime/Cargo.toml` with target-specific configuration
- [ ] 1.1.2 Update PyO3 features to include `abi3-py312` for static linking compatibility (matches local Python 3.12.3)
- [ ] 1.1.3 Create `runtime/build.rs` with `configure_static_libs()` for wasm32-wasi target
- [ ] 1.1.4 Add `wasm` feature flag to `Cargo.toml` for conditional compilation
- [ ] 1.1.5 Test build setup: `cargo build --target wasm32-wasi` (expect link errors, that's okay)

### 1.2 WASM Binary Target
- [ ] 1.2.1 Create `runtime/src/bin/pipeline_executor_wasm.rs` with main entry point
- [ ] 1.2.2 Implement manifest input via stdin (WASI stdio)
- [ ] 1.2.3 Initialize PyO3 with `prepare_freethreaded_python()`
- [ ] 1.2.4 Call `Executor::execute_sync()` with parsed manifest
- [ ] 1.2.5 Output results to stdout as JSON
- [ ] 1.2.6 Add error handling with structured error output

### 1.3 Synchronous Execution Path
- [ ] 1.3.1 Add `execute_sync()` method to `Executor` (use `futures::executor::block_on`)
- [ ] 1.3.2 Add `#[cfg(target_family = "wasm")]` guards for WASM-specific code
- [ ] 1.3.3 Test native execution still works (no regression)
- [ ] 1.3.4 Document sync vs async execution differences

### 1.4 Data Marshaling Adaptation
- [ ] 1.4.1 Add WASM-specific numpy serialization in `python/numpy_marshal.rs`
- [ ] 1.4.2 Implement JSON/base64 encoding for numpy arrays (similar to RustPython path)
- [ ] 1.4.3 Implement JSON/base64 decoding for numpy arrays (use `np.frombuffer()`)
- [ ] 1.4.4 Add `#[cfg(target_family = "wasm")]` conditionals for rust-numpy vs base64 paths
- [ ] 1.4.5 Test round-trip: Python array → JSON → WASM → JSON → Python array (verify data integrity)

### 1.5 Build and Local Testing
- [ ] 1.5.1 Install wasm32-wasi target: `rustup target add wasm32-wasi`
- [ ] 1.5.2 Build WASM binary: `cargo build --target wasm32-wasi --bin pipeline_executor_wasm --release`
- [ ] 1.5.3 Install wasmtime: `curl https://wasmtime.dev/install.sh -sSf | bash`
- [ ] 1.5.4 Create test manifest for simple pipeline (Multiply → Add)
- [ ] 1.5.5 Run with wasmtime: `wasmtime --mapdir /usr::target/wasm32-wasi/wasi-deps/usr target/wasm32-wasi/release/pipeline_executor_wasm.wasm < test.json`
- [ ] 1.5.6 Verify output matches native runtime results
- [ ] 1.5.7 Test error handling (invalid manifest, execution errors)

### 1.6 Documentation
- [ ] 1.6.1 Document WASM build process in `docs/WASM_BUILD.md`
- [ ] 1.6.2 Add troubleshooting guide for common build errors
- [ ] 1.6.3 Document differences between native and WASM execution
- [ ] 1.6.4 Create example manifests for WASM execution

## Phase 2: Browser Integration

### 2.1 Package Format Extension
- [ ] 2.1.1 Define .rmpkg structure for WASM bundles
- [ ] 2.1.2 Add `runtime_target: "wasm32-wasi"` metadata to manifest
- [ ] 2.1.3 Create packaging script to bundle .wasm + manifest + dependencies
- [ ] 2.1.4 Include wasi-deps/usr directory in .rmpkg
- [ ] 2.1.5 Test package extraction and validation

### 2.2 Browser Runtime Integration
- [ ] 2.2.1 Create `browser-demo/` directory with TypeScript project
- [ ] 2.2.2 Install `@wasmer/sdk` dependency
- [ ] 2.2.3 Create `PipelineRunner` class that loads and executes WASM modules
- [ ] 2.2.4 Implement manifest passing via WASI stdin
- [ ] 2.2.5 Implement result retrieval via WASI stdout
- [ ] 2.2.6 Add error handling for WASM execution errors
- [ ] 2.2.7 Test in browser with simple pipeline

### 2.3 Browser Demo Application
- [ ] 2.3.1 Create HTML/CSS interface for pipeline demo
- [ ] 2.3.2 Add manifest editor (JSON textarea)
- [ ] 2.3.3 Add .rmpkg file upload functionality
- [ ] 2.3.4 Display pipeline execution results
- [ ] 2.3.5 Add performance metrics display (execution time, memory usage)
- [ ] 2.3.6 Style with responsive design
- [ ] 2.3.7 Test in Chrome, Firefox, Safari

### 2.4 WASI Filesystem Integration
- [ ] 2.4.1 Configure Wasmer preopen directories for /usr
- [ ] 2.4.2 Bundle wasi-deps in .rmpkg
- [ ] 2.4.3 Test filesystem access from WASM (model loading, etc.)
- [ ] 2.4.4 Add fallback for missing files (graceful degradation)

### 2.5 Deployment
- [ ] 2.5.1 Configure build for production (wasm-opt optimization)
- [ ] 2.5.2 Deploy demo to GitHub Pages or Vercel
- [ ] 2.5.3 Create demo video/GIF for documentation
- [ ] 2.5.4 Update README with browser demo link

## Phase 3: Whisper WASM (Optional)

### 3.1 Whisper.cpp WASM Investigation
- [ ] 3.1.1 Review existing whisper.wasm project (https://github.com/ggerganov/whisper.cpp/tree/master/examples/whisper.wasm)
- [ ] 3.1.2 Test whisper.wasm in browser standalone
- [ ] 3.1.3 Determine integration approach (wasm-bindgen interop vs native compilation)
- [ ] 3.1.4 Document Whisper WASM limitations and capabilities

### 3.2 RustWhisperNode WASM Adaptation
- [ ] 3.2.1 Add `#[cfg(target_family = "wasm")]` path for Whisper node
- [ ] 3.2.2 Integrate whisper.wasm via wasm-bindgen
- [ ] 3.2.3 Adapt audio data marshaling for Whisper WASM API
- [ ] 3.2.4 Test transcription in browser
- [ ] 3.2.5 Compare performance: WASM vs native
- [ ] 3.2.6 Document Whisper WASM model loading (bundle in .rmpkg)

### 3.3 Full Audio Pipeline Test
- [ ] 3.3.1 Create browser demo: file upload → audio processing → Whisper transcription
- [ ] 3.3.2 Test with sample audio files (various formats, durations)
- [ ] 3.3.3 Measure real-time factor (RTF) in browser
- [ ] 3.3.4 Optimize for performance (worker threads, streaming)
- [ ] 3.3.5 Add progress reporting for long transcriptions

## Testing

### Unit Tests
- [ ] Test numpy marshaling round-trip (JSON/base64)
- [ ] Test synchronous executor with various pipelines
- [ ] Test WASM binary with invalid manifests
- [ ] Test error propagation from WASM to JavaScript

### Integration Tests
- [ ] Test full pipeline: Python → serialize → WASM → execute → results
- [ ] Test browser integration: load .rmpkg → execute → results
- [ ] Test multiple pipelines in sequence (state isolation)
- [ ] Test concurrent pipeline execution (if supported)

### Performance Tests
- [ ] Benchmark WASM vs native execution (simple pipeline)
- [ ] Benchmark numpy serialization overhead (base64 vs zero-copy)
- [ ] Benchmark browser WASM startup time (cold vs warm)
- [ ] Benchmark Whisper WASM transcription (Phase 3)

## Documentation

### User Documentation
- [ ] Browser demo README with usage instructions
- [ ] .rmpkg packaging guide
- [ ] Troubleshooting guide for browser WASM
- [ ] Performance tuning guide

### Developer Documentation
- [ ] Architecture diagram: Browser → WASM → PyO3 → CPython
- [ ] Build configuration guide
- [ ] WASM vs native code paths explanation
- [ ] Contributing guide for WASM-specific code

## Validation Checklist

- [ ] All tasks marked complete
- [ ] WASM binary builds without errors
- [ ] Wasmtime local test passes
- [ ] Browser demo loads and executes
- [ ] Documentation updated
- [ ] Tests pass (unit + integration)
- [ ] No regressions in native runtime
- [ ] Proposal approved by stakeholders

---

**Total Tasks**: 77 (Phase 1: 29, Phase 2: 21, Phase 3: 15, Testing: 8, Documentation: 4)
**Estimated Effort**: 22-30 hours
**Dependencies**: Sequential phases (1 → 2 → 3)
