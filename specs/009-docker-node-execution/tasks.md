# Implementation Tasks: Docker-Based Node Execution with iceoryx2 IPC

**Feature Branch**: `009-docker-node-execution`
**Created**: 2025-11-11
**Status**: Ready for Implementation

## Overview

This document provides an actionable task breakdown for implementing Docker-based node execution with iceoryx2 shared memory IPC. Tasks are organized by user story priority to enable independent, incremental delivery.

**Tech Stack**:
- **Language**: Rust 1.87 (runtime-core), Python 3.9-3.11 (node containers)
- **Dependencies**: bollard 0.19 (Docker API), rusqlite (image cache), iceoryx2 0.7.0 (IPC), tokio 1.35 (async runtime)
- **Testing**: cargo test (unit + integration), cargo bench (latency comparison)

**Estimated Total Tasks**: 52
**MVP Scope**: User Story 1 only (P1 - ~25 tasks)

---

## Implementation Strategy

### MVP-First Approach

**Recommended MVP**: Complete User Story 1 (Deploy Isolated Python Node Environment) first
- Delivers core value: environment isolation with zero-copy IPC
- Independently testable and deployable
- Provides foundation for P2 and P3 enhancements

**Incremental Delivery**:
1. **Phase 1**: Setup (T001-T004) - Project infrastructure
2. **Phase 2**: Foundational (T005-T010) - Shared components
3. **Phase 3**: User Story 1 (T011-T035) - MVP delivery
4. **Phase 4**: User Story 2 (T036-T042) - Multi-container support
5. **Phase 5**: User Story 3 (T043-T049) - Image caching optimization
6. **Final Phase**: Polish (T050-T052) - Documentation and benchmarks

### Parallel Execution Opportunities

Tasks marked `[P]` can be executed in parallel with other `[P]` tasks that don't share dependencies. See "Parallel Execution Guide" section for detailed groupings per user story.

---

## Phase 1: Setup

**Goal**: Initialize project structure and dependencies

**Duration**: ~2 hours

### Tasks

- [x] T001 Add bollard dependency to runtime-core/Cargo.toml with version 0.19
- [x] T002 Add rusqlite dependency to runtime-core/Cargo.toml for image cache persistence
- [x] T003 Create runtime-core/src/python/docker/ directory structure with mod.rs
- [x] T004 Create docker/ directory at repository root with subdirectories: base-images/, scripts/

**Validation**: `cargo build` succeeds with new dependencies ✅

---

## Phase 2: Foundational

**Goal**: Implement shared infrastructure required by all user stories

**Duration**: ~6 hours

### Tasks

- [x] T005 [P] Extend runtime-core/src/manifest.rs with DockerExecutorConfig struct per contracts/manifest-docker-extension.yaml
- [x] T006 [P] Implement ResourceLimits struct in runtime-core/src/python/docker/config.rs with validation logic
- [x] T007 [P] Create SQLite schema for image cache in runtime-core/src/python/docker/image_cache_schema.sql
- [x] T008 Implement ImageCache struct in runtime-core/src/python/docker/image_builder.rs with SQLite CRUD operations
- [x] T009 [P] Create standard Dockerfiles in docker/base-images/ for Python 3.9, 3.10, 3.11 with iceoryx2 pre-installed
- [x] T010 Implement validate_custom_base_image function in runtime-core/src/python/docker/image_builder.rs (checks iceoryx2 presence)

**Validation**: Manifest parsing accepts docker field, SQLite schema creates successfully, Dockerfiles build ✅

---

## Phase 3: User Story 1 - Deploy Isolated Python Node Environment (Priority: P1)

**Goal**: Enable single Docker node execution with zero-copy IPC

**Why this is MVP**: Core value proposition - environment isolation with performance parity to multiprocess nodes

**Independent Test Criteria**:
- Pipeline with one Docker node processes audio data via iceoryx2 IPC
- Latency within 5ms of equivalent multiprocess node
- Container starts, processes data, and cleans up without resource leaks

