# Tasks: Native Rust gRPC Service for Remote Execution

**Feature Branch**: `003-rust-grpc-service`  
**Input**: Design documents from `/specs/003-rust-grpc-service/`  
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/*.proto ✅

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Project Initialization)

**Purpose**: Initialize project structure, dependencies, and protocol buffer compilation

- [ ] T001 Create directory structure: `runtime/src/grpc_service/`, `runtime/protos/`, `runtime/bin/`, `runtime/tests/grpc_integration/`
- [ ] T002 Update `runtime/Cargo.toml` with gRPC dependencies: tonic 0.10+, prost 0.12+, tokio 1.35+, tower, prometheus, tracing
- [ ] T003 Update `runtime/build.rs` to add prost-build proto compilation targeting `runtime/protos/*.proto`
- [ ] T004 [P] Copy protocol buffer schemas to `runtime/protos/`: common.proto, execution.proto, streaming.proto
- [ ] T005 [P] Create `runtime/src/grpc_service/mod.rs` with module structure (server, execution, streaming, auth, limits, metrics, version submodules)
- [ ] T006 Test proto compilation: Run `cargo build` and verify generated Rust types in `target/` output

**Checkpoint**: Proto compilation working, directory structure ready

---

## Phase 2: Foundational (Shared Infrastructure)

**Purpose**: Core infrastructure that ALL user stories depend on - MUST be complete before ANY story implementation

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T007 [P] Implement authentication middleware in `runtime/src/grpc_service/auth.rs`: API token validation via tower interceptor
- [ ] T008 [P] Implement metrics collection in `runtime/src/grpc_service/metrics.rs`: Prometheus registry with request counters, latency histograms
- [ ] T009 [P] Implement structured logging in `runtime/src/grpc_service/mod.rs`: tracing subscriber with JSON format to stdout
- [ ] T010 Implement resource limits enforcement in `runtime/src/grpc_service/limits.rs`: Memory cap, timeout enforcement, buffer size validation
- [ ] T011 Implement version negotiation in `runtime/src/grpc_service/version.rs`: Protocol version checking, compatibility matrix
- [ ] T012 [P] Implement error mapping utilities in `runtime/src/grpc_service/mod.rs`: Convert Rust errors to protobuf ErrorResponse types
- [ ] T013 Create configuration struct in `runtime/src/grpc_service/mod.rs`: ServiceConfig with default limits, auth keys, port binding
- [ ] T014 Implement tonic server setup in `runtime/src/grpc_service/server.rs`: Server builder with middleware stack (auth, metrics, logging)
- [ ] T015 Create binary entry point in `runtime/bin/grpc_server.rs`: CLI arg parsing (--port, --auth-keys, --config-file), graceful shutdown

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Remote Pipeline Execution (Priority: P1) 🎯 MVP

**Goal**: Enable clients to submit complete pipeline manifests and receive processed results via unary RPC. Delivers core remote execution capability.

**Independent Test**: Deploy service, connect client, submit resample pipeline with 1-second audio, verify output matches local execution and latency <10ms

### Tests for User Story 1

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [ ] T016 [P] [US1] Contract test for ExecutePipeline RPC in `runtime/tests/grpc_integration/test_execution_contract.rs`: Submit valid manifest, verify response schema matches ExecuteResponse proto
- [ ] T017 [P] [US1] Integration test: Simple resample pipeline in `runtime/tests/grpc_integration/test_execution_resample.rs`: 44.1kHz → 16kHz, verify sample-accurate output
- [ ] T018 [P] [US1] Integration test: Multi-node pipeline in `runtime/tests/grpc_integration/test_execution_multi_node.rs`: Resample → VAD → output, verify node execution order
- [ ] T019 [P] [US1] Performance test in `runtime/tests/grpc_integration/test_execution_performance.rs`: Measure latency for 1s audio resample, assert <5ms p50, <10ms p95
- [ ] T020 [P] [US1] Metrics test in `runtime/tests/grpc_integration/test_execution_metrics.rs`: Verify ExecutionMetrics includes wall_time, memory_used, node_metrics

### Implementation for User Story 1

- [ ] T021 [P] [US1] Implement manifest deserialization in `runtime/src/grpc_service/execution.rs`: Parse PipelineManifest proto to runtime::Manifest struct
- [ ] T022 [P] [US1] Implement audio buffer conversion in `runtime/src/grpc_service/execution.rs`: Convert AudioBuffer proto to runtime::AudioBuffer with zero-copy where possible
- [ ] T023 [US1] Implement ExecutePipeline handler in `runtime/src/grpc_service/execution.rs`: Validate manifest → deserialize audio inputs → execute pipeline → serialize results
- [ ] T024 [US1] Implement manifest validation in `runtime/src/grpc_service/execution.rs`: Check version, validate node IDs unique, verify connections form DAG, validate node types exist
- [ ] T025 [US1] Implement execution result serialization in `runtime/src/grpc_service/execution.rs`: Convert runtime outputs to ExecutionResult proto with audio_outputs and data_outputs maps
- [ ] T026 [US1] Implement metrics collection in `runtime/src/grpc_service/execution.rs`: Capture wall_time, cpu_time, memory_used, serialization_time, populate per-node NodeMetrics
- [ ] T027 [US1] Implement error handling in `runtime/src/grpc_service/execution.rs`: Map Rust runtime errors to ErrorResponse proto with appropriate ErrorType
- [ ] T028 [US1] Wire ExecutePipeline RPC to PipelineExecutionService in `runtime/src/grpc_service/server.rs`: Add tonic service impl with auth/metrics middleware
- [ ] T029 [US1] Add GetVersion RPC implementation in `runtime/src/grpc_service/version.rs`: Return VersionInfo with protocol_version, runtime_version, supported_node_types
- [ ] T030 [US1] Integration: Test ExecutePipeline RPC end-to-end with tonic client from `runtime/tests/grpc_integration/test_execution.rs`

**Checkpoint**: User Story 1 complete - unary pipeline execution fully functional with <5ms latency for simple operations

---

## Phase 4: User Story 2 - Concurrent Multi-Client Support (Priority: P2)

**Goal**: Enable service to handle 1000+ concurrent client connections with independent pipeline executions without performance degradation

**Independent Test**: Connect 100 concurrent clients, submit identical pipelines simultaneously, verify all complete successfully within expected time bounds

### Tests for User Story 2

- [ ] T031 [P] [US2] Load test in `runtime/tests/grpc_integration/test_concurrent_load.rs`: Launch 100 concurrent clients, submit ExecutePipeline requests, verify all succeed
- [ ] T032 [P] [US2] Isolation test in `runtime/tests/grpc_integration/test_concurrent_isolation.rs`: Run concurrent pipelines with one failing execution, verify others unaffected
- [ ] T033 [P] [US2] Performance degradation test in `runtime/tests/grpc_integration/test_concurrent_performance.rs`: Measure latency at 1, 10, 100, 1000 concurrent requests, verify <20% degradation
- [ ] T034 [P] [US2] Connection pooling test in `runtime/tests/grpc_integration/test_concurrent_connections.rs`: Verify 1000 concurrent connections accepted without errors
- [ ] T035 [P] [US2] Memory test in `runtime/tests/grpc_integration/test_concurrent_memory.rs`: Verify memory usage per concurrent execution <10MB

### Implementation for User Story 2

- [ ] T036 [P] [US2] Configure tokio runtime in `runtime/bin/grpc_server.rs`: Multi-threaded runtime with worker thread pool sized to CPU cores
- [ ] T037 [US2] Implement connection pooling in `runtime/src/grpc_service/server.rs`: Configure tonic server with connection limits, keep-alive settings
- [ ] T038 [US2] Implement per-request resource isolation in `runtime/src/grpc_service/execution.rs`: Each ExecutePipeline spawns isolated tokio task with dedicated memory allocator
- [ ] T039 [US2] Add concurrency metrics in `runtime/src/grpc_service/metrics.rs`: Active connections gauge, concurrent executions gauge, connection pool utilization
- [ ] T040 [US2] Implement graceful degradation in `runtime/src/grpc_service/execution.rs`: Return service-unavailable gRPC status when connection limit reached, include retry-after hint
- [ ] T041 [US2] Add backpressure mechanism in `runtime/src/grpc_service/server.rs`: Queue incoming requests when all workers busy, reject after threshold
- [ ] T042 [US2] Optimize audio buffer allocation in `runtime/src/grpc_service/execution.rs`: Use memory pools for common buffer sizes, reduce per-request allocations
- [ ] T043 [US2] Add load shedding in `runtime/src/grpc_service/limits.rs`: Drop requests when CPU >90% or memory >80% capacity with appropriate error response

**Checkpoint**: User Story 2 complete - service handles 1000+ concurrent connections with <20% performance degradation

---

## Phase 5: User Story 3 - Streaming Audio Processing (Priority: P2)

**Goal**: Enable real-time chunk-by-chunk audio processing with bidirectional streaming RPC for latency-sensitive applications

**Independent Test**: Connect client, stream 100ms audio chunks at 10 Hz for VAD processing, verify results arrive with <50ms latency per chunk

### Tests for User Story 3

- [ ] T044 [P] [US3] Contract test for StreamPipeline RPC in `runtime/tests/grpc_integration/test_streaming_contract.rs`: Verify StreamRequest/StreamResponse message types
- [ ] T045 [P] [US3] Integration test: Streaming VAD in `runtime/tests/grpc_integration/test_streaming_vad.rs`: Stream 100ms chunks, verify ChunkResult responses in order
- [ ] T046 [P] [US3] Latency test in `runtime/tests/grpc_integration/test_streaming_latency.rs`: Stream 100 chunks, measure per-chunk latency, assert <50ms average
- [ ] T047 [P] [US3] Backpressure test in `runtime/tests/grpc_integration/test_streaming_backpressure.rs`: Stream chunks faster than processing, verify buffer overflow handling
- [ ] T048 [P] [US3] Session lifecycle test in `runtime/tests/grpc_integration/test_streaming_lifecycle.rs`: Test COMMAND_CLOSE graceful shutdown, verify final metrics returned

### Implementation for User Story 3

- [ ] T049 [P] [US3] Implement StreamInit handling in `runtime/src/grpc_service/streaming.rs`: Parse manifest, initialize pipeline, return StreamReady with session_id
- [ ] T050 [P] [US3] Implement AudioChunk processing in `runtime/src/grpc_service/streaming.rs`: Deserialize chunk, execute pipeline with streaming node, return ChunkResult
- [ ] T051 [US3] Implement StreamPipeline handler in `runtime/src/grpc_service/streaming.rs`: Bidirectional stream loop - receive StreamRequest, send StreamResponse
- [ ] T052 [US3] Implement sequence number validation in `runtime/src/grpc_service/streaming.rs`: Detect out-of-order or missing chunks, return STREAM_ERROR_INVALID_SEQUENCE
- [ ] T053 [US3] Implement streaming buffer management in `runtime/src/grpc_service/streaming.rs`: Bounded queue for incoming chunks, backpressure when full
- [ ] T054 [US3] Implement StreamControl handling in `runtime/src/grpc_service/streaming.rs`: COMMAND_CLOSE flushes pending chunks, COMMAND_CANCEL aborts immediately
- [ ] T055 [US3] Implement StreamMetrics emission in `runtime/src/grpc_service/streaming.rs`: Periodic metrics updates (every 10 chunks) with latency, buffer occupancy
- [ ] T056 [US3] Implement session management in `runtime/src/grpc_service/streaming.rs`: Track active sessions, cleanup on disconnect, enforce session timeout
- [ ] T057 [US3] Add streaming-specific metrics in `runtime/src/grpc_service/metrics.rs`: Active streams gauge, chunks_per_second rate, stream_latency histogram
- [ ] T058 [US3] Wire StreamPipeline RPC to StreamingPipelineService in `runtime/src/grpc_service/server.rs`: Add tonic service impl with auth/metrics middleware
- [ ] T059 [US3] Integration: Test StreamPipeline RPC end-to-end with tonic streaming client from `runtime/tests/grpc_integration/test_streaming.rs`

**Checkpoint**: User Story 3 complete - bidirectional streaming with <50ms per-chunk latency for real-time processing

---

## Phase 6: User Story 4 - Error Handling & Diagnostics (Priority: P3)

**Goal**: Provide detailed, actionable error messages that enable clients to quickly diagnose and fix issues with manifests, parameters, or execution failures

**Independent Test**: Submit invalid requests (malformed manifest, unsupported node, invalid audio format), verify each returns specific error with diagnostic context

### Tests for User Story 4

- [ ] T060 [P] [US4] Validation error test in `runtime/tests/grpc_integration/test_error_validation.rs`: Submit malformed manifest JSON, verify ERROR_TYPE_VALIDATION with parse error details
- [ ] T061 [P] [US4] Unsupported node test in `runtime/tests/grpc_integration/test_error_unsupported_node.rs`: Reference non-existent node type, verify error lists available types
- [ ] T062 [P] [US4] Execution error test in `runtime/tests/grpc_integration/test_error_execution.rs`: Trigger runtime error (invalid sample rate), verify ERROR_TYPE_NODE_EXECUTION with node ID and context
- [ ] T063 [P] [US4] Resource limit error test in `runtime/tests/grpc_integration/test_error_resource_limit.rs`: Exceed memory limit, verify ERROR_TYPE_RESOURCE_LIMIT with limit values
- [ ] T064 [P] [US4] Auth error test in `runtime/tests/grpc_integration/test_error_auth.rs`: Send request without token, verify ERROR_TYPE_AUTHENTICATION with clear message
- [ ] T065 [P] [US4] Version mismatch test in `runtime/tests/grpc_integration/test_error_version.rs`: Send unsupported protocol version, verify ERROR_TYPE_VERSION_MISMATCH with supported versions

### Implementation for User Story 4

- [ ] T066 [P] [US4] Implement detailed manifest validation in `runtime/src/grpc_service/execution.rs`: Check manifest.version, validate JSON schema, return line-specific parse errors
- [ ] T067 [P] [US4] Implement node type validation in `runtime/src/grpc_service/execution.rs`: Query node registry, list available types in error if unsupported node referenced
- [ ] T068 [US4] Implement execution context capture in `runtime/src/grpc_service/execution.rs`: Serialize node inputs, parameters to JSON for error context field
- [ ] T069 [US4] Implement stack trace extraction in `runtime/src/grpc_service/mod.rs`: Capture Rust panic backtraces, include in ErrorResponse.stack_trace when available
- [ ] T070 [US4] Implement error categorization in `runtime/src/grpc_service/mod.rs`: Map all runtime errors to appropriate ErrorType (validation, execution, resource, etc.)
- [ ] T071 [US4] Implement detailed resource limit errors in `runtime/src/grpc_service/limits.rs`: Include current usage, limit values, suggested actions in error message
- [ ] T072 [US4] Implement authentication error details in `runtime/src/grpc_service/auth.rs`: Return whether token missing vs invalid, include auth setup instructions
- [ ] T073 [US4] Implement version compatibility errors in `runtime/src/grpc_service/version.rs`: List supported versions, link to compatibility matrix documentation
- [ ] T074 [US4] Add debug logging for all error paths in `runtime/src/grpc_service/execution.rs` and `streaming.rs`: Log full error context before returning to client
- [ ] T075 [US4] Create error response examples in `specs/003-rust-grpc-service/quickstart.md`: Document common errors and resolutions

**Checkpoint**: User Story 4 complete - all error types return actionable diagnostic information

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, deployment, optimization, and validation across all user stories

- [ ] T076 [P] Add deployment documentation in `specs/003-rust-grpc-service/quickstart.md`: Systemd service file, Docker container, environment variables
- [ ] T077 [P] Create client library examples in `python-client/examples/grpc_execution_example.py`: ExecutePipeline, StreamPipeline, error handling
- [ ] T078 [P] Create client library examples in `nodejs-client/examples/grpc_execution_example.ts`: ExecutePipeline with TypeScript types
- [ ] T079 [P] Document metrics endpoints in `specs/003-rust-grpc-service/quickstart.md`: Prometheus scraping, example Grafana dashboards
- [ ] T080 [P] Document authentication setup in `specs/003-rust-grpc-service/quickstart.md`: API token generation, client configuration
- [ ] T081 Implement Python gRPC client wrapper in `python-client/remotemedia/grpc_client.py`: High-level API wrapping tonic-generated stubs
- [ ] T082 Implement TypeScript gRPC client wrapper in `nodejs-client/src/grpc_client.ts`: High-level API wrapping grpc-tools-generated stubs
- [ ] T083 [P] Add service health check endpoint in `runtime/src/grpc_service/server.rs`: HTTP /health endpoint for load balancer probes
- [ ] T084 [P] Optimize proto serialization in `runtime/src/grpc_service/execution.rs`: Use Bytes wrapper for zero-copy audio samples
- [ ] T085 [P] Add request tracing in `runtime/src/grpc_service/mod.rs`: Distributed tracing with correlation IDs for multi-service debugging
- [ ] T086 Profile and optimize hot paths in `runtime/src/grpc_service/execution.rs`: Reduce allocations, optimize audio buffer copies
- [ ] T087 [P] Create benchmark suite in `runtime/tests/grpc_integration/benchmark.rs`: Measure throughput, latency at 1/10/100/1000 concurrent requests
- [ ] T088 [P] Add security hardening in `runtime/src/grpc_service/auth.rs`: Rate limiting, token rotation, audit logging
- [ ] T089 Validate all quickstart.md examples: Build service, run client examples, verify outputs match expected results
- [ ] T090 [P] Update CHANGELOG.md with feature summary, breaking changes, migration guide from Python gRPC service

**Checkpoint**: Feature complete - service production-ready with documentation, client libraries, and performance validation

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - User stories can proceed in parallel after Phase 2 (if staffed)
  - Or sequentially in priority order: US1 (P1) → US2 (P2) → US3 (P2) → US4 (P3)
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Independent of US1 but benefits from US1 testing
- **User Story 3 (P2)**: Can start after Foundational (Phase 2) - Reuses execution.rs patterns from US1
- **User Story 4 (P3)**: Can start after Foundational (Phase 2) - Enhances error handling across US1/US2/US3

### Within Each User Story

- Tests MUST be written and FAIL before implementation
- Contract tests before integration tests
- Core handler implementation before wire-up to gRPC service
- Metrics/logging after core functionality works
- Story complete and independently testable before moving to next priority

### Parallel Opportunities

#### Phase 1 (Setup)
```bash
# Can run simultaneously:
T004 [P] Copy protocol buffer schemas
T005 [P] Create grpc_service/mod.rs structure
```

#### Phase 2 (Foundational)
```bash
# Can run simultaneously:
T007 [P] Auth middleware
T008 [P] Metrics collection
T009 [P] Structured logging
T012 [P] Error mapping utilities
```

#### Phase 3 (User Story 1)
```bash
# Tests in parallel:
T016-T020 [P] All contract/integration/performance tests for US1

# Implementation in parallel:
T021 [P] Manifest deserialization
T022 [P] Audio buffer conversion
```

#### Phase 4-6 (User Stories 2-4)
All user stories can be developed in parallel by different team members after Phase 2 completes.

#### Phase 7 (Polish)
```bash
# Documentation and examples in parallel:
T076-T080 [P] Documentation tasks
T081-T082 [P] Client library wrappers
T084-T085 [P] Optimizations
```

---

## Parallel Example: User Story 1

```bash
# Launch all US1 tests together:
cargo test --test test_execution_contract &
cargo test --test test_execution_resample &
cargo test --test test_execution_multi_node &
cargo test --test test_execution_performance &
cargo test --test test_execution_metrics &
wait

# Launch parallel implementation tasks:
# Terminal 1: Manifest deserialization (T021)
# Terminal 2: Audio buffer conversion (T022)
# Both complete, then proceed to T023 (ExecutePipeline handler)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T006)
2. Complete Phase 2: Foundational (T007-T015) ← CRITICAL blocking phase
3. Complete Phase 3: User Story 1 (T016-T030)
4. **STOP and VALIDATE**: Deploy service, run all US1 tests, verify <5ms latency
5. Demo/deploy MVP if validation passes

### Incremental Delivery

1. **Foundation**: Setup + Foundational (T001-T015) → Protocol compilation working, auth/metrics/logging ready
2. **MVP**: + User Story 1 (T016-T030) → Unary pipeline execution working, <5ms latency ✅
3. **Scale**: + User Story 2 (T031-T043) → 1000+ concurrent connections ✅
4. **Real-time**: + User Story 3 (T044-T059) → Bidirectional streaming <50ms latency ✅
5. **Diagnostics**: + User Story 4 (T060-T075) → Production-quality error handling ✅
6. **Production**: + Polish (T076-T090) → Documentation, client libs, benchmarks ✅

Each increment is independently deployable and testable.

### Parallel Team Strategy

With multiple developers:

1. **Entire team**: Complete Setup + Foundational together (T001-T015)
2. **Once Phase 2 done**:
   - **Developer A**: User Story 1 (T016-T030) - Remote execution
   - **Developer B**: User Story 2 (T031-T043) - Concurrency
   - **Developer C**: User Story 3 (T044-T059) - Streaming
   - **Developer D**: User Story 4 (T060-T075) - Error handling
3. Stories complete independently, integrate at Phase 7

---

## Performance Validation Checkpoints

After each user story completion, validate success criteria:

### User Story 1 (T030 checkpoint)
- ✅ SC-001: <5ms p50 latency for simple operations
- ✅ SC-003: <10% serialization overhead
- ✅ SC-004: 10x faster than Python-based execution

### User Story 2 (T043 checkpoint)
- ✅ SC-002: 1000+ concurrent connections without failures
- ✅ SC-005: 95% of requests complete within 2x local execution time
- ✅ SC-008: <10MB memory per concurrent execution

### User Story 3 (T059 checkpoint)
- ✅ <50ms average latency per chunk (from spec.md US3)
- ✅ Support 1000+ concurrent streaming sessions

### User Story 4 (T075 checkpoint)
- ✅ All error types include actionable diagnostic context
- ✅ SC-007: Integration under 1 hour with examples

---

## Notes

- [P] tasks = different files, no dependencies, can parallelize
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Tests written FIRST, must FAIL before implementation
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Foundation (Phase 2) is CRITICAL - blocks all stories until complete
- Proto compilation (T006) must succeed before any implementation
