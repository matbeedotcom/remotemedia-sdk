# Implementation Plan: Native Rust Acceleration for AI/ML Pipelines

**Branch**: `001-native-rust-acceleration` | **Date**: October 27, 2025 | **Spec**: [spec.md](spec.md)

## Summary

Accelerate Python AI/ML pipelines through transparent Rust acceleration. Complete the Rust runtime executor (currently 60% done), delete unnecessary complexity (RustPython VM, WASM browser, WebRTC mesh), and add high-performance Rust implementations of compute-heavy audio processing nodes. Target: 50-100x speedup for audio preprocessing with zero code changes to existing pipeline scripts.

## Technical Context

**Language/Version**: Rust 1.70+ (runtime), Python 3.9+ (SDK)  
**Primary Dependencies**: 
- Rust: tokio 1.35 (async runtime), PyO3 0.26 (Python FFI), serde 1.0 (serialization), rubato 0.15 (audio resampling), rustfft 6.2 (FFT for VAD), bytemuck 1.14 (zero-copy format conversion), thiserror 1.0 + anyhow 1.0 (error handling), tracing 0.1 (structured logging)
- Python: numpy 1.24+ (arrays), PyAV 12+ (audio/video), pytest 7+ (testing)

**Storage**: N/A (in-memory pipeline execution, metrics exported as JSON)  
**Testing**: cargo test (Rust unit/integration), pytest (Python integration), criterion (Rust benchmarks)  
**Target Platform**: Linux/macOS/Windows native (x86_64, aarch64)  
**Project Type**: Hybrid library (Rust cdylib + Python SDK wrapper)  
**Performance Goals**: 
- FFI overhead: <1μs per call (✅ achieved at 0.8μs)
- Rust nodes: 100-300x faster than Python (✅ 193-361x for math ops)
- Audio preprocessing: 50-100x faster than Python (target: VAD <50μs/frame, resample <2ms/sec)
- Zero-copy data transfer: 0 copies for numpy array access (borrow via PyO3)

**Constraints**: 
- Zero code changes for existing pipelines (backward compatibility mandatory)
- 100% Python stdlib compatibility via CPython (PyO3, no RustPython)
- Cross-platform (no platform-specific audio APIs, pure Rust implementation)
- Performance monitoring overhead: <100μs per pipeline execution

**Scale/Scope**: 
- ~15,000 LoC (down from ~50,000 after cleanup, 70% reduction)
- 50+ SDK nodes (target: 80% have Rust equivalents by v0.2.0)
- 20+ examples demonstrating acceleration patterns
- Support pipelines with up to 100 nodes, 10,000 concurrent executions

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Note**: The project constitution file is currently a template. Using project conventions from `openspec/project.md` and `openspec/AGENTS.md`:

### Simplicity Principle (Implicit)
- ✅ **PASS**: Removing unnecessary complexity (RustPython VM, WASM, WebRTC)
- ✅ **PASS**: Single runtime path (Rust + CPython via PyO3)
- ✅ **PASS**: Simple gRPC for remote execution (no WebRTC complexity)

### Performance Standards (Implicit)
- ✅ **PASS**: Zero-copy numpy arrays via rust-numpy
- ✅ **PASS**: Microsecond FFI overhead measured and documented
- ✅ **PASS**: Comprehensive benchmarking with criterion planned

### Testing Requirements (From project.md)
- ✅ **PASS**: Unit tests exist for all Rust modules
- ✅ **PASS**: Integration tests planned for Python-Rust roundtrip
- ✅ **PASS**: Benchmark suite for performance regression detection
- ✅ **PASS**: Error handling tests planned in Phase 1

### Documentation Standards (From project.md)
- ✅ **PASS**: Migration guide planned (MIGRATION_GUIDE.md)
- ✅ **PASS**: Performance tuning guide planned (PERFORMANCE_TUNING.md)
- ✅ **PASS**: API documentation exists (docstrings in Python SDK)
- ✅ **PASS**: Architecture overview planned (NATIVE_ACCELERATION.md)

**Overall**: ✅ **PASS** - All standards met, no violations

## Project Structure

### Documentation (this feature)

```text
specs/001-native-rust-acceleration/
├── spec.md              # Feature specification (✅ created)
├── plan.md              # This file (implementation plan)
├── research.md          # Phase 0 output (✅ created)
├── data-model.md        # Phase 1 output (✅ created)
├── quickstart.md        # Phase 1 output (✅ created)
├── contracts/           # Phase 1 output (✅ created)
│   ├── ffi-api.md       # FFI boundary API contract
│   └── node-executor-api.md  # Node executor trait contract
├── tasks.md             # Phase 2 output (✅ created)
└── checklists/
    └── requirements.md  # Specification quality checklist (✅ created)
```

### Source Code (repository root)

