# Tasks: Low-Latency Real-Time Streaming Pipeline

**Input**: Design documents from `/specs/007-low-latency-streaming/`
**Prerequisites**: plan.md (✅), spec.md (✅), research.md (✅), data-model.md (✅), contracts/ (✅)

**Tests**: Included as per Test-First requirement in constitution check (plan.md)

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and dependencies

- [x] T001 Add new dependencies to Cargo.toml: crossbeam = "0.8", hdrhistogram = "7.5"
- [x] T002 [P] Create runtime-core/src/data/ module directory if not exists
- [x] T003 [P] Create runtime-core/src/data/mod.rs with module exports
- [x] T004 Run cargo check to verify dependencies resolve correctly

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core data structures and infrastructure that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Data Structures (Foundational)

- [x] T005 [P] Create SpeculativeSegment struct in runtime-core/src/data/speculative_segment.rs with SegmentStatus enum
- [x] T006 [P] Create ControlMessage struct in runtime-core/src/data/control_message.rs with ControlMessageType enum
- [x] T007 [P] Create NodeCapabilities struct in runtime-core/src/executor/node_capabilities.rs with OverflowPolicy enum
- [x] T008 [P] Create BufferingPolicy struct in runtime-core/src/data/buffering_policy.rs with MergeStrategy enum
- [x] T009 [P] Create LatencyMetrics struct in runtime-core/src/executor/latency_metrics.rs with Histogram integration
- [x] T010 [P] Create RingBuffer struct in runtime-core/src/data/ring_buffer.rs wrapping crossbeam::ArrayQueue

### Unit Tests for Data Structures

- [x] T011 [P] Write unit tests for SpeculativeSegment state transitions in runtime-core/src/data/speculative_segment.rs
- [x] T012 [P] Write unit tests for RingBuffer (push_overwrite, get_range, clear_before) in tests/unit/test_ring_buffer.rs
- [x] T013 [P] Write unit tests for NodeCapabilities validation in tests/unit/test_node_capabilities.rs
- [x] T014 [P] Write unit tests for MergeStrategy (ConcatenateText, ConcatenateAudio) in tests/unit/test_merge_strategies.rs
- [x] T015 Verify all unit tests FAIL before implementation, then implement and make them PASS

### Control Message Serialization

- [x] T016 Extend RuntimeData enum in runtime-core/src/python/multiprocess/data_transfer.rs with ControlMessage variant (DataType::ControlMessage = 5)
- [x] T017 Implement ControlMessage::to_bytes() serialization per wire format spec (type byte + session + timestamp + JSON payload)
- [x] T018 Implement ControlMessage::from_bytes() deserialization per wire format spec
- [x] T019 Write unit tests for control message serialization/deserialization in tests/unit/test_control_message_format.rs
- [x] T020 Verify tests FAIL, implement, then PASS

### Metrics Infrastructure

- [x] T021 Implement LatencyMetrics::record_latency() using hdrhistogram with rotating windows (1min/5min/15min)
- [x] T022 Implement LatencyMetrics::percentile() to query P50/P95/P99 from specified window
- [x] T023 Implement LatencyMetrics::to_prometheus() to export metrics in Prometheus format
- [x] T024 Write unit tests for histogram recording and rotation in tests/unit/test_latency_metrics.rs
- [x] T025 Verify tests FAIL, implement, then PASS

**Checkpoint**: Foundation ready - all data structures, serialization, and metrics infrastructure complete. User story implementation can now begin in parallel.

---

## Phase 3: User Story 1 - Real-Time Voice Interaction with Minimal Perceived Latency (Priority: P1) 🎯 MVP

**Goal**: Achieve sub-250ms P99 end-to-end latency via speculative VAD forwarding with retroactive cancellation

**Independent Test**: Stream audio through VAD → ASR pipeline, measure latency with/without speculation, verify P99 < 250ms @ 100 concurrent sessions

