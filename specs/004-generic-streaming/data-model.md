# Data Model: Universal Generic Streaming Protocol

**Feature**: `004-generic-streaming` | **Phase**: 1 - Design
**Generated**: 2025-01-15

This document defines the extended protocol buffer message types for the universal generic streaming protocol. These types enable streaming and processing of any protocol bufferable data type (audio, video, tensors, JSON, text, binary) while maintaining backward compatibility with the audio-only protocol from Feature 003.

## Overview

The generic streaming protocol extends Feature 003's audio-only protocol to support universal data types. The data model is organized into five categories:

1. **Core Data Containers**: Generic data buffers with discriminated union types
2. **Data Type Variants**: Specific formats (audio, video, tensor, JSON, text, binary)
3. **Generic Messages**: Updated streaming and execution messages using generic data
4. **Type System**: Type hints and validation enums
5. **Backward Compatibility**: Legacy message support and migration path

## Design Principles

- **Type Safety via Oneof**: Protobuf `oneof` provides discriminated unions with compile-time type safety
- **Zero-Copy for Binary Data**: Audio, video, and tensor data use `bytes` fields to minimize copying
- **Extensibility**: Metadata maps and optional fields support future extensions without breaking changes
- **Backward Compatible**: Legacy `AudioChunk` messages automatically convert to generic `DataChunk`
- **Language Neutral**: Types map cleanly to Rust enums, TypeScript discriminated unions, Python type hints

## Core Data Containers

### 1. DataBuffer

**Purpose**: Universal container for any protocol bufferable data type. Uses `oneof` discriminator for type-safe variant selection.

**Source**: Research decision #1 (Protobuf oneof design)

**Fields**:

```protobuf
message DataBuffer {
  // Data type discriminator (exactly one must be set)
  oneof data_type {
    AudioBuffer audio = 1;
    VideoFrame video = 2;
    TensorBuffer tensor = 3;
    JsonData json = 4;
    TextBuffer text = 5;
    BinaryBuffer binary = 6;
  }

  // Optional metadata for extensibility
  // Examples: compression="gzip", encoding="base64", custom_key="value"
  map<string, string> metadata = 10;
}
```

**Field Details**:

- `data_type` (oneof, required): Exactly one variant must be set. Protobuf enforces this constraint at serialization time.
- `audio` (AudioBuffer): Audio samples with format and sample rate (unchanged from Feature 003)
- `video` (VideoFrame): Video frame with pixel data and format information
- `tensor` (TensorBuffer): Multi-dimensional tensor with shape and dtype
- `json` (JsonData): JSON payload as string with optional schema hint
- `text` (TextBuffer): UTF-8 text data with optional language/encoding metadata
- `binary` (BinaryBuffer): Raw binary data with mime type
- `metadata` (map<string, string>, optional): Custom key-value pairs for extensions (compression, encoding, custom headers)

**Validation Rules**:
- Exactly one `oneof` field must be set (protobuf enforces)
- Empty `DataBuffer` (no variant set) is invalid
- Metadata keys should use lowercase snake_case by convention

**Type Mapping**:

```rust
// Rust (prost-generated)
pub struct DataBuffer {
    pub data_type: Option<data_buffer::DataType>,
    pub metadata: HashMap<String, String>,
}

pub mod data_buffer {
    pub enum DataType {
        Audio(AudioBuffer),
        Video(VideoFrame),
        Tensor(TensorBuffer),
        Json(JsonData),
        Text(TextBuffer),
        Binary(BinaryBuffer),
    }
}
```

```typescript
// TypeScript (discriminated union)
type DataBuffer = {
  audio?: AudioBuffer;
  video?: VideoFrame;
  tensor?: TensorBuffer;
  json?: JsonData;
  text?: TextBuffer;
  binary?: BinaryBuffer;
  metadata: { [key: string]: string };
};
```

---

### 2. DataChunk

**Purpose**: Generic streaming message that replaces `AudioChunk` from Feature 003. Carries any data type to any node.

**Source**: Research decision #5 (Multi-input node support)

**Fields**:

```protobuf
message DataChunk {
  // Target node ID (must match manifest node)
  string node_id = 1;

  // EITHER: Single unnamed buffer (backward compatible with audio-only)
  DataBuffer buffer = 2;

  // OR: Multiple named buffers (for multi-input nodes)
  // Example: {"audio": audio_buffer, "control": json_buffer}
  map<string, DataBuffer> named_buffers = 3;

  // Sequence number for ordering (0, 1, 2, ...)
  uint64 sequence = 4;

  // Timestamp in milliseconds since stream start
  uint64 timestamp_ms = 5;
}
```

**Field Details**:

- `node_id` (string, required): Target node ID from manifest. Service validates node exists and accepts streaming input.
- `buffer` (DataBuffer, optional): Single data buffer for simple single-input nodes. Mutually exclusive with `named_buffers` (use one or the other).
- `named_buffers` (map<string, DataBuffer>, optional): Multiple named inputs for multi-input nodes. Keys map to node input port names.
- `sequence` (uint64, required): Monotonically increasing sequence number. Service validates ordering and detects gaps.
- `timestamp_ms` (uint64, required): Timestamp relative to stream start. Used for synchronization and latency measurement.

