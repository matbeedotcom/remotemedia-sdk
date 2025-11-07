# Tasks: Model Registry and Shared Memory Tensors

**Input**: Design documents from `/specs/006-model-sharing/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: No explicit test generation requested - tasks focus on implementation

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure

- [x] T001 Add model-registry and shared-memory features to runtime-core/Cargo.toml
- [x] T002 [P] Add shared_memory crate dependency to runtime-core/Cargo.toml
- [x] T003 [P] Add PyO3 feature flag for Python bindings in runtime-core/Cargo.toml
- [x] T004 Create runtime-core/src/model_registry/ directory structure
- [x] T005 Create runtime-core/src/model_worker/ directory structure
- [x] T006 Create runtime-core/src/tensor/ directory structure
- [x] T007 [P] Create python-client/remotemedia/core/ directory for Python bindings

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T008 Define InferenceModel trait in runtime-core/src/model_registry/mod.rs
- [x] T009 Define TensorStorage enum in runtime-core/src/tensor/mod.rs
- [x] T010 Define DataType and DeviceType enums in runtime-core/src/tensor/mod.rs
- [x] T011 Create error types for model registry in runtime-core/src/model_registry/error.rs
- [x] T012 [P] Create error types for shared memory in runtime-core/src/tensor/error.rs
- [x] T013 Export new modules in runtime-core/src/lib.rs
- [x] T014 Create base configuration structs in runtime-core/src/model_registry/config.rs

**Checkpoint**: Foundation ready - user story implementation can now begin

---

## Phase 3: User Story 1 - Process-Local Model Sharing (Priority: P1) 🎯 MVP

**Goal**: Enable multiple nodes within the same process to share a single loaded model instance

**Independent Test**: Run two nodes using the same model in a single pipeline and verify only one instance is loaded

### Implementation for User Story 1

- [x] T015 [US1] Implement ModelRegistry struct with RwLock<HashMap> in runtime-core/src/model_registry/mod.rs
- [x] T016 [US1] Implement ModelHandle with Arc reference counting in runtime-core/src/model_registry/handle.rs
- [x] T017 [US1] Implement get_or_load method with singleton loading in runtime-core/src/model_registry/mod.rs
- [x] T018 [US1] Implement reference counting logic in Drop trait for ModelHandle in runtime-core/src/model_registry/handle.rs
- [x] T019 [US1] Implement automatic cleanup with TTL in runtime-core/src/model_registry/cache.rs
- [x] T020 [P] [US1] Implement LRU eviction policy in runtime-core/src/model_registry/cache.rs
- [x] T021 [P] [US1] Implement registry metrics tracking in runtime-core/src/model_registry/metrics.rs
- [x] T022 [US1] Create Python ModelRegistry class in python-client/remotemedia/core/model_registry.py
- [x] T023 [US1] Implement Python get_or_load function in python-client/remotemedia/core/model_registry.py
- [x] T024 [US1] Create integration test for process-local sharing in tests/integration/test_model_sharing.rs
- [x] T025 [US1] Update LFM2AudioNode to use registry in python-client/remotemedia/nodes/ml/lfm2_audio.py

**Checkpoint**: ✅ User Story 1 COMPLETE - process-local model sharing functional and demonstrated

---

## Phase 4: User Story 2 - Cross-Process Model Worker (Priority: P2)

**Goal**: Enable a dedicated worker process to own a model and serve requests from multiple clients

**Independent Test**: Start a model worker and have multiple clients send requests, verify single model instance

### Implementation for User Story 2

- [x] T026 [US2] Implement ModelWorker struct in runtime-core/src/model_worker/mod.rs
- [x] T027 [US2] Implement worker gRPC service in runtime-core/src/model_worker/service.rs
- [x] T028 [US2] Implement ModelWorkerClient in runtime-core/src/model_worker/client.rs
- [x] T029 [US2] Define IPC protocol messages in runtime-core/src/model_worker/protocol.rs
- [x] T030 [P] [US2] Implement request batching logic in runtime-core/src/model_worker/batch.rs
- [x] T031 [P] [US2] Implement health check endpoint in runtime-core/src/model_worker/health.rs
- [x] T032 [US2] Implement worker status tracking in runtime-core/src/model_worker/status.rs
- [x] T033 [US2] Create worker binary in runtime-core/bin/model-worker.rs
- [x] T034 [US2] Implement Python ModelWorkerClient in python-client/remotemedia/core/worker_client.py
- [x] T035 [US2] Add worker failure handling and reconnection logic in runtime-core/src/model_worker/client.rs
- [x] T036 [US2] Create gRPC service adapter in transports/remotemedia-grpc/src/model_worker_service.rs
- [x] T037 [US2] Generate protobuf definitions and integrate with gRPC transport

**Checkpoint**: ✅ User Story 2 COMPLETE - cross-process model workers integrated with gRPC transport

---

## Phase 5: User Story 3 - Shared Memory Tensor Transfer (Priority: P2)

**Goal**: Enable zero-copy tensor transfer between processes using shared memory

**Independent Test**: Transfer a large tensor between processes and verify zero-copy via performance metrics

### Implementation for User Story 3

- [x] T038 [US3] Implement SharedMemoryRegion struct in runtime-core/src/tensor/shared_memory.rs
- [x] T039 [US3] Implement platform-specific SHM creation (Linux) in runtime-core/src/tensor/shared_memory.rs
- [x] T040 [US3] Implement platform-specific SHM creation (Windows) in runtime-core/src/tensor/shared_memory.rs
- [x] T041 [US3] Implement platform-specific SHM creation (macOS) in runtime-core/src/tensor/shared_memory.rs
- [x] T042 [US3] Implement TensorBuffer with SharedMemory storage in runtime-core/src/tensor/mod.rs
- [x] T043 [US3] Implement SharedMemoryAllocator in runtime-core/src/tensor/allocator.rs
- [x] T044 [P] [US3] Implement per-session quota enforcement in runtime-core/src/tensor/allocator.rs
- [x] T045 [P] [US3] Implement automatic cleanup with TTL in runtime-core/src/tensor/allocator.rs
- [x] T046 [US3] Implement fallback to heap allocation when SHM unavailable in runtime-core/src/tensor/mod.rs
- [x] T047 [US3] Create Python SharedMemoryRegion class in python-client/remotemedia/core/tensor_bridge.py
- [x] T048 [US3] Implement Python TensorBuffer with SHM support in python-client/remotemedia/core/tensor_bridge.py
- [x] T049 [US3] Create integration test for SHM tensors in tests/integration/test_shm_tensors.rs
- [x] T050 [US3] Add capability detection for SHM availability in runtime-core/src/tensor/capabilities.rs

**Checkpoint**: ✅ User Story 3 COMPLETE - shared memory tensor transfer infrastructure functional

---

## Phase 6: User Story 4 - Python Zero-Copy Integration (Priority: P3)

**Goal**: Enable Python nodes to exchange tensors with runtime using zero-copy mechanisms

**Independent Test**: Pass NumPy arrays to/from Python nodes and verify no copying via memory profiling

### Implementation for User Story 4

- [ ] T051 [US4] Implement DLPack support in runtime-core/src/tensor/dlpack.rs
- [ ] T052 [US4] Implement NumPy buffer protocol in runtime-core/src/tensor/numpy.rs
- [ ] T053 [US4] Create PyO3 bindings for TensorBuffer in runtime-core/src/tensor/python.rs
- [ ] T054 [US4] Implement from_numpy method with zero-copy in python-client/remotemedia/core/tensor_bridge.py
- [ ] T055 [US4] Implement to_numpy method with zero-copy in python-client/remotemedia/core/tensor_bridge.py
- [ ] T056 [P] [US4] Implement __dlpack__ protocol in python-client/remotemedia/core/tensor_bridge.py
- [ ] T057 [P] [US4] Implement __array__ protocol in python-client/remotemedia/core/tensor_bridge.py
- [ ] T058 [US4] Create torch_to_buffer helper function in python-client/remotemedia/core/tensor_bridge.py
- [ ] T059 [US4] Create buffer_to_torch helper function in python-client/remotemedia/core/tensor_bridge.py
- [ ] T060 [US4] Create Python integration test in tests/python/test_zero_copy.py
- [ ] T061 [US4] Update existing Python nodes to use zero-copy tensors in python-client/remotemedia/nodes/

**Checkpoint**: User Story 4 complete - Python zero-copy integration functional

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [ ] T062 [P] Add comprehensive documentation to all public APIs in runtime-core/src/model_registry/
- [ ] T063 [P] Add comprehensive documentation to all public APIs in runtime-core/src/tensor/
- [ ] T064 [P] Add comprehensive documentation to all public APIs in runtime-core/src/model_worker/
- [ ] T065 Create performance benchmarks in runtime-core/benches/model_registry.rs
- [ ] T066 Create performance benchmarks in runtime-core/benches/shared_memory.rs
- [ ] T067 Add telemetry and observability hooks in runtime-core/src/model_registry/metrics.rs
- [ ] T068 Validate quickstart.md examples work end-to-end
- [ ] T069 Add memory leak detection tests in tests/integration/test_memory_leaks.rs
- [ ] T070 Optimize critical paths identified by benchmarks

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phases 3-6)**: All depend on Foundational phase completion
  - US1 (Process-Local): Can start after Foundational
  - US2 (Model Worker): Can start after Foundational
  - US3 (Shared Memory): Can start after Foundational
  - US4 (Python Zero-Copy): Depends on US3 for full functionality
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Independent - no dependencies on other stories
- **User Story 2 (P2)**: Independent - can integrate with US1 but works standalone
- **User Story 3 (P2)**: Independent - provides infrastructure for US4
- **User Story 4 (P3)**: Best implemented after US3 for shared memory support

### Parallel Opportunities

**Within Setup Phase:**
- T002, T003, T007 can run in parallel (different files)

**Within Foundational Phase:**
- T012 can run parallel to other tasks (different error file)

**Within User Story 1:**
- T020, T021 can run in parallel (different aspects of registry)

**Within User Story 2:**
- T030, T031 can run in parallel (batching vs health checks)

**Within User Story 3:**
- T044, T045 can run in parallel (quota vs cleanup)

**Within User Story 4:**
- T056, T057 can run in parallel (different Python protocols)

**Across User Stories:**
- US1, US2, and US3 can be developed in parallel by different team members
- US4 should wait for US3 completion for best integration

---

## Parallel Example: User Story 1

```bash
# After T015-T019 complete, launch parallel tasks:
Task T020: "Implement LRU eviction policy in runtime-core/src/model_registry/cache.rs"
Task T021: "Implement registry metrics tracking in runtime-core/src/model_registry/metrics.rs"