### Integration Tests for User Story 1 (TDD)

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [x] T026 [P] [US1] Write integration test for speculative VAD forwarding (audio immediately forwarded, cancellation on false positive) in tests/integration/test_speculative_vad.rs
- [x] T027 [P] [US1] Write integration test for control message propagation (local Rust, IPC, gRPC, WebRTC, HTTP) in tests/integration/test_control_messages.rs
- [x] T028 [P] [US1] Write benchmark for end-to-end latency measurement (P50/P95/P99) in tests/benchmarks/bench_latency.rs
- [x] T029 [US1] Verify all integration tests FAIL before starting implementation

### Implementation for User Story 1

#### SpeculativeVADGate Node

- [x] T030 [US1] Create SpeculativeVADGate struct in runtime-core/src/nodes/speculative_vad_gate.rs with RingBuffer (100-200ms lookback, 25-50ms lookahead)
- [x] T031 [US1] Implement StreamingNode trait for SpeculativeVADGate: process_streaming() forwards audio immediately, stores in ring buffer
- [x] T032 [US1] Implement VAD decision handling: on is_speech_end check, emit ControlMessage::CancelSpeculation if false positive
- [x] T033 [US1] Implement ring buffer maintenance: clear_before() to remove committed segments
- [x] T034 [US1] Add speculation acceptance rate tracking to LatencyMetrics

#### Control Message Handling in Nodes

- [x] T035 [P] [US1] Extend StreamingNode trait in runtime-core/src/nodes/streaming_node.rs with process_control_message() method
- [x] T036 [P] [US1] Implement process_control_message() for existing nodes (SileroVAD, AudioResample) to handle cancellation
- [x] T037 [US1] Update remote_pipeline.rs to forward control messages to remote transports

#### Control Message Propagation

- [x] T038 [P] [US1] Modify runtime-core/src/python/multiprocess/data_transfer.rs to serialize/deserialize control messages for iceoryx2
- [x] T039 [P] [US1] Modify runtime-core/src/python/multiprocess/multiprocess_executor.rs to forward control messages to Python via IPC
- [x] T040 [P] [US1] Update python-client/remotemedia/nodes/node.py to honor cancellation messages (terminate processing, discard partial results)
- [x] T041 [US1] Add control message deserialization to Python (deserialize_control_message, handle_control_message functions)

#### Transport Integration

- [x] T042 [P] [US1] Add ControlMessage to protobuf schema in transports/remotemedia-grpc/proto/ and regenerate
- [x] T043 [P] [US1] Modify transports/remotemedia-grpc/src/ to propagate control messages via gRPC streaming
- [x] T044 [P] [US1] Modify transports/remotemedia-webrtc/src/ to propagate control messages via WebRTC data channel (JSON)
- [x] T045 [P] [US1] Modify transports/remotemedia-http/src/ to propagate control messages via SSE (JSON events)

#### Registry and Configuration

- [x] T046 [US1] Register SpeculativeVADGate in runtime-core/src/nodes/streaming_registry.rs
- [x] T047 [US1] Add manifest configuration support for VAD thresholds (min_speech_ms, min_silence_ms, pad_ms, lookback_ms, lookahead_ms)
- [x] T048 [US1] Update quickstart.md example: pipeline_speculative_vad.yaml to demonstrate configuration

#### Testing and Validation

- [x] T049 [US1] Run integration test test_speculative_vad.rs and verify: audio forwarded immediately, cancellation messages generated and received
- [x] T050 [US1] Run integration test test_control_messages.rs and verify: <10ms P95 propagation across all contexts
- [x] T051 [US1] Run benchmark bench_latency.rs and verify: P99 < 250ms @ 100 concurrent sessions
- [ ] T052 [US1] Run load test for 1 hour and verify: no memory leaks, stable performance (SC-007)
- [x] T053 [US1] Measure speculation acceptance rate and verify: >95%, false positive rate <5% (SC-006)

**Checkpoint**: User Story 1 complete - speculative VAD with control message propagation achieving P99 < 250ms

---

## Phase 4: User Story 2 - Efficient Batch Processing for Non-Parallelizable Operations (Priority: P2)

**Goal**: Automatically batch inputs for non-parallelizable nodes (TTS) to increase throughput by 30% while maintaining latency < 300ms P95

