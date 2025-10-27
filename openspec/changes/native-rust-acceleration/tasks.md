# Implementation Tasks: Native Rust Acceleration

**Feature**: Native Rust Acceleration for AI/ML Pipelines  
**Branch**: `feat/native-acceleration`  
**Total Tasks**: 52  
**Estimated Duration**: 6 weeks

## Overview

This document breaks down the implementation into executable tasks organized by functional requirements. Each requirement from spec.md becomes a phase with independent test criteria.

## Task Summary

| Phase | Requirement | Tasks | Duration |
|-------|-------------|-------|----------|
| **Phase 1** | Setup & Cleanup | 6 | Week 1 |
| **Phase 2** | Complete Rust Pipeline Executor | 12 | Weeks 2-3 |
| **Phase 3** | Audio Processing Nodes | 10 | Week 4 |
| **Phase 4** | Error Handling with Retry | 8 | Week 4 |
| **Phase 5** | Performance Monitoring | 6 | Week 5 |
| **Phase 6** | Zero-Copy Data Flow | 4 | Week 5 |
| **Phase 7** | Python SDK Transparency | 4 | Week 6 |
| **Phase 8** | Polish & Documentation | 2 | Week 6 |

---

## Phase 1: Setup & Cleanup (Week 1)

**Goal**: Clean up codebase by removing unnecessary complexity and prepare for focused development.

**Success Criteria**:
- ✅ RustPython code deleted (vm.rs, rustpython_executor.rs)
- ✅ WASM branch archived with documentation
- ✅ New branch created and dependencies updated
- ✅ All existing tests still pass

### Tasks

- [ ] T001 Archive WASM browser implementation to docs/WASM_ARCHIVE.md
- [ ] T002 Create new branch `feat/native-acceleration` from main
- [ ] T003 Delete runtime/src/python/vm.rs (RustPython VM, ~1,200 LoC)
- [ ] T004 Delete runtime/src/python/rustpython_executor.rs (~800 LoC)
- [ ] T005 Delete runtime/tests/compatibility/test_rustpython.rs (~2,000 LoC)
- [ ] T006 [P] Update runtime/Cargo.toml to remove RustPython dependencies

---

## Phase 2: Complete Rust Pipeline Executor (Weeks 2-3)

**Requirement**: Complete Rust Pipeline Executor (spec.md)

**Goal**: Finish the 40% remaining work on the Rust executor core - graph construction, topological sort, async execution orchestration.

**Success Criteria**:
- ✅ Manifest parsing with schema validation
- ✅ Pipeline graph constructed from manifest
- ✅ Topological sort determines execution order
- ✅ Cycle detection prevents invalid graphs
- ✅ Async execution of nodes in correct order
- ✅ All executor unit tests pass

### Tasks

- [ ] T007 Create runtime/src/executor/graph.rs with PipelineGraph struct
- [ ] T008 Implement PipelineGraph::from_manifest() parsing in runtime/src/executor/graph.rs
- [ ] T009 [P] Implement topological_sort() using Kahn's algorithm in runtime/src/executor/graph.rs
- [ ] T010 [P] Implement cycle detection in topological_sort() in runtime/src/executor/graph.rs
- [ ] T011 Add adjacency list construction in PipelineGraph::from_manifest()
- [ ] T012 Create runtime/src/executor/scheduler.rs for execution orchestration
- [ ] T013 Implement Executor::execute() with graph traversal in runtime/src/executor/mod.rs
- [ ] T014 Add async node execution loop with data passing in runtime/src/executor/mod.rs
- [ ] T015 Implement NodeInstance wrapper in runtime/src/executor/graph.rs
- [ ] T016 Add manifest validation in runtime/src/manifest/mod.rs
- [ ] T017 Write unit tests for graph construction in runtime/tests/executor/test_graph.rs
- [ ] T018 Write unit tests for topological sort and cycle detection in runtime/tests/executor/test_graph.rs

---

## Phase 3: Audio Processing Nodes (Week 4)

**Requirement**: Audio Processing Nodes (Rust Native) (spec.md)

**Goal**: Implement high-performance Rust implementations of VAD, resampling, and format conversion.

**Success Criteria**:
- ✅ VADNode processes audio in <50μs per 30ms frame
- ✅ ResampleNode processes in <2ms per second of audio
- ✅ FormatConverterNode uses zero-copy where possible
- ✅ All audio nodes pass unit tests
- ✅ Performance benchmarks meet targets

### Tasks