**Usage Patterns**:

```protobuf
// Pattern 1: Single-input node (audio VAD)
DataChunk {
  node_id: "vad",
  buffer: DataBuffer { audio: { ... } },
  sequence: 42,
  timestamp_ms: 1000
}

// Pattern 2: Multi-input node (audio + JSON control)
DataChunk {
  node_id: "dynamic_filter",
  named_buffers: {
    "audio": DataBuffer { audio: { ... } },
    "control": DataBuffer { json: { gain: 0.8 } }
  },
  sequence: 42,
  timestamp_ms: 1000
}
```

**Validation Rules**:
- Exactly one of `buffer` OR `named_buffers` must be set (not both, not neither)
- If `buffer` is set: must contain valid `DataBuffer` with one `oneof` variant
- If `named_buffers` is set: map must not be empty, all values must be valid `DataBuffer`
- `sequence` must be greater than previous chunk's sequence (strict monotonic)
- `timestamp_ms` should be non-decreasing (warnings for time inversions)

**Type Mapping**:

```rust
// Rust
pub struct DataChunk {
    pub node_id: String,
    pub buffer: Option<DataBuffer>,
    pub named_buffers: HashMap<String, DataBuffer>,
    pub sequence: u64,
    pub timestamp_ms: u64,
}
```

```typescript
// TypeScript
interface DataChunk {
  nodeId: string;
  buffer?: DataBuffer;
  namedBuffers?: { [name: string]: DataBuffer };
  sequence: number;
  timestampMs: number;
}
```

---

### 3. RuntimeData (Rust Internal)

**Purpose**: Rust in-memory representation of deserialized data. Provides unified interface for all data types.

**Source**: Research decision #2 (RuntimeData enum design)

**Definition**:

```rust
pub enum RuntimeData {
    Audio(AudioBuffer),
    Video(VideoFrame),
    Tensor(TensorBuffer),
    Json(serde_json::Value),  // Parsed from JsonData.json_payload
    Text(String),
    Binary(Bytes),  // Zero-copy binary data
}

impl RuntimeData {
    // Get data type hint for routing
    pub fn data_type(&self) -> DataTypeHint {
        match self {
            RuntimeData::Audio(_) => DataTypeHint::AUDIO,
            RuntimeData::Video(_) => DataTypeHint::VIDEO,
            RuntimeData::Tensor(_) => DataTypeHint::TENSOR,
            RuntimeData::Json(_) => DataTypeHint::JSON,
            RuntimeData::Text(_) => DataTypeHint::TEXT,
            RuntimeData::Binary(_) => DataTypeHint::BINARY,
        }
    }

    // Get item count (samples, frames, tokens, objects)
    pub fn item_count(&self) -> usize {
        match self {
            RuntimeData::Audio(buf) => buf.num_samples as usize,
            RuntimeData::Video(_) => 1,  // One frame
            RuntimeData::Tensor(t) => t.shape.iter().product::<u64>() as usize,
            RuntimeData::Json(v) => {
                // For arrays: element count, for objects: field count, else 1
                match v {
                    serde_json::Value::Array(arr) => arr.len(),
                    serde_json::Value::Object(obj) => obj.len(),
                    _ => 1,
                }
            },
            RuntimeData::Text(s) => s.chars().count(),  // Character count
            RuntimeData::Binary(b) => b.len(),  // Byte count
        }
    }

    // Get memory size in bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            RuntimeData::Audio(buf) => buf.samples.len(),
            RuntimeData::Video(frame) => frame.pixel_data.len(),
            RuntimeData::Tensor(t) => t.data.len(),
            RuntimeData::Json(v) => {
                // Approximate JSON size
                serde_json::to_string(v).map(|s| s.len()).unwrap_or(0)
            },
            RuntimeData::Text(s) => s.len(),  // UTF-8 byte length
            RuntimeData::Binary(b) => b.len(),
        }
    }
}
```

**Conversion Functions**:

```rust
// Protobuf → Runtime
pub fn convert_proto_to_runtime_data(proto: DataBuffer) -> Result<RuntimeData> {
    match proto.data_type {
        Some(data_buffer::DataType::Audio(buf)) => {
            Ok(RuntimeData::Audio(buf))
        },
        Some(data_buffer::DataType::Video(frame)) => {
            Ok(RuntimeData::Video(frame))
        },
        Some(data_buffer::DataType::Tensor(tensor)) => {
            validate_tensor_size(&tensor)?;  // Check shape matches data length
            Ok(RuntimeData::Tensor(tensor))
        },
        Some(data_buffer::DataType::Json(json_data)) => {
            // Parse JSON string into serde_json::Value
            let value = serde_json::from_str(&json_data.json_payload)
                .map_err(|e| Error::JsonParsing {
                    message: format!("Invalid JSON at line {}: {}", e.line(), e),
                    schema_type: json_data.schema_type,
                })?;
            Ok(RuntimeData::Json(value))
        },
        Some(data_buffer::DataType::Text(text_buf)) => {
            // Validate UTF-8
            String::from_utf8(text_buf.text_data.into())
                .map(RuntimeData::Text)
                .map_err(|e| Error::InvalidUtf8(e))
        },
        Some(data_buffer::DataType::Binary(bin)) => {
            Ok(RuntimeData::Binary(Bytes::from(bin.data)))
        },
        None => Err(Error::EmptyDataBuffer {
            message: "DataBuffer has no data_type variant set".into()
        }),
    }
}

// Runtime → Protobuf
pub fn convert_runtime_to_proto_data(runtime: RuntimeData) -> DataBuffer {
    DataBuffer {
        data_type: Some(match runtime {
            RuntimeData::Audio(buf) => data_buffer::DataType::Audio(buf),
            RuntimeData::Video(frame) => data_buffer::DataType::Video(frame),
            RuntimeData::Tensor(tensor) => data_buffer::DataType::Tensor(tensor),
            RuntimeData::Json(value) => data_buffer::DataType::Json(JsonData {
                json_payload: serde_json::to_string(&value).unwrap(),
                schema_type: String::new(),
            }),
            RuntimeData::Text(s) => data_buffer::DataType::Text(TextBuffer {
                text_data: s.into_bytes(),
                encoding: "utf-8".into(),
                language: String::new(),
            }),
            RuntimeData::Binary(bytes) => data_buffer::DataType::Binary(BinaryBuffer {
                data: bytes.to_vec(),
                mime_type: "application/octet-stream".into(),
            }),
        }),
        metadata: Default::default(),
    }
}
```

---

## Data Type Variants

### 4. AudioBuffer (Unchanged from Feature 003)

**Purpose**: Multi-channel audio data with format and sample rate metadata.

**Fields**:

```protobuf
message AudioBuffer {
  bytes samples = 1;
  uint32 sample_rate = 2;
  uint32 channels = 3;
  AudioFormat format = 4;
  uint64 num_samples = 5;
}

enum AudioFormat {
  AUDIO_FORMAT_UNSPECIFIED = 0;
  AUDIO_FORMAT_F32 = 1;  // 32-bit float [-1.0, 1.0]
  AUDIO_FORMAT_I16 = 2;  // 16-bit signed int
  AUDIO_FORMAT_I32 = 3;  // 32-bit signed int
}
```

**Notes**: This type is unchanged from Feature 003 to maintain backward compatibility. All existing audio pipelines work without modification.

---

### 5. VideoFrame

**Purpose**: Video frame data with pixel format and dimensions.

**Source**: Research decision #7 (Video frame format support)

**Fields**:

```protobuf
message VideoFrame {
  // Raw pixel data (format specified by format field)
  bytes pixel_data = 1;

  // Frame width in pixels
  uint32 width = 2;

  // Frame height in pixels
  uint32 height = 3;

  // Pixel format
  PixelFormat format = 4;

  // Frame sequence number
  uint64 frame_number = 5;

  // Timestamp in microseconds
  uint64 timestamp_us = 6;
}

enum PixelFormat {
  PIXEL_FORMAT_UNSPECIFIED = 0;
  PIXEL_FORMAT_RGB24 = 1;    // Packed RGB, 8-bit per channel (3 bytes/pixel)
  PIXEL_FORMAT_RGBA32 = 2;   // Packed RGBA, 8-bit per channel (4 bytes/pixel)
  PIXEL_FORMAT_YUV420P = 3;  // Planar YUV 4:2:0 (1.5 bytes/pixel)
  PIXEL_FORMAT_GRAY8 = 4;    // Grayscale, 8-bit (1 byte/pixel)
}
```

**Field Details**:

- `pixel_data` (bytes, required): Raw pixel data. Layout determined by `format`.
- `width` (uint32, required): Frame width in pixels (must be > 0).
- `height` (uint32, required): Frame height in pixels (must be > 0).
- `format` (PixelFormat, required): Pixel format. Determines bytes-per-pixel calculation.
- `frame_number` (uint64, required): Frame sequence number for ordering.
- `timestamp_us` (uint64, required): Frame timestamp in microseconds for synchronization.

**Validation Rules**:

```rust
fn validate_video_frame(frame: &VideoFrame) -> Result<()> {
    if frame.width == 0 || frame.height == 0 {
        return Err(Error::InvalidDimensions {
            width: frame.width,
            height: frame.height,
        });
    }

    let expected_bytes = match frame.format {
        PixelFormat::RGB24 => frame.width * frame.height * 3,
        PixelFormat::RGBA32 => frame.width * frame.height * 4,
        PixelFormat::YUV420P => {
            // Y plane: width*height, U plane: (width/2)*(height/2), V plane: same as U
            frame.width * frame.height * 3 / 2
        },
        PixelFormat::GRAY8 => frame.width * frame.height,
        _ => return Err(Error::UnknownPixelFormat),
    };

    if frame.pixel_data.len() != expected_bytes as usize {
        return Err(Error::VideoSizeMismatch {
            expected: expected_bytes,
            actual: frame.pixel_data.len(),
            dimensions: (frame.width, frame.height),
            format: frame.format,
        });
    }

    Ok(())
}
```

**Example** (1920x1080 RGB24 frame):

```protobuf
VideoFrame {
  pixel_data: <6220800 bytes>,  // 1920*1080*3
  width: 1920,
  height: 1080,
  format: PIXEL_FORMAT_RGB24,
  frame_number: 42,
  timestamp_us: 1400000  // 1.4 seconds
}
```

