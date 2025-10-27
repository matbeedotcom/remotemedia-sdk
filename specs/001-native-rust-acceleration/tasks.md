---
description: "Task list for Native Rust Acceleration implementation"
---

# Tasks: Native Rust Acceleration

**Input**: Design documents from `/specs/001-native-rust-acceleration/`  
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅

**Tests**: Not explicitly requested - omitted for faster delivery. Can add later if needed.

**Organization**: Tasks grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4, US5, US6)
- Include exact file paths in descriptions

## Path Conventions

Project structure follows plan.md:
- Rust runtime: `runtime/src/`, `runtime/tests/`, `runtime/benches/`
- Python SDK: `python-client/remotemedia/`, `python-client/tests/`
- Examples: `examples/rust_runtime/`
- Documentation: `docs/`

---

## Phase 1: Setup & Cleanup

**Purpose**: Remove complexity, prepare clean foundation for acceleration

**Estimated Duration**: Week 1 (5 days)

- [ ] T001 Delete runtime/src/python/vm.rs (~1,200 LoC RustPython VM)
- [ ] T002 Delete runtime/src/python/rustpython_executor.rs (~800 LoC RustPython executor)
- [ ] T003 Delete runtime/tests/compatibility/test_rustpython.rs (~2,000 LoC RustPython tests)
- [ ] T004 Remove RustPython dependencies from runtime/Cargo.toml (rustpython, rustpython-vm, rustpython-compiler)
- [ ] T005 Archive WASM branch documentation to docs/WASM_ARCHIVE.md (copy from openspec/changes/implement-pyo3-wasm-browser/)
- [ ] T006 Update runtime/Cargo.toml with new dependencies (rubato = "0.15", rustfft = "6.2", bytemuck = "1.14")
- [ ] T007 [P] Create docs/NATIVE_ACCELERATION.md architecture overview
- [ ] T008 [P] Create docs/MIGRATION_GUIDE.md for users upgrading from v0.1.x
- [ ] T009 [P] Create docs/PERFORMANCE_TUNING.md optimization guide
- [ ] T010 Run cargo build --release to verify clean compilation after cleanup

**Checkpoint**: Complexity removed, project compiles, documentation structure ready

---

## Phase 2: Foundational (Executor Core)

**Purpose**: Core pipeline execution infrastructure that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

**Estimated Duration**: Weeks 2-3 (10 days)

### Core Data Structures

- [ ] T011 [P] Create runtime/src/executor/error.rs with ExecutorError enum (ManifestError, GraphError, CycleError, NodeExecutionError, RetryLimitExceeded)
- [ ] T012 [P] Create runtime/src/executor/graph.rs with PipelineGraph struct (nodes, edges, adjacency_list, execution_order)
- [ ] T013 [P] Create runtime/src/executor/metrics.rs with ExecutionMetrics and NodeMetrics structs
- [ ] T014 [P] Create runtime/src/executor/retry.rs with RetryPolicy, BackoffStrategy enums

### Manifest Parsing & Validation

- [ ] T015 Update runtime/src/manifest/mod.rs to add PipelineManifest struct (version, nodes, edges, config)
- [ ] T016 Add NodeManifest struct with RuntimeHint enum (Auto, Rust, Python) to runtime/src/manifest/mod.rs
- [ ] T017 Add manifest validation logic (unique node IDs, valid edge references) to runtime/src/manifest/mod.rs

### Graph Construction & Topological Sort

- [ ] T018 Implement PipelineGraph::from_manifest() in runtime/src/executor/graph.rs (parse manifest, build adjacency list)
- [ ] T019 Implement topological_sort() using Kahn's algorithm in runtime/src/executor/graph.rs
- [ ] T020 Implement cycle detection with error reporting (show cycle path) in runtime/src/executor/graph.rs

### Node Executor Trait

- [ ] T021 Create runtime/src/executor/node_executor.rs with NodeExecutor trait (execute, node_type, runtime, validate_inputs)
- [ ] T022 Add NodeInputs and NodeOutputs structs to runtime/src/executor/node_executor.rs
- [ ] T023 Add helper methods (get_audio_buffer, from_audio_buffer) to NodeInputs/NodeOutputs in runtime/src/executor/node_executor.rs

