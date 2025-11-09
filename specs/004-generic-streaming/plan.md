# Implementation Plan: Universal Generic Streaming Protocol

**Branch**: `004-generic-streaming` | **Date**: 2025-01-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/004-generic-streaming/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Extend the streaming protocol from audio-only to support universal data types (video, tensors, JSON, text, binary) while maintaining 100% backward compatibility with existing audio streaming clients. Replace hardcoded `AudioChunk`/`AudioBuffer` messages with generic `DataChunk`/`DataBuffer` using protobuf `oneof` discriminators. Enable type-safe client APIs (TypeScript, Python) and server-side type validation. Target: <5% performance overhead vs audio-only protocol, maintain <50ms streaming latency.

## Technical Context

**Language/Version**:
- Rust 1.75+ (server-side protocol handling, data conversion)
- TypeScript 5.0+ (Node.js client type-safe APIs)
- Python 3.11+ (Python client with type hints)
- Protobuf 3.20+ (protocol definitions with `oneof` support)

**Primary Dependencies**:
- **Rust**: tonic 0.10+, prost 0.12+, serde_json 1.0+ (JSON parsing), remotemedia_runtime v0.2.0
- **TypeScript**: @grpc/grpc-js ^1.9.0, ts-proto or grpc-tools (type-safe proto codegen)
- **Python**: grpcio, grpcio-tools, mypy (type checking)
- **Protobuf**: protoc 3.20+ (oneof optional fields, map types)

**Storage**: In-memory streaming state only (stateless protocol, session management reuses existing Feature 003 infrastructure)

**Testing**:
- Rust: cargo test (unit tests for data conversion), integration tests for streaming handlers
- TypeScript: jest (type safety tests, client API tests)
- Python: pytest (type hint validation, client tests)
- Contract tests: protobuf schema validation across languages
- Performance tests: benchmark audio-only vs generic protocol overhead

**Target Platform**: Cross-platform (Linux, macOS, Windows servers; Node.js 14+, Python 3.11+)

**Project Type**: Protocol extension (modifies existing gRPC service + client libraries)

**Performance Goals**:
- <5% latency overhead vs existing audio-only streaming (SC-003: <5% vs audio-only pipelines)
- <1ms JSON processing latency for simple operations (SC-002)
- <50ms average chunk processing latency maintained (User Story 2 from Feature 003)
- Zero-copy audio performance maintained (SC-008: <5% overhead, FR-024)
- Same throughput as audio-only: 1000+ concurrent sessions

**Constraints**:
- 100% backward compatibility with existing `AudioChunk` API (SC-004, FR-017)
- Protobuf message size limit: 4MB per chunk (Assumption 1)
- Type-safe APIs must catch type mismatches at compile time (SC-005: 100% detection)
- Migration path requires <20 lines of code changes (SC-006)
- No breaking changes to Feature 003 streaming infrastructure

**Scale/Scope**:
- Protocol extension: ~8 new protobuf message types (DataBuffer variants)
- Rust changes: ~1500-2000 LoC (data conversion, type validation, streaming handler updates)
- TypeScript client: ~800-1000 LoC (type-safe builder, generic streaming APIs)
- Python client: ~600-800 LoC (type hints, generic streaming wrapper)
- 4 new example pipelines (video, tensor, JSON calculator, mixed-type)
- Migration guide documentation (~500 lines markdown)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Constitution Status**: Template constitution detected (no project-specific gates defined). Proceeding with best practices gates:

✅ **Library-First Principle**: Protocol extension builds on existing `remotemedia_runtime` library (v0.2.0) and Feature 003 gRPC infrastructure. New data conversion logic will be library functions, independently testable.

✅ **Backward Compatibility**: Existing `AudioChunk` API preserved via compatibility shim (FR-017). Legacy clients continue working without code changes (SC-004, User Story 4).

✅ **Test-First Development**: Will generate contract tests from protobuf schemas, integration tests for each user story (US1-US5), type safety tests for client APIs, performance regression tests.

✅ **Observability**: Reuses existing structured logging and metrics from Feature 003. New metrics: data type distribution, type validation errors, conversion overhead.

✅ **Versioning**: Protocol remains "v1" (backward compatible extension). Deprecation markers on legacy types. Client library versions follow semver.

⚠️ **Complexity Justification Required**: Adding 8 new data buffer types and polymorphic streaming - see Complexity Tracking section.

---