- [ ] T019 Create runtime/src/nodes/audio/mod.rs module structure
- [ ] T020 [P] Add rubato dependency to runtime/Cargo.toml
- [ ] T021 [P] Add rustfft dependency to runtime/Cargo.toml
- [ ] T022 [P] Add bytemuck dependency to runtime/Cargo.toml
- [ ] T023 Implement VADNode struct in runtime/src/nodes/audio/vad.rs
- [ ] T024 Implement VADNode::compute_energy() using rustfft in runtime/src/nodes/audio/vad.rs
- [ ] T025 Implement VADNode::process() with segment detection in runtime/src/nodes/audio/vad.rs
- [ ] T026 Implement ResampleNode struct in runtime/src/nodes/audio/resample.rs
- [ ] T027 Integrate rubato Resampler in ResampleNode::process() in runtime/src/nodes/audio/resample.rs
- [ ] T028 Implement FormatConverterNode struct in runtime/src/nodes/audio/format.rs
- [ ] T029 Implement format conversions (f32↔i16, f32↔i32) using bytemuck in runtime/src/nodes/audio/format.rs
- [ ] T030 Register audio nodes in runtime/src/nodes/registry.rs
- [ ] T031 Write unit tests for VADNode in runtime/tests/nodes/test_vad.rs
- [ ] T032 Write unit tests for ResampleNode in runtime/tests/nodes/test_resample.rs
- [ ] T033 Write unit tests for FormatConverterNode in runtime/tests/nodes/test_format.rs
- [ ] T034 Write performance benchmarks for audio nodes in runtime/benches/audio_nodes.rs

---

## Phase 4: Error Handling with Retry (Week 4)

**Requirement**: Error Handling with Retry (spec.md)

**Goal**: Implement comprehensive error handling with exponential backoff retry and circuit breaker.

**Success Criteria**:
- ✅ Retry policy retries transient errors 3 times with exponential backoff
- ✅ Non-retryable errors propagate immediately
- ✅ Circuit breaker trips after 5 consecutive failures
- ✅ Error context includes node ID, operation, stack trace
- ✅ Python exceptions convert correctly across FFI

### Tasks

- [ ] T035 Create runtime/src/executor/error.rs with ExecutorError enum
- [ ] T036 Add thiserror and anyhow dependencies to runtime/Cargo.toml
- [ ] T037 Implement error variant types (ManifestError, GraphError, CycleError, etc.) in runtime/src/executor/error.rs
- [ ] T038 Implement ExecutorError::is_retryable() method in runtime/src/executor/error.rs
- [ ] T039 Implement ExecutorError::to_python_error() for PyO3 conversion in runtime/src/executor/error.rs
- [ ] T040 Create RetryPolicy struct in runtime/src/executor/error.rs
- [ ] T041 Implement RetryPolicy::exponential_backoff() with delays 100ms, 200ms, 400ms in runtime/src/executor/error.rs
- [ ] T042 Implement RetryPolicy::execute() async wrapper in runtime/src/executor/error.rs
- [ ] T043 Integrate retry policy into Executor::execute_node() in runtime/src/executor/mod.rs
- [ ] T044 Add circuit breaker logic (5 consecutive failures) in runtime/src/executor/mod.rs
- [ ] T045 Write unit tests for error types in runtime/tests/executor/test_error.rs
- [ ] T046 Write integration tests for retry behavior in runtime/tests/integration/test_retry.rs

---

## Phase 5: Performance Monitoring (Week 5)

**Requirement**: Performance Monitoring (spec.md)

**Goal**: Track execution time, memory usage, and export metrics as JSON.

**Success Criteria**:
- ✅ Execution time tracked with microsecond precision per node
- ✅ Memory usage tracked per node
- ✅ Metrics exported as JSON with schema
- ✅ Overhead <100μs per execution

### Tasks

- [ ] T047 Create runtime/src/executor/metrics.rs with PipelineMetrics struct
- [ ] T048 Add tracing dependency to runtime/Cargo.toml for structured logging
- [ ] T049 Implement MetricsCollector with start/record/finalize methods in runtime/src/executor/metrics.rs
- [ ] T050 Add timing instrumentation in Executor::execute() in runtime/src/executor/mod.rs
- [ ] T051 Implement memory usage tracking (OS-specific APIs) in runtime/src/executor/metrics.rs
- [ ] T052 Implement PipelineMetrics::to_json() export in runtime/src/executor/metrics.rs
- [ ] T053 Update execute_pipeline FFI to return metrics in runtime/src/python/ffi.rs
- [ ] T054 Write unit tests for metrics collection in runtime/tests/executor/test_metrics.rs

---

## Phase 6: Zero-Copy Data Flow (Week 5)

**Requirement**: Zero-Copy Data Flow (spec.md)

**Goal**: Minimize data copying between Python and Rust using rust-numpy.

**Success Criteria**:
- ✅ Numpy input borrows via rust-numpy (no copy)
- ✅ Format conversions use bytemuck zero-copy casts
- ✅ FFI overhead remains <1μs
- ✅ Memory profiling confirms zero copies

### Tasks

- [ ] T055 Audit existing numpy marshaling in runtime/src/python/numpy_marshal.rs
- [ ] T056 Optimize numpy_to_json to use zero-copy views in runtime/src/python/numpy_marshal.rs
- [ ] T057 Optimize json_to_numpy to avoid intermediate allocations in runtime/src/python/numpy_marshal.rs
- [ ] T058 Add memory profiling tests in runtime/tests/integration/test_zero_copy.rs

---

## Phase 7: Python SDK Transparency (Week 6)

**Requirement**: Python SDK Transparency (spec.md)

**Goal**: Ensure existing Python pipelines work with zero code changes.