**Independent Test**: Send rapid bursts of text to TTS node, measure batch sizes formed, queue depths, throughput improvement vs. non-batched baseline

### Integration Tests for User Story 2 (TDD)

- [ ] T054 [P] [US2] Write integration test for BufferedProcessor auto-batching (merge strategies, timeout behavior) in tests/integration/test_buffered_processor.rs
- [ ] T055 [P] [US2] Write benchmark for TTS throughput with/without batching in tests/benchmarks/bench_buffering.rs
- [ ] T056 [US2] Verify all integration tests FAIL before starting implementation

### Implementation for User Story 2

#### BufferedProcessor Wrapper

- [ ] T057 [US2] Create BufferedProcessor struct in runtime-core/src/nodes/buffered_processor.rs with internal queue and timer
- [ ] T058 [US2] Implement BufferedProcessor::new(inner_node, policy) constructor accepting any StreamingNode
- [ ] T059 [US2] Implement input queueing logic: accumulate inputs until min_batch_size or max_wait_ms timeout
- [ ] T060 [US2] Implement merge strategy execution (ConcatenateText, ConcatenateAudio, KeepSeparate, Custom)
- [ ] T061 [US2] Implement StreamingNode trait for BufferedProcessor: process_streaming() queues, merges, forwards to inner node
- [ ] T062 [US2] Add batch size and queue depth tracking to LatencyMetrics

#### NodeCapabilities Registry

- [ ] T063 [P] [US2] Create NodeCapabilities registry in runtime-core/src/executor/node_capabilities.rs as static HashMap
- [ ] T064 [P] [US2] Populate default capabilities for known node types (TTS: parallelizable=false, AudioResample: parallelizable=true, etc.)
- [ ] T065 [US2] Implement avg_processing_us tracking via exponential moving average (EMA α=0.1)
- [ ] T066 [US2] Add capability override support via manifest configuration

#### Executor Auto-Wrapping

- [ ] T067 [US2] Modify runtime-core/src/executor/mod.rs to detect non-parallelizable nodes at initialization (check NodeCapabilities)
- [ ] T068 [US2] Implement automatic BufferedProcessor wrapping for non-parallelizable nodes with default BufferingPolicy
- [ ] T069 [US2] Update scheduler.rs to integrate with NodeCapabilities for scheduling decisions
- [ ] T070 [US2] Add per-node bounded queues (tokio::sync::mpsc with capacity from NodeCapabilities)

#### Overflow Policies

- [ ] T071 [P] [US2] Implement OverflowPolicy::DropOldest in BufferedProcessor
- [ ] T072 [P] [US2] Implement OverflowPolicy::DropNewest in BufferedProcessor
- [ ] T073 [P] [US2] Implement OverflowPolicy::Block (backpressure via tokio async wait)
- [ ] T074 [US2] Implement OverflowPolicy::MergeOnOverflow (apply merge strategy to reduce queue depth)

#### Configuration and Testing

- [ ] T075 [US2] Register BufferedProcessor in runtime-core/src/nodes/streaming_registry.rs (as wrapper, not direct node)
- [ ] T076 [US2] Add manifest configuration support for buffering policies per node (min_batch_size, max_wait_ms, merge_strategy)
- [ ] T077 [US2] Update quickstart.md example: pipeline_batched_tts.yaml to demonstrate auto-batching

#### Testing and Validation

- [ ] T078 [US2] Run integration test test_buffered_processor.rs and verify: batches formed correctly, timeout honored, merge strategies work
- [ ] T079 [US2] Run benchmark bench_buffering.rs and verify: >30% throughput increase with batching (SC-003)
- [ ] T080 [US2] Verify P95 latency remains <300ms with auto-batching (SC-003)
- [ ] T081 [US2] Test overflow policies under load: DropOldest preserves real-time, Block applies backpressure
- [ ] T082 [US2] Measure queue depths and batch sizes via metrics, verify correct behavior

**Checkpoint**: User Story 2 complete - auto-batching for non-parallelizable nodes increasing throughput without sacrificing latency

---

## Phase 5: User Story 3 - Graceful Handling of Speculative Processing Corrections (Priority: P2)

