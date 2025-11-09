# Feature Specification: Universal Generic Streaming Protocol

**Feature Branch**: `004-generic-streaming`
**Created**: 2025-01-15
**Status**: Draft
**Input**: User description: "Universal streaming protocol supporting audio, video, tensors, JSON and any protocol bufferable data type with backward compatibility"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Stream Non-Audio Data Types (Priority: P1) 🎯 MVP

A machine learning developer needs to stream video frames through a real-time object detection pipeline and receive JSON metadata results without being forced to use audio-specific APIs.

**Why this priority**: This is the core value proposition - enabling developers to use the streaming infrastructure for ANY data type, not just audio. Without this, the system remains audio-only and blocks entire classes of use cases (computer vision, NLP, embeddings).

**Independent Test**: Create a manifest with a video processing node, stream 10 video frames with detection parameters, verify JSON results are returned with bounding boxes and confidence scores. System should handle video frames identically to how it currently handles audio chunks.

**Acceptance Scenarios**:

1. **Given** a pipeline manifest with a video detection node, **When** streaming video frames via `DataChunk` with `VideoFrame` buffer, **Then** system processes frames and returns JSON results with detection metadata
2. **Given** a pipeline with an embedding processor node, **When** streaming tensor data via `DataChunk` with `TensorBuffer`, **Then** system processes embeddings and returns similarity scores as JSON
3. **Given** a calculator pipeline with JSON input node, **When** streaming calculation requests via `DataChunk` with `JsonData` buffer, **Then** system returns computed results as JSON
4. **Given** a text tokenization pipeline, **When** streaming text via `DataChunk` with `TextBuffer`, **Then** system returns token IDs and metadata

---

### User Story 2 - Mixed-Type Pipeline Chains (Priority: P1) 🎯 MVP

A speech analytics developer needs to chain audio processing (VAD) with JSON processing (confidence threshold calculation) and conditional audio filtering, where data flows seamlessly between different data type domains.

**Why this priority**: Real-world pipelines rarely use a single data type - they need to convert between types (audio → JSON metadata, JSON control → audio filtering). This capability is essential for complex workflows and is the key differentiator from single-type systems.

**Independent Test**: Create a manifest with three nodes: RustVADNode (audio → JSON), CalculatorNode (JSON → JSON), DynamicAudioFilter (audio + JSON control → audio). Stream audio chunks, verify VAD generates JSON confidence scores, calculator processes them, and filter applies JSON-controlled gain to audio output.

**Acceptance Scenarios**:

1. **Given** a pipeline with audio→JSON→audio flow, **When** streaming audio chunks through VAD then calculator then filter, **Then** each node receives correct data type and produces expected output type
2. **Given** a node requiring multiple input types (audio + JSON control), **When** streaming both audio chunks and JSON control data, **Then** node receives both inputs synchronized by sequence number and processes correctly
3. **Given** a JSON metadata aggregator node downstream from audio VAD, **When** VAD outputs JSON speech segments, **Then** aggregator receives JSON and produces summary statistics as JSON
4. **Given** a conditional processing node with JSON threshold control, **When** JSON threshold exceeds 0.8, **Then** downstream audio processing is enabled, otherwise audio is passed through unchanged

---

### User Story 3 - Type-Safe Client APIs (Priority: P2)

A TypeScript developer using the gRPC client needs compile-time type safety when building pipelines, ensuring audio nodes connect to audio inputs and JSON nodes connect to JSON inputs without runtime errors.

**Why this priority**: Developer experience and early error detection are critical for adoption. Type safety prevents entire classes of bugs (connecting incompatible node types) and provides autocomplete/intellisense for better productivity.

**Independent Test**: Write a TypeScript pipeline using the type-safe builder API, attempt to connect a JSON output to an audio-only input node. Compiler should reject this at build time with a clear error message. Valid connections (JSON→JSON, audio→audio) should compile successfully.