**Out of Scope**: Video codecs (H.264, H.265) are not supported in protocol. Nodes must handle encoding/decoding if needed.

---

### 6. TensorBuffer

**Purpose**: Multi-dimensional tensor data with shape and dtype.

**Source**: Research decision #8 (Tensor layout and shape representation)

**Fields**:

```protobuf
message TensorBuffer {
  // Raw tensor data (row-major layout by default)
  bytes data = 1;

  // Shape array (e.g., [1, 3, 224, 224] for batch=1, channels=3, 224x224 image)
  repeated uint64 shape = 2;

  // Data type
  TensorDtype dtype = 3;

  // Optional layout hint ("NCHW", "NHWC", "row-major", etc.)
  string layout = 4;
}

enum TensorDtype {
  TENSOR_DTYPE_UNSPECIFIED = 0;
  TENSOR_DTYPE_F32 = 1;   // 32-bit float (4 bytes)
  TENSOR_DTYPE_F16 = 2;   // 16-bit float (2 bytes)
  TENSOR_DTYPE_I32 = 3;   // 32-bit int (4 bytes)
  TENSOR_DTYPE_I8 = 4;    // 8-bit int (1 byte, quantized models)
  TENSOR_DTYPE_U8 = 5;    // 8-bit unsigned int (1 byte)
}
```

**Field Details**:

- `data` (bytes, required): Raw tensor data. Must match calculated size from shape and dtype.
- `shape` (repeated uint64, required): Tensor dimensions. Empty shape = scalar. [N] = vector. [N, M] = matrix.
- `dtype` (TensorDtype, required): Element data type. Determines bytes-per-element.
- `layout` (string, optional): Layout hint for multi-dimensional tensors. Common values: "NCHW" (PyTorch), "NHWC" (TensorFlow). Nodes document expected layout.

**Validation Rules**:

```rust
fn validate_tensor_size(tensor: &TensorBuffer) -> Result<()> {
    let expected_elements: u64 = tensor.shape.iter().product();

    let bytes_per_element = match tensor.dtype {
        TensorDtype::F32 | TensorDtype::I32 => 4,
        TensorDtype::F16 => 2,
        TensorDtype::I8 | TensorDtype::U8 => 1,
        _ => return Err(Error::UnknownDtype),
    };

    let expected_bytes = expected_elements * bytes_per_element;

    if tensor.data.len() != expected_bytes as usize {
        return Err(Error::TensorSizeMismatch {
            expected: expected_bytes,
            actual: tensor.data.len(),
            shape: tensor.shape.clone(),
            dtype: tensor.dtype,
        });
    }

    Ok(())
}
```

**Example** (512-dimensional embedding vector, F32):

```protobuf
TensorBuffer {
  data: <2048 bytes>,  // 512 * 4 bytes
  shape: [512],
  dtype: TENSOR_DTYPE_F32,
  layout: ""  // Not needed for 1D vector
}
```

**Example** (3x224x224 image tensor, F32, NCHW):

```protobuf
TensorBuffer {
  data: <602112 bytes>,  // 3*224*224*4
  shape: [3, 224, 224],
  dtype: TENSOR_DTYPE_F32,
  layout: "NCHW"
}
```

---

### 7. JsonData

**Purpose**: JSON payloads for structured data, control parameters, and metadata.

**Source**: Research decision #6 (JSON data handling)

**Fields**:

```protobuf
message JsonData {
  // JSON payload as string (required)
  string json_payload = 1;

  // Optional schema type hint for validation
  // Example: "CalculatorRequest", "VADConfig", "DetectionResult"
  string schema_type = 2;
}
```

**Field Details**:

- `json_payload` (string, required): JSON-encoded string. Must be valid JSON.
- `schema_type` (string, optional): Schema identifier for validation. Nodes can use this to validate structure.

**Server Processing**:

```rust
// Server always parses JSON into serde_json::Value
fn convert_json_data(json: JsonData) -> Result<serde_json::Value> {
    serde_json::from_str(&json.json_payload)
        .map_err(|e| Error::JsonParsing {
            message: format!("Invalid JSON at line {}: {}", e.line(), e),
            schema_type: json.schema_type,
        })
}
```

**Example** (Calculator request):

```protobuf
JsonData {
  json_payload: "{\"operation\": \"add\", \"operands\": [10, 20]}",
  schema_type: "CalculatorRequest"
}
```

**Example** (VAD result):

```protobuf
JsonData {
  json_payload: "{\"has_speech\": true, \"confidence\": 0.87, \"segments\": [[0.5, 2.3]]}",
  schema_type: "VADResult"
}
```

**Validation**: Server validates JSON syntax. Schema validation is node-specific (out of scope for protocol).

---

### 8. TextBuffer

**Purpose**: UTF-8 text data with optional encoding and language metadata.

**Fields**:

```protobuf
message TextBuffer {
  // Text data as UTF-8 encoded bytes
  bytes text_data = 1;

  // Text encoding (default: "utf-8")
  string encoding = 2;

  // Optional language code (ISO 639-1, e.g., "en", "es", "zh")
  string language = 3;
}
```

**Field Details**:

- `text_data` (bytes, required): UTF-8 encoded text. Server validates UTF-8 correctness.
- `encoding` (string, optional): Encoding type. Default: "utf-8". Other values: "ascii", "utf-16".
- `language` (string, optional): Language hint for text processing nodes (NLP, tokenization).

**Validation**:

```rust
fn validate_text_buffer(text_buf: &TextBuffer) -> Result<String> {
    // Validate UTF-8 encoding
    String::from_utf8(text_buf.text_data.clone())
        .map_err(|e| Error::InvalidUtf8 {
            message: format!("Invalid UTF-8 at byte {}", e.utf8_error().valid_up_to()),
            encoding: text_buf.encoding.clone(),
        })
}
```

**Example** (English text):

```protobuf
TextBuffer {
  text_data: "Hello, world! This is a test.",  // UTF-8 bytes
  encoding: "utf-8",
  language: "en"
}
```

---

### 9. BinaryBuffer

**Purpose**: Raw binary data with mime type hint.

**Fields**:

```protobuf
message BinaryBuffer {
  // Raw binary data
  bytes data = 1;

  // MIME type (e.g., "application/octet-stream", "image/png", "application/protobuf")
  string mime_type = 2;
}
```

**Field Details**:

- `data` (bytes, required): Raw binary data. No validation beyond size limits.
- `mime_type` (string, required): MIME type hint. Not validated by protocol (client responsibility).

**Example** (Binary protobuf message):

```protobuf
BinaryBuffer {
  data: <serialized protobuf bytes>,
  mime_type: "application/protobuf"
}
```

**Example** (PNG image):

```protobuf
BinaryBuffer {
  data: <PNG file bytes>,
  mime_type: "image/png"
}
```

**Note**: MIME type accuracy is client responsibility. Service passes through without validation.

---

## Type System

### 10. DataTypeHint

**Purpose**: Declares expected input/output types for nodes in manifests. Used for compile-time and runtime validation.

**Source**: Research decision #3 (Type validation strategy)

**Fields**:

```protobuf
enum DataTypeHint {
  DATA_TYPE_HINT_UNSPECIFIED = 0;
  DATA_TYPE_HINT_AUDIO = 1;
  DATA_TYPE_HINT_VIDEO = 2;
  DATA_TYPE_HINT_TENSOR = 3;
  DATA_TYPE_HINT_JSON = 4;
  DATA_TYPE_HINT_TEXT = 5;
  DATA_TYPE_HINT_BINARY = 6;
  DATA_TYPE_HINT_ANY = 7;  // Accept any type (polymorphic node)
}
```

**Usage in Node Manifests**:

```protobuf
message NodeManifest {
  string id = 1;
  string node_type = 2;
  string params = 3;
  bool is_streaming = 4;
  CapabilityRequirements capabilities = 5;
  string host = 6;
  RuntimeHint runtime_hint = 7;

  // NEW: Declare expected input/output types
  repeated DataTypeHint input_types = 8;
  repeated DataTypeHint output_types = 9;
}
```

**Example** (VAD node: audio in, JSON out):

```protobuf
NodeManifest {
  id: "vad",
  node_type: "RustVADNode",
  params: "{}",
  input_types: [DATA_TYPE_HINT_AUDIO],
  output_types: [DATA_TYPE_HINT_JSON]
}
```

**Example** (Multi-input filter: audio + JSON in, audio out):

```protobuf
NodeManifest {
  id: "dynamic_filter",
  node_type: "DynamicAudioFilter",
  params: "{}",
  input_types: [DATA_TYPE_HINT_AUDIO, DATA_TYPE_HINT_JSON],
  output_types: [DATA_TYPE_HINT_AUDIO]
}
```

**Example** (Polymorphic logger: any type in, JSON out):

```protobuf
NodeManifest {
  id: "logger",
  node_type: "GenericLogger",
  params: "{}",
  input_types: [DATA_TYPE_HINT_ANY],
  output_types: [DATA_TYPE_HINT_JSON]
}
```

**Validation Strategy**:

```rust
// Layer 1: Manifest validation (at StreamInit)
pub fn validate_manifest_types(manifest: &PipelineManifest) -> Result<()> {
    for conn in &manifest.connections {
        let source_node = find_node(&manifest.nodes, &conn.from)?;
        let target_node = find_node(&manifest.nodes, &conn.to)?;

        // Check if types compatible
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

fn types_compatible(
    output_types: &[DataTypeHint],
    input_types: &[DataTypeHint]
) -> bool {
    // If input accepts ANY, always compatible
    if input_types.contains(&DataTypeHint::ANY) {
        return true;
    }

    // If both empty, assume compatible (untyped nodes)
    if output_types.is_empty() && input_types.is_empty() {
        return true;
    }

    // Check if any output type matches any input type
    output_types.iter().any(|out_type| {
        input_types.contains(out_type)
    })
}

// Layer 2: Runtime chunk validation
pub fn validate_chunk_type(chunk: &DataChunk, node: &NodeManifest) -> Result<()> {
    let chunk_type = get_chunk_data_type(chunk)?;

    // If node has no input types declared, accept anything
    if node.input_types.is_empty() {
        return Ok(());
    }

    // If node accepts ANY, allow
    if node.input_types.contains(&DataTypeHint::ANY) {
        return Ok(());
    }

    // Check if chunk type matches node's input types
    if !node.input_types.contains(&chunk_type) {
        return Err(Error::ChunkTypeMismatch {
            node_id: node.id.clone(),
            expected: node.input_types.clone(),
            actual: chunk_type,
        });
    }

    Ok(())
}

fn get_chunk_data_type(chunk: &DataChunk) -> Result<DataTypeHint> {
    // Handle single buffer
    if let Some(buffer) = &chunk.buffer {
        return get_buffer_data_type(buffer);
    }

    // Handle named buffers (return primary type if single, or first type)
    if !chunk.named_buffers.is_empty() {
        let first_buffer = chunk.named_buffers.values().next().unwrap();
        return get_buffer_data_type(first_buffer);
    }

    Err(Error::EmptyDataChunk)
}

fn get_buffer_data_type(buffer: &DataBuffer) -> Result<DataTypeHint> {
    match &buffer.data_type {
        Some(data_buffer::DataType::Audio(_)) => Ok(DataTypeHint::AUDIO),
        Some(data_buffer::DataType::Video(_)) => Ok(DataTypeHint::VIDEO),
        Some(data_buffer::DataType::Tensor(_)) => Ok(DataTypeHint::TENSOR),
        Some(data_buffer::DataType::Json(_)) => Ok(DataTypeHint::JSON),
        Some(data_buffer::DataType::Text(_)) => Ok(DataTypeHint::TEXT),
        Some(data_buffer::DataType::Binary(_)) => Ok(DataTypeHint::BINARY),
        None => Err(Error::EmptyDataBuffer),
    }
}
```

---

## Generic Message Types

### 11. StreamRequest (Updated)

**Purpose**: Client streaming request supporting generic data.

**Changes from Feature 003**: Replaces `AudioChunk` with `DataChunk` in primary path. Keeps `AudioChunk` for backward compatibility.

**Fields**:

```protobuf
message StreamRequest {
  oneof request {
    StreamInit init = 1;

    // NEW: Generic data chunk (preferred)
    DataChunk data_chunk = 2;

    // DEPRECATED: Legacy audio chunk (backward compatibility)
    AudioChunk audio_chunk = 3;

    StreamControl control = 4;
  }
}
```

**Migration Path**:

```rust
// Compatibility shim converts legacy AudioChunk to DataChunk
pub fn convert_legacy_audio_chunk(legacy: AudioChunk) -> DataChunk {
    DataChunk {
        node_id: legacy.node_id,
        buffer: Some(DataBuffer {
            data_type: Some(data_buffer::DataType::Audio(legacy.buffer)),
            metadata: Default::default(),
        }),
        named_buffers: Default::default(),
        sequence: legacy.sequence,
        timestamp_ms: legacy.timestamp_ms,
    }
}

// Handler routes both to generic path
match request.request {
    Some(stream_request::Request::AudioChunk(legacy)) => {
        let generic_chunk = convert_legacy_audio_chunk(legacy);
        handle_data_chunk(generic_chunk, session).await
    },
    Some(stream_request::Request::DataChunk(chunk)) => {
        handle_data_chunk(chunk, session).await
    },
    // ... other cases
}
```

---

### 12. StreamResponse (Updated)

**Purpose**: Server streaming response with generic data.

**Changes from Feature 003**: `ChunkResult` now uses generic data outputs.

**Fields**:

```protobuf
message StreamResponse {
  oneof response {
    StreamReady ready = 1;
    ChunkResult result = 2;
    ErrorResponse error = 3;
    StreamMetrics metrics = 4;
    StreamClosed closed = 5;
  }
}

message ChunkResult {
  uint64 sequence = 1;

  // CHANGED: Generic data outputs (was audio-specific)
  map<string, DataBuffer> data_outputs = 2;

  double processing_time_ms = 3;

  // CHANGED: Generic item count (was samples)
  uint64 total_items_processed = 4;
}
```

**Migration Notes**:
- `audio_outputs` removed (use `data_outputs` with audio variant)
- `total_samples_processed` renamed to `total_items_processed` (samples, frames, tokens, etc.)

---

### 13. ExecuteRequest (Updated)

**Purpose**: Unary execution request with generic inputs.

**Changes from Feature 003**: Replaces `audio_inputs` with generic `data_inputs`.

**Fields**:

```protobuf
message ExecuteRequest {
  PipelineManifest manifest = 1;

  // CHANGED: Generic data inputs (keyed by node ID)
  map<string, DataBuffer> data_inputs = 2;

  // REMOVED: audio_inputs (use data_inputs with audio variant)

  ResourceLimits resource_limits = 3;
  string client_version = 4;
}
```

**Example** (Audio processing):

```protobuf
ExecuteRequest {
  manifest: { ... },
  data_inputs: {
    "resample": DataBuffer { audio: { ... } }
  }
}
```

**Example** (Mixed types):

```protobuf
ExecuteRequest {
  manifest: { ... },
  data_inputs: {
    "vad": DataBuffer { audio: { ... } },
    "config": DataBuffer { json: { threshold: 0.5 } }
  }
}
```

---

### 14. ExecuteResponse (Updated)