**Goal**: Ensure downstream nodes honor cancellation messages, preventing wasted compute on false-positive segments

**Independent Test**: Inject audio with ambiguous boundaries, verify cancellation messages generated and honored (processing terminated, partial results discarded)

**Dependencies**: Requires User Story 1 (control message infrastructure) to be complete

### Implementation for User Story 3

**NOTE**: Most infrastructure for US3 was already built in US1. These tasks are for refinement and edge case handling.

#### Edge Case Handling

- [ ] T083 [US3] Implement race condition handling in SpeculativeVADGate: concurrent cancellation and output (timestamp-based ordering, mark output as "potentially cancelled")
- [ ] T084 [US3] Implement ring buffer overflow handling: dynamic expansion or commit oldest segments to confirmed status
- [ ] T085 [US3] Add cascade cancellation logic: ensure all downstream nodes receive cancellation even if intermediate node fails

#### Python Cancellation Enhancement

- [ ] T086 [US3] Enhance python-client/remotemedia/nodes/node.py cancellation handling: add segment tracking, graceful termination of async tasks
- [ ] T087 [US3] Add cancellation acknowledgment: Python nodes send ACK back to Rust via control message when cancellation processed
- [ ] T088 [US3] Implement partial result cleanup: ensure no stale data in buffers after cancellation

#### Reliability and Delivery Confirmation

- [ ] T089 [P] [US3] Add delivery confirmation to gRPC transport (ack on control message receipt)
- [ ] T090 [P] [US3] Add delivery confirmation to WebRTC transport (ack via data channel)
- [ ] T091 [P] [US3] Add delivery confirmation to HTTP/SSE transport (ack via HTTP response)
- [ ] T092 [US3] Implement retry logic for control messages: if no ACK within 50ms, retry up to 3 times

#### Testing and Validation

- [ ] T093 [US3] Test edge case: cancellation arrives during output emission, verify race condition handled gracefully
- [ ] T094 [US3] Test edge case: ring buffer overflow during long utterance, verify oldest segments committed or buffer expanded
- [ ] T095 [US3] Test cascade cancellation: inject failure in intermediate node, verify downstream still receives cancellation
- [ ] T096 [US3] Verify control message delivery confirmation: inject network delay, verify retry and eventual delivery
- [ ] T097 [US3] Measure cancellation overhead: verify cancellation processing adds <5ms to total latency

**Checkpoint**: User Story 3 complete - robust cancellation handling with edge case coverage and delivery confirmation

---

## Phase 6: User Story 4 - Streaming-Optimized Audio Resampling (Priority: P3)

**Goal**: Accept variable-sized audio chunks in resampler to eliminate buffering delays, achieving 10-30ms latency reduction

**Independent Test**: Send variable-sized chunks (64-2048 samples) through resampler, measure output availability latency and continuity

### Integration Tests for User Story 4 (TDD)

- [ ] T098 [P] [US4] Write integration test for streaming resampler (variable chunk sizes, continuity) in tests/integration/test_streaming_resampler.rs
- [ ] T099 [US4] Verify integration test FAILS before starting implementation

### Implementation for User Story 4

#### StreamingResampler Wrapper

- [ ] T100 [US4] Create StreamingResampler struct in runtime-core/src/nodes/audio/resample_streaming.rs wrapping rubato::FftFixedIn
- [ ] T101 [US4] Implement input buffer accumulation: collect samples until inner resampler's chunk_size reached
- [ ] T102 [US4] Implement output buffer management: store remainder samples for next iteration
- [ ] T103 [US4] Add timeout logic: if input <chunk_size and 5-10ms elapsed, process partial chunk with padding
- [ ] T104 [US4] Implement StreamingNode trait for StreamingResampler: process_streaming() accepts arbitrary chunk sizes

#### Configuration and Integration

- [ ] T105 [US4] Add streaming resampler option to AudioResample node configuration (streaming: true/false)
- [ ] T106 [US4] Update manifest schema to support streaming resampler flag per pipeline
- [ ] T107 [US4] Add fallback: if streaming=false, use existing fixed-chunk resampler (backward compatibility)

#### Testing and Validation