**Acceptance Scenarios**:

1. **Given** TypeScript pipeline builder with typed node interfaces, **When** connecting nodes with compatible types (audio→audio, JSON→JSON), **Then** code compiles without errors
2. **Given** TypeScript pipeline builder, **When** attempting to connect incompatible types (video→audio node that expects only audio), **Then** compiler shows type error with helpful message
3. **Given** Python client with type hints, **When** creating `DataChunk` with wrong buffer type for target node, **Then** type checker (mypy) flags error before execution
4. **Given** a multi-input node accepting [audio, JSON], **When** developer provides only audio input, **Then** type system requires explicit handling of missing JSON or provides default

---

### User Story 4 - Backward Compatibility for Existing Audio Code (Priority: P2)

A developer with existing audio streaming code using `AudioChunk` and `streamAudioPipeline()` needs their code to continue working without modifications after upgrading to the generic streaming protocol.

**Why this priority**: Breaking existing production code blocks adoption. Backward compatibility ensures smooth migration path and allows developers to upgrade at their own pace while new projects can use the generic API from day one.

**Independent Test**: Run existing TypeScript example `streaming_audio_pipeline.ts` (currently using `AudioChunk`) against the new gRPC service supporting generic protocol. All tests should pass without code changes. Deprecation warnings should appear in logs but not break functionality.

**Acceptance Scenarios**:

1. **Given** existing code using `AudioChunk` message type, **When** connecting to new gRPC service supporting generic protocol, **Then** service automatically converts `AudioChunk` to `DataChunk` internally and processes correctly
2. **Given** existing TypeScript client using `streamAudioPipeline()` helper method, **When** calling method with audio generator, **Then** method wraps audio in generic `DataChunk` transparently and returns results
3. **Given** existing Rust streaming handler using `handle_audio_chunk()`, **When** receiving legacy `AudioChunk` via compatibility shim, **Then** handler converts to generic path and executes pipeline
4. **Given** protobuf definitions with deprecated `AudioChunk`, **When** compiling client code, **Then** deprecation warnings appear but code compiles and runs successfully

---

### User Story 5 - Server-Side Type Validation (Priority: P3)

A platform operator needs the gRPC service to validate that incoming data chunks match the expected input types declared in the pipeline manifest, rejecting mismatched types with clear error messages before execution.

**Why this priority**: Runtime validation prevents resource waste (processing invalid data) and provides fast feedback to developers. However, P3 priority because type-safe clients (US3) prevent most issues at compile time, making runtime validation a defensive fallback.

**Independent Test**: Submit a manifest declaring a node expects `AudioBuffer` input, then stream a `VideoFrame` chunk to that node. Service should reject the request with `ERROR_TYPE_VALIDATION` and message specifying "Node 'vad' expects audio input but received video".

**Acceptance Scenarios**:

1. **Given** a manifest with node declaring `inputTypes: ['audio']`, **When** streaming `DataChunk` with `VideoFrame` buffer to that node, **Then** service returns validation error before processing
2. **Given** a multi-input node expecting `['audio', 'json']`, **When** streaming only audio chunks without required JSON control, **Then** service returns error specifying missing required input type
3. **Given** a manifest with invalid node type (typo in `nodeType` field), **When** initializing stream, **Then** service returns error listing available node types
4. **Given** a pipeline with type-incompatible connection (JSON output→audio-only input), **When** validating manifest during `StreamInit`, **Then** service rejects with error showing the incompatible connection

---

### Edge Cases

- **What happens when a chunk contains multiple data types in `named_buffers`?** System routes each buffer to the corresponding node input based on name mapping (e.g., `"audio"` key → audio input port, `"control"` key → JSON control port).

- **How does the system handle a node that can accept multiple input types (polymorphic nodes)?** Node declares `inputTypes: ['audio', 'json', 'ANY']` where `ANY` means accept any type. System validates that at least one provided input type matches the accepted types.