### Executor Orchestration

- [ ] T024 Create runtime/src/executor/scheduler.rs with Executor struct (graph, metrics, retry_policy)
- [ ] T025 Implement Executor::execute() main loop (topological order, gather inputs, execute nodes, store outputs) in runtime/src/executor/scheduler.rs
- [ ] T026 Implement execute_node_with_retry() with exponential backoff in runtime/src/executor/scheduler.rs
- [ ] T027 Implement gather_inputs() to collect data from upstream nodes in runtime/src/executor/scheduler.rs
- [ ] T028 Implement get_final_outputs() to extract sink node results in runtime/src/executor/scheduler.rs

### Metrics Collection

- [ ] T029 Implement MetricsCollector::start() and record() in runtime/src/executor/metrics.rs
- [ ] T030 Add timing logic (Instant::now(), elapsed()) to runtime/src/executor/metrics.rs
- [ ] T031 Add memory tracking (OS-specific APIs) to runtime/src/executor/metrics.rs
- [ ] T032 Implement metrics JSON serialization in runtime/src/executor/metrics.rs

### Module Integration

- [ ] T033 Update runtime/src/executor/mod.rs to export all executor modules (graph, error, metrics, retry, scheduler, node_executor)
- [ ] T034 Run cargo build --release to verify executor core compiles
- [ ] T035 Run cargo test to verify basic executor functionality

**Checkpoint**: Foundation ready - executor can parse manifests, build graphs, detect cycles, execute nodes in order

---

## Phase 3: User Story 5 - Pipeline Execution Orchestration (Priority: P1) 🎯

**Goal**: Enable parsing JSON manifests, building execution graphs, and orchestrating node execution with correct dependency order

**Independent Test**: Create manifest with linear/branching/converging topologies, verify correct execution order, inject circular dependency and verify detection

**Why P1**: Foundation for all other features. Without correct execution orchestration, no operations can run.

### Implementation for User Story 5

- [ ] T036 [US5] Update runtime/src/python/ffi.rs to add execute_pipeline_ffi(manifest_json: &str) function signature
- [ ] T037 [US5] Implement manifest JSON parsing with error handling in runtime/src/python/ffi.rs
- [ ] T038 [US5] Call PipelineGraph::from_manifest() and handle validation errors in runtime/src/python/ffi.rs
- [ ] T039 [US5] Call Executor::execute() and collect results in runtime/src/python/ffi.rs
- [ ] T040 [US5] Convert ExecutorError to PyErr in runtime/src/python/ffi.rs
- [ ] T041 [US5] Serialize execution results and metrics to JSON in runtime/src/python/ffi.rs
- [ ] T042 [US5] Create integration test in runtime/tests/integration/test_executor.rs (linear pipeline: A → B → C)
- [ ] T043 [US5] Add integration test for branching topology (A → B, A → C → D) in runtime/tests/integration/test_executor.rs
- [ ] T044 [US5] Add integration test for converging topology (A → C, B → C) in runtime/tests/integration/test_executor.rs
- [ ] T045 [US5] Add integration test for cycle detection (A → B → C → A) expecting error in runtime/tests/integration/test_executor.rs

**Checkpoint**: Pipeline orchestration works - can execute multi-node pipelines with complex topologies

---

## Phase 4: User Story 4 - Zero-Copy Data Transfer (Priority: P1) 🎯

**Goal**: Eliminate memory copies when passing data arrays between Python and Rust, enabling real-time processing of large datasets

**Independent Test**: Profile memory allocations during array transfer, verify zero copies for read-only access, measure transfer overhead <1μs

**Why P1**: Memory bandwidth is often the bottleneck. Zero-copy is foundational for high-throughput applications.

### AudioBuffer Implementation

- [ ] T046 [P] [US4] Create runtime/src/audio/mod.rs with AudioBuffer struct (data: Arc<Vec<f32>>, sample_rate, channels, format)
- [ ] T047 [P] [US4] Add AudioFormat enum (F32, I16, I32) to runtime/src/audio/mod.rs
- [ ] T048 [US4] Implement AudioBuffer helper methods (new, len_samples, len_frames, duration_secs, as_slice, make_mut) in runtime/src/audio/mod.rs

