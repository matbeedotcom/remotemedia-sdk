# Implementation Tasks: Universal Generic Streaming Protocol

**Feature**: `004-generic-streaming` | **Branch**: `004-generic-streaming`
**Status**: Ready for Implementation | **Last Updated**: 2025-01-15

## Overview

This task list implements Feature 004: Universal Generic Streaming Protocol, which extends RemoteMedia SDK's streaming capabilities from audio-only to support any protocol bufferable data type (audio, video, tensors, JSON, text, binary). The implementation follows the MVP-first strategy: User Stories 1 & 2 (P1) first, then US3-US5 (P2-P3).

**Key Deliverables**:
- Generic `DataBuffer` with 6 data type variants (audio, video, tensor, JSON, text, binary)
- Generic `DataChunk` replacing audio-only `AudioChunk`
- Rust `RuntimeData` enum with conversion functions
- Type validation at manifest and runtime levels
- Type-safe TypeScript and Python client APIs
- Backward compatibility shim for legacy `AudioChunk` API
- 4 example pipelines demonstrating all capabilities

**Success Criteria**:
- ✅ Stream video frames through object detection pipeline (US1)
- ✅ Chain mixed-type pipelines (audio→JSON→audio) (US2)
- ✅ TypeScript/Python type checkers catch type mismatches at compile time (US3)
- ✅ Existing audio streaming examples run without code changes (US4)
- ✅ Server validates type mismatches with actionable errors (US5)
- ✅ <5% performance overhead vs audio-only protocol for audio pipelines

---

## Dependencies

### Story Completion Order

```
Phase 1 (Setup) → Phase 2 (Foundation) → Phase 3-7 (User Stories) → Phase 8 (Polish)
                                              ↓
                                    ┌─────────┴──────────┐
                                    ↓                    ↓
                            MVP (US1 + US2)    Secondary (US3, US4, US5)
                            P1 Priority         P2-P3 Priority
```

**Critical Path**:
1. **Phase 2 (Foundational)** blocks all user stories - must complete first
2. **US1 (P1)** and **US2 (P1)** form MVP - implement together
3. **US3 (P2)**, **US4 (P2)**, **US5 (P3)** can be implemented in parallel after MVP

**Parallelization Strategy**:
- Tasks marked `[P]` can run in parallel (different files, no dependencies)
- Tasks with `[US1]`, `[US2]`, etc. labels belong to specific user story phases
- Within each phase, `[P]` tasks can execute simultaneously

---

## Phase 1: Setup & Protobuf Foundation

**Goal**: Set up project structure and define protobuf contracts for generic streaming protocol.

**Outcome**: Complete protobuf definitions ready for code generation across all languages.

### Tasks