- **What happens when streaming very large tensors that exceed the protobuf message size limit (typically 4MB)?** System returns `ERROR_TYPE_RESOURCE_LIMIT` with message "DataChunk size exceeds maximum (4MB). Consider chunking large tensors or using streaming RPC with smaller segments."

- **How does backward compatibility handle mixed clients (old audio-only clients + new generic clients) connecting to the same service?** Service detects message type (legacy `AudioChunk` vs new `DataChunk`) via protobuf `oneof` discriminator and routes to appropriate handler. Both paths converge to the same generic executor after initial conversion.

- **What happens when a JSON node receives malformed JSON that fails parsing?** System returns `ERROR_TYPE_VALIDATION` with specific JSON parse error (line number, character position) in the `context` field to help developers debug.

- **How does the system handle video frames with different resolutions in the same stream?** Nodes are responsible for handling resolution changes. If a node requires fixed resolution, it should return `ERROR_TYPE_NODE_EXECUTION` with message "Resolution change detected: 1920x1080 → 1280x720. Node 'detector' requires fixed resolution."

- **What happens when a developer declares `outputTypes: ['audio']` but node implementation actually returns JSON?** Runtime type mismatch is logged as warning, and if downstream node expects audio but receives JSON, it fails with clear error showing expected vs actual type. This incentivizes accurate type declarations.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support generic `DataBuffer` protobuf message with `oneof` discriminator for data types: `AudioBuffer`, `VideoFrame`, `TensorBuffer`, `JsonData`, `TextBuffer`, `BinaryBuffer`

- **FR-002**: System MUST replace streaming `AudioChunk` message with generic `DataChunk` message that carries `DataBuffer` instead of hardcoded `AudioBuffer`

- **FR-003**: System MUST replace unary `ExecuteRequest` audio-specific fields (`audio_inputs`, `audio_outputs`) with generic `data_inputs`/`data_outputs` using `map<string, DataBuffer>`

- **FR-004**: System MUST provide Rust `RuntimeData` enum with variants for each supported data type (Audio, Video, Tensor, Json, Text, Binary) with conversion functions to/from protobuf

- **FR-005**: System MUST provide `convert_proto_to_runtime_data()` function that deserializes protobuf `DataBuffer` to `RuntimeData` based on `oneof` discriminator

- **FR-006**: System MUST provide `convert_runtime_to_proto_data()` function that serializes `RuntimeData` back to protobuf `DataBuffer` with correct `oneof` variant

- **FR-007**: Streaming handler MUST replace `handle_audio_chunk()` with `handle_data_chunk()` that processes any `DataBuffer` type via generic data conversion

- **FR-008**: Executor MUST provide `execute_generic_pipeline()` method that accepts `HashMap<String, RuntimeData>` inputs and routes to appropriate node processors based on data type

- **FR-009**: Node manifests MUST support optional `inputTypes` and `outputTypes` fields (repeated `DataTypeHint` enum) to declare expected data types for validation

- **FR-010**: System MUST validate incoming `DataChunk` buffer type matches the target node's declared `inputTypes` during `StreamInit` manifest validation or chunk processing

- **FR-011**: TypeScript client MUST provide generic `DataChunk` interface with discriminated union `DataBuffer` type supporting all data variants

- **FR-012**: TypeScript client MUST provide type-safe `streamPipeline()` method accepting `AsyncGenerator<DataChunk>` that works with any data type

- **FR-013**: TypeScript client MUST provide backward-compatible `streamAudioPipeline()` helper method that wraps audio data in generic `DataChunk` automatically

- **FR-014**: Python client MUST provide equivalent generic streaming API with type hints for static type checking

- **FR-015**: System MUST support multi-input nodes that accept multiple data types simultaneously via `named_buffers` map in `DataChunk` (e.g., audio + JSON control)