**Purpose**: Unary execution response with generic outputs.

**Changes from Feature 003**: `ExecutionResult` uses generic `data_outputs`.

**Fields**:

```protobuf
message ExecuteResponse {
  oneof outcome {
    ExecutionResult result = 1;
    ErrorResponse error = 2;
  }
}

message ExecutionResult {
  // CHANGED: Generic data outputs (keyed by node ID)
  map<string, DataBuffer> data_outputs = 1;

  // REMOVED: audio_outputs, data_outputs (merged into single data_outputs)

  ExecutionMetrics metrics = 2;
  repeated NodeResult node_results = 3;
  ExecutionStatus status = 4;
}
```

---

### 15. StreamMetrics (Updated)

**Purpose**: Periodic streaming metrics with generic item counting.

**Changes from Feature 003**: `total_samples` renamed to `total_items`.

**Fields**:

```protobuf
message StreamMetrics {
  string session_id = 1;
  uint64 chunks_processed = 2;
  double average_latency_ms = 3;

  // CHANGED: Generic item count (was total_samples)
  uint64 total_items = 4;

  uint64 buffer_samples = 5;
  uint64 chunks_dropped = 6;
  uint64 peak_memory_bytes = 7;

  // NEW: Track type distribution
  map<string, uint64> data_type_breakdown = 8;
}
```

**Example**:

```protobuf
StreamMetrics {
  session_id: "abc123",
  chunks_processed: 100,
  average_latency_ms: 12.5,
  total_items: 160000,  // Audio samples or video frames or tokens
  data_type_breakdown: {
    "audio": 80,
    "json": 20
  }
}
```

---

## Backward Compatibility

### 16. AudioChunk (Deprecated)

**Status**: Marked as `deprecated` in protobuf. Maintained for backward compatibility.

**Migration Timeline**: 6+ months support after generic protocol release.

**Fields** (unchanged from Feature 003):

```protobuf
message AudioChunk {
  string node_id = 1;
  AudioBuffer buffer = 2;
  uint64 sequence = 3;
  uint64 timestamp_ms = 4;
}
```

**Automatic Conversion**:

```rust
// runtime/src/grpc_service/compat_shim.rs
pub fn convert_legacy_audio_chunk(legacy: AudioChunk) -> DataChunk {
    DataChunk {
        node_id: legacy.node_id,
        buffer: Some(DataBuffer {
            data_type: Some(data_buffer::DataType::Audio(legacy.buffer)),
            metadata: Default::default(),
        }),
        named_buffers: Default::default(),
        sequence: legacy.sequence,
        timestamp_ms: legacy.timestamp_ms,
    }
}
```

**Client Migration**:

```typescript
// BEFORE (Feature 003, still works)
const chunk: AudioChunk = {
  nodeId: "vad",
  buffer: audioBuffer,
  sequence: 42,
  timestampMs: 1000
};

// AFTER (Feature 004, recommended)
const chunk: DataChunk = {
  nodeId: "vad",
  buffer: {
    audio: audioBuffer,
    metadata: {}
  },
  sequence: 42,
  timestampMs: 1000
};
```

---

## Type Mappings by Language

### Rust

```rust
// Protobuf types (prost-generated)
pub struct DataBuffer {
    pub data_type: Option<data_buffer::DataType>,
    pub metadata: HashMap<String, String>,
}

pub mod data_buffer {
    pub enum DataType {
        Audio(AudioBuffer),
        Video(VideoFrame),
        Tensor(TensorBuffer),
        Json(JsonData),
        Text(TextBuffer),
        Binary(BinaryBuffer),
    }
}

// Runtime representation
pub enum RuntimeData {
    Audio(AudioBuffer),
    Video(VideoFrame),
    Tensor(TensorBuffer),
    Json(serde_json::Value),
    Text(String),
    Binary(Bytes),
}
```

### TypeScript

```typescript
// Discriminated union
type DataBuffer = {
  audio?: AudioBuffer;
  video?: VideoFrame;
  tensor?: TensorBuffer;
  json?: JsonData;
  text?: TextBuffer;
  binary?: BinaryBuffer;
  metadata: { [key: string]: string };
};

// Type guards
function isAudio(buf: DataBuffer): buf is { audio: AudioBuffer } {
  return buf.audio !== undefined;
}

function isVideo(buf: DataBuffer): buf is { video: VideoFrame } {
  return buf.video !== undefined;
}

// Usage with type narrowing
function processBuffer(buf: DataBuffer) {
  if (isAudio(buf)) {
    // buf.audio has type AudioBuffer
    console.log(`Audio: ${buf.audio.sampleRate} Hz`);
  } else if (isVideo(buf)) {
    // buf.video has type VideoFrame
    console.log(`Video: ${buf.video.width}x${buf.video.height}`);
  }
}
```

### Python

```python
# Type hints with discriminated union
from typing import Union, Dict
from remotemedia.proto import execution_pb2

DataBufferVariant = Union[
    execution_pb2.AudioBuffer,
    execution_pb2.VideoFrame,
    execution_pb2.TensorBuffer,
    execution_pb2.JsonData,
    execution_pb2.TextBuffer,
    execution_pb2.BinaryBuffer,
]

# Runtime type checking
def process_buffer(buf: execution_pb2.DataBuffer) -> None:
    if buf.HasField("audio"):
        audio: execution_pb2.AudioBuffer = buf.audio
        print(f"Audio: {audio.sample_rate} Hz")
    elif buf.HasField("video"):
        video: execution_pb2.VideoFrame = buf.video
        print(f"Video: {video.width}x{video.height}")
```