- [ ] [T001] [P] Create contracts directory at `C:\Users\mail\dev\personal\remotemedia-sdk\specs\004-generic-streaming\contracts\`
- [ ] [T002] [P] Copy existing Feature 003 protobuf files as baseline to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`, `streaming.proto`, `execution.proto`
- [ ] [T003] Add `DataBuffer` message with oneof variants to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T004] Add `AudioBuffer` message (unchanged from Feature 003) to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T005] Add `VideoFrame` message with PixelFormat enum to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T006] Add `TensorBuffer` message with TensorDtype enum to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T007] Add `JsonData` message to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T008] Add `TextBuffer` message to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T009] Add `BinaryBuffer` message to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T010] Add `DataTypeHint` enum to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T011] Update `ExecutionMetrics` with proto_to_runtime_ms, runtime_to_proto_ms, data_type_breakdown fields in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T012] Add `ERROR_TYPE_TYPE_VALIDATION` to ErrorType enum in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\common.proto`
- [ ] [T013] Add `DataChunk` message with named_buffers support to `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\streaming.proto`
- [ ] [T014] Mark `AudioChunk` as deprecated in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\streaming.proto`
- [ ] [T015] Update `StreamRequest` oneof to include data_chunk variant in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\streaming.proto`
- [ ] [T016] Update `ChunkResult` to use generic data_outputs map in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\streaming.proto`
- [ ] [T017] Update `StreamMetrics` with total_items and data_type_breakdown in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\streaming.proto`
- [ ] [T018] Update `ExecuteRequest` to use generic data_inputs map in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\execution.proto`
- [ ] [T019] Update `ExecutionResult` to use generic data_outputs map in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\execution.proto`
- [ ] [T020] Add input_types and output_types fields to NodeManifest in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\protos\execution.proto`
- [ ] [T021] [P] Run protoc to regenerate Rust code from protos in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\proto\`
- [ ] [T022] [P] Verify Rust compilation succeeds after proto changes in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\`

**Checkpoint**: All protobuf files compile successfully. Rust code generation produces expected types.

---

## Phase 2: Foundational Data Types & Conversion

**Goal**: Implement core Rust data structures and conversion functions that ALL user stories depend on.

**Outcome**: `RuntimeData` enum with proto↔runtime conversions working for all 6 data types.

**CRITICAL**: This phase blocks all user stories. Must complete before US1-US5.

### Tasks

- [ ] [T023] Create data module at `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\mod.rs`
- [ ] [T024] Define `RuntimeData` enum with variants (Audio, Video, Tensor, Json, Text, Binary) in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\runtime_data.rs`
- [ ] [T025] Implement `RuntimeData::data_type()` method returning DataTypeHint in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\runtime_data.rs`
- [ ] [T026] Implement `RuntimeData::item_count()` method for all variants in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\runtime_data.rs`
- [ ] [T027] Implement `RuntimeData::size_bytes()` method for all variants in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\runtime_data.rs`
- [ ] [T028] Create conversions module at `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T029] Implement `convert_proto_to_runtime_data()` for AudioBuffer variant in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T030] Implement `convert_proto_to_runtime_data()` for VideoFrame variant with pixel data validation in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T031] Implement `convert_proto_to_runtime_data()` for TensorBuffer variant with shape/dtype validation in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T032] Implement `convert_proto_to_runtime_data()` for JsonData variant with JSON parsing in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T033] Implement `convert_proto_to_runtime_data()` for TextBuffer variant with UTF-8 validation in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T034] Implement `convert_proto_to_runtime_data()` for BinaryBuffer variant in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T035] Implement `convert_runtime_to_proto_data()` for all RuntimeData variants in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T036] Create validation module at `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\validation.rs`
- [ ] [T037] Implement `validate_video_frame()` checking pixel_data length matches width*height*format in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\validation.rs`
- [ ] [T038] Implement `validate_tensor_size()` checking data length matches shape.product()*dtype in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\validation.rs`
- [ ] [T039] Implement `validate_text_buffer()` checking UTF-8 correctness in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\validation.rs`
- [ ] [T040] [P] Write unit tests for RuntimeData methods in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\unit\data\test_runtime_data.rs`
- [ ] [T041] [P] Write unit tests for proto→runtime conversions (all 6 types) in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\unit\data\test_conversions.rs`
- [ ] [T042] [P] Write unit tests for runtime→proto conversions (all 6 types) in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\unit\data\test_conversions.rs`
- [ ] [T043] [P] Write unit tests for validation functions in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\unit\data\test_validation.rs`

**Checkpoint**: All conversion and validation functions pass unit tests. RuntimeData enum supports all 6 data types.

---

## Phase 3: User Story 1 - Stream Non-Audio Data Types (P1, MVP)

**Goal**: Enable developers to stream video frames, tensors, and JSON through pipelines with same API as audio.

**User Story**: A machine learning developer needs to stream video frames through a real-time object detection pipeline and receive JSON metadata results without being forced to use audio-specific APIs.

**Independent Test Criteria**:
- ✅ Create manifest with video processing node
- ✅ Stream 10 video frames via DataChunk with VideoFrame buffer
- ✅ Verify JSON results contain bounding boxes and confidence scores
- ✅ System handles video frames identically to audio chunks

### Tasks

- [ ] [T044] [US1] Update streaming handler to accept DataChunk in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T045] [US1] Implement `handle_data_chunk()` using generic data conversion in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T046] [US1] Update executor to support generic data inputs via `execute_generic_pipeline()` in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\executor\mod.rs`
- [ ] [T047] [US1] Update node executor to route RuntimeData to nodes based on type in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\executor\node_executor.rs`
- [ ] [T048] [US1] Create CalculatorNode processing JSON requests in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\calculator.rs`
- [ ] [T049] [US1] Implement CalculatorNode::process() for basic operations (add, multiply, divide) in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\calculator.rs`
- [ ] [T050] [US1] Create VideoProcessorNode stub accepting VideoFrame in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\video_processor.rs`
- [ ] [T051] [US1] Implement VideoProcessorNode::process() returning dummy detection JSON in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\video_processor.rs`
- [X] [T052] [US1] Register CalculatorNode in NodeRegistry in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\mod.rs`
- [X] [T053] [US1] Register VideoProcessorNode in NodeRegistry in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\mod.rs`
- [X] [T054] [US1] [P] Write integration test streaming 10 video frames through VideoProcessorNode in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_generic_streaming.rs`
- [X] [T055] [US1] [P] Write integration test for JSON calculator pipeline in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_generic_streaming.rs`
- [X] [T056] [US1] [P] Write integration test for tensor/embedding streaming in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_generic_streaming.rs`
- [X] [T057] [US1] Verify test: Stream 10 video frames, receive JSON with bounding boxes