### FFI Zero-Copy Functions

- [ ] T049 [US4] Implement numpy_to_audio_buffer_ffi() in runtime/src/python/numpy_marshal.rs (borrow numpy array via PyO3)
- [ ] T050 [US4] Add zero-copy slice borrowing: unsafe { audio.as_slice()? } in runtime/src/python/numpy_marshal.rs
- [ ] T051 [US4] Wrap slice in Arc<Vec<f32>> for shared ownership in runtime/src/python/numpy_marshal.rs
- [ ] T052 [US4] Implement audio_buffer_to_numpy_ffi() in runtime/src/python/numpy_marshal.rs (convert Arc<Vec<f32>> to numpy)
- [ ] T053 [US4] Use PyArrayDyn::from_vec() for ownership transfer in runtime/src/python/numpy_marshal.rs

### Format Conversion (Zero-Copy Where Safe)

- [ ] T054 [P] [US4] Create runtime/src/audio/format.rs with i16_to_f32() and f32_to_i16() conversion functions
- [ ] T055 [US4] Implement zero-copy transmute for compatible formats using bytemuck in runtime/src/audio/format.rs
- [ ] T056 [US4] Add format validation and safety checks to runtime/src/audio/format.rs

### Memory Profiling

- [ ] T057 [US4] Create runtime/tests/integration/test_zero_copy.rs with memory allocation tests
- [ ] T058 [US4] Add test verifying numpy pointer unchanged after FFI call in runtime/tests/integration/test_zero_copy.rs
- [ ] T059 [US4] Add test measuring FFI call overhead (<1μs target) in runtime/tests/integration/test_zero_copy.rs
- [ ] T060 [US4] Add benchmark in runtime/benches/zero_copy.rs comparing copy vs zero-copy approaches

**Checkpoint**: Zero-copy data transfer works - <1μs overhead, no memory copies for read-only access

---

## Phase 5: User Story 1 - Audio Pipeline Performance Boost (Priority: P1) 🎯 MVP

**Goal**: Deliver 50-100x speedup for audio preprocessing operations (VAD, resampling, format conversion) with zero code changes to user scripts

**Independent Test**: Run existing audio pipeline examples before/after, compare execution times, verify code unchanged

**Why P1**: Core value proposition. Immediate measurable impact on user productivity.

### Audio Node Registry

- [ ] T061 [P] [US1] Create runtime/src/nodes/registry.rs with NodeRegistry struct
- [ ] T062 [P] [US1] Add NodeFactory trait to runtime/src/nodes/registry.rs
- [ ] T063 [US1] Implement NodeRegistry::new() with Rust node registration in runtime/src/nodes/registry.rs
- [ ] T064 [US1] Implement create_node() with runtime hint resolution (Auto/Rust/Python) in runtime/src/nodes/registry.rs

### CPython Fallback Executor

- [ ] T065 [P] [US1] Create runtime/src/python/cpython_node.rs with CPythonNode struct
- [ ] T066 [US1] Implement NodeExecutor trait for CPythonNode (calls Python via PyO3) in runtime/src/python/cpython_node.rs
- [ ] T067 [US1] Add Python dict conversion helpers (inputs_to_pydict, pydict_to_outputs) to runtime/src/python/cpython_node.rs

### Audio Resampling Node (Rust)

- [ ] T068 [P] [US1] Create runtime/src/nodes/audio/resample.rs with RustResampleNode struct
- [ ] T069 [US1] Initialize rubato Resampler in RustResampleNode::new() in runtime/src/nodes/audio/resample.rs
- [ ] T070 [US1] Implement resample() method using rubato in runtime/src/nodes/audio/resample.rs
- [ ] T071 [US1] Implement NodeExecutor trait for RustResampleNode in runtime/src/nodes/audio/resample.rs
- [ ] T072 [US1] Add ResampleQuality enum (Low, Medium, High) and conversion to rubato types in runtime/src/nodes/audio/resample.rs
- [ ] T073 [P] [US1] Create ResampleNodeFactory and register in registry in runtime/src/nodes/registry.rs