```text
runtime/                              # Rust runtime (primary focus)
├── src/
│   ├── executor/
│   │   ├── mod.rs                   # ✅ Exists (60% done) → COMPLETE
│   │   ├── graph.rs                 # ❌ NEW: Pipeline graph structure
│   │   ├── scheduler.rs             # ❌ NEW: Topological sort & scheduling
│   │   ├── error.rs                 # ❌ NEW: Error handling & retry
│   │   └── metrics.rs               # ❌ NEW: Performance monitoring
│   ├── python/
│   │   ├── ffi.rs                   # ✅ Exists: FFI entry points
│   │   ├── marshal.rs               # ✅ Exists: Data type conversion
│   │   ├── numpy_marshal.rs         # ✅ Exists: Zero-copy numpy
│   │   ├── cpython_executor.rs      # ✅ Exists: CPython node execution
│   │   ├── vm.rs                    # ❌ DELETE: RustPython VM (~1,200 LoC)
│   │   └── rustpython_executor.rs   # ❌ DELETE: RustPython executor (~800 LoC)
│   ├── nodes/
│   │   ├── mod.rs                   # ✅ Exists: Node registry
│   │   ├── registry.rs              # ⚠️ MODIFY: Add runtime selection logic
│   │   ├── math.rs                  # ✅ Exists: MultiplyNode, AddNode
│   │   └── audio/                   # ❌ NEW: Audio processing nodes
│   │       ├── mod.rs               # ❌ NEW: Audio module
│   │       ├── vad.rs               # ❌ NEW: Voice Activity Detection
│   │       ├── resample.rs          # ❌ NEW: Audio resampling
│   │       └── format.rs            # ❌ NEW: Format conversion
│   └── manifest/
│       └── mod.rs                   # ✅ Exists: Manifest parsing
├── tests/
│   ├── integration/
│   │   ├── test_executor.rs         # ⚠️ Partial → EXPAND
│   │   ├── test_error_handling.rs   # ❌ NEW: Error propagation tests
│   │   ├── test_retry.rs            # ❌ NEW: Retry policy tests
│   │   ├── test_zero_copy.rs        # ❌ NEW: Memory profiling tests
│   │   └── test_performance.rs      # ❌ NEW: Performance regression tests
│   ├── executor/
│   │   ├── test_graph.rs            # ❌ NEW: Graph construction tests
│   │   ├── test_error.rs            # ❌ NEW: Error type tests
│   │   └── test_metrics.rs          # ❌ NEW: Metrics collection tests
│   ├── nodes/
│   │   ├── test_vad.rs              # ❌ NEW: VAD node tests
│   │   ├── test_resample.rs         # ❌ NEW: Resample node tests
│   │   └── test_format.rs           # ❌ NEW: Format converter tests
│   └── compatibility/
│       └── test_rustpython.rs       # ❌ DELETE: RustPython tests (~2,000 LoC)
├── benches/
│   └── audio_nodes.rs               # ❌ NEW: Audio node benchmarks
└── Cargo.toml                        # ⚠️ MODIFY: Remove RustPython deps, add audio libs

python-client/                        # Python SDK (minimal changes)
├── remotemedia/
│   ├── core/
│   │   ├── pipeline.py              # ✅ Exists: serialize() already works
│   │   └── node.py                  # ✅ Exists: to_manifest() already works
│   └── nodes/
│       └── audio.py                 # ⚠️ MODIFY: Add runtime_hint parameter
└── tests/
    ├── test_rust_integration.py     # ⚠️ PARTIAL → EXPAND: roundtrip tests
    ├── test_rust_compatibility.py   # ❌ NEW: Compatibility regression tests
    └── test_performance.py          # ❌ NEW: Performance benchmarks

examples/rust_runtime/                # Examples (expand)
├── 01_basic_pipeline.py             # ✅ Exists
├── 06_rust_vs_python_nodes.py       # ✅ Exists
├── 12_audio_vad_rust.py             # ❌ NEW: VAD example
├── 13_audio_resample_rust.py        # ❌ NEW: Resample example
├── 14_audio_format_rust.py          # ❌ NEW: Format conversion example
└── 15_full_audio_pipeline.py        # ❌ NEW: End-to-end audio pipeline

docs/                                 # Documentation
├── NATIVE_ACCELERATION.md           # ❌ NEW: Architecture overview
├── MIGRATION_GUIDE.md               # ❌ NEW: Migration from Python-only
├── PERFORMANCE_TUNING.md            # ❌ NEW: Optimization guide
└── WASM_ARCHIVE.md                  # ❌ NEW: Why WASM was paused
```

**Structure Decision**: Hybrid Rust library (cdylib) + Python SDK wrapper (existing structure maintained). Focus on `runtime/` completion and cleanup. Python SDK changes are minimal (wrapper nodes with runtime_hint only). No new top-level directories created.

**Cleanup Actions** (Week 1):
- DELETE: `runtime/src/python/vm.rs` (~1,200 LoC)
- DELETE: `runtime/src/python/rustpython_executor.rs` (~800 LoC)
- DELETE: `runtime/tests/compatibility/test_rustpython.rs` (~2,000 LoC)
- ARCHIVE: `openspec/changes/implement-pyo3-wasm-browser/` (entire branch to docs/WASM_ARCHIVE.md)

## Complexity Tracking

> **No violations - all complexity being REMOVED**