# These work on different files and aspects, no conflicts
```

---

## Parallel Example: User Story 3

```bash
# After T038-T043 complete, launch parallel tasks:
Task T044: "Implement per-session quota enforcement in runtime-core/src/tensor/allocator.rs"
Task T045: "Implement automatic cleanup with TTL in runtime-core/src/tensor/allocator.rs"

# Different aspects of the allocator, can be developed simultaneously
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup → Project structure ready
2. Complete Phase 2: Foundational → Core types and traits defined
3. Complete Phase 3: User Story 1 → Process-local model sharing working
4. **STOP and VALIDATE**: Test with LFM2AudioNode sharing models
5. Measure memory savings and performance

### Incremental Delivery

1. Setup + Foundational → Foundation ready for all stories
2. Add User Story 1 → Deploy process-local sharing (immediate value)
3. Add User Story 2 → Deploy model workers (GPU sharing)
4. Add User Story 3 → Deploy SHM tensors (performance boost)
5. Add User Story 4 → Deploy Python zero-copy (research enablement)

### Parallel Team Strategy

With 3 developers after Foundational phase:

1. Developer A: User Story 1 (Process-local sharing)
   - Focus: Registry, handles, caching
   - Deliverable: Memory-efficient model sharing

2. Developer B: User Story 2 (Model workers)
   - Focus: Worker process, client, batching
   - Deliverable: Cross-process GPU sharing

3. Developer C: User Story 3 (Shared memory)
   - Focus: SHM regions, tensor storage, allocator
   - Deliverable: Zero-copy tensor transfers

All can work independently and integrate at the end.

---

## Task Summary

**Total Tasks**: 70
- Phase 1 (Setup): 7 tasks
- Phase 2 (Foundational): 7 tasks
- Phase 3 (US1 - Process-Local): 11 tasks
- Phase 4 (US2 - Model Worker): 12 tasks
- Phase 5 (US3 - Shared Memory): 13 tasks
- Phase 6 (US4 - Python Zero-Copy): 11 tasks
- Phase 7 (Polish): 9 tasks

**Parallel Opportunities**: 16 tasks marked [P] can run in parallel within their phases

**Independent Test Criteria**:
- US1: Verify single model instance with multiple handles
- US2: Verify worker serves multiple clients with one model
- US3: Verify zero-copy transfer via performance metrics
- US4: Verify Python arrays share memory without copying

**MVP Scope**: Phase 1 + Phase 2 + Phase 3 (US1 only) = 25 tasks for immediate value