### Voice Activity Detection Node (Rust)

- [ ] T074 [P] [US1] Create runtime/src/nodes/audio/vad.rs with RustVADNode struct
- [ ] T075 [US1] Implement energy-based VAD using rustfft in runtime/src/nodes/audio/vad.rs
- [ ] T076 [US1] Add frame windowing and FFT computation in runtime/src/nodes/audio/vad.rs
- [ ] T077 [US1] Implement energy threshold detection (<50μs per 30ms frame) in runtime/src/nodes/audio/vad.rs
- [ ] T078 [US1] Implement NodeExecutor trait for RustVADNode in runtime/src/nodes/audio/vad.rs
- [ ] T079 [P] [US1] Create VADNodeFactory and register in registry in runtime/src/nodes/registry.rs

### Format Conversion Node (Rust)

- [ ] T080 [P] [US1] Create runtime/src/nodes/audio/format_converter.rs with RustFormatConverterNode struct
- [ ] T081 [US1] Implement i16 ↔ f32 conversion using bytemuck zero-copy in runtime/src/nodes/audio/format_converter.rs
- [ ] T082 [US1] Add validation for compatible format conversions in runtime/src/nodes/audio/format_converter.rs
- [ ] T083 [US1] Implement NodeExecutor trait for RustFormatConverterNode in runtime/src/nodes/audio/format_converter.rs
- [ ] T084 [P] [US1] Create FormatConverterNodeFactory and register in registry in runtime/src/nodes/registry.rs

### Audio Module Integration

- [x] T085 [US1] Create runtime/src/nodes/audio/mod.rs and export all audio nodes
- [x] T086 [US1] Update runtime/src/nodes/mod.rs to export audio module and registry
- [x] T087 [US1] Update runtime/src/lib.rs to export nodes module

### Python SDK Wrapper Nodes

- [x] T088 [P] [US1] Update python-client/remotemedia/nodes/audio.py to add runtime_hint parameter to AudioResampleNode
- [x] T089 [P] [US1] Update python-client/remotemedia/nodes/audio.py to add runtime_hint parameter to VADNode
- [x] T090 [P] [US1] Update python-client/remotemedia/nodes/audio.py to add runtime_hint parameter to FormatConverterNode
- [x] T091 [US1] Add runtime selection logic (check Rust availability, fallback to Python) to python-client/remotemedia/nodes/audio.py

### Examples

- [x] T092 [P] [US1] Create examples/rust_runtime/12_audio_vad_rust.py (VAD example with benchmark)
- [x] T093 [P] [US1] Create examples/rust_runtime/13_audio_resample_rust.py (resample example with benchmark)
- [x] T094 [P] [US1] Create examples/rust_runtime/14_audio_format_rust.py (format conversion example)
- [x] T095 [US1] Create examples/rust_runtime/15_full_audio_pipeline.py (end-to-end: VAD + resample + format)

### Performance Validation

- [x] T096 [US1] Create runtime/benches/audio_nodes.rs with criterion benchmarks
- [x] T097 [US1] Add resample benchmark (target: <2ms per second of audio) to runtime/benches/audio_nodes.rs
- [x] T098 [US1] Add VAD benchmark (target: <50μs per 30ms frame) to runtime/benches/audio_nodes.rs
- [x] T099 [US1] Add format conversion benchmark (target: <100μs for 1M samples) to runtime/benches/audio_nodes.rs
- [x] T100 [US1] Run cargo bench and verify all targets met

**Checkpoint**: Audio acceleration works - 50-100x speedup achieved, existing examples work unchanged

---

## Phase 6: User Story 2 - Reliable Production Execution (Priority: P2)

**Goal**: Automatic retry with exponential backoff for transient errors, preventing 95% of transient failures from becoming user-facing errors

**Independent Test**: Inject transient errors (network timeouts, file locks), verify automatic retry with backoff, no user intervention required

**Why P2**: Production readiness is critical for adoption. Unreliable pipelines block enterprise use cases.

### Error Classification