**Acceptance Scenarios** (from spec.md):
1. Manifest with docker executor → system creates container with specified environment
2. Host sends audio data → node receives via shared memory IPC (zero serialization)
3. Node yields outputs → host receives via IPC and routes to downstream nodes
4. Session terminates → container stops and releases shared memory resources

### Setup & Configuration

- [ ] T011 [US1] Implement DockerizedNodeConfiguration in runtime-core/src/python/docker/config.rs with validate() and compute_config_hash() methods
- [ ] T012 [US1] Implement config_hash computation using SHA256 of all configuration fields (python_version, dependencies, resource_limits)
- [ ] T013 [US1] Add DockerExecutor registration in runtime-core/src/executor/executor_bridge.rs to support "docker" executor type

### Container Management

- [ ] T014 [US1] Implement ContainerManager in runtime-core/src/python/docker/container_manager.rs with bollard Docker client initialization
- [ ] T015 [US1] Implement create_container() method with volume mounts for /tmp/iceoryx2 and /dev/shm, shm_size=2GB
- [ ] T016 [US1] Implement start_container() method with health check polling until container ready
- [ ] T017 [US1] Implement stop_container() method with graceful SIGTERM, timeout, then SIGKILL fallback
- [ ] T018 [US1] Implement remove_container() method with cleanup of orphaned volumes

### Image Building

- [ ] T019 [P] [US1] Implement build_docker_image() in runtime-core/src/python/docker/image_builder.rs using bollard BuildImageOptions
- [ ] T020 [P] [US1] Implement Dockerfile generation from DockerizedNodeConfiguration (multi-stage build with builder + runtime stages)
- [ ] T021 [US1] Integrate image cache lookup in build_docker_image() - check SQLite for existing image by config_hash before building

### IPC Bridge

- [ ] T022 [US1] Implement IpcBridge in runtime-core/src/python/docker/ipc_bridge.rs adapting multiprocess IPC patterns
- [ ] T023 [US1] Create spawn_ipc_thread_for_container() function with dedicated OS thread for iceoryx2 Publisher/Subscriber (!Send types)
- [ ] T024 [US1] Implement send_data_to_container() using IpcCommand::SendData pattern from multiprocess executor
- [ ] T025 [US1] Implement receive_data_from_container() with continuous polling loop (yield_now, not sleep)
- [ ] T026 [US1] Implement session-scoped channel naming: format!("{session_id}_{node_id}_input") and _output

### Docker Executor

- [ ] T027 [US1] Implement DockerExecutor struct in runtime-core/src/python/docker/docker_executor.rs implementing StreamingNodeExecutor trait
- [ ] T028 [US1] Implement initialize() method: validate Docker daemon accessible, build/pull image, create container, setup IPC channels
- [ ] T029 [US1] Implement execute_streaming() method: send data via IPC bridge, receive outputs, route to session router
- [ ] T030 [US1] Implement cleanup() method: stop container, cleanup IPC channels, remove from registry

### Logging & Observability

- [ ] T031 [P] [US1] Implement container log streaming in runtime-core/src/python/docker/container_manager.rs using bollard logs API
- [ ] T032 [P] [US1] Add tracing instrumentation to all Docker executor methods (info, debug, error levels)
- [ ] T033 [P] [US1] Implement FR-017 error messages for resource limit violations (parse Docker exit codes, report exceeded limits)

### Integration

- [ ] T034 [US1] Create integration test in runtime-core/tests/integration/test_docker_executor.rs: single node pipeline with audio streaming
- [ ] T035 [US1] Validate SC-001 acceptance criterion: measure latency, verify within 5ms of multiprocess node

**User Story 1 Validation**:
- Run: `cargo test test_docker_executor`
- Expected: Pipeline with Docker node processes audio, latency ≤ multiprocess + 5ms, clean shutdown

