# Implementation Plan: Native Rust Acceleration for AI/ML Pipelines

**Branch**: `feat/native-acceleration` | **Date**: 2025-10-27 | **Proposal**: [proposal.md](proposal.md)

## Summary

Refocus RemoteMedia SDK on its core value proposition: **accelerating Python AI/ML pipelines through transparent Rust acceleration**. Complete the Rust runtime executor (currently 60% done), delete unnecessary complexity (RustPython VM, WASM browser, WebRTC mesh), and add high-value Rust implementations of compute-heavy audio processing nodes. Target: 50-100x speedup for audio preprocessing with zero code changes.

## Technical Context

**Language/Version**: Rust 1.70+ (runtime), Python 3.9+ (SDK)  
**Primary Dependencies**: 
- Rust: tokio (async runtime), PyO3 0.26 (Python FFI), serde (serialization)
- Python: numpy (arrays), PyAV (audio/video), pytest (testing)

**Storage**: N/A (in-memory pipeline execution)  
**Testing**: cargo test (Rust), pytest (Python integration)  
**Target Platform**: Linux/macOS/Windows native (x86_64, aarch64)  
**Project Type**: Hybrid library (Rust cdylib + Python SDK)  
**Performance Goals**: 
- FFI overhead: <1μs per call (✅ achieved)
- Rust nodes: 100-300x faster than Python (✅ 193-361x for math ops)
- Audio preprocessing: 50-100x faster than Python (target)

**Constraints**: 
- Zero code changes for existing pipelines
- 100% Python stdlib compatibility via CPython (PyO3)
- Cross-platform (no platform-specific audio APIs)

**Scale/Scope**: 
- ~15,000 LoC (down from ~50,000 after cleanup)
- 50+ SDK nodes (target: 80% have Rust equivalents)
- 20+ examples demonstrating acceleration

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Note**: The project constitution is currently a template. Using project conventions from `openspec/project.md` and `openspec/AGENTS.md`:

### Simplicity Principle (Implicit)
- ✅ **PASS**: Removing unnecessary complexity (RustPython VM, WASM, WebRTC)
- ✅ **PASS**: Single runtime path (Rust + CPython via PyO3)
- ✅ **PASS**: Simple gRPC for remote execution (no WebRTC complexity)

### Performance Standards (Implicit)
- ✅ **PASS**: Zero-copy numpy arrays via rust-numpy
- ✅ **PASS**: Microsecond FFI overhead measured
- ✅ **PASS**: Comprehensive benchmarking planned

### Testing Requirements (From project.md)
- ⚠️ **REVIEW NEEDED**: Unit tests exist, integration tests planned
- ✅ **PASS**: Benchmark suite for performance regression
- ⚠️ **REVIEW NEEDED**: Error handling tests (planned in Phase 1)

### Documentation Standards (From project.md)
- ✅ **PASS**: Migration guide planned
- ✅ **PASS**: Performance tuning guide planned
- ✅ **PASS**: API documentation (existing docstrings)

**Overall**: ✅ PASS with minor gaps (integration tests, error handling tests) to be filled in Phase 1

## Project Structure

### Documentation (this feature)

```text
openspec/changes/native-rust-acceleration/
├── proposal.md          # This change proposal
├── plan.md              # This file (implementation plan)
├── research.md          # Phase 0: Technical research (to be created)
├── design.md            # Phase 1: Architecture decisions (to be created)
├── tasks.md             # Phase 2: Task breakdown (to be created)
└── specs/
    └── rust-acceleration/
        └── spec.md      # Requirements specification (to be created)
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
│   │   ├── vm.rs                    # ❌ DELETE: RustPython VM
│   │   └── rustpython_executor.rs   # ❌ DELETE: RustPython executor
│   ├── nodes/
│   │   ├── mod.rs                   # ✅ Exists: Node registry
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
│   │   └── test_performance.rs      # ❌ NEW: Performance regression tests
│   └── compatibility/
│       └── test_rustpython.rs       # ❌ DELETE: RustPython tests
└── Cargo.toml                        # ⚠️ MODIFY: Remove RustPython deps

python-client/                        # Python SDK (minimal changes)
├── remotemedia/
│   ├── core/
│   │   ├── pipeline.py              # ✅ Exists: serialize() already works
│   │   └── node.py                  # ✅ Exists: to_manifest() already works
│   └── nodes/
│       └── audio.py                 # ⚠️ MODIFY: Add Rust node wrappers
└── tests/
    ├── test_rust_integration.py     # ⚠️ Partial → EXPAND
    └── test_performance.py          # ❌ NEW: Performance benchmarks

examples/rust_runtime/                # Examples (expand)
├── 01_basic_pipeline.py             # ✅ Exists
├── 06_rust_vs_python_nodes.py       # ✅ Exists
├── 12_audio_vad_rust.py             # ❌ NEW: VAD example
├── 13_audio_resample_rust.py        # ❌ NEW: Resample example
└── 14_full_audio_pipeline.py        # ❌ NEW: End-to-end audio pipeline

docs/                                 # Documentation
├── NATIVE_ACCELERATION.md           # ❌ NEW: Architecture overview
├── MIGRATION_GUIDE.md               # ❌ NEW: Migration from Python-only
├── PERFORMANCE_TUNING.md            # ❌ NEW: Optimization guide
└── WASM_ARCHIVE.md                  # ❌ NEW: Why WASM was paused
```

**Structure Decision**: Hybrid Rust library + Python SDK (existing structure maintained). Focus on `runtime/` completion and cleanup. Python SDK changes are minimal (wrapper nodes only).