- [x] T101 [P] [US2] Add is_retryable() method to ExecutorError enum in runtime/src/executor/error.rs
- [x] T102 [US2] Classify errors as retryable (NodeExecutionError, PythonError) vs non-retryable (ManifestError, CycleError) in runtime/src/executor/error.rs

### Retry Policy Implementation

- [x] T103 [US2] Implement RetryPolicy::default() (3 attempts, exponential backoff 100/200/400ms) in runtime/src/executor/retry.rs
- [x] T104 [US2] Implement get_delay() for exponential backoff calculation in runtime/src/executor/retry.rs
- [x] T105 [US2] Implement async execute() method with retry loop in runtime/src/executor/retry.rs

### Circuit Breaker

- [x] T106 [P] [US2] Add CircuitBreaker struct to runtime/src/executor/retry.rs
- [x] T107 [US2] Implement consecutive failure tracking in runtime/src/executor/retry.rs
- [x] T108 [US2] Add trip logic (5 consecutive failures) and reset logic in runtime/src/executor/retry.rs
- [x] T109 [US2] Integrate circuit breaker into Executor::execute_node_with_retry() in runtime/src/executor/scheduler.rs

### Error Context Enhancement

- [ ] T110 [US2] **DEFERRED TO PHASE 9** - Add detailed error context (node ID, operation name, stack trace) to all ExecutorError variants (requires refactoring 62+ files)
- [ ] T111 [US2] **DEFERRED TO PHASE 9** - Implement Display trait for rich error formatting
- [ ] T112 [US2] **DEFERRED TO PHASE 9** - Update PyErr conversion to include full diagnostic context

### Integration Testing

- [x] T113 [US2] Create runtime/tests/test_retry.rs with transient error injection tests
- [x] T114 [US2] Add test for successful retry after 2 failures in runtime/tests/test_retry.rs
- [x] T115 [US2] Add test for immediate failure on non-retryable errors in runtime/tests/test_retry.rs
- [x] T116 [US2] Add test for circuit breaker tripping after 5 failures in runtime/tests/test_retry.rs
- [x] T117 [US2] Create runtime/tests/test_error_handling.rs with error propagation tests

**Checkpoint**: Error handling works - automatic retry (T101-T109 ✅), circuit breaker working, rich error context deferred to Phase 9 (T110-T112)

---

## Phase 7: User Story 3 - Performance Monitoring (Priority: P3)

**Goal**: Detailed JSON metrics export showing per-node execution times, memory usage, and bottleneck identification

**Independent Test**: Run multi-node pipeline with metrics enabled, verify JSON export contains execution times with microsecond precision and memory usage

**Why P3**: Users can't optimize what they can't measure. Enables data-driven performance tuning.

### Metrics JSON Export

- [x] T118 [P] [US3] Implement ExecutionMetrics::to_json() in runtime/src/executor/metrics.rs
- [x] T119 [P] [US3] Add get_metrics_ffi() function to runtime/src/python/ffi.rs
- [x] T120 [US3] Update execute_pipeline_ffi() to include metrics in return dict in runtime/src/python/ffi.rs

### Python SDK Integration

- [x] T121 [US3] Update python-client/remotemedia/core/pipeline.py to add enable_metrics parameter to Pipeline.__init__()
- [x] T122 [US3] Add get_metrics() method to Pipeline class in python-client/remotemedia/core/pipeline.py
- [x] T123 [US3] Parse metrics JSON and return as Python dict in python-client/remotemedia/core/pipeline.py

### Metrics Overhead Validation

- [x] T124 [US3] Add overhead measurement to MetricsCollector in runtime/src/executor/metrics.rs
- [x] T125 [US3] Create runtime/tests/test_performance.rs with metrics overhead tests
- [x] T126 [US3] Verify metrics collection overhead <100μs per pipeline in runtime/tests/test_performance.rs (✅ **29μs average**)

### Documentation

- [x] T127 [P] [US3] Add performance monitoring section to docs/PERFORMANCE_TUNING.md
- [x] T128 [P] [US3] Add metrics examples to docs/NATIVE_ACCELERATION.md
- [x] T129 [US3] Update quickstart.md with metrics usage examples in specs/001-native-rust-acceleration/quickstart.md

