# Research: Universal Generic Streaming Protocol

**Feature**: `004-generic-streaming` | **Phase**: 0 - Research
**Generated**: 2025-01-15

This document captures research findings and design decisions for extending the streaming protocol from audio-only to support universal data types.

## Research Questions

### 1. Protobuf `oneof` Design for DataBuffer

**Question**: How should we structure the `DataBuffer` message to support multiple data types while maintaining type safety and efficient serialization?

**Research**:
- Protobuf `oneof` provides discriminated unions with exactly-one-set semantics
- `oneof` fields have zero overhead when not set (no wasted bytes for unused types)
- `oneof` generates type-safe accessors in Rust (enum), TypeScript (discriminated unions), Python (property checks)
- Alternative approaches considered:
  - **Separate messages per type**: Requires changes to RPC signatures, breaks backward compatibility
  - **Single bytes field + type enum**: Loses compile-time type safety, requires manual (de)serialization

**Decision**: Use `oneof data_type` in `DataBuffer` with variants for each data type:

```protobuf
message DataBuffer {
  oneof data_type {
    AudioBuffer audio = 1;
    VideoFrame video = 2;
    TensorBuffer tensor = 3;
    JsonData json = 4;
    TextBuffer text = 5;
    BinaryBuffer binary = 6;
  }

  // Optional metadata for extensibility (custom headers, compression info, etc.)
  map<string, string> metadata = 10;
}
```

**Rationale**:
- Maintains type safety across languages (Rust enum, TS discriminated union, Python type hints)
- Zero serialization overhead for unused variants
- Supports future extension (can add new variants with field numbers 7, 8, 9...)
- Metadata map allows custom extensions without protocol changes

---

### 2. RuntimeData Enum Design for In-Memory Representation

**Question**: How should Rust represent deserialized data in memory to support efficient type-based routing and zero-copy operations?

**Research**:
- Rust enums with data provide type-safe pattern matching
- `bytes::Bytes` allows zero-copy buffer sharing
- `serde_json::Value` provides efficient JSON manipulation
- Need common interface for operations like "count items" (samples, frames, tokens, objects)

**Decision**: Create `RuntimeData` enum with unified interface:

```rust
pub enum RuntimeData {
    Audio(AudioBuffer),
    Video(VideoFrame),
    Tensor(TensorBuffer),
    Json(serde_json::Value),
    Text(String),
    Binary(Bytes),
}

impl RuntimeData {
    // Common operations across all types
    pub fn item_count(&self) -> usize { /* samples, frames, tokens, etc. */ }
    pub fn data_type(&self) -> DataTypeHint { /* AUDIO, VIDEO, TENSOR, etc. */ }
    pub fn size_bytes(&self) -> usize { /* memory footprint */ }
}
```

**Rationale**:
- Pattern matching ensures exhaustive handling of all types
- Common interface enables generic metric collection (total_items_processed)
- Supports zero-copy for binary data (Audio, Video, Tensor use `Bytes`)
- Clear conversion path: Protobuf `DataBuffer` → `RuntimeData` → Node processing

---

### 3. Type Validation Strategy (Compile-Time vs Runtime)

**Question**: Where should type mismatches be detected: client-side at compile time, or server-side at runtime?

**Research**:
- **Compile-time (TypeScript, Python type hints)**: Catches errors early, better developer experience, requires type-safe builder API
- **Runtime (Server validation)**: Defensive validation, catches mismatches from untyped clients, provides actionable error messages
- Both layers needed for complete safety (defense in depth)

**Decision**: Implement validation at three layers:

1. **Compile-Time (Client Libraries)**:
   - TypeScript: Generic types `streamPipeline<AudioChunk | VideoChunk>()`
   - Python: Type hints with mypy checking `stream_pipeline(chunks: Iterator[DataChunk])`
   - Type-safe builders that enforce connection compatibility

2. **Manifest Validation (Server, at StreamInit)**:
   - Validate all connections: output type of source node matches input type of target node
   - Check node `inputTypes` declarations against incoming data types
   - Return `ERROR_TYPE_VALIDATION` with detailed mismatch information

3. **Runtime Chunk Validation (Server, per chunk)**:
   - Verify `DataChunk.buffer.data_type` matches target node's expected `inputTypes`
   - Log warnings for type hint mismatches (declared vs actual)
   - Block execution if incompatible type detected

**Rationale**:
- Compile-time validation provides best developer experience (SC-005: 100% detection in typed languages)
- Manifest validation catches issues before any data is streamed
- Runtime validation defends against untyped clients or manifest errors
- Layered approach maximizes safety while maintaining flexibility

---

### 4. Backward Compatibility Shim Implementation

**Question**: How should the service support legacy `AudioChunk` messages without duplicating streaming logic?