**Checkpoint**: Video streaming test passes. CalculatorNode processes JSON. Tensor streaming works.

---

## Phase 4: User Story 2 - Mixed-Type Pipeline Chains (P1, MVP)

**Goal**: Enable chaining nodes that process different data types (audio→JSON→audio).

**User Story**: A speech analytics developer needs to chain audio processing (VAD) with JSON processing (confidence threshold calculation) and conditional audio filtering.

**Independent Test Criteria**:
- ✅ Create pipeline: RustVADNode (audio→JSON) → CalculatorNode (JSON→JSON) → DynamicAudioFilter (audio+JSON→audio)
- ✅ Stream audio chunks
- ✅ VAD generates JSON confidence scores
- ✅ Calculator processes JSON
- ✅ Filter applies JSON-controlled gain to audio output

### Tasks

- [ ] [T058] [US2] Extend DataChunk handling to support named_buffers map in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T059] [US2] Update executor to support multi-input nodes receiving HashMap<String, RuntimeData> in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\executor\node_executor.rs`
- [ ] [T060] [US2] Create DynamicAudioFilterNode accepting audio + JSON control in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\dynamic_audio_filter.rs`
- [ ] [T061] [US2] Implement DynamicAudioFilterNode::process() applying JSON-controlled gain in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\dynamic_audio_filter.rs`
- [ ] [T062] [US2] Update existing RustVADNode to output JSON confidence scores in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\vad.rs`
- [ ] [T063] [US2] Register DynamicAudioFilterNode in NodeRegistry in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\nodes\mod.rs`
- [ ] [T064] [US2] Write integration test for audio→JSON→audio pipeline in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_mixed_pipeline.rs`
- [ ] [T065] [US2] Write integration test for multi-input node with named_buffers in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_mixed_pipeline.rs`
- [ ] [T066] [US2] Verify test: VAD outputs JSON, calculator processes it, filter receives both audio and JSON

**Checkpoint**: Mixed-type pipeline test passes. Multi-input nodes receive synchronized data.

---

## Phase 5: User Story 3 - Type-Safe Client APIs (P2)

**Goal**: Provide compile-time type safety in TypeScript and Python clients.

**User Story**: A TypeScript developer using the gRPC client needs compile-time type safety when building pipelines, ensuring audio nodes connect to audio inputs and JSON nodes connect to JSON inputs without runtime errors.

**Independent Test Criteria**:
- ✅ Write TypeScript pipeline with type-safe builder API
- ✅ Attempt to connect JSON output to audio-only input node
- ✅ Compiler rejects at build time with clear error message
- ✅ Valid connections (JSON→JSON, audio→audio) compile successfully

### Tasks

- [ ] [T067] [US3] [P] Regenerate TypeScript protobuf types from updated .proto files in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\proto\`
- [ ] [T068] [US3] Create TypeScript DataBuffer discriminated union type in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\types.ts`
- [ ] [T069] [US3] Create TypeScript DataChunk interface in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\types.ts`
- [ ] [T070] [US3] Add generic streamPipeline<T extends DataBuffer>() method to StreamingClient in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\streaming_client.ts`
- [ ] [T071] [US3] Create type guards (isAudio, isVideo, isJson, etc.) in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\types.ts`
- [ ] [T072] [US3] Create type-safe PipelineBuilder class in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\type_safe_builder.ts`
- [ ] [T073] [US3] Implement PipelineBuilder.addNode() with type constraints in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\type_safe_builder.ts`
- [ ] [T074] [US3] Implement PipelineBuilder.connect() with type compatibility checks in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\type_safe_builder.ts`
- [ ] [T075] [US3] [P] Write TypeScript tests attempting invalid type connections in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\tests\type_safety.test.ts`
- [ ] [T076] [US3] [P] Verify TypeScript compiler rejects invalid connections at build time
- [ ] [T077] [US3] [P] Regenerate Python protobuf types from updated .proto files in `C:\Users\mail\dev\personal\remotemedia-sdk\python-client\remotemedia\proto\`
- [ ] [T078] [US3] Create Python DataBuffer type hints in `C:\Users\mail\dev\personal\remotemedia-sdk\python-client\remotemedia\data_types.py`
- [ ] [T079] [US3] Create Python DataChunk class with type hints in `C:\Users\mail\dev\personal\remotemedia-sdk\python-client\remotemedia\data_types.py`
- [ ] [T080] [US3] Add type hints to stream_pipeline() method in `C:\Users\mail\dev\personal\remotemedia-sdk\python-client\remotemedia\grpc_client.py`
- [ ] [T081] [US3] [P] Write Python tests with type hints for mypy checking in `C:\Users\mail\dev\personal\remotemedia-sdk\python-client\tests\test_type_hints.py`
- [ ] [T082] [US3] [P] Verify mypy catches type mismatches in Python test code

**Checkpoint**: TypeScript compiler rejects invalid type connections. Python mypy detects type errors.

---

## Phase 6: User Story 4 - Backward Compatibility (P2)

**Goal**: Ensure existing audio code continues working without modifications.

**User Story**: A developer with existing audio streaming code using `AudioChunk` and `streamAudioPipeline()` needs their code to continue working without modifications after upgrading to the generic streaming protocol.

**Independent Test Criteria**:
- ✅ Run existing `streaming_audio_pipeline.ts` example (currently using AudioChunk)
- ✅ All tests pass without code changes
- ✅ Deprecation warnings appear in logs but don't break functionality

### Tasks

- [ ] [T083] [US4] Create backward compatibility shim module at `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\compat_shim.rs`
- [ ] [T084] [US4] Implement convert_legacy_audio_chunk() converting AudioChunk→DataChunk in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\compat_shim.rs`
- [ ] [T085] [US4] Update streaming handler to route legacy AudioChunk through compat_shim in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T086] [US4] Add deprecation warning logging when AudioChunk received in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T087] [US4] Create streamAudioPipeline() backward-compat wrapper in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\streaming_audio_compat.ts`
- [ ] [T088] [US4] Mark streamAudioPipeline() as @deprecated with migration guidance in `C:\Users\mail\dev\personal\remotemedia-sdk\nodejs-client\src\streaming_audio_compat.ts`
- [ ] [T089] [US4] Create stream_audio_pipeline() wrapper in Python client in `C:\Users\mail\dev\personal\remotemedia-sdk\python-client\remotemedia\streaming_audio_compat.py`
- [ ] [T090] [US4] Write integration test running existing audio example without changes in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_backward_compat.rs`
- [ ] [T091] [US4] Verify test: Existing streaming_audio_pipeline.ts example passes all tests
- [ ] [T092] [US4] Verify deprecation warnings appear in server logs