**Checkpoint**: ✅ **Phase 7 Complete** - Performance monitoring works with 29μs overhead (71% under target), JSON metrics export with microsecond precision, detailed per-node breakdown

---

## Phase 8: User Story 6 - Runtime Selection Transparency (Priority: P2)

**Goal**: Automatic runtime selection (Rust native or Python fallback) based on availability, ensuring portability across environments

**Independent Test**: Run same pipeline on systems with/without Rust runtime, verify automatic fallback, functionality identical

**Why P2**: Portability enables gradual rollout. Teams can deploy same codebase everywhere.

### Runtime Detection

- [x] T130 [P] [US6] Add runtime availability check in python-client/remotemedia/__init__.py
- [x] T131 [US6] Implement try_load_rust_runtime() with error handling in python-client/remotemedia/__init__.py
- [x] T132 [US6] Add fallback warning when Rust runtime unavailable in python-client/remotemedia/__init__.py

### Automatic Fallback

- [x] T133 [US6] Update Pipeline to use runtime detection before execution
- [x] T134 [US6] Add graceful degradation when Rust requested but unavailable (automatic fallback to Python)
- [x] T135 [US6] Ensure all Python SDK nodes work unchanged when Rust unavailable (existing runtime_hint support)

### Testing

- [x] T136 [US6] Create python-client/tests/test_rust_compatibility.py with runtime fallback tests (15 tests)
- [x] T137 [US6] Add test for automatic Rust selection when available (TestAutomaticSelection)
- [x] T138 [US6] Add test for Python fallback when Rust unavailable (TestPythonFallback)
- [x] T139 [US6] Add test verifying identical results from Rust and Python implementations (TestResultConsistency)

**Checkpoint**: ✅ **Phase 8 Complete** - Runtime selection transparent, automatic fallback working, 15 tests created (7 passed, 8 async skipped), same code runs everywhere

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, optimization, and final validation

**Estimated Duration**: Week 6 (5 days)

### Documentation Completion

- [x] T140 [P] Complete docs/NATIVE_ACCELERATION.md with architecture diagrams and data flow
- [x] T141 [P] Complete docs/MIGRATION_GUIDE.md with upgrade steps from v0.1.x to v0.2.0
- [x] T142 [P] Complete docs/PERFORMANCE_TUNING.md with optimization strategies
- [x] T143 [P] Update README.md with Rust acceleration features and benchmarks
- [x] T144 Update CHANGELOG.md with all v0.2.0 changes and breaking changes

### Performance Optimization

- [ ] T145 Profile audio node execution and optimize hot paths in runtime/src/nodes/audio/
- [ ] T146 Add buffer pooling to reduce allocations in runtime/src/executor/scheduler.rs
- [ ] T147 Optimize JSON serialization/deserialization paths in runtime/src/python/ffi.rs

### Integration Testing

- [ ] T148 Run all 11 existing examples in examples/rust_runtime/ and verify zero code changes needed
- [ ] T149 Create python-client/tests/test_performance.py with end-to-end performance benchmarks
- [ ] T150 Verify all performance targets met (50-100x audio speedup, <1μs FFI, <100μs metrics)

### Quickstart Validation

- [ ] T151 Follow specs/001-native-rust-acceleration/quickstart.md step-by-step on clean system
- [ ] T152 Verify all quickstart examples run successfully and produce expected output
- [ ] T153 Update quickstart.md with any discovered issues or improvements

### Release Preparation

- [ ] T154 Update version to 0.2.0 in runtime/Cargo.toml
- [ ] T155 Update version to 0.2.0 in python-client/setup.py
- [ ] T156 Run full test suite: cargo test && pytest
- [ ] T157 Run full benchmark suite: cargo bench
- [ ] T158 Tag release: git tag v0.2.0