**Success Criteria**:
- ✅ All existing examples work unchanged
- ✅ Automatic runtime selection (Rust native → CPython fallback)
- ✅ Performance improvements transparent to users
- ✅ Compatibility tests pass 100%

### Tasks

- [ ] T059 Implement automatic runtime selection in runtime/src/nodes/registry.rs
- [ ] T060 Add runtime_hint support to manifest schema in runtime/src/manifest/mod.rs
- [ ] T061 Test all examples in examples/rust_runtime/ for compatibility
- [ ] T062 Write compatibility regression tests in python-client/tests/test_rust_compatibility.py

---

## Phase 8: Polish & Documentation (Week 6)

**Goal**: Production-ready release with comprehensive documentation.

**Success Criteria**:
- ✅ Migration guide complete
- ✅ Performance tuning guide complete
- ✅ All tests passing
- ✅ v0.2.0 release ready

### Tasks

- [ ] T063 Write docs/NATIVE_ACCELERATION.md architecture overview
- [ ] T064 Write docs/MIGRATION_GUIDE.md for users upgrading from Python-only
- [ ] T065 Write docs/PERFORMANCE_TUNING.md optimization guide
- [ ] T066 Update examples/rust_runtime/README.md with new audio nodes
- [ ] T067 Run full test suite and fix any regressions
- [ ] T068 Create v0.2.0 release notes

---

## Dependencies & Execution Order

### Critical Path (Must Complete in Order)

1. **Phase 1** (Setup) → Blocks all other phases
2. **Phase 2** (Executor Core) → Blocks phases 3-7
3. **Phases 3-6** (Features) → Can run in parallel after Phase 2
4. **Phase 7** (Transparency) → Requires Phase 2 + Phase 3 complete
5. **Phase 8** (Polish) → Requires all phases complete

### Parallel Opportunities

**After Phase 1 complete**:
- No parallelism (Phase 2 is blocking)

**After Phase 2 complete**:
- Phase 3 (Audio Nodes) ║ Phase 4 (Error Handling) ║ Phase 5 (Metrics)
- Phase 6 (Zero-Copy) can run with above phases
- All 4 phases work on different files, no conflicts

**Week 4 Example**:
- Developer A: Audio nodes (T019-T034)
- Developer B: Error handling (T035-T046)
- Both can work in parallel, no merge conflicts

---

## Implementation Strategy

### MVP Scope (Week 1-3)

**Minimum for v0.2.0-alpha**:
- Phase 1: Cleanup (T001-T006)
- Phase 2: Executor Core (T007-T018)
- Phase 3: At least VADNode (T019-T025, T031)

This provides:
- Working executor with one Rust audio node
- Proof of concept for performance
- Foundation for remaining features

### Incremental Delivery

- **Week 1**: Cleanup, branch ready
- **Week 2-3**: Executor complete, basic audio node
- **Week 4**: All audio nodes + error handling
- **Week 5**: Monitoring + zero-copy optimization
- **Week 6**: Polish + release

---

## Testing Strategy

### Unit Tests (Rust)

- **Executor**: Graph construction, topological sort, cycle detection
- **Audio Nodes**: Each node with various inputs
- **Error Handling**: Retry policies, circuit breaker
- **Metrics**: Collection accuracy, JSON export

**Target**: >80% code coverage

### Integration Tests (Python + Rust)

- **Roundtrip**: Python → Rust → Python data flow
- **Error Propagation**: Errors across FFI boundary
- **Performance**: Regression detection with criterion
- **Compatibility**: All examples work unchanged

**Target**: 100% of examples passing

### Performance Tests

- **Benchmarks**: criterion for each audio node
- **Regression**: Automated performance monitoring
- **Profiling**: Memory leak detection with valgrind

**Target**: All targets met (see spec.md performance table)

---

## Format Validation

✅ **All tasks follow checklist format**:
- Checkbox: `- [ ]` (markdown checkbox)
- Task ID: Sequential (T001-T068)
- [P] marker: Used for parallelizable tasks
- File paths: Included in all implementation tasks
- Clear descriptions: Actionable without additional context

---

## Task Count Summary

- **Total Tasks**: 68
- **Parallelizable**: 11 tasks marked [P]
- **Phase 1 (Setup)**: 6 tasks
- **Phase 2 (Executor)**: 12 tasks
- **Phase 3 (Audio)**: 16 tasks
- **Phase 4 (Error)**: 12 tasks
- **Phase 5 (Metrics)**: 8 tasks
- **Phase 6 (Zero-Copy)**: 4 tasks
- **Phase 7 (Transparency)**: 4 tasks
- **Phase 8 (Polish)**: 6 tasks

**Estimated Total Duration**: 6 weeks (with parallel execution)

---

## Next Steps

1. ✅ Review and approve this task breakdown
2. ⏳ Begin Phase 1 (Setup & Cleanup)
3. ⏳ Execute tasks sequentially within each phase
4. ⏳ Run tests after each phase completion
5. ⏳ Release v0.2.0 after Phase 8

**Ready to begin implementation!**