- [ ] T108 [US4] Run integration test test_streaming_resampler.rs and verify: variable chunks accepted, output continuous, no artifacts
- [ ] T109 [US4] Benchmark streaming vs. fixed-chunk resampler, verify >15ms P95 latency reduction (SC-005)
- [ ] T110 [US4] Test edge case: very small chunks (64 samples), verify timeout triggers and partial processing works
- [ ] T111 [US4] Test edge case: very large chunks (2048 samples), verify fragmentation and output correctness

**Checkpoint**: User Story 4 complete - streaming resampler reducing latency by 10-30ms for variable-sized chunks

---

## Phase 7: User Story 5 - Observable Latency Metrics for Performance Tuning (Priority: P3)

**Goal**: Expose per-node P50/P95/P99 metrics and queue depths for debugging and tuning

**Independent Test**: Run pipeline under load, query metrics endpoint, verify histograms, queue depths, and speculation rates reported correctly

### Implementation for User Story 5

**NOTE**: LatencyMetrics infrastructure was built in Phase 2. These tasks are for integration and observability.

#### Metrics Collection Integration

- [ ] T112 [P] [US5] Integrate LatencyMetrics into executor: wrap each node invocation with timing measurement
- [ ] T113 [P] [US5] Add queue depth tracking to per-node bounded queues (current depth, max depth observed)
- [ ] T114 [US5] Add batch size tracking to BufferedProcessor (average batch size via EMA)
- [ ] T115 [US5] Add speculation acceptance rate tracking to SpeculativeVADGate (confirmed vs. cancelled ratio)

#### Prometheus Export

- [ ] T116 [US5] Implement Prometheus metrics endpoint in runtime-core/src/executor/metrics.rs (HTTP server on configurable port, default 9090)
- [ ] T117 [US5] Expose histograms as Prometheus histogram type (quantile labels for 0.5, 0.95, 0.99)
- [ ] T118 [US5] Expose queue depths as Prometheus gauge type
- [ ] T119 [US5] Expose batch sizes and speculation rates as Prometheus gauge type
- [ ] T120 [US5] Add window labels (1min, 5min, 15min) to histogram metrics

#### Configuration and Documentation

- [ ] T121 [US5] Add metrics configuration to manifest: enable_metrics (bool), metrics_port (u16)
- [ ] T122 [US5] Update quickstart.md with metrics querying examples (curl commands, sample output)
- [ ] T123 [US5] Add metrics dashboard example (Grafana JSON or Prometheus alert rules)

#### Testing and Validation

- [ ] T124 [US5] Write benchmark for metrics collection overhead in tests/benchmarks/bench_metrics_overhead.rs
- [ ] T125 [US5] Run benchmark and verify: <1% overhead on node processing time (SC-008)
- [ ] T126 [US5] Verify metrics query latency: <5ms overhead per query (SC-008)
- [ ] T127 [US5] Test histogram rotation: run pipeline for 5 minutes, verify 1min/5min windows update correctly
- [ ] T128 [US5] Load test: 100 concurrent sessions for 1 hour, verify metrics remain accurate and system stable

**Checkpoint**: User Story 5 complete - comprehensive observability with P50/P95/P99 metrics, queue depths, and speculation rates

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories and final validation

### Deadline Hints (Optional Extension)

- [ ] T129 [P] Add optional deadline_hint_us field to RuntimeData in runtime-core/src/python/multiprocess/data_transfer.rs
- [ ] T130 [P] Implement deadline proximity checks in BufferedProcessor: if deadline approaching, increase batch size or reduce quality
- [ ] T131 Document deadline hints usage in quickstart.md with example scenarios

### TextCollector Batch Window (Optional Extension)

- [ ] T132 [P] Add batch_window_ms configuration to TextCollector in runtime-core/src/nodes/text_collector.rs (if exists, else create)
- [ ] T133 [P] Implement batch window logic: wait up to batch_window_ms before emitting first sentence to enable batching
- [ ] T134 Update quickstart.md example: pipeline_batched_tts.yaml to demonstrate batch window

### Documentation and Examples