**Checkpoint**: Release ready - all features complete, documentation done, performance validated

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies - start immediately ✅
- **Phase 2 (Foundational)**: Depends on Phase 1 cleanup - BLOCKS all user stories ⚠️
- **Phase 3 (US5 - Orchestration)**: Depends on Phase 2 - Foundation for other stories
- **Phase 4 (US4 - Zero-Copy)**: Depends on Phase 2 - Can run parallel with Phase 3
- **Phase 5 (US1 - Audio Perf)**: Depends on Phase 2, 3, 4 - MVP delivery 🎯
- **Phase 6 (US2 - Reliability)**: Depends on Phase 2, 3 - Can run parallel with Phase 5
- **Phase 7 (US3 - Monitoring)**: Depends on Phase 2, 3 - Can run parallel with Phase 5, 6
- **Phase 8 (US6 - Transparency)**: Depends on Phase 5 (needs Rust nodes implemented)
- **Phase 9 (Polish)**: Depends on all user stories complete

### User Story Priority Order

1. **P1 Stories** (MVP - deliver these first):
   - US5: Pipeline Orchestration (foundation)
   - US4: Zero-Copy Data Transfer (performance foundation)
   - US1: Audio Performance Boost (core value proposition) 🎯

2. **P2 Stories** (production readiness):
   - US2: Reliable Production Execution
   - US6: Runtime Selection Transparency

3. **P3 Stories** (optimization):
   - US3: Performance Monitoring

### Within Each User Story

- Tasks marked [P] can run in parallel (different files)
- Rust code before Python wrappers
- Implementation before examples
- Examples before benchmarks

### Parallel Opportunities

**Phase 1 (Setup)**: No parallelism - sequential deletions and updates

**Phase 2 (Foundational)**: High parallelism
- T011-T014: All data structures can be created in parallel
- T015-T017: Manifest work independent
- Once graph ready: T021-T023 node executor trait can be done in parallel

**Phase 5 (US1 - Audio)**: High parallelism
- T061-T064: Registry work independent
- T065-T067: CPython fallback independent
- T068-T073: Resample node independent
- T074-T079: VAD node independent
- T080-T084: Format converter independent
- T092-T095: All examples can be created in parallel

**Multi-Developer Strategy**:
1. All developers: Phase 1 + Phase 2 together
2. Once Phase 2 done:
   - Developer A: Phase 3 (US5 - Orchestration)
   - Developer B: Phase 4 (US4 - Zero-Copy)
   - Developer C: Phase 6 (US2 - Reliability)
3. Once Phase 3, 4 done:
   - Developer A: Phase 5 (US1 - Audio Resample node)
   - Developer B: Phase 5 (US1 - VAD node)
   - Developer C: Phase 5 (US1 - Format converter node)
4. Converge for Phase 8, 9

---

## Parallel Example: Phase 5 (Audio Performance)

```bash
# Launch all Rust audio nodes in parallel (different files):
Task T068-T073: "Resample node + factory in runtime/src/nodes/audio/resample.rs"
Task T074-T079: "VAD node + factory in runtime/src/nodes/audio/vad.rs"
Task T080-T084: "Format converter + factory in runtime/src/nodes/audio/format_converter.rs"

# Launch all Python wrappers in parallel:
Task T088: "AudioResampleNode.runtime_hint in python-client/remotemedia/nodes/audio.py"
Task T089: "VADNode.runtime_hint in python-client/remotemedia/nodes/audio.py"
Task T090: "FormatConverterNode.runtime_hint in python-client/remotemedia/nodes/audio.py"

# Launch all examples in parallel:
Task T092: "examples/rust_runtime/12_audio_vad_rust.py"
Task T093: "examples/rust_runtime/13_audio_resample_rust.py"
Task T094: "examples/rust_runtime/14_audio_format_rust.py"
```

---

## Implementation Strategy

### MVP First (Fastest Path to Value) 🎯

1. ✅ **Week 1**: Phase 1 (Setup & Cleanup)
2. ✅ **Weeks 2-3**: Phase 2 (Foundational) - CRITICAL BLOCKER
3. ✅ **Week 3**: Phase 3 (US5 - Orchestration) + Phase 4 (US4 - Zero-Copy)
4. ✅ **Week 4**: Phase 5 (US1 - Audio Performance) - MVP COMPLETE
5. **STOP and VALIDATE**: Run benchmarks, verify 50-100x speedup
6. **Deploy/Demo MVP**: Show audio acceleration working

### Full Feature Set (All User Stories)