- **FR-016**: Protobuf definitions MUST mark legacy `AudioChunk` message as `deprecated` with migration guidance in comments

- **FR-017**: System MUST provide automatic conversion shim that accepts legacy `AudioChunk` messages and converts them to generic `DataChunk` internally for backward compatibility

- **FR-018**: Error responses MUST include type mismatch information showing expected vs actual data type when validation fails (e.g., "Expected audio, received video")

- **FR-019**: System MUST handle JSON data by parsing `JsonData.json_payload` string into `serde_json::Value` for Rust nodes or equivalent structures in other languages

- **FR-020**: System MUST provide example pipelines demonstrating: (1) video streaming, (2) tensor/embedding streaming, (3) JSON-only pipeline (CalculatorNode), (4) mixed-type pipeline (audio→JSON→audio)

- **FR-021**: Migration guide MUST document the upgrade path from audio-specific to generic APIs with code examples showing before/after for common patterns

- **FR-022**: ChunkResult MUST use generic `data_outputs` map instead of separate `audio_outputs` + `data_outputs` maps

- **FR-023**: StreamMetrics MUST use generic `total_items_processed` counter instead of audio-specific `total_samples` (where items = samples, frames, tokens, or objects depending on data type)

- **FR-024**: System MUST maintain zero-copy performance for audio data after migration to generic protocol (validate with benchmarks showing <5% overhead)

### Key Entities

- **DataBuffer**: Universal container holding any protocol bufferable data type. Contains `oneof data_type` discriminator with variants for audio, video, tensors, JSON, text, and binary blobs. Includes optional metadata field for extensibility.

- **DataChunk**: Streaming message that replaces AudioChunk. Contains target `node_id`, generic `DataBuffer` payload, sequence number for ordering, and timestamp. Supports multi-input nodes via optional `named_buffers` map for providing multiple typed inputs simultaneously.

- **RuntimeData**: Rust enum representing deserialized data in memory. Provides unified interface across all data types with methods for item counting (samples, frames, tokens) and type hints for routing decisions.

- **DataTypeHint**: Enum declaring expected input/output types for nodes in manifests. Values: AUDIO, VIDEO, TENSOR, JSON, TEXT, BINARY, ANY (polymorphic). Used for both client-side type checking and server-side validation.

- **JsonData**: Protobuf message containing JSON payload as string with optional schema type hint. Used for calculator nodes, metadata processors, control flow nodes, and any structured data that doesn't fit binary formats.

- **VideoFrame**: Protobuf message containing video frame data with width, height, pixel format (RGB24, RGBA32, YUV420P), and frame number for synchronization.

- **TensorBuffer**: Protobuf message containing tensor/embedding data with shape array (e.g., [1, 512] for embeddings), dtype (F32, F16, I32, I8), and optional layout string (NCHW, NHWC) for multidimensional data.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Developers can stream video frames through object detection pipeline with same API ergonomics as current audio streaming (measured by example code line count ±10% of audio example)

- **SC-002**: System supports JSON-only pipelines (e.g., CalculatorNode) with <1ms average latency per chunk for simple operations (maintain current performance)

- **SC-003**: Mixed-type pipelines (audio→JSON→audio) process chunks with <5% latency overhead compared to audio-only pipelines (validated via benchmark comparison)

- **SC-004**: All existing audio streaming examples (3 TypeScript examples in `examples/grpc_examples/typescript/`) run without code changes and pass original tests after protocol upgrade

- **SC-005**: TypeScript type checker catches 100% of invalid type connections (JSON→audio-only node) at compile time in test suite with deliberate type mismatches

- **SC-006**: Migration from audio-specific to generic APIs requires <20 lines of code changes for typical streaming client (documented in migration guide with before/after examples)

- **SC-007**: Service correctly validates and rejects mismatched data types with actionable error messages (manual test: 10 invalid combinations all return errors with expected vs actual types)

