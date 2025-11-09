# Proposal: Implement PyO3 WASM Browser Runtime

## Why
The current RemoteMedia runtime only runs natively (Linux/macOS/Windows) via Python FFI (cdylib) or as a standalone gRPC service. There is no way to execute pipelines directly in a web browser, limiting deployment scenarios for:
- Client-side audio/video processing
- Privacy-preserving local computation
- Offline-capable web applications
- Edge computing in browsers

**The Opportunity**: VMware Labs has demonstrated that PyO3 + CPython can be compiled to WASM using their `webassembly-language-runtimes` project, providing:
- Pre-built `libpython3.12.a` static library for `wasm32-wasi` (matches local Python 3.12.3)
- Proven `wlr-libpy` helper crate for build configuration
- Reference implementation showing PyO3 working in WASM

Our runtime is already built with PyO3, RustPython, and language-neutral manifest execution - perfect foundation for WASM compilation. This unlocks browser-based pipeline execution with full Python compatibility.

## What Changes

### Phase 1: Local WASM Execution (MVP)
- **Build configuration**: Add `wlr-libpy` for static libpython linking
- **WASM binary target**: `pipeline_executor_wasm` with WASI Command entry point
- **Data marshaling**: Replace rust-numpy with JSON/base64 for WASM compatibility
- **Synchronous execution**: Use futures executor instead of tokio (WASM limitation)
- **Local testing**: Run pipelines in wasmtime with test manifests

### Phase 2: Browser Integration
- **Package format**: Extend .rmpkg with WASM bundles and wasi-deps
- **TypeScript integration**: @wasmer/sdk-based PipelineRunner class
- **Browser demo**: HTML/CSS/JS application showcasing WASM execution
- **WASI filesystem**: Preopen directories for Python stdlib and models

### Phase 3: Whisper WASM (Optional)
- **whisper.cpp compilation**: Integrate existing whisper.wasm project
- **RustWhisperNode adaptation**: WASM-specific audio transcription path
- **Performance validation**: Ensure RTF < 2.0 for real-time capability

### Out of Scope (Future Work)
- Full tokio async runtime in WASM (use futures executor for MVP)
- GPU acceleration via WebGPU (requires WebGPU API investigation)
- Network I/O in WASM (browser provides data via JavaScript)
- Multi-threading in WASM (experimental, not currently stable)

## Impact

### Affected Components (New)
- **`runtime/build.rs`**: WASM build configuration with wlr-libpy
- **`runtime/src/bin/pipeline_executor_wasm.rs`**: WASM binary entry point
- **`runtime/src/executor/mod.rs`**: Synchronous execution method
- **`runtime/src/python/numpy_marshal.rs`**: Conditional WASM marshaling
- **`browser-demo/`**: TypeScript integration and demo application

### Affected Specs
- **`pyo3-wasm-browser` (NEW)**: Browser-specific WASM requirements
- **`wasm-sandbox` (EXTENDS)**: Implements subset focused on browser execution
- **`pipeline-packaging` (EXTENDS)**: Adds WASM-specific .rmpkg format

### Dependencies
- Existing `refactor-language-neutral-runtime` change (Phase 1 complete)
- PyO3 0.26 with abi3-py312 support (matches local Python 3.12.3)
- VMware Labs `wlr-libpy` crate with py312 feature (external)
- @wasmer/sdk JavaScript library (browser runtime)

## Impact Metrics

### Phase 1: Local WASM Execution
- ✅ Successfully builds `cargo build --target wasm32-wasi --bin pipeline_executor_wasm`
- ✅ Runs in wasmtime with test pipeline (Multiply → Add nodes)
- ✅ Output matches native Rust runtime results (bit-identical)
- ✅ Pipeline serialization → WASM → execution → results works end-to-end

### Phase 2: Browser Integration
- ✅ Loads .rmpkg bundle in browser via @wasmer/sdk
- ✅ Executes pipeline from JavaScript
- ✅ Returns results to JavaScript consumer
- ✅ Live browser demo deployed (GitHub Pages or similar)