**Checkpoint**: Legacy audio examples run unchanged. Deprecation warnings logged. Performance <5% overhead.

---

## Phase 7: User Story 5 - Server-Side Type Validation (P3)

**Goal**: Validate incoming data chunks match expected types declared in pipeline manifest.

**User Story**: A platform operator needs the gRPC service to validate that incoming data chunks match the expected input types declared in the pipeline manifest, rejecting mismatched types with clear error messages before execution.

**Independent Test Criteria**:
- ✅ Submit manifest declaring node expects AudioBuffer input
- ✅ Stream VideoFrame chunk to that node
- ✅ Service rejects with ERROR_TYPE_TYPE_VALIDATION
- ✅ Error message specifies "Node 'vad' expects audio input but received video"

### Tasks

- [ ] [T093] [US5] Implement validate_manifest_types() checking connection type compatibility in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\validation.rs`
- [ ] [T094] [US5] Implement types_compatible() helper function in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\validation.rs`
- [ ] [T095] [US5] Implement validate_chunk_type() checking runtime chunk type vs node input_types in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\validation.rs`
- [ ] [T096] [US5] Integrate manifest validation into StreamInit handler in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T097] [US5] Integrate chunk validation into handle_data_chunk() in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T098] [US5] Create ERROR_TYPE_TYPE_VALIDATION error responses with context in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T099] [US5] Write integration test for type mismatch detection (video→audio node) in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_type_validation.rs`
- [ ] [T100] [US5] Write integration test for missing required multi-input in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_type_validation.rs`
- [ ] [T101] [US5] Write integration test for manifest connection type incompatibility in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\tests\grpc_integration\test_type_validation.rs`
- [ ] [T102] [US5] Verify test: VideoFrame sent to audio-only node returns ERROR_TYPE_TYPE_VALIDATION