**Research**:
- gRPC supports multiple message types via `oneof` in streaming requests
- Can detect message type via protobuf discriminator
- Conversion should happen at protocol boundary (before entering executor)

**Decision**: Implement compatibility shim at streaming handler entry point:

```rust
// runtime/src/grpc_service/compat_shim.rs
pub fn convert_legacy_audio_chunk(legacy: AudioChunk) -> DataChunk {
    DataChunk {
        node_id: legacy.node_id,
        buffer: Some(DataBuffer {
            data_type: Some(data_buffer::DataType::Audio(legacy.buffer))
        }),
        sequence: legacy.sequence,
        timestamp_ms: legacy.timestamp_ms,
        named_buffers: Default::default(), // Legacy doesn't support multi-input
    }
}
```

**Integration Point**:
```rust
// streaming.rs handler
match request.request {
    stream_request::Request::AudioChunk(legacy) => {
        let generic_chunk = compat_shim::convert_legacy_audio_chunk(legacy);
        handle_data_chunk(generic_chunk, session).await
    },
    stream_request::Request::DataChunk(chunk) => {
        handle_data_chunk(chunk, session).await
    },
    // ... other cases
}
```

**Rationale**:
- Single code path after conversion (no logic duplication)
- Legacy clients work without changes (SC-004)
- Clear deprecation path (mark `AudioChunk` as deprecated in proto)
- Easy to remove shim after deprecation period

---

### 5. Multi-Input Node Support (Named Buffers)

**Question**: How should nodes that require multiple input types simultaneously (e.g., audio + JSON control) receive data?

**Research**:
- Single `DataChunk` with `oneof` can only carry one buffer
- Need mechanism to send multiple inputs with same sequence number
- Options:
  - **Multiple chunks with same sequence**: Requires synchronization logic, ordering issues
  - **Named buffers map**: Single chunk carries multiple named inputs
  - **Separate control channel**: Complexity, state synchronization

**Decision**: Add `named_buffers` map to `DataChunk`:

```protobuf
message DataChunk {
  string node_id = 1;

  // EITHER: Single unnamed buffer (backward compatible)
  DataBuffer buffer = 2;

  // OR: Multiple named buffers (for multi-input nodes)
  map<string, DataBuffer> named_buffers = 3;

  uint64 sequence = 4;
  uint64 timestamp_ms = 5;
}
```

**Usage Example** (Audio + JSON control):
```typescript
const chunk: DataChunk = {
  node_id: "dynamic_filter",
  named_buffers: {
    "audio": { audio: audioBuffer },      // Main audio stream
    "control": { json: { gain: 0.8 } }   // JSON control parameters
  },
  sequence: 42,
  timestamp_ms: 1000
};
```

**Rationale**:
- Synchronization guaranteed (same sequence number)
- Backward compatible (unnamed `buffer` field for single-input nodes)
- Clear semantics (buffer name maps to node input port)
- Supports User Story 2 (mixed-type pipelines with multi-input nodes)

---

### 6. JSON Data Handling (Parsing vs Pass-Through)

**Question**: Should the protocol parse JSON payloads into structured types, or treat them as opaque strings?

**Research**:
- Parsing on server enables validation, type-safe node implementation
- Pass-through reduces latency, allows nodes to choose parser (serde_json, simd-json, etc.)
- JSON Schema validation adds complexity, not always needed

**Decision**: Hybrid approach with optional parsing:

```protobuf
message JsonData {
  // JSON payload as string (required)
  string json_payload = 1;

  // Optional schema type hint for validation (e.g., "CalculatorRequest")
  string schema_type = 2;

  // Optional: pre-parsed fields for common cases (future extension)
  // google.protobuf.Struct parsed_value = 3;
}
```

**Server Processing**:
```rust
pub enum RuntimeData {
    Json(serde_json::Value), // Always parsed for consistency
    // ...
}

// Conversion function
fn convert_json_data(json: JsonData) -> Result<serde_json::Value> {
    serde_json::from_str(&json.json_payload)
        .map_err(|e| Error::JsonParsing {
            message: format!("Invalid JSON at line {}: {}", e.line(), e),
            schema_type: json.schema_type,
        })
}
```

**Rationale**:
- Parsing on server ensures valid JSON before node execution (fail-fast)
- Nodes work with structured `serde_json::Value` (easier than string manipulation)
- Schema type hint enables future validation extensions without protocol changes
- Error messages include parse location (SC-007: actionable errors)

---

### 7. Video Frame Format Support

**Question**: Which pixel formats should be supported, and should the protocol handle format conversion?