### Phase 3: Whisper WASM (Stretch)
- ✅ Whisper transcription works in WASM
- ✅ Performance comparable to native (< 2x slower)
- ✅ Full audio pipeline: MediaReader → Resample → Whisper → Output

## Non-Goals

- Replace existing native runtime (WASM is additive deployment target)
- Support all Python stdlib in WASM (use RustPython or CPython subset)
- Match native performance exactly (WASM overhead is acceptable)
- Full WASI preview2 support (preview1 is sufficient for MVP)

## Alternatives Considered

### Alternative 1: Compile Python nodes to WASM individually
**Rejected**: Would require transpiling Python → WASM for each node. Complex toolchain, limited Python compatibility.

### Alternative 2: Use Pyodide (Python WASM distribution)
**Rejected**: Pyodide is ~30MB+ bundle, focused on scientific Python stack. Our runtime needs minimal footprint and already has RustPython + PyO3 infrastructure.

### Alternative 3: Browser-only runtime (no CPython)
**Rejected**: Would lose Python node compatibility. Need CPython for full ecosystem access (transformers, etc.) even if only available server-side.

### Alternative 4: Emscripten instead of wasm32-wasi
**Rejected**: `wasm32-wasi` is more portable (works in Node.js, Deno, Cloudflare Workers). Emscripten ties us to browser-specific APIs.

## Risk Assessment

### Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| PyO3 incompatibility with wasm32-wasi | Low | High | VMware Labs already proved it works with PyO3 0.19; we're using 0.26 (compatible) |
| Tokio doesn't work in WASM | Medium | Medium | Use futures executor or synchronous execution; add wasm-bindgen-futures for browser async |
| rust-numpy unavailable in WASM | High | Low | Use JSON/base64 serialization (already implemented for RustPython path) |
| whisper.cpp won't compile to WASM | Medium | Medium | Use existing whisper.wasm project; Phase 3 is optional |
| Large bundle size (>50MB) | High | Low | Acceptable for specialized use cases; optimize with wasm-opt |

### Organizational Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Browser WASM runtime ecosystem changes | Low | Medium | Use stable @wasmer/sdk; provide fallback to wasmtime-js |
| Maintenance burden (two targets) | Medium | Low | Share 95% of codebase; WASM-specific code is minimal (<5%) |

## Timeline Estimate

| Phase | Duration | Dependencies |
|-------|----------|--------------|
| Phase 1: Local WASM | 8-10 hours | None (can start immediately) |
| Phase 2: Browser Integration | 6-8 hours | Phase 1 complete |
| Phase 3: Whisper WASM | 8-12 hours | Phase 2 complete, whisper.cpp investigation |
| **Total** | **22-30 hours** | Sequential phases |

## Open Questions

1. **Async runtime strategy**: Should we invest in full tokio WASM support (experimental) or use synchronous execution for MVP?
   - **Recommendation**: Start with synchronous execution (futures::executor::block_on), add async later if needed

2. **Numpy serialization performance**: Is JSON/base64 acceptable for audio data, or do we need binary format?
   - **Recommendation**: Start with base64 (proven in RustPython path), optimize later if bottleneck

3. **.rmpkg format**: Should we extend existing packaging spec or create browser-specific format?
   - **Recommendation**: Extend existing spec with `runtime_target: "wasm32-wasi"` metadata

4. **Browser WASM runtime**: @wasmer/sdk vs wasmtime-js vs custom?
   - **Recommendation**: Start with @wasmer/sdk (better WASI filesystem), add wasmtime-js fallback

## Stakeholder Input

_This section will be filled during proposal review_

## Approval

- [ ] Technical lead review
- [ ] Architecture review (if needed)
- [ ] Security review (WASM sandboxing implications)

---

**Change ID**: `implement-pyo3-wasm-browser`
**Proposed by**: AI Assistant
**Date**: 2025-01-24
**Status**: Pending Review