- [ ] T135 [P] Update CLAUDE.md with all new nodes, data structures, and configuration options
- [ ] T136 [P] Create example manifest: specs/007-low-latency-streaming/examples/pipeline_full_optimized.yaml combining all optimizations
- [ ] T137 [P] Add troubleshooting section to quickstart.md: control messages not propagating, high memory usage, metrics not updating
- [ ] T138 Document performance tuning guide: VAD thresholds, chunk sizes, batch policies, overflow policies

### Final Validation

- [ ] T139 Run full test suite: cargo test --all-features
- [ ] T140 Run all integration tests: cargo test --test '*'
- [ ] T141 Run all benchmarks: cargo bench
- [ ] T142 Validate quickstart.md: run all example pipelines and verify expected behavior
- [ ] T143 Load test: 100 concurrent sessions for 1 hour, verify all success criteria (SC-001 through SC-009)
- [ ] T144 Generate performance report: P50/P95/P99 latencies, throughput improvements, false positive rates, memory usage

### Code Quality

- [ ] T145 [P] Run cargo clippy and fix all warnings
- [ ] T146 [P] Run cargo fmt to format all code
- [ ] T147 [P] Add inline documentation for all public structs and functions
- [ ] T148 Review and clean up TODOs and FIXMEs in codebase
- [ ] T149 Create PR description summarizing all changes, metrics, and success criteria validation

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational (Phase 2) - Can start after Phase 2 complete
- **User Story 2 (Phase 4)**: Depends on Foundational (Phase 2) - Can start after Phase 2 complete (independent of US1)
- **User Story 3 (Phase 5)**: Depends on User Story 1 (Phase 3) - Requires control message infrastructure from US1
- **User Story 4 (Phase 6)**: Depends on Foundational (Phase 2) - Can start after Phase 2 complete (independent of US1-3)
- **User Story 5 (Phase 7)**: Depends on Foundational (Phase 2) and partially on US1-4 for metrics integration - Can start after Phase 2, integrate with other stories as they complete
- **Polish (Phase 8)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Independent of US1 (can run in parallel)
- **User Story 3 (P2)**: Requires User Story 1 complete (control message infrastructure)
- **User Story 4 (P3)**: Can start after Foundational (Phase 2) - Independent of US1-3
- **User Story 5 (P3)**: Can start after Foundational (Phase 2), integrates with all other stories

### Within Each User Story

- Tests (if included) MUST be written and FAIL before implementation
- Data structures before business logic
- Core implementation before integration with other components
- Integration tests before validation
- Story complete and validated before moving to next priority

### Parallel Opportunities

**Phase 1 (Setup)**: All tasks marked [P] can run in parallel (T002, T003)

**Phase 2 (Foundational)**:
- All data structure tasks (T005-T010) can run in parallel
- All unit test tasks (T011-T014) can run in parallel after data structures complete
- Control message serialization tasks (T016-T019) can run in parallel
- Metrics infrastructure tasks (T021-T024) can run in parallel

**Phase 3 (User Story 1)**:
- Integration tests (T026-T028) can be written in parallel
- Control message handling in nodes (T035-T036) can run in parallel
- Python IPC and transport integration (T038-T041, T042-T045) can run in parallel after control message format complete

**Phase 4 (User Story 2)**:
- NodeCapabilities registry and overflow policies (T063-T066, T071-T074) can run in parallel

**Across User Stories**:
- Once Phase 2 (Foundational) completes:
  - User Story 1 and User Story 2 can start in parallel (if team capacity allows)
  - User Story 4 can start in parallel with US1 and US2
- User Story 3 can start once User Story 1 completes
- User Story 5 can start incrementally, integrating metrics as each story completes

---

## Parallel Example: Foundational Phase