**Research**:
- Common formats: RGB24 (3 bytes/pixel), RGBA32 (4 bytes/pixel), YUV420P (planar, 1.5 bytes/pixel)
- Format conversion is compute-intensive (should be explicit node operation, not protocol overhead)
- Nodes like video encoders/decoders have format preferences

**Decision**: Support multiple formats with explicit declarations:

```protobuf
message VideoFrame {
  bytes pixel_data = 1;          // Raw pixel data (format specified below)
  uint32 width = 2;              // Frame width in pixels
  uint32 height = 3;             // Frame height in pixels
  PixelFormat format = 4;        // Pixel format
  uint64 frame_number = 5;       // Frame sequence number
  uint64 timestamp_us = 6;       // Timestamp in microseconds
}

enum PixelFormat {
  PIXEL_FORMAT_UNSPECIFIED = 0;
  PIXEL_FORMAT_RGB24 = 1;        // Packed RGB, 8-bit per channel
  PIXEL_FORMAT_RGBA32 = 2;       // Packed RGBA, 8-bit per channel
  PIXEL_FORMAT_YUV420P = 3;      // Planar YUV 4:2:0
  PIXEL_FORMAT_GRAY8 = 4;        // Grayscale, 8-bit
}
```

**Format Conversion Policy**:
- Protocol transports raw pixels, no automatic conversion
- Nodes declare supported formats in `capabilities` field
- If format mismatch: Node returns `ERROR_TYPE_NODE_EXECUTION` with message specifying expected format
- Explicit `VideoFormatConverter` node available for pipelines needing conversion

**Rationale**:
- Avoids hidden performance costs (format conversion is expensive)
- Clear error messages when format unsupported (SC-007)
- Extensible (new formats added as enum values)
- Out of scope: codec support (H.264, H.265) - nodes handle compression if needed

---

### 8. Tensor Layout and Shape Representation

**Question**: How should multi-dimensional tensor data be represented and validated?

**Research**:
- ML frameworks use different layouts: NCHW (Channels first, PyTorch), NHWC (Channels last, TensorFlow)
- Shape needs to be explicit for validation and memory allocation
- Data type (F32, I8, etc.) affects byte size calculation

**Decision**: Explicit shape, dtype, and optional layout hint:

```protobuf
message TensorBuffer {
  bytes data = 1;                     // Raw tensor data (row-major by default)
  repeated uint64 shape = 2;          // Shape array, e.g., [1, 3, 224, 224]
  TensorDtype dtype = 3;              // Data type
  string layout = 4;                  // Optional layout hint ("NCHW", "NHWC", etc.)
}

enum TensorDtype {
  TENSOR_DTYPE_UNSPECIFIED = 0;
  TENSOR_DTYPE_F32 = 1;               // 32-bit float
  TENSOR_DTYPE_F16 = 2;               // 16-bit float
  TENSOR_DTYPE_I32 = 3;               // 32-bit int
  TENSOR_DTYPE_I8 = 4;                // 8-bit int (quantized models)
  TENSOR_DTYPE_U8 = 5;                // 8-bit unsigned int
}
```

**Validation**:
```rust
fn validate_tensor_size(tensor: &TensorBuffer) -> Result<()> {
    let expected_elements: u64 = tensor.shape.iter().product();
    let bytes_per_element = dtype_size(tensor.dtype);
    let expected_bytes = expected_elements * bytes_per_element;

    if tensor.data.len() != expected_bytes as usize {
        return Err(Error::TensorSizeMismatch {
            expected: expected_bytes,
            actual: tensor.data.len(),
            shape: tensor.shape.clone(),
        });
    }
    Ok(())
}
```

**Rationale**:
- Explicit shape enables validation before processing
- Dtype ensures correct byte size calculation
- Layout hint is optional (nodes document expectations, no automatic conversion)
- Supports common ML use cases (embeddings, image tensors, etc.)

---

## Design Patterns

### Pattern 1: Proto ↔ Runtime Conversion

**Consistency**: All data types follow the same conversion pattern:

```rust
// Protobuf → Runtime
pub fn convert_proto_to_runtime_data(proto: DataBuffer) -> Result<RuntimeData> {
    match proto.data_type {
        Some(data_buffer::DataType::Audio(buf)) => Ok(RuntimeData::Audio(buf)),
        Some(data_buffer::DataType::Video(frame)) => Ok(RuntimeData::Video(frame)),
        Some(data_buffer::DataType::Json(json)) => {
            let value = serde_json::from_str(&json.json_payload)?;
            Ok(RuntimeData::Json(value))
        },
        // ... other types
        None => Err(Error::EmptyDataBuffer),
    }
}

// Runtime → Protobuf
pub fn convert_runtime_to_proto_data(runtime: RuntimeData) -> DataBuffer {
    DataBuffer {
        data_type: Some(match runtime {
            RuntimeData::Audio(buf) => data_buffer::DataType::Audio(buf),
            RuntimeData::Video(frame) => data_buffer::DataType::Video(frame),
            RuntimeData::Json(value) => data_buffer::DataType::Json(JsonData {
                json_payload: serde_json::to_string(&value).unwrap(),
                schema_type: String::new(),
            }),
            // ... other types
        }),
        metadata: Default::default(),
    }
}
```

