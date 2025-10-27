# Proposal: Native Rust Acceleration for AI/ML Pipelines

## Why

**Current Problem**: RemoteMedia SDK has grown complex with multiple competing initiatives (WASM browser runtime, WebRTC mesh, RustPython VM) that distract from the core value proposition.

**Core Insight**: Users want **fast Python AI/ML pipelines**, not browser demos or P2P networking. The existing PyO3 integration shows 193-361x performance gains for compute-heavy operations, but only 5-10% of nodes are accelerated.

**Opportunity**: Complete the Rust native runtime with focus on AI/ML acceleration. Transparent speedup with zero code changes.

## What Changes

### PAUSE/ARCHIVE
- **WASM Browser Runtime** (Phase 2 complete but unused) → Archive branch
- **WebRTC Transport** (Not started, no user demand) → Defer indefinitely
- **Pipeline Mesh** (Over-engineered) → Defer indefinitely
- **RustPython VM** (Inferior to CPython via PyO3) → Delete

### COMPLETE
- **Rust Pipeline Executor** (60% done) → Finish core orchestration
- **Error Handling & Observability** (Partial) → Production-ready monitoring
- **Performance Optimization** (Partial) → Comprehensive profiling

### ADD (High-Value)
- **Audio Processing Nodes** (Rust native) → VAD, Resample, FormatConverter
- **Batch Processing Optimization** → Zero-copy data flow
- **Remote GPU Execution** (Keep simple gRPC) → No WebRTC complexity

## Impact

### Affected Capabilities

**COMPLETE (No Change)**:
- `python-rust-interop`: PyO3 FFI, data marshaling (✅ Done)
- `runtime-executor`: Manifest parsing, node registry (✅ 60% done)

**NEW**:
- `native-audio-processing`: Rust implementations of VAD, Resample, FormatConverter
- `performance-monitoring`: Execution profiling, bottleneck detection
- `production-hardening`: Error handling, retry policies, graceful degradation

**ARCHIVE**:
- `wasm-sandbox`: Browser execution (moved to `archive/2025-10-27-wasm-browser`)
- `webrtc-transport`: P2P streaming (deferred indefinitely)
- `pipeline-mesh`: Distributed architecture (deferred indefinitely)

### Affected Code

**DELETE** (~5,000 lines):
- `runtime/src/python/vm.rs` - RustPython VM (replaced by CPython via PyO3)
- `runtime/src/python/rustpython_executor.rs` - RustPython node executor

**COMPLETE** (~3,000 lines):
- `runtime/src/executor/mod.rs` - Finish pipeline orchestration
- `runtime/src/executor/error.rs` - Error propagation and retry
- `runtime/src/executor/metrics.rs` - Performance monitoring

**ADD** (~2,500 lines):
- `runtime/src/nodes/audio/vad.rs` - Voice Activity Detection
- `runtime/src/nodes/audio/resample.rs` - Audio resampling
- `runtime/src/nodes/audio/format.rs` - Format conversion

### Migration Path

**Immediate (Week 1)**:
1. Archive `feat/pyo3-wasm-browser` branch
2. Create `feat/native-acceleration` branch
3. Delete RustPython VM code

**Short-term (Weeks 2-4)**:
4. Complete Rust executor (tasks 1.3.2-1.3.5)
5. Add error handling (tasks 1.8.x, 1.12.x)
6. Performance monitoring (tasks 1.13.x)

**Medium-term (Weeks 5-6)**:
7. Rust audio processing nodes
8. Port all examples
9. Comprehensive benchmarks

## Success Criteria

1. **Zero Code Changes**: All existing examples work with `pipeline.run()`
2. **Performance**: Audio preprocessing 50-100x faster than Python
3. **Developer Experience**: Clear migration guide, performance tuning docs
4. **Production Ready**: Error handling, monitoring, retry policies complete
5. **Adoption**: 80% of common nodes have Rust equivalents

## Non-Goals

- Browser execution (archived, revisit if demand emerges)
- P2P networking via WebRTC (simple gRPC is sufficient)
- Pipeline packaging/distribution (file-based is fine for now)
- RustPython compatibility (CPython via PyO3 is strictly superior)

## Timeline Estimate

- **Week 1**: Cleanup and branch management
- **Weeks 2-4**: Complete Rust executor core
- **Weeks 5-6**: Audio nodes and production hardening
- **Total**: 6 weeks to v0.2.0 release

## Open Questions

1. **RustPython deletion confirmed?** Recommend: Yes, CPython via PyO3 is superior
2. **WASM archive strategy?** Recommend: Keep branch, document in `docs/WASM_ARCHIVE.md`
3. **Remote execution transport?** Recommend: Keep gRPC, defer WebRTC
4. **Node priority order?** Recommend: VAD → Resample → FormatConverter

---

**Change ID**: `native-rust-acceleration`  
**Supersedes**: `implement-pyo3-wasm-browser`, `refactor-language-neutral-runtime`  
**Date**: 2025-10-27  
**Status**: Proposed