---

## Validation Rules Summary

### Manifest Validation (StreamInit)

1. All node IDs must be unique
2. All connection references must point to existing nodes
3. If nodes declare `input_types` and `output_types`, connections must be type-compatible
4. Graph must be acyclic (topological sort must succeed)
5. Streaming nodes (`is_streaming=true`) must be downstream of source nodes

### Chunk Validation (Runtime)

1. `DataChunk.node_id` must exist in manifest
2. Exactly one of `buffer` OR `named_buffers` must be set
3. `DataBuffer` must have exactly one `oneof` variant set
4. Sequence numbers must be strictly monotonic increasing
5. Timestamps should be non-decreasing (warnings for inversions)
6. If node declares `input_types`, chunk data type must match

### Data Type Validation

1. **AudioBuffer**: `samples.len() == num_samples * channels * format.bytes_per_sample()`
2. **VideoFrame**: `pixel_data.len() == width * height * format.bytes_per_pixel()`
3. **TensorBuffer**: `data.len() == shape.product() * dtype.bytes_per_element()`
4. **JsonData**: `json_payload` must parse as valid JSON
5. **TextBuffer**: `text_data` must be valid UTF-8
6. **BinaryBuffer**: No validation (accept any bytes)

---

## Performance Considerations

### Zero-Copy Paths

**Audio Processing** (maintain <5% overhead vs audio-only):

```rust
// Zero-copy for audio samples
impl RuntimeData {
    pub fn into_audio_bytes(self) -> Option<Bytes> {
        match self {
            RuntimeData::Audio(buf) => Some(Bytes::from(buf.samples)),
            _ => None,
        }
    }
}

// Reuse protobuf buffers
pub struct DataChunkPool {
    buffers: Vec<DataBuffer>,
}

impl DataChunkPool {
    pub fn get(&mut self) -> DataBuffer {
        self.buffers.pop().unwrap_or_default()
    }

    pub fn return_buf(&mut self, buf: DataBuffer) {
        self.buffers.push(buf);
    }
}
```

### Serialization Overhead Tracking

```rust
pub struct ExecutionMetrics {
    pub wall_time_ms: f64,
    pub cpu_time_ms: f64,
    pub memory_used_bytes: u64,

    // NEW: Track conversion overhead
    pub serialization_time_ms: f64,
    pub proto_to_runtime_ms: f64,
    pub runtime_to_proto_ms: f64,

    // NEW: Track type distribution
    pub data_type_breakdown: HashMap<DataTypeHint, u64>,
}
```

**Performance Targets**:
- Audio-only path: <5% overhead vs Feature 003 (SC-008)
- JSON parsing: <1ms for simple operations (SC-002)
- Video frame validation: <2ms for 1920x1080 frame
- Tensor validation: <1ms for embeddings (<1MB)

---

## Migration Checklist

### Server-Side

- [ ] Add `DataBuffer` message to protobuf
- [ ] Add all data type variants (Video, Tensor, Json, Text, Binary)
- [ ] Add `DataChunk` message with `named_buffers` support
- [ ] Update `StreamRequest` to include `DataChunk` variant
- [ ] Update `ExecuteRequest` to use `data_inputs` map
- [ ] Update `ExecutionResult` to use generic `data_outputs`
- [ ] Add `RuntimeData` enum to Rust executor
- [ ] Implement `convert_proto_to_runtime_data()`
- [ ] Implement `convert_runtime_to_proto_data()`
- [ ] Add backward compatibility shim for `AudioChunk`
- [ ] Update streaming handler to route both legacy and generic chunks
- [ ] Add `DataTypeHint` enum and validation functions
- [ ] Update metrics to use `total_items` instead of `total_samples`
- [ ] Add `data_type_breakdown` tracking

### Client-Side (TypeScript)

- [ ] Regenerate protobuf types with new `DataBuffer`
- [ ] Add `DataChunk` interface
- [ ] Update `streamPipeline()` to accept generic chunks
- [ ] Keep `streamAudioPipeline()` wrapper for compatibility
- [ ] Add type guards for `DataBuffer` variants
- [ ] Update examples to show generic usage
- [ ] Add migration guide

### Client-Side (Python)

- [ ] Regenerate protobuf types
- [ ] Add type hints for `DataBuffer` discriminated union
- [ ] Update streaming helpers
- [ ] Add examples for video, tensor, JSON

---

## References

- **Feature 003 Data Model**: `specs/003-rust-grpc-service/data-model.md` (baseline for audio-only protocol)
- **Research Decisions**: `specs/004-generic-streaming/research.md` (design rationale)
- **Protobuf Language Guide**: https://developers.google.com/protocol-buffers/docs/proto3
- **Rust prost**: https://docs.rs/prost/latest/prost/
- **TypeScript Discriminated Unions**: https://www.typescriptlang.org/docs/handbook/2/narrowing.html