- **SC-008**: Zero-copy audio performance maintained after migration (benchmark shows <5% overhead in mean chunk processing latency compared to pre-migration baseline)

- **SC-009**: Developers successfully implement 4 new data type examples (video, tensor, JSON calculator, mixed pipeline) using only API documentation without source code inspection

- **SC-010**: Backward compatibility shim handles 100% of legacy `AudioChunk` messages from old clients without errors (regression test suite with old client binary)

## Assumptions

1. **Protobuf message size limit**: Assuming standard gRPC default of 4MB per message. Large tensors/videos exceeding this should be chunked by clients or use alternative transfer mechanisms.

2. **JSON parsing performance**: Assuming JSON nodes process small-to-medium payloads (<100KB). Large JSON documents may need special handling or binary alternatives (MessagePack, CBOR).

3. **Type declaration enforcement**: Assuming `inputTypes`/`outputTypes` in manifests are optional for backward compatibility but strongly recommended. Nodes without type declarations accept `ANY` type by default.

4. **Multi-input synchronization**: Assuming clients sending multiple input types to a single node use the same sequence number to indicate which inputs should be processed together.

5. **Video format conversion**: Assuming video nodes are responsible for format conversion (RGB↔YUV). The protocol only transports raw pixel data without codec support (no H.264/HEVC encoding in protocol layer).

6. **Tensor layout**: Assuming tensor-consuming nodes document their expected layout (NCHW vs NHWC). Protocol provides layout hint but doesn't enforce automatic conversion.

7. **Binary blob mime types**: Assuming clients set accurate mime types for binary data. Service doesn't validate mime type correctness but passes it through for downstream nodes.

8. **Deprecation timeline**: Assuming legacy `AudioChunk` API remains supported for at least 6 months after generic protocol release to allow gradual migration.

## Dependencies

- **Feature 003 (Rust gRPC Service)**: Generic streaming builds on the existing streaming infrastructure (`StreamingPipelineService`, session management, sequence validation, metrics). All foundational streaming logic (backpressure, session timeout, sequence tracking) is reused.

- **Protobuf compiler version**: Requires protobuf compiler v3.20+ for `oneof` optional field support and map types. Older versions may have compatibility issues with complex nested messages.

- **TypeScript protobuf library**: Requires `ts-proto` or `grpc-tools` with discriminated union support for `oneof` types to generate type-safe TypeScript interfaces.

- **Existing node registry**: Assumes existing `NodeRegistry` and `NodeFactory` trait from Feature 001 (Native Rust Acceleration) can be extended to support generic data inputs.

## Out of Scope

- **Codec integration**: Video/audio codecs (H.264, H.265, Opus, AAC) are not part of the protocol. Nodes must handle codec operations if needed; protocol only transports raw data.

- **Automatic type coercion**: System does not automatically convert between types (e.g., audio→tensor, JSON→video). Explicit conversion nodes must be implemented if needed.

- **Schema validation for JSON**: Beyond basic JSON parsing, the protocol doesn't validate JSON structure against schemas (JSON Schema, Protobuf). Nodes are responsible for validating their expected JSON structure.

- **Tensor operations**: Generic protocol transports tensors but doesn't provide tensor manipulation functions (reshape, transpose, etc.). Nodes implement tensor logic using external libraries (ndarray, nalgebra).

- **Binary format registry**: No central registry of binary mime types or format definitions. Clients and nodes must agree on formats out-of-band.

- **Cross-language type mapping**: Type safety features (US3) are language-specific. Python type hints and TypeScript types don't interoperate; each language has its own type system.

- **Compression**: Protocol doesn't compress data chunks automatically. Nodes or clients can compress binary data before wrapping in `BinaryBuffer` if needed.

- **Streaming large files**: Protocol is designed for chunk-by-chunk processing, not bulk file transfer. Large file uploads should use separate file upload RPCs, not streaming pipeline protocol.