---

### Pattern 2: Type Validation

**Layered Validation**:

```rust
// Layer 1: Manifest validation (at StreamInit)
pub fn validate_manifest_types(manifest: &PipelineManifest) -> Result<()> {
    for conn in &manifest.connections {
        let source_node = find_node(&manifest.nodes, &conn.from)?;
        let target_node = find_node(&manifest.nodes, &conn.to)?;

        // Check output types compatible with input types
        if !types_compatible(&source_node.output_types, &target_node.input_types) {
            return Err(Error::TypeMismatch {
                source: conn.from.clone(),
                source_types: source_node.output_types.clone(),
                target: conn.to.clone(),
                target_types: target_node.input_types.clone(),
            });
        }
    }
    Ok(())
}

// Layer 2: Runtime chunk validation
pub fn validate_chunk_type(chunk: &DataChunk, node: &NodeManifest) -> Result<()> {
    let chunk_type = get_data_type(&chunk.buffer)?;

    if !node.input_types.is_empty() && !node.input_types.contains(&chunk_type) {
        return Err(Error::ChunkTypeMismatch {
            node_id: node.id.clone(),
            expected: node.input_types.clone(),
            actual: chunk_type,
        });
    }
    Ok(())
}
```

---

## Performance Considerations

### Zero-Copy Audio Path

**Requirement**: Maintain <5% overhead vs audio-only protocol (FR-024, SC-008)

**Strategy**:
1. Use `bytes::Bytes` for audio samples (reference-counted, zero-copy cloning)
2. Avoid unnecessary serialization (reuse protobuf buffers where possible)
3. Benchmark critical path: `AudioChunk receive → convert_proto_to_runtime_data → node.process() → convert_runtime_to_proto_data → AudioChunk send`

**Measurement**:
```rust
// In ExecutionMetrics
pub struct ExecutionMetrics {
    // ... existing fields
    pub serialization_time_ms: f64,  // Track conversion overhead
    pub data_type_breakdown: HashMap<DataTypeHint, u64>, // Track type distribution
}
```

---

### JSON Parsing Overhead

**Target**: <1ms for simple operations (SC-002)

**Optimization**:
- Use `serde_json` (fast, widely tested)
- For future: Consider `simd-json` for hot paths (4-10x faster for large JSONs)
- Cache parsed values in session state when same JSON used repeatedly

---

## Migration Strategy

### Client Migration Path

**Phase 1: Introduce generic APIs (backward compatible)**
- Add `streamPipeline<T>()` alongside existing `streamAudioPipeline()`
- Mark `streamAudioPipeline()` as `@deprecated` in TypeScript, Python
- Existing code continues working unchanged

**Phase 2: Update examples**
- Add 4 new examples (video, tensor, JSON, mixed)
- Keep existing audio examples (prove backward compat)
- Migration guide shows before/after

**Phase 3: Eventual removal (6+ months)**
- Remove `streamAudioPipeline()` helpers
- Remove `AudioChunk` from protobuf (breaking change, requires v2)

---

## References

- **Protobuf Language Guide (proto3)**: https://developers.google.com/protocol-buffers/docs/proto3
- **Rust prost documentation**: https://docs.rs/prost/latest/prost/
- **TypeScript Discriminated Unions**: https://www.typescriptlang.org/docs/handbook/2/narrowing.html#discriminated-unions
- **Feature 003 Streaming Protocol**: `specs/003-rust-grpc-service/contracts/streaming.proto`
- **Existing RuntimeData**: `runtime/src/executor/mod.rs` (audio-only, to be extended)

---

## Open Questions / Future Research

1. **Compression**: Should protocol support chunk-level compression (gzip, zstd)? Deferred to future feature (clients can compress `bytes` fields manually if needed).

2. **Schema Registry**: For JSON validation, consider central schema registry? Out of scope for MVP (nodes validate their own schemas).

3. **Streaming Large Tensors**: 4MB limit may be tight for large models. Consider chunked tensor streaming? Deferred (clients can split large tensors into smaller chunks).

4. **Binary Format Detection**: Should `BinaryBuffer` auto-detect mime type? No - client responsibility (assumption 7).

5. **Cross-Language Type Guarantees**: How to ensure Python type hints match TypeScript types match Rust enums? Solved by generating all from same .proto files (protobuf is source of truth).