---

## Phase 4: User Story 2 - Support Multiple Concurrent Container Nodes (Priority: P2)

**Goal**: Enable pipelines with multiple Docker nodes, each with different environments

**Why this priority**: Real-world scenarios require environment isolation across multiple nodes

**Independent Test Criteria**:
- Pipeline with 2-3 Docker nodes processes data through all containers
- Each container has isolated Python environment (different PyTorch versions verified)
- Node failure doesn't affect other containers

**Acceptance Scenarios** (from spec.md):
1. Multiple nodes with different docker configs → separate containers created
2. Data flows through pipeline → each node processes in sequence, routing works
3. Resources monitored → each container's usage isolated and measurable
4. One node fails → other nodes continue operating

### Shared Container Registry

- [ ] T036 [US2] Implement GLOBAL_CONTAINER_REGISTRY in runtime-core/src/python/docker/container_registry.rs with Arc<RwLock<HashMap>>
- [ ] T037 [US2] Implement get_or_create_container() method (FR-012): lookup by node_id, reuse if exists, create if new
- [ ] T038 [US2] Implement reference counting in ContainerSessionInstance with add_session() and remove_session() methods (FR-015)
- [ ] T039 [US2] Update DockerExecutor::initialize() to use GLOBAL_CONTAINER_REGISTRY instead of always creating new containers

### Multi-Container Data Flow

- [ ] T040 [US2] Implement container-to-container IPC routing in session_router (extend existing session routing for Docker nodes)
- [ ] T041 [US2] Add health monitoring in runtime-core/src/python/docker/health_check.rs with periodic container stats checks (30s interval)

### Testing

- [ ] T042 [US2] Create integration test in runtime-core/tests/integration/test_docker_multiprocess.rs: pipeline with 3 Docker nodes (Py 3.9, 3.10, 3.11) + 1 multiprocess node

**User Story 2 Validation**:
- Run: `cargo test test_docker_multiprocess`
- Expected: 3 Docker containers + 1 process, data flows correctly, isolated environments verified

---

## Phase 5: User Story 3 - Persist and Reuse Container Images (Priority: P3)

**Goal**: Optimize startup time by caching and reusing Docker images across sessions

**Why this priority**: Developer experience optimization, not critical for basic functionality

**Independent Test Criteria**:
- First pipeline run builds image (measured time)
- Second run reuses image (measured time < 50% of first run)
- Changed config triggers rebuild

**Acceptance Scenarios** (from spec.md):
1. First use of node config → builds image, tags with unique ID
2. Same config used again → detects existing image, reuses instead of rebuilding
3. Config changes → builds new image instead of reusing
4. Cleanup requested → removes unused images, preserves active/recent

### Image Cache Implementation

- [ ] T043 [US3] Implement upsert_image() method in ImageCache to persist image metadata to SQLite after build
- [ ] T044 [US3] Implement get_image_by_config_hash() method for fast cache lookup before building
- [ ] T045 [US3] Update build_docker_image() to check cache first, skip build if available image found
- [ ] T046 [US3] Implement mark_image_used() to update last_used timestamp (for LRU eviction)

### Image Eviction

- [ ] T047 [P] [US3] Implement evict_lru_images() in ImageCache with configurable max_count (default: 50)
- [ ] T048 [P] [US3] Implement docker image rm command in container_manager for removing evicted images from Docker daemon

### Testing

- [ ] T049 [US3] Create integration test in runtime-core/tests/integration/test_docker_shared_containers.rs: verify image reuse across 2 sessions

**User Story 3 Validation**:
- Run: `cargo test test_docker_shared_containers`
- Expected: Second session startup <5s (SC-005), image cache hit confirmed in logs

---

## Final Phase: Polish & Cross-Cutting Concerns

**Goal**: Finalize documentation, examples, and performance validation

**Duration**: ~4 hours

### Documentation