**Checkpoint**: Type validation catches all invalid type combinations. Error messages specify expected vs actual types.

---

## Phase 8: Examples, Documentation & Polish

**Goal**: Provide production-ready examples and documentation for all capabilities.

**Outcome**: Developers can implement 4 new data type examples using only API documentation.

### Tasks

#### TypeScript Examples

- [ ] [T103] [P] Create video streaming example in `C:\Users\mail\dev\personal\remotemedia-sdk\examples\grpc_examples\typescript\video_streaming.ts`
- [ ] [T104] [P] Create JSON calculator pipeline example in `C:\Users\mail\dev\personal\remotemedia-sdk\examples\grpc_examples\typescript\json_calculator.ts`
- [ ] [T105] [P] Create mixed-type pipeline example (audio→JSON→audio) in `C:\Users\mail\dev\personal\remotemedia-sdk\examples\grpc_examples\typescript\mixed_pipeline.ts`
- [ ] [T106] [P] Create tensor/embedding streaming example in `C:\Users\mail\dev\personal\remotemedia-sdk\examples\grpc_examples\typescript\tensor_streaming.ts`

#### Python Examples

- [ ] [T107] [P] Create video streaming example in `C:\Users\mail\dev\personal\remotemedia-sdk\examples\grpc_examples\python\video_streaming.py`
- [ ] [T108] [P] Create JSON calculator pipeline example in `C:\Users\mail\dev\personal\remotemedia-sdk\examples\grpc_examples\python\json_calculator.py`
- [ ] [T109] [P] Create mixed-type pipeline example in `C:\Users\mail\dev\personal\remotemedia-sdk\examples\grpc_examples\python\mixed_pipeline.py`
- [ ] [T110] [P] Create tensor/embedding streaming example in `C:\Users\mail\dev\personal\remotemedia-sdk\examples\grpc_examples\python\tensor_streaming.py`

#### Documentation

- [ ] [T111] [P] Create migration guide showing before/after code examples in `C:\Users\mail\dev\personal\remotemedia-sdk\docs\migration-guide.md`
- [ ] [T112] [P] Document type-safe builder API usage in `C:\Users\mail\dev\personal\remotemedia-sdk\docs\type-safe-apis.md`
- [ ] [T113] [P] Document multi-input node patterns in `C:\Users\mail\dev\personal\remotemedia-sdk\docs\multi-input-nodes.md`
- [ ] [T114] [P] Create troubleshooting guide for type validation errors in `C:\Users\mail\dev\personal\remotemedia-sdk\docs\troubleshooting-type-errors.md`