```bash
# Launch all data structure tasks together:
Task: "Create SpeculativeSegment struct in runtime-core/src/data/speculative_segment.rs"
Task: "Create ControlMessage struct in runtime-core/src/data/control_message.rs"
Task: "Create NodeCapabilities struct in runtime-core/src/executor/node_capabilities.rs"
Task: "Create BufferingPolicy struct in runtime-core/src/data/buffering_policy.rs"
Task: "Create LatencyMetrics struct in runtime-core/src/executor/latency_metrics.rs"
Task: "Create RingBuffer struct in runtime-core/src/data/ring_buffer.rs"

# After data structures complete, launch all unit tests together:
Task: "Write unit tests for SpeculativeSegment state transitions"
Task: "Write unit tests for RingBuffer operations"
Task: "Write unit tests for NodeCapabilities validation"
Task: "Write unit tests for MergeStrategy"
```

---

## Parallel Example: User Story 1

```bash
# Launch all integration tests together (write first, ensure they fail):
Task: "Write integration test for speculative VAD forwarding in tests/integration/test_speculative_vad.rs"
Task: "Write integration test for control message propagation in tests/integration/test_control_messages.rs"
Task: "Write benchmark for end-to-end latency in tests/benchmarks/bench_latency.rs"

# After core implementation, launch all transport integration tasks together:
Task: "Modify gRPC transport to propagate control messages"
Task: "Modify WebRTC transport to propagate control messages"
Task: "Modify HTTP transport to propagate control messages"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T004)
2. Complete Phase 2: Foundational (T005-T025) - **CRITICAL, blocks all stories**
3. Complete Phase 3: User Story 1 (T026-T053)
4. **STOP and VALIDATE**:
   - Run all US1 integration tests
   - Verify P99 latency <250ms @ 100 sessions
   - Verify speculation acceptance >95%
   - Verify control message propagation <10ms P95
5. Deploy/demo MVP with speculative VAD

**Expected Outcome**: Achieve core low-latency goal (P99 <250ms) with speculative forwarding and cancellation. This is the minimum viable feature that delivers primary business value.

### Incremental Delivery

1. **Foundation** (Phases 1-2): Setup + data structures → ~25 tasks, ~3-5 days
2. **MVP: User Story 1** (Phase 3): Speculative VAD → ~28 tasks, ~5-7 days
   - **Checkpoint**: Test independently, deploy/demo
3. **Enhanced: User Story 2** (Phase 4): Auto-batching → ~29 tasks, ~4-6 days
   - **Checkpoint**: Test independently (TTS throughput +30%), deploy/demo
4. **Robust: User Story 3** (Phase 5): Cancellation refinement → ~15 tasks, ~2-3 days
   - **Checkpoint**: Test edge cases, validate robustness
5. **Optimized: User Story 4** (Phase 6): Streaming resampler → ~14 tasks, ~2-3 days
   - **Checkpoint**: Benchmark latency reduction (10-30ms)
6. **Observable: User Story 5** (Phase 7): Metrics → ~17 tasks, ~3-4 days
   - **Checkpoint**: Query metrics, validate observability
7. **Polish** (Phase 8): Documentation, validation → ~15 tasks, ~2-3 days

**Total**: ~143 tasks, ~20-30 days for full feature

### Parallel Team Strategy

With multiple developers:

1. **Single focus**: Team completes Setup + Foundational together (Phases 1-2)
2. **Once Foundational complete, split:**
   - **Developer A**: User Story 1 (Phase 3) - Core MVP
   - **Developer B**: User Story 2 (Phase 4) - Can start immediately after Phase 2
   - **Developer C**: User Story 4 (Phase 6) - Can start immediately after Phase 2
3. **After US1 complete:**
   - **Developer A**: User Story 3 (Phase 5) - Depends on US1
4. **User Story 5**: Can be integrated incrementally by any developer as stories complete
5. **Final phase**: Team converges on Polish (Phase 8)

**Parallelization Gain**: With 3 developers, completion time reduces to ~15-20 days vs. ~25-30 days sequential

---

## Task Count Summary

| Phase | Task Count | Can Start After | Parallel with |
|-------|-----------|-----------------|---------------|
| Phase 1: Setup | 4 | Immediate | - |
| Phase 2: Foundational | 21 | Phase 1 | - (blocks all stories) |
| Phase 3: User Story 1 (P1) 🎯 | 28 | Phase 2 | US2, US4 |
| Phase 4: User Story 2 (P2) | 29 | Phase 2 | US1, US4 |
| Phase 5: User Story 3 (P2) | 15 | Phase 3 (US1) | US4, US5 |
| Phase 6: User Story 4 (P3) | 14 | Phase 2 | US1, US2, US3 |
| Phase 7: User Story 5 (P3) | 17 | Phase 2 (partial) | All (integrates) |
| Phase 8: Polish | 15 | All stories | - |
| **Total** | **143 tasks** | | |

### Parallel Opportunities Identified

- **43 tasks** marked [P] for parallel execution within their phase
- **3 user stories** (US1, US2, US4) can run in parallel after Foundational complete
- Potential **50-60% time reduction** with parallel execution (15-20 days vs. 25-30 days)

---

## Independent Test Criteria by User Story

### User Story 1 (P1): Real-Time Voice Interaction
- ✅ Stream audio through VAD → ASR pipeline
- ✅ Measure end-to-end latency: P99 < 250ms @ 100 concurrent sessions
- ✅ Verify speculation: audio forwarded immediately, cancellation on false positive
- ✅ Verify control messages: propagation <10ms P95 across all contexts
- ✅ Verify speculation acceptance >95%, false positive rate <5%

### User Story 2 (P2): Efficient Batch Processing
- ✅ Send rapid bursts of text to TTS node
- ✅ Measure batch sizes formed (should be 2-5 based on policy)
- ✅ Measure queue depths (should remain <20 under load)
- ✅ Measure throughput: >30% increase with batching vs. baseline
- ✅ Measure per-request latency: P95 < 300ms despite batching

### User Story 3 (P2): Graceful Cancellation
- ✅ Inject audio with ambiguous speech boundaries
- ✅ Verify cancellation messages generated when VAD refines decision
- ✅ Verify downstream nodes honor cancellation (processing terminated, partial results discarded)
- ✅ Test edge cases: concurrent cancellation and output, ring buffer overflow, cascade cancellation
- ✅ Verify delivery confirmation and retry for control messages

### User Story 4 (P3): Streaming Resampler
- ✅ Send variable-sized audio chunks (64-2048 samples) through resampler
- ✅ Measure output availability latency (should be <5ms for 320-sample chunks)
- ✅ Verify audio continuity: no artifacts or discontinuities
- ✅ Compare to fixed-chunk resampler: P95 latency reduction >15ms

### User Story 5 (P3): Observable Metrics
- ✅ Run pipeline under load
- ✅ Query Prometheus metrics endpoint
- ✅ Verify histograms: P50/P95/P99 latencies reported for all nodes
- ✅ Verify queue depths tracked (current and max)
- ✅ Verify speculation acceptance rates, batch sizes, and other custom metrics
- ✅ Verify metrics collection overhead <1%, query latency <5ms

---

## Suggested MVP Scope

**Minimum Viable Product**: User Story 1 only (Phase 1 + Phase 2 + Phase 3)
- **Task count**: 53 tasks (Setup + Foundational + US1)
- **Estimated effort**: 8-12 days (single developer), 5-7 days (team)
- **Value delivered**: Core low-latency optimization (P99 <250ms) with speculative VAD
- **Deployment-ready**: Yes - delivers primary business value, all critical success criteria met

**Enhanced MVP**: User Story 1 + User Story 2 (add auto-batching)
- **Task count**: 82 tasks
- **Estimated effort**: 12-18 days (single developer), 8-12 days (team)
- **Value delivered**: Low latency + cost efficiency (throughput +30%)
- **Deployment-ready**: Yes - production-grade with scalability improvements

---

## Notes

- All tasks follow strict checklist format: `- [ ] [ID] [P?] [Story] Description with file path`
- [P] indicates parallelizable tasks (different files, no dependencies)
- [Story] maps task to user story (US1, US2, US3, US4, US5) for traceability
- Tests are included per Test-First requirement (TDD approach)
- Each user story is independently completable and testable
- Stop at any checkpoint to validate story independently before proceeding
- Verify tests FAIL before implementation, then implement and make them PASS
- Commit after each task or logical group
- Total estimated effort: 20-30 days (single developer), 15-20 days (parallel team)