**Post-Design Re-evaluation** (after Phase 1):

✅ **Library-First Confirmed**: New `data/` module in `runtime/src/` provides standalone data conversion functions (`convert_proto_to_runtime_data`, `convert_runtime_to_proto_data`) that are independently testable. No coupling to gRPC service layer.

✅ **Backward Compatibility Validated**: Protobuf contracts include deprecated `AudioChunk` with automatic conversion shim in `compat_shim.rs`. All existing Feature 003 streaming tests will pass without changes. Migration requires 6 lines of code (quickstart.md demonstrates <20 lines requirement met).

✅ **Test Coverage Planned**: Contract tests generated from .proto files, 5 integration tests mapping to user stories (test_generic_streaming.rs, test_mixed_pipeline.rs, test_backward_compat.rs, test_type_validation.rs), type safety tests in TypeScript/Python, performance regression tests for <5% overhead validation.

✅ **Observability Extended**: ExecutionMetrics enhanced with `proto_to_runtime_ms`, `runtime_to_proto_ms` conversion tracking, `data_type_breakdown` map, generic `items_processed` counter. All existing metrics preserved. Error types extended with `ERROR_TYPE_TYPE_VALIDATION` for type mismatches.

✅ **Versioning Maintained**: Protocol remains "v1" (backward-compatible extension per research.md decision). No breaking changes to Feature 003 contracts. Deprecation markers on legacy types with 6-month timeline. Client libraries follow semver with clear migration path.

✅ **Complexity Justified**: All 8 data buffer types serve distinct use cases with type-specific metadata (validated in data-model.md). Polymorphic nodes enable User Story 2 (mixed-type pipelines) which is core differentiator. Backward compatibility shim is smallest viable implementation for zero-downtime migration.

**No Gate Failures**: All constitution principles satisfied. Ready for Phase 2 (task generation).

## Project Structure

### Documentation (this feature)

```text
specs/004-generic-streaming/
├── plan.md              # This file
├── research.md          # Phase 0: Protocol design patterns, type system design
├── data-model.md        # Phase 1: DataBuffer types, RuntimeData enum
├── quickstart.md        # Phase 1: Migration guide, generic streaming examples
├── contracts/           # Phase 1: Updated .proto files
│   ├── common.proto     # DataBuffer, DataTypeHint, RuntimeData types
│   ├── streaming.proto  # DataChunk (replaces AudioChunk), updated StreamingPipelineService
│   └── execution.proto  # Updated ExecuteRequest with generic data_inputs/data_outputs
└── tasks.md             # Phase 2: Implementation tasks (NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
runtime/
├── protos/                              # Protocol buffer definitions (UPDATED)
│   ├── common.proto                     # UPDATED: Add DataBuffer, VideoFrame, TensorBuffer, JsonData, etc.
│   ├── streaming.proto                  # UPDATED: Replace AudioChunk with DataChunk, add named_buffers
│   └── execution.proto                  # UPDATED: Replace audio_inputs with data_inputs map
├── src/
│   ├── data/                            # NEW: Generic data type module
│   │   ├── mod.rs                       # Public API
│   │   ├── runtime_data.rs              # RuntimeData enum (Audio, Video, Tensor, Json, Text, Binary)
│   │   ├── conversions.rs               # convert_proto_to_runtime_data(), convert_runtime_to_proto_data()
│   │   └── validation.rs                # Type validation logic (check DataTypeHint compatibility)
│   ├── grpc_service/                    # EXISTING: gRPC service (UPDATED)
│   │   ├── streaming.rs                 # UPDATED: Replace handle_audio_chunk() with handle_data_chunk()
│   │   ├── execution.rs                 # UPDATED: Support generic data_inputs/data_outputs maps
│   │   └── compat_shim.rs               # NEW: Legacy AudioChunk → DataChunk conversion
│   ├── executor/                        # EXISTING: Pipeline executor (UPDATED)
│   │   ├── mod.rs                       # UPDATED: execute_generic_pipeline() method
│   │   └── node_executor.rs             # UPDATED: Route RuntimeData to nodes based on type
│   └── nodes/                           # EXISTING: Audio/video/JSON nodes
│       ├── calculator.rs                # NEW: JSON calculator node (example)
│       └── video_processor.rs           # NEW: Video frame processor node (example)
└── tests/
    ├── grpc_integration/                # EXISTING integration tests (UPDATED + NEW)
    │   ├── test_generic_streaming.rs    # NEW: Test video, tensor, JSON streaming
    │   ├── test_mixed_pipeline.rs       # NEW: Test audio→JSON→audio pipelines
    │   ├── test_backward_compat.rs      # NEW: Test legacy AudioChunk still works
    │   └── test_type_validation.rs      # NEW: Test type mismatch detection
    └── unit/
        └── data/
            ├── test_conversions.rs      # NEW: Test proto ↔ RuntimeData conversions
            └── test_validation.rs       # NEW: Test type validation logic

nodejs-client/                           # EXISTING: TypeScript client (UPDATED)
├── protos/                              # UPDATED: Sync with runtime/protos/
│   ├── common.proto
│   ├── streaming.proto
│   └── execution.proto
├── src/
│   ├── types.ts                         # UPDATED: Add DataChunk, DataBuffer discriminated unions
│   ├── streaming_client.ts              # UPDATED: streamPipeline<T extends DataBuffer>() generic method
│   ├── streaming_audio_compat.ts        # NEW: streamAudioPipeline() backward-compat wrapper
│   └── type_safe_builder.ts             # NEW: Type-safe pipeline builder (prevents type mismatches)
├── examples/
│   ├── video_streaming.ts               # NEW: Video processing example
│   ├── json_calculator.ts               # NEW: JSON-only pipeline example
│   ├── mixed_pipeline.ts                # NEW: Audio→JSON→Audio example
│   └── streaming_audio_pipeline.ts      # EXISTING: Should work without changes (US4 test)
└── tests/
    └── type_safety.test.ts              # NEW: Compile-time type mismatch tests

python-client/                           # EXISTING: Python client (UPDATED)
├── remotemedia/
│   ├── grpc_client.py                   # UPDATED: stream_pipeline() with type hints
│   ├── data_types.py                    # NEW: Type-safe DataChunk, DataBuffer classes with hints
│   └── streaming_audio_compat.py        # NEW: stream_audio_pipeline() wrapper
├── examples/
│   ├── video_streaming.py               # NEW: Video example
│   ├── json_calculator.py               # NEW: JSON calculator example
│   └── mixed_pipeline.py                # NEW: Mixed-type pipeline
└── tests/
    └── test_type_hints.py               # NEW: Mypy type checking tests

specs/003-rust-grpc-service/contracts/   # EXISTING contracts (REFERENCE ONLY, not modified)
└── (Feature 003 protobuf definitions - used as baseline for generic extension)
```