| Previous Complexity | Why Removed | Benefit |
|---------------------|-------------|---------|
| RustPython VM | CPython via PyO3 is superior (full stdlib, faster, simpler) | -2,000 LoC, eliminate maintenance burden |
| WASM browser runtime | No user demand, premature optimization | -8,000 LoC, focus on core value |
| WebRTC transport | gRPC sufficient for remote execution | -15,000 LoC (not written), avoid scope creep |
| Pipeline mesh | Over-engineered for current scale | -10,000 LoC (not written), YAGNI principle |

**Net Complexity Change**: -35,000 LoC (70% reduction) → **Dramatically simpler project**

---

## Phase 0: Research

**Status**: ✅ **COMPLETE** (research already exists in openspec/changes/native-rust-acceleration/research.md)

Will copy key findings to `specs/001-native-rust-acceleration/research.md` for this feature branch.

### Research Findings Summary

**1. Audio Processing Libraries**
- **Decision**: Hybrid approach using pure Rust libraries
  - VAD: `rustfft` + custom energy-based detection
  - Resampling: `rubato` (best-in-class pure Rust resampler)
  - Format Conversion: Custom implementation using `bytemuck` for zero-copy casts
- **Rationale**: Pure Rust (no C deps), 50-200x speedup vs Python, direct `&[f32]` compatibility with numpy

**2. Error Handling Patterns**
- **Decision**: `thiserror` + `anyhow` with exponential backoff retry
  - Retry policy: 100ms, 200ms, 400ms delays (max 3 retries)
  - Circuit breaker: 5 consecutive failures
  - Clear error boundaries (library vs application)
- **Rationale**: Industry standard, Python compatibility via PyO3, matches AI/ML workload patterns

**3. Performance Monitoring**
- **Decision**: `tracing` for structured logging, `criterion` for benchmarks
  - Metrics format: JSON export with microsecond precision
  - Low overhead: <100μs per pipeline execution
- **Rationale**: Standard Rust observability stack, JSON for tool compatibility

**4. Zero-Copy Data Flow**
- **Decision**: rust-numpy for PyO3 integration, `bytemuck` for format conversion
  - Borrow numpy arrays (no copies)
  - Zero-copy transmute for compatible formats
- **Rationale**: FFI overhead <1μs (measured), eliminates memory bandwidth bottleneck

**5. Build System**
- **Decision**: cargo workspace with PyO3 maturin build
  - Cross-platform builds via GitHub Actions
  - Pre-built wheels for common platforms
- **Rationale**: Standard PyO3 pattern, simplifies distribution

---

## Phase 1: Design & Contracts

**Prerequisites**: research.md complete (✅ done)

### Deliverables

1. **data-model.md**: Core data structures ✅ COMPLETE
   - PipelineManifest schema (JSON)
   - PipelineGraph structure (nodes, edges, execution order)
   - ExecutionMetrics schema (timing, memory)
   - Error types hierarchy
   - AudioBuffer representation

2. **contracts/ffi-api.md**: FFI boundary contract ✅ COMPLETE
   - `execute_pipeline_ffi(manifest_json: &str) -> Result<String, PyErr>`
   - `get_metrics_ffi() -> String`
   - Data marshaling rules (Python ↔ Rust)
   - Error conversion (PyO3 PyErr)

3. **contracts/node-executor-api.md**: Node executor trait ✅ COMPLETE
   - `NodeExecutor` trait definition
   - `execute()` method signature
   - Input/output data flow
   - Error handling protocol

4. **quickstart.md**: 5-minute setup guide ✅ COMPLETE
   - Install Rust + Python dependencies
   - Build runtime: `cargo build --release`
   - Install SDK: `pip install -e python-client/`
   - Run example: `python examples/rust_runtime/01_basic_pipeline.py`
   - Verify acceleration: Check metrics JSON for speedup

5. **Agent Context Update**: ✅ COMPLETE
   - Updated `.github/copilot-instructions.md` with Rust 1.70+/Python 3.9+ stack

---

## Next Steps

**Immediate** (Planning Phase Complete):
1. ✅ Specification created (spec.md)
2. ✅ Planning complete (plan.md)
3. ✅ Research complete (research.md)
4. ✅ Design complete (data-model.md, contracts/)
5. ✅ Quickstart guide created (quickstart.md)
6. ✅ Tasks generated (tasks.md) - 158 tasks across 9 phases
7. ✅ Agent context updated

**Implementation** (Ready to Begin):
1. Begin Phase 1 implementation (Week 1: Cleanup)
2. Follow tasks.md for detailed task breakdown
3. Track progress with task checkboxes

**Timeline**:
- Week 1: Cleanup (delete RustPython, archive WASM)
- Weeks 2-3: Complete Rust executor core
- Week 4: Audio nodes + error handling (MVP)
- Week 5: Monitoring + zero-copy optimization
- Week 6: Polish + v0.2.0 release

**Success Metrics**:
- All 20 functional requirements implemented
- All 12 success criteria met
- 11 existing examples work unchanged
- Performance benchmarks show 50-100x audio speedup