- [ ] T050 [P] Create example pipeline in examples/docker-node/ with manifest.yaml, custom_node.py, README.md
- [ ] T051 [P] Update runtime-core/README.md with Docker executor usage section referencing specs/009-docker-node-execution/quickstart.md

### Performance Validation

- [ ] T052 Create benchmark in runtime-core/benches/bench_docker_latency.rs comparing multiprocess vs docker latency for same node

**Final Validation**:
- Run: `cargo bench bench_docker_latency`
- Expected: Docker latency ≤ multiprocess + 5ms (SC-001)
- Run: Example pipeline from examples/docker-node/
- Expected: Works end-to-end, demonstrates feature

---

## Dependency Graph

### User Story Dependencies

```
Phase 1 (Setup)
    ↓
Phase 2 (Foundational)
    ↓
Phase 3 (User Story 1 - P1) ────┐
    ↓                            │ Can be independent
Phase 4 (User Story 2 - P2) ←───┘ (requires US1 foundation)
    ↓
Phase 5 (User Story 3 - P3) ←──── (requires US1, benefits from US2 testing)
    ↓
Final Phase (Polish)
```

### Critical Path

**Must Complete Before US1**:
1. T001-T004 (Setup)
2. T005-T010 (Foundational)

**US1 Internal Dependencies**:
- T011-T013 (Config) → T014-T018 (Container Mgmt)
- T019-T021 (Image Building) can be parallel with Container Mgmt
- T022-T026 (IPC Bridge) can be parallel with Image Building
- T027-T030 (Docker Executor) requires all above
- T031-T033 (Logging) can be parallel with Executor
- T034-T035 (Integration Test) requires all above

**US2 Dependencies**:
- Requires: All of US1 (T001-T035)
- T036-T039 (Registry) → T040-T041 (Multi-container flow)
- T042 (Test) requires all US2 tasks

**US3 Dependencies**:
- Requires: US1 complete (T001-T035)
- T043-T046 (Cache) can be parallel
- T047-T048 (Eviction) requires T043-T046
- T049 (Test) requires all US3 tasks

---

## Parallel Execution Guide

### Phase 3 (User Story 1) Parallel Groups

**Group 1** (can run in parallel):
- T011-T013 (Config setup)
- T019-T020 (Image building foundation)

**Group 2** (after Group 1):
- T014-T018 (Container management)
- T022-T026 (IPC bridge)

**Group 3** (after Group 2):
- T027-T030 (Docker executor - sequential)
- T031-T033 (Logging - parallel with executor)

**Group 4** (after Group 3):
- T034-T035 (Integration tests)

### Phase 4 (User Story 2) Parallel Groups

**Group 1**:
- T036-T039 (Registry) - sequential within group

**Group 2** (after Group 1):
- T040 (Routing)
- T041 (Health monitoring) - can be parallel with T040

**Group 3** (after Group 2):
- T042 (Integration test)

### Phase 5 (User Story 3) Parallel Groups

**Group 1** (can run in parallel):
- T043-T046 (Cache implementation)
- T047-T048 (Eviction logic)

**Group 2** (after Group 1):
- T049 (Integration test)

### Final Phase Parallel Groups

- T050-T051 (Documentation) - fully parallel
- T052 (Benchmark) - can be parallel with docs

---

## Testing Strategy

### Test-First Approach (Optional)

While TDD is not explicitly required in the spec, it's recommended for critical components:

**Recommended TDD Tasks**:
- T011 (DockerizedNodeConfiguration validation)
- T015-T018 (Container lifecycle)
- T026 (Session-scoped channel naming)
- T038 (Reference counting)

**Integration Test Order**:
1. T034: Single Docker node (US1)
2. T042: Multiple Docker nodes (US2)
3. T049: Image caching (US3)
4. T052: Performance benchmark (Final)

### Acceptance Criteria Validation

Each user story maps to specific tests:

| User Story | Integration Test | Success Criteria Validated |
|------------|------------------|----------------------------|
| US1 (P1) | test_docker_executor.rs | SC-001, SC-003, SC-004, SC-005 |
| US2 (P2) | test_docker_multiprocess.rs | SC-002, SC-007 |
| US3 (P3) | test_docker_shared_containers.rs | SC-005, SC-006 |
| All | bench_docker_latency.rs | SC-001, SC-002 |

---

## Task Checklist Summary

**Total Tasks**: 52
- **Phase 1 (Setup)**: 4 tasks
- **Phase 2 (Foundational)**: 6 tasks
- **Phase 3 (US1 - P1)**: 25 tasks ← **MVP SCOPE**
- **Phase 4 (US2 - P2)**: 7 tasks
- **Phase 5 (US3 - P3)**: 7 tasks
- **Final Phase (Polish)**: 3 tasks

**Parallel Opportunities**: 15 tasks marked `[P]` can be parallelized

**Estimated Effort**:
- MVP (US1): ~3-4 days (1 developer)
- US2: +1 day
- US3: +1 day
- Polish: +0.5 day
- **Total**: ~5.5-6.5 days

---

## Success Metrics

Upon completion of all tasks, the following must be validated:

### Functional Validation

- [x] FR-001: Docker and multiprocess nodes coexist in same pipeline
- [x] FR-002-003: iceoryx2 IPC works between host↔container and container↔container
- [x] FR-004: Manifest parsing supports docker executor with all config fields
- [x] FR-005: Containers have /tmp/iceoryx2 and /dev/shm mounts
- [x] FR-006: Unique runtime names prevent conflicts
- [x] FR-007: Container lifecycle managed (create/start/stop/remove)
- [x] FR-008: Docker daemon validation before node creation
- [x] FR-009: Container failures propagate to error handling
- [x] FR-010: Session-scoped channel naming implemented
- [x] FR-011: Container logs streamed to host logging
- [x] FR-012: Containers shared across sessions with same config
- [x] FR-013: Standard base images for Py 3.9/3.10/3.11 + custom image support
- [x] FR-014: Strict resource limits enforced via Docker hard limits
- [x] FR-015: Reference counting implemented, cleanup on zero refs
- [x] FR-016: Custom base image validation before use
- [x] FR-017: Clear error messages for resource violations

### Performance Validation

Run benchmarks after US1 completion:

```bash
cd runtime-core
cargo bench bench_docker_latency
```

Expected results (from spec.md success criteria):

- **SC-001**: Docker node latency ≤ multiprocess + 5ms ✓
- **SC-002**: 3 Docker nodes pipeline <100ms end-to-end ✓ (validate in US2)
- **SC-003**: Zero-copy verified (constant memory usage) ✓
- **SC-004**: Container failure detected within 2s ✓
- **SC-005**: Cached image startup <5s ✓ (validate in US3)
- **SC-006**: Zero orphaned resources after 100 sessions ✓
- **SC-007**: 5 concurrent sessions without conflicts ✓ (validate in US2)
- **SC-008**: Container logs visible in host logs ✓

---

## Next Steps

1. **Review this task list** with the team
2. **Start with Phase 1** (Setup tasks T001-T004)
3. **Complete Phase 2** (Foundational tasks T005-T010)
4. **Implement US1 (MVP)** (T011-T035)
5. **Validate MVP** against acceptance criteria
6. **Decide**: Ship MVP or continue to US2/US3
7. **Iterate** through remaining user stories based on priority

For implementation guidance, refer to:
- [quickstart.md](quickstart.md) - Developer usage guide
- [data-model.md](data-model.md) - Entity structures and validation
- [research.md](research.md) - Technology decisions and rationale
- [contracts/manifest-docker-extension.yaml](contracts/manifest-docker-extension.yaml) - Manifest schema

---

**Ready to implement!** Start with `/task T001` or jump to any specific task.