**Structure Decision**: Extend existing `runtime/` crate with new `data/` module for generic data handling. Update Feature 003 gRPC service handlers (`streaming.rs`, `execution.rs`) to support generic `DataChunk`/`DataBuffer`. Add backward compatibility shim for legacy `AudioChunk` messages. Update TypeScript and Python clients with generic streaming APIs while preserving existing audio-specific helpers.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| 8 data buffer types (Audio, Video, Tensor, Json, Text, Binary + deprecated AudioChunk + DataChunk wrapper) | Enables universal streaming for all data types (FR-001). Each type has domain-specific metadata (video: width/height/format, tensor: shape/dtype, JSON: schema hint). Without variety, system remains audio-locked. | Single "BinaryBuffer" type: Loses type safety (SC-005: 100% compile-time detection impossible). Forces all metadata into unstructured JSON strings, making validation impossible (US5). Doesn't support backward compatibility (AudioBuffer must remain). |
| Polymorphic node inputs (nodes accept multiple data types via `inputTypes: ['audio', 'json', 'ANY']`) | Required for mixed-type pipelines (US2: audio + JSON control → filtered audio). Real-world nodes like DynamicAudioFilter need both data stream and control parameters. Critical differentiator from single-type systems. | Separate nodes for each type combination: Combinatorial explosion (AudioFilter, AudioJsonFilter, AudioTensorFilter, etc.). User Story 2 impossible without polymorphism. Breaks composability. |
| Backward compatibility shim (compat_shim.rs converts legacy AudioChunk → DataChunk) | Zero breaking changes requirement (SC-004, US4). Existing production clients using AudioChunk must work without upgrades. Enables gradual migration (Assumption 8: 6-month deprecation timeline). | Force immediate migration: Violates SC-004 (all existing examples must run unchanged). Breaks production systems. Alternative: Maintain two parallel protocols: Doubles maintenance burden, complicates testing, delays feature delivery. |