**Cleanup Actions**:
- DELETE: `runtime/src/python/vm.rs` (~1,200 LoC)
- DELETE: `runtime/src/python/rustpython_executor.rs` (~800 LoC)
- DELETE: `runtime/tests/compatibility/test_rustpython.rs` (~2,000 LoC)
- ARCHIVE: `openspec/changes/implement-pyo3-wasm-browser/` (entire branch)

## Complexity Tracking

> **No violations - all complexity being REMOVED**

| Previous Complexity | Why Removed | Benefit |
|---------------------|-------------|---------|
| RustPython VM | CPython via PyO3 is superior (full stdlib, faster) | -2,000 LoC, simpler maintenance |
| WASM browser runtime | No user demand, premature optimization | -8,000 LoC, focus on core value |
| WebRTC transport | gRPC sufficient for remote execution | -15,000 LoC (not written), avoid scope creep |
| Pipeline mesh | Over-engineered for current scale | -10,000 LoC (not written), YAGNI |

**Net Complexity Change**: -35,000 LoC (70% reduction) → **Dramatically simpler project**

---

## Phase 0: Research (Next Steps)

**Prerequisites**: None (can start immediately)

**Research Tasks**:

1. **Audio Processing Libraries**
   - Evaluate: `rubato` (Rust resample), `rustfft` (FFT), `dasp` (audio DSP)
   - Decision: Which library for VAD, resample, format conversion?
   - Deliverable: `research.md` section on audio processing

2. **Error Handling Patterns**
   - Review: Rust error handling best practices (thiserror, anyhow)
   - Review: Retry policies (exponential backoff, circuit breaker)
   - Deliverable: `research.md` section on error handling

3. **Performance Monitoring**
   - Evaluate: `tracing` (structured logging), `criterion` (benchmarking)
   - Decision: Metrics format (JSON, Prometheus, custom?)
   - Deliverable: `research.md` section on observability

4. **Zero-Copy Audio Buffers**
   - Research: PyAV → numpy → Rust zero-copy patterns
   - Constraint: Must work with numpy arrays from PyAV
   - Deliverable: `research.md` section on data flow

**Output**: `research.md` with concrete technology choices and rationales

---

## Phase 1: Design & Contracts (After Research)

**Prerequisites**: `research.md` complete

**Design Tasks**:

1. **Data Model** (`data-model.md`)
   - Pipeline graph structure (nodes, edges, execution order)
   - Error types (ParseError, ExecutionError, RetryableError)
   - Metrics schema (execution time, memory usage, node performance)

2. **API Contracts** (`contracts/`)
   - Rust executor API (execute_pipeline, execute_with_input)
   - Node trait (initialize, process, cleanup)
   - Error handling API (retry policies, fallback strategies)

3. **Quickstart Guide** (`quickstart.md`)
   - "Add Rust audio node in 5 minutes"
   - "Profile your pipeline performance"
   - "Handle errors gracefully"

4. **Agent Context Update**
   - Run: `.specify/scripts/powershell/update-agent-context.ps1 -AgentType copilot`
   - Add: Audio processing libraries, error handling patterns
   - Preserve: Existing context between markers

**Output**: `design.md`, `data-model.md`, `contracts/*`, `quickstart.md`, updated agent context

---

## Phase 2: Tasks (After Design - Separate Command)

**Note**: This phase is executed by `/speckit.tasks` command, NOT by `/speckit.plan`

**Task Categories** (preview):

1. **Cleanup** (Week 1)
   - Delete RustPython VM code
   - Archive WASM branch
   - Update documentation

2. **Executor Core** (Weeks 2-3)
   - Pipeline graph structure
   - Topological sort
   - Error handling
   - Performance metrics

3. **Audio Nodes** (Weeks 4-5)
   - VADNode (Rust)
   - ResampleNode (Rust)
   - FormatConverterNode (Rust)

4. **Production Hardening** (Week 6)
   - Integration tests
   - Performance benchmarks
   - Migration guide
   - Release preparation

**Output**: Detailed `tasks.md` with ~40-50 concrete implementation tasks

---

## Risks & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Audio library performance worse than expected | Low | Medium | Benchmark early (Phase 0), have fallback to PyAV |
| Breaking changes to existing examples | Low | High | Comprehensive regression testing |
| PyO3 GIL contention | Medium | Low | Already using release_gil patterns, monitor metrics |
| Rust learning curve for contributors | High | Low | Good documentation, clear FFI boundaries |

---

## Success Metrics

### Quantitative
- **Performance**: Audio preprocessing 50-100x faster than Python
- **Code Quality**: Test coverage >80%, zero clippy warnings
- **Maintenance**: -70% LoC (35,000 lines removed)
- **Developer Experience**: 100% of examples work with zero code changes

### Qualitative
- **Developer Feedback**: "I didn't change any code and my pipeline runs 10x faster"
- **Use Case Fit**: Solving real audio preprocessing bottlenecks
- **Community Growth**: More contributors (simpler codebase)

---

## Next Actions

1. ✅ Create `proposal.md` (this proposal)
2. ✅ Create `plan.md` (this document)
3. ⏳ Execute Phase 0: Research (`/speckit.plan` command continues to `research.md`)
4. ⏳ Execute Phase 1: Design (after research complete)
5. ⏳ Execute Phase 2: Tasks (`/speckit.tasks` command)
6. ⏳ Implement (follow `tasks.md`)

**Command Flow**:
- `/speckit.plan` → Creates `plan.md`, `research.md`, `design.md`, `contracts/*`
- `/speckit.tasks` → Creates `tasks.md` with detailed implementation steps
- `/speckit.implement` → Executes tasks

---

**Status**: Phase 0 (Research) ready to begin  
**Estimated Completion**: 6 weeks (through Phase 2 implementation)  
**Dependencies**: None (can start immediately)