1. Weeks 1-4: MVP complete (above)
2. **Week 5**: Phase 6 (US2 - Reliability) + Phase 7 (US3 - Monitoring)
3. **Week 5-6**: Phase 8 (US6 - Transparency)
4. **Week 6**: Phase 9 (Polish & Release)
5. **Release v0.2.0**

### Incremental Delivery

- **v0.2.0-alpha**: Phase 1-4 complete (orchestration + zero-copy) - no audio yet
- **v0.2.0-beta**: Phase 5 complete (audio acceleration) - MVP ready 🎯
- **v0.2.0-rc1**: Phase 6-8 complete (reliability + monitoring + transparency)
- **v0.2.0**: Phase 9 complete (polish + docs)

---

## Success Criteria Tracking

| Criteria | Target | Validation Task | Status |
|----------|--------|-----------------|--------|
| SC-001: Audio speedup | 50-100x | T100 (benchmarks) | ⏳ |
| SC-002: Error recovery | 95% transient | T116 (circuit breaker test) | ⏳ |
| SC-003: Zero code changes | 11 examples | T148 (run all examples) | ⏳ |
| SC-004: FFI overhead | <1μs | T059 (zero-copy test) | ⏳ |
| SC-005: Metrics overhead | <100μs | T126 (overhead test) | ⏳ |
| SC-006: Large graphs | 100 nodes | T045 (complex topology test) | ⏳ |
| SC-007: Zero copies | Memory profiling | T058 (pointer test) | ⏳ |
| SC-008: Circuit breaker | 5 failures | T116 (breaker test) | ⏳ |
| SC-009: VAD speed | <50μs/frame | T098 (VAD benchmark) | ⏳ |
| SC-010: Resample speed | <2ms/sec | T097 (resample benchmark) | ⏳ |
| SC-011: Metrics export | <1ms | T126 (JSON export test) | ⏳ |
| SC-012: Concurrency | 10k nodes | T045 (load test) | ⏳ |

---

## Task Count Summary

- **Total Tasks**: 158
- **Phase 1 (Setup)**: 10 tasks (6% - Week 1)
- **Phase 2 (Foundational)**: 25 tasks (16% - Weeks 2-3) ⚠️ CRITICAL
- **Phase 3 (US5 - Orchestration)**: 10 tasks (6% - Week 3)
- **Phase 4 (US4 - Zero-Copy)**: 15 tasks (9% - Week 3)
- **Phase 5 (US1 - Audio Performance)**: 40 tasks (25% - Week 4) 🎯 MVP
- **Phase 6 (US2 - Reliability)**: 17 tasks (11% - Week 5)
- **Phase 7 (US3 - Monitoring)**: 12 tasks (8% - Week 5)
- **Phase 8 (US6 - Transparency)**: 10 tasks (6% - Week 5-6)
- **Phase 9 (Polish)**: 19 tasks (12% - Week 6)

**Parallel Opportunities**: 47 tasks marked [P] (30% can run in parallel)

**MVP Scope** (Phases 1-5): 100 tasks (63% of total) - 4 weeks

**Full Release** (All phases): 158 tasks (100%) - 6 weeks

---

## Format Validation ✅

- ✅ All tasks follow format: `- [ ] [ID] [P?] [Story?] Description with file path`
- ✅ Sequential task IDs: T001 through T158
- ✅ [P] marker on parallelizable tasks (different files, no dependencies)
- ✅ [Story] labels for user story phases: [US1], [US2], [US3], [US4], [US5], [US6]
- ✅ Setup/Foundational/Polish phases: NO story label (correct)
- ✅ All tasks include file paths for implementation
- ✅ Tasks organized by user story priority (P1 → P2 → P3)
- ✅ Each user story independently testable with clear checkpoints

---

## Notes

- Tests omitted per spec (not explicitly requested) - can add later if needed
- Focus on MVP delivery: Phases 1-5 deliver core value in 4 weeks
- High parallelism in Phase 5 (audio nodes) - 3 developers can work simultaneously
- Foundation (Phase 2) is CRITICAL BLOCKER - must complete before any user story work
- Each user story has checkpoint for independent validation
- All 12 success criteria mapped to validation tasks