#### Performance & Metrics

- [ ] [T115] Update StreamMetrics tracking to use generic total_items in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\grpc_service\streaming.rs`
- [ ] [T116] Add data_type_breakdown tracking to ExecutionMetrics in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\executor\mod.rs`
- [ ] [T117] Add proto_to_runtime_ms and runtime_to_proto_ms timing in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\src\data\conversions.rs`
- [ ] [T118] [P] Write benchmark comparing audio-only vs generic protocol overhead in `C:\Users\mail\dev\personal\remotemedia-sdk\runtime\benches\generic_vs_audio_benchmark.rs`
- [ ] [T119] [P] Verify benchmark: <5% overhead for audio pipelines (SC-008, FR-024)
- [ ] [T120] [P] Verify benchmark: <1ms JSON processing latency (SC-002)

#### Integration & Validation

- [ ] [T121] Run all integration tests and verify 100% pass
- [ ] [T122] Run TypeScript type checker on all examples and verify no errors
- [ ] [T123] Run Python mypy on all examples and verify no errors
- [ ] [T124] Test backward compatibility with existing Feature 003 audio clients
- [ ] [T125] Verify all user story acceptance criteria met (US1-US5)
- [ ] [T126] Verify all success criteria met (SC-001 through SC-010)
- [ ] [T127] Update CHANGELOG.md with Feature 004 release notes in `C:\Users\mail\dev\personal\remotemedia-sdk\CHANGELOG.md`

**Checkpoint**: All examples run successfully. Documentation complete. Performance targets met.

---

## Implementation Strategy

### MVP-First Approach

**Phase 1-2**: Foundation (Weeks 1-2)
- Complete protobuf definitions and data conversion layer
- This blocks all user stories - highest priority

**Phase 3-4**: MVP (Weeks 3-4)
- User Story 1 (P1): Stream non-audio data types
- User Story 2 (P1): Mixed-type pipeline chains
- These are the core value propositions

**Phase 5-7**: Secondary Features (Weeks 5-6)
- User Story 3 (P2): Type-safe client APIs
- User Story 4 (P2): Backward compatibility
- User Story 5 (P3): Server-side type validation
- Can be implemented in parallel

**Phase 8**: Polish (Week 7)
- Examples, documentation, performance validation

### Parallel Execution Examples

Tasks that can run in parallel within phases:

**Phase 1 (Setup)**:
```
T001, T002 → T003-T020 (all protobuf message additions)
T021, T022 (code generation) can run after T003-T020 complete
```

**Phase 2 (Foundation)**:
```
T040, T041, T042, T043 (unit tests) can run in parallel after T023-T039 complete
```

**Phase 3 (US1)**:
```
T048-T049 (CalculatorNode) ∥ T050-T051 (VideoProcessorNode)
T054, T055, T056 (integration tests) can run in parallel after nodes registered
```

**Phase 5 (US3)**:
```
T067-T076 (TypeScript client) ∥ T077-T082 (Python client)
```

**Phase 8 (Examples)**:
```
T103-T106 (TS examples) ∥ T107-T110 (Python examples) ∥ T111-T114 (Docs)
T118-T120 (benchmarks) can run in parallel with examples
```

---

## Testing Checklist

### Functional Tests

- [ ] Stream 10 video frames through VideoProcessorNode, verify JSON results (US1)
- [ ] Stream JSON calculator requests, verify <1ms latency (US1)
- [ ] Stream tensor embeddings, verify similarity scores returned (US1)
- [ ] Chain audio→JSON→audio pipeline, verify all type conversions (US2)
- [ ] Multi-input node receives synchronized audio + JSON control (US2)
- [ ] TypeScript compiler rejects invalid type connections (US3)
- [ ] Python mypy catches type mismatches (US3)
- [ ] Legacy AudioChunk API works without code changes (US4)
- [ ] Server rejects VideoFrame sent to audio-only node (US5)
- [ ] Error message shows expected vs actual type (US5)

### Performance Tests

- [ ] Audio-only pipeline shows <5% overhead vs Feature 003 baseline
- [ ] JSON calculator pipeline <1ms per operation
- [ ] Mixed-type pipeline <5% latency overhead vs audio-only
- [ ] Zero-copy audio maintains performance (benchmark validation)

### Backward Compatibility Tests

- [ ] Run all existing Feature 003 audio examples unchanged
- [ ] Verify deprecation warnings appear in logs
- [ ] Confirm 100% test pass rate for legacy code

---

## Success Criteria Validation

### SC-001: Video streaming ergonomics
- [ ] Video example code ±10% line count of audio example

### SC-002: JSON-only pipeline latency
- [ ] CalculatorNode <1ms average latency per chunk

### SC-003: Mixed-type pipeline overhead
- [ ] Audio→JSON→audio <5% latency overhead vs audio-only

### SC-004: Backward compatibility
- [ ] All 3 existing TypeScript audio examples run unchanged

### SC-005: Type safety
- [ ] Type checker catches 100% of invalid connections in test suite

### SC-006: Migration effort
- [ ] Migration requires <20 lines of code changes (verified in migration guide)

### SC-007: Type validation errors
- [ ] 10 invalid type combinations return actionable errors

### SC-008: Zero-copy performance
- [ ] <5% overhead in chunk processing latency vs pre-migration

### SC-009: Documentation quality
- [ ] 4 new examples implementable using only API docs (no source inspection)

### SC-010: Legacy client support
- [ ] 100% of legacy AudioChunk messages processed without errors

---

## Risks & Mitigations

| Risk | Impact | Mitigation | Task |
|------|--------|------------|------|
| Protobuf oneof generates incompatible code across languages | High | Test proto codegen in all 3 languages early (Phase 1) | T021, T067, T077 |
| JSON parsing overhead exceeds 1ms target | Medium | Benchmark early, optimize or use simd-json if needed | T118-T120 |
| Type validation complexity delays P3 story | Low | US5 is P3, doesn't block MVP (US1+US2) | Phase 7 |
| Backward compat shim introduces bugs in audio path | Medium | Extensive testing with legacy clients | T090-T092 |
| Multi-input node synchronization complexity | Medium | Start with simple test case (audio + JSON), iterate | T064-T066 |

---

## Definition of Done

A task is complete when:
- [ ] Code implements requirements from spec.md
- [ ] Unit tests pass (where applicable)
- [ ] Integration tests pass (where applicable)
- [ ] Type checking passes (TypeScript tsc, Python mypy)
- [ ] Code reviewed (self-review against data-model.md)
- [ ] No compiler warnings
- [ ] Performance targets met (if applicable)

A phase is complete when:
- [ ] All phase tasks marked complete
- [ ] Checkpoint criteria verified
- [ ] No blocking issues for next phase

Feature 004 is complete when:
- [ ] All 127 tasks completed
- [ ] All 5 user stories pass independent tests
- [ ] All 10 success criteria validated
- [ ] Performance benchmarks meet targets
- [ ] Documentation published

---

## Notes

**Task Numbering**: T001-T127 in execution order. Tasks within a phase may be executed in parallel if marked `[P]`.

**File Paths**: All file paths are absolute Windows paths starting from `C:\Users\mail\dev\personal\remotemedia-sdk\`.

**Story Labels**: `[US1]`, `[US2]`, `[US3]`, `[US4]`, `[US5]` indicate which user story a task belongs to.

**Parallelization**: Tasks marked `[P]` have no dependencies within their phase and can execute in parallel.

**Critical Path**: Phase 2 (Foundational) → Phase 3 (US1) → Phase 4 (US2) forms the MVP critical path.

**Performance Targets**:
- <5% overhead vs audio-only (SC-008, FR-024)
- <1ms JSON processing (SC-002)
- <50ms average chunk latency (maintained from Feature 003)

**Type Safety Goals**:
- 100% compile-time detection in TypeScript/Python (SC-005)
- Clear error messages with expected vs actual types (SC-007)

**Backward Compatibility**:
- 6-month deprecation timeline for AudioChunk
- Zero breaking changes for existing audio clients (SC-004)
