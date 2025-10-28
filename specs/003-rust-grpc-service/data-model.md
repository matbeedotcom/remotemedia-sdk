# Data Model: Rust gRPC Service Protocol Buffers

**Feature**: `003-rust-grpc-service` | **Phase**: 1 - Design  
**Generated**: 2025-10-28

This document defines all protocol buffer message types used in the Rust gRPC service for remote pipeline execution. These types enable cross-language communication between clients (Python, TypeScript) and the Rust service.

## Overview

The gRPC service uses protocol buffers (proto3) for efficient serialization of audio data, pipeline manifests, execution results, and metadata. The data model is organized into four categories:

1. **Input Types**: Client requests (pipeline manifest, audio data, configuration)
2. **Output Types**: Service responses (execution results, audio data, metrics)
3. **Shared Types**: Common structures used in both directions (audio buffers, errors, metadata)
4. **Control Types**: Service management (version info, health checks, resource limits)

## Design Principles

- **Zero-Copy Where Possible**: Audio samples use `bytes` fields to avoid unnecessary copying during serialization
- **Backwards Compatibility**: All optional fields use explicit field numbers, enabling protocol evolution
- **Language Neutral**: Types map cleanly to Python, Rust, and TypeScript native structures
- **Metrics First**: Execution results always include performance metrics for observability

## Message Type Reference

### 1. PipelineManifest

**Purpose**: Represents a complete audio processing pipeline specification, compatible with Rust runtime v0.2.1.

**Source**: Based on `runtime/src/manifest/mod.rs::Manifest`

**Fields**:

```protobuf
message PipelineManifest {
  // Schema version (e.g., "v1")
  string version = 1;
  
  // Pipeline metadata
  ManifestMetadata metadata = 2;
  
  // List of processing nodes
  repeated NodeManifest nodes = 3;
  
  // Connections between nodes (edges in DAG)
  repeated Connection connections = 4;
}
```

**Field Details**:

- `version` (string, required): Schema version identifier. Service validates this matches supported versions ("v1" in initial release).
- `metadata` (ManifestMetadata, required): Human-readable pipeline information (name, description, timestamps).
- `nodes` (repeated NodeManifest, required): Processing nodes in the pipeline. Must contain at least one node. Node IDs must be unique.
- `connections` (repeated Connection, required): Directed edges connecting node outputs to inputs. Forms a directed acyclic graph (DAG).

**Validation Rules**:
- `version` must be "v1" (strict validation)
- `nodes` must not be empty
- All node IDs must be unique
- All connection references must point to existing node IDs
- Graph must be acyclic (topological sort must succeed)

---

### 2. ManifestMetadata

**Purpose**: Descriptive information about a pipeline for logging and debugging.

**Fields**:

```protobuf
message ManifestMetadata {
  // Pipeline name (required)
  string name = 1;
  
  // Optional human-readable description
  string description = 2;
  
  // ISO 8601 timestamp of creation (optional)
  string created_at = 3;
}
```

**Field Details**:

- `name` (string, required): Unique identifier for the pipeline (used in logs and metrics).
- `description` (string, optional): Human-readable description of pipeline purpose.
- `created_at` (string, optional): ISO 8601 timestamp (e.g., "2025-10-28T10:30:00Z").

---

### 3. NodeManifest

**Purpose**: Defines a single processing node in the pipeline with its type, parameters, and execution hints.

**Source**: Based on `runtime/src/manifest/mod.rs::NodeManifest`

**Fields**:

```protobuf
message NodeManifest {
  // Unique node ID within pipeline
  string id = 1;
  
  // Node type (e.g., "AudioResample", "VAD", "HFPipelineNode")
  string node_type = 2;
  
  // Node-specific parameters (JSON encoded)
  string params = 3;
  
  // Whether node uses streaming (async generator)
  bool is_streaming = 4;
  
  // Optional capability requirements (GPU, CPU, memory)
  CapabilityRequirements capabilities = 5;
  
  // Optional execution host preference
  string host = 6;
  
  // Optional runtime hint for Python nodes
  RuntimeHint runtime_hint = 7;
}
```

**Field Details**:

- `id` (string, required): Unique identifier for this node within the pipeline.
- `node_type` (string, required): Node class name (e.g., "AudioResample", "VAD"). Service validates against registered node types.
- `params` (string, required): JSON-encoded node parameters. Service deserializes into `serde_json::Value`.
- `is_streaming` (bool, optional, default=false): If true, node's `process()` method is an async generator.
- `capabilities` (CapabilityRequirements, optional): Hardware requirements (GPU, CPU, memory).
- `host` (string, optional): Preferred execution host (reserved for future multi-instance deployments).
- `runtime_hint` (RuntimeHint, optional): Python runtime selection (RustPython vs CPython).

**Example**:

```json
{
  "id": "resample",
  "node_type": "AudioResample",
  "params": "{\"target_sample_rate\": 16000}",
  "is_streaming": false
}
```

---

### 4. Connection

**Purpose**: Represents a directed edge in the pipeline DAG from one node's output to another node's input.

**Fields**:

```protobuf
message Connection {
  // Source node ID
  string from = 1;
  
  // Target node ID
  string to = 2;
}
```

**Field Details**:

- `from` (string, required): ID of the node producing output.
- `to` (string, required): ID of the node consuming input.

**Validation Rules**:
- Both `from` and `to` must reference existing node IDs
- No self-loops allowed (`from` != `to`)
- No cycles in the connection graph

---

### 5. CapabilityRequirements

**Purpose**: Specifies hardware/resource requirements for node execution.

**Source**: Based on `runtime/src/manifest/mod.rs::CapabilityRequirements`

**Fields**:

```protobuf
message CapabilityRequirements {
  // GPU requirements
  GpuRequirement gpu = 1;
  
  // CPU requirements
  CpuRequirement cpu = 2;
  
  // Memory requirement (gigabytes)
  double memory_gb = 3;
}
```

**Field Details**:

- `gpu` (GpuRequirement, optional): GPU type and memory requirements.
- `cpu` (CpuRequirement, optional): CPU cores and architecture preferences.
- `memory_gb` (double, optional): Minimum required memory in gigabytes.

---

### 6. GpuRequirement

**Purpose**: Specifies GPU hardware requirements for ML inference nodes.

**Fields**:

```protobuf
message GpuRequirement {
  // GPU type: "cuda", "rocm", "metal"
  string type = 1;
  
  // Minimum GPU memory (GB)
  double min_memory_gb = 2;
  
  // Whether GPU is required or optional
  bool required = 3;
}
```

---

### 7. CpuRequirement

**Purpose**: Specifies CPU requirements for compute-intensive nodes.

**Fields**:

```protobuf
message CpuRequirement {
  // Minimum number of cores
  uint32 cores = 1;
  
  // CPU architecture preference ("x86_64", "aarch64")
  string arch = 2;
}
```

---

### 8. RuntimeHint

**Purpose**: Specifies which Python runtime to use for executing Python nodes.

**Source**: Based on `runtime/src/manifest/mod.rs::RuntimeHint`

**Fields**:

```protobuf
enum RuntimeHint {
  RUNTIME_HINT_UNSPECIFIED = 0;
  RUNTIME_HINT_RUSTPYTHON = 1;  // Pure Rust, limited stdlib
  RUNTIME_HINT_CPYTHON = 2;     // Full Python via PyO3
  RUNTIME_HINT_CPYTHON_WASM = 3; // Sandboxed WASM (future)
  RUNTIME_HINT_AUTO = 4;        // Automatic selection
}
```

---

### 9. AudioBuffer

**Purpose**: Represents multi-channel audio data with metadata. Optimized for efficient serialization.

**Source**: Based on `runtime/src/audio/mod.rs::AudioBuffer`

**Fields**:

```protobuf
message AudioBuffer {
  // Raw audio samples (interleaved, little-endian)
  bytes samples = 1;
  
  // Sample rate in Hz (e.g., 16000, 44100, 48000)
  uint32 sample_rate = 2;
  
  // Number of channels (1=mono, 2=stereo)
  uint32 channels = 3;
  
  // Audio format
  AudioFormat format = 4;
  
  // Total number of samples (including all channels)
  uint64 num_samples = 5;
}
```

**Field Details**:

- `samples` (bytes, required): Raw audio data. Format determined by `format` field. For stereo, samples are interleaved (L, R, L, R, ...).
- `sample_rate` (uint32, required): Sample rate in Hz. Common values: 8000, 16000, 22050, 44100, 48000.
- `channels` (uint32, required): Number of audio channels (1=mono, 2=stereo, 6=5.1 surround).
- `format` (AudioFormat, required): Sample encoding format (F32, I16, I32).
- `num_samples` (uint64, required): Total number of samples. For stereo with 1000 frames, this is 2000.

**Calculation**:
- `num_frames = num_samples / channels`
- `duration_seconds = num_frames / sample_rate`
- `bytes.len() = num_samples * format.bytes_per_sample()`

**Example** (1-second mono 16kHz F32 audio):
```protobuf
AudioBuffer {
  samples: <16000 float32 samples, 64KB>,
  sample_rate: 16000,
  channels: 1,
  format: AUDIO_FORMAT_F32,
  num_samples: 16000
}
```

---

### 10. AudioFormat

**Purpose**: Specifies the encoding format of audio samples.

**Source**: Based on `runtime/src/audio/mod.rs::AudioFormat`

**Fields**:

```protobuf
enum AudioFormat {
  AUDIO_FORMAT_UNSPECIFIED = 0;
  AUDIO_FORMAT_F32 = 1;  // 32-bit float, range [-1.0, 1.0]
  AUDIO_FORMAT_I16 = 2;  // 16-bit signed int, range [-32768, 32767]
  AUDIO_FORMAT_I32 = 3;  // 32-bit signed int
}
```

**Bytes Per Sample**:
- `AUDIO_FORMAT_F32`: 4 bytes
- `AUDIO_FORMAT_I16`: 2 bytes
- `AUDIO_FORMAT_I32`: 4 bytes

---

### 11. ExecutionResult

**Purpose**: Contains all outputs from a successful pipeline execution, including processed audio and metrics.

**Fields**:

```protobuf
message ExecutionResult {
  // Processed audio outputs (keyed by node ID)
  map<string, AudioBuffer> audio_outputs = 1;
  
  // Non-audio outputs (JSON encoded, keyed by node ID)
  map<string, string> data_outputs = 2;
  
  // Execution performance metrics
  ExecutionMetrics metrics = 3;
  
  // Per-node execution results
  repeated NodeResult node_results = 4;
  
  // Pipeline completion status
  ExecutionStatus status = 5;
}
```

**Field Details**:

- `audio_outputs` (map<string, AudioBuffer>, required): Audio outputs keyed by node ID. Empty for pipelines with no audio outputs.
- `data_outputs` (map<string, string>, required): Non-audio outputs (JSON-encoded) keyed by node ID. Used for feature vectors, transcriptions, etc.
- `metrics` (ExecutionMetrics, required): Overall pipeline performance metrics.
- `node_results` (repeated NodeResult, required): Per-node execution details for debugging and profiling.
- `status` (ExecutionStatus, required): Overall execution status (success, partial success, failure).

---

### 12. ExecutionMetrics

**Purpose**: Performance measurements for pipeline execution.

**Fields**:

```protobuf
message ExecutionMetrics {
  // Wall-clock time (milliseconds)
  double wall_time_ms = 1;
  
  // CPU time (milliseconds)
  double cpu_time_ms = 2;
  
  // Peak memory usage (bytes)
  uint64 memory_used_bytes = 3;
  
  // Per-node execution statistics
  map<string, NodeMetrics> node_metrics = 4;
  
  // Serialization overhead (milliseconds)
  double serialization_time_ms = 5;
}
```

**Field Details**:

- `wall_time_ms` (double, required): Total elapsed time from request receipt to response ready.
- `cpu_time_ms` (double, required): Total CPU time consumed by all threads.
- `memory_used_bytes` (uint64, required): Peak memory usage during execution.
- `node_metrics` (map<string, NodeMetrics>, required): Per-node metrics keyed by node ID.
- `serialization_time_ms` (double, required): Time spent serializing/deserializing audio data and protobuf messages.

**Performance Targets**:
- `wall_time_ms < 5ms` for simple operations (FR-001, SC-001)
- `serialization_time_ms < 10%` of `wall_time_ms` (SC-003)

---

### 13. NodeMetrics

**Purpose**: Performance metrics for a single node execution.

**Fields**:

```protobuf
message NodeMetrics {
  // Node execution time (milliseconds)
  double execution_time_ms = 1;
  
  // Memory allocated by this node (bytes)
  uint64 memory_bytes = 2;
  
  // Number of samples processed
  uint64 samples_processed = 3;
  
  // Node-specific metrics (JSON encoded)
  string custom_metrics = 4;
}
```

**Field Details**:

- `execution_time_ms` (double, required): Time spent executing this node's `process()` method.
- `memory_bytes` (uint64, required): Memory allocated by this node.
- `samples_processed` (uint64, required): Total audio samples processed (across all channels).
- `custom_metrics` (string, optional): Node-specific metrics as JSON (e.g., `{"vad_segments": 12}`).

---

### 14. NodeResult

**Purpose**: Execution details for a single node, including outputs and errors.

**Fields**:

```protobuf
message NodeResult {
  // Node ID
  string node_id = 1;
  
  // Execution status
  NodeStatus status = 2;
  
  // Error details (if status != SUCCESS)
  ErrorResponse error = 3;
  
  // Node-specific output metadata
  string output_metadata = 4;
}
```

**Field Details**:

- `node_id` (string, required): Node identifier from manifest.
- `status` (NodeStatus, required): Success, skipped, or failed.
- `error` (ErrorResponse, optional): Error details if node failed.
- `output_metadata` (string, optional): JSON-encoded node-specific metadata.

---

### 15. NodeStatus

**Purpose**: Execution status for a single node.

**Fields**:

```protobuf
enum NodeStatus {
  NODE_STATUS_UNSPECIFIED = 0;
  NODE_STATUS_SUCCESS = 1;    // Node executed successfully
  NODE_STATUS_SKIPPED = 2;    // Node skipped (conditional execution)
  NODE_STATUS_FAILED = 3;     // Node execution failed
}
```

---

### 16. ExecutionStatus

**Purpose**: Overall pipeline execution status.

**Fields**:

```protobuf
enum ExecutionStatus {
  EXECUTION_STATUS_UNSPECIFIED = 0;
  EXECUTION_STATUS_SUCCESS = 1;         // All nodes succeeded
  EXECUTION_STATUS_PARTIAL_SUCCESS = 2; // Some nodes skipped but pipeline completed
  EXECUTION_STATUS_FAILED = 3;          // Pipeline execution failed
}
```

---

### 17. ErrorResponse

**Purpose**: Structured error information for debugging and diagnostics.

**Fields**:

```protobuf
message ErrorResponse {
  // Error category
  ErrorType error_type = 1;
  
  // Human-readable error message
  string message = 2;
  
  // Failing node ID (if applicable)
  string failing_node_id = 3;
  
  // Execution context at time of error (JSON)
  string context = 4;
  
  // Stack trace (if available)
  string stack_trace = 5;
}
```

**Field Details**:

- `error_type` (ErrorType, required): Error category for programmatic handling.
- `message` (string, required): Human-readable error description.
- `failing_node_id` (string, optional): Node ID where error occurred (empty for manifest validation errors).
- `context` (string, optional): JSON-encoded execution context (inputs, parameters, state).
- `stack_trace` (string, optional): Rust panic stack trace (if available).

**Example**:

```json
{
  "error_type": "ERROR_TYPE_NODE_EXECUTION",
  "message": "AudioResample node failed: invalid target sample rate",
  "failing_node_id": "resample",
  "context": "{\"input_sample_rate\": 44100, \"target_sample_rate\": -1}"
}
```

---

### 18. ErrorType

**Purpose**: Categorizes errors for client-side error handling.

**Fields**:

```protobuf
enum ErrorType {
  ERROR_TYPE_UNSPECIFIED = 0;
  ERROR_TYPE_VALIDATION = 1;       // Manifest validation error
  ERROR_TYPE_NODE_EXECUTION = 2;   // Node execution failure
  ERROR_TYPE_RESOURCE_LIMIT = 3;   // Resource limit exceeded
  ERROR_TYPE_AUTHENTICATION = 4;   // Auth token invalid/missing
  ERROR_TYPE_VERSION_MISMATCH = 5; // Protocol version incompatible
  ERROR_TYPE_INTERNAL = 6;         // Service internal error
}
```

**Client Handling**:
- `VALIDATION`: Fix manifest and retry
- `NODE_EXECUTION`: Check node parameters and input data
- `RESOURCE_LIMIT`: Reduce pipeline complexity or request higher limits
- `AUTHENTICATION`: Check API token configuration
- `VERSION_MISMATCH`: Upgrade client library
- `INTERNAL`: Retry with exponential backoff, contact support if persistent

---

### 19. ResourceLimits

**Purpose**: Configurable resource constraints for pipeline execution.

**Fields**:

```protobuf
message ResourceLimits {
  // Maximum memory allocation (bytes)
  uint64 max_memory_bytes = 1;
  
  // Maximum execution timeout (milliseconds)
  uint64 max_timeout_ms = 2;
  
  // Maximum audio buffer size (samples)
  uint64 max_audio_samples = 3;
}
```

**Field Details**:

- `max_memory_bytes` (uint64, optional): Memory limit for this pipeline. Service enforces caps (default: 100MB, max: 1GB).
- `max_timeout_ms` (uint64, optional): Execution timeout. Service enforces maximum (default: 5000ms, max: 30000ms).
- `max_audio_samples` (uint64, optional): Maximum audio buffer size. Prevents OOM attacks (default: 10M samples = ~200MB stereo F32).

**Usage**: Clients can request custom limits via `ExecuteRequest.resource_limits`. Service validates against configured maximums and applies defaults if not specified.

---

### 20. VersionInfo

**Purpose**: Service version and protocol compatibility information.

**Fields**:

```protobuf
message VersionInfo {
  // Protocol version (e.g., "v1")
  string protocol_version = 1;
  
  // Service runtime version (e.g., "0.2.1")
  string runtime_version = 2;
  
  // List of supported node types
  repeated string supported_node_types = 3;
  
  // Supported protocol versions (for compatibility)
  repeated string supported_protocols = 4;
  
  // Service build timestamp
  string build_timestamp = 5;
}
```

**Field Details**:

- `protocol_version` (string, required): Current protocol version (e.g., "v1").
- `runtime_version` (string, required): Rust runtime version (e.g., "0.2.1").
- `supported_node_types` (repeated string, required): List of registered node types this service can execute.
- `supported_protocols` (repeated string, required): All protocol versions this service supports (e.g., ["v1"] initially, may expand to ["v1", "v2"]).
- `build_timestamp` (string, required): ISO 8601 timestamp of service build.

**Usage**: Clients call `GetVersion()` RPC during connection initialization to verify compatibility. Service responds with this message.

---

## RPC Method Signatures

### ExecutePipeline (Unary)

**Request**:

```protobuf
message ExecuteRequest {
  // Pipeline manifest
  PipelineManifest manifest = 1;
  
  // Input audio buffers (keyed by node ID)
  map<string, AudioBuffer> audio_inputs = 2;
  
  // Non-audio inputs (JSON, keyed by node ID)
  map<string, string> data_inputs = 3;
  
  // Optional resource limits
  ResourceLimits resource_limits = 4;
  
  // Client protocol version
  string client_version = 5;
}
```

**Response**:

```protobuf
message ExecuteResponse {
  // Either result or error (oneof)
  oneof outcome {
    ExecutionResult result = 1;
    ErrorResponse error = 2;
  }
}
```

---

### StreamPipeline (Bidirectional Streaming)

**Client Stream**:

```protobuf
message StreamRequest {
  // First message: pipeline setup
  oneof request {
    PipelineManifest manifest = 1;
    AudioChunk audio_chunk = 2;
    StreamControl control = 3;
  }
}

message AudioChunk {
  // Node ID to send this chunk to
  string node_id = 1;
  
  // Audio data
  AudioBuffer buffer = 2;
  
  // Sequence number for ordering
  uint64 sequence = 3;
}

message StreamControl {
  enum Command {
    COMMAND_UNSPECIFIED = 0;
    COMMAND_CLOSE = 1;     // Graceful close
    COMMAND_CANCEL = 2;    // Abort execution
  }
  Command command = 1;
}
```

**Server Stream**:

```protobuf
message StreamResponse {
  oneof response {
    StreamReady ready = 1;           // Pipeline initialized
    ExecutionResult result = 2;      // Processed chunk result
    ErrorResponse error = 3;         // Execution error
    StreamMetrics metrics = 4;       // Periodic metrics update
  }
}

message StreamReady {
  string session_id = 1;
}

message StreamMetrics {
  uint64 chunks_processed = 1;
  double average_latency_ms = 2;
  uint64 total_samples = 3;
}
```

---

### GetVersion (Unary)

**Request**:

```protobuf
message VersionRequest {
  // Client version for compatibility check
  string client_version = 1;
}
```

**Response**:

```protobuf
message VersionResponse {
  VersionInfo version_info = 1;
  bool compatible = 2;           // Is client version compatible?
  string compatibility_message = 3; // Details if incompatible
}
```

---

## Type Mappings

### Rust

```rust
// Generated by prost-build in build.rs
pub struct PipelineManifest { ... }
pub struct AudioBuffer { ... }
pub struct ExecutionResult { ... }
pub enum AudioFormat { ... }
```

### Python

```python
# Generated by grpcio-tools
from remotemedia.proto import execution_pb2

manifest = execution_pb2.PipelineManifest(
    version="v1",
    metadata=execution_pb2.ManifestMetadata(name="test")
)
```

### TypeScript

```typescript
// Generated by grpc-tools
import { PipelineManifest, AudioBuffer } from './generated/execution_pb';

const manifest = new PipelineManifest();
manifest.setVersion('v1');
```

---

## Versioning Strategy

**Protocol Version**: Included in every request (`client_version` field) and response (`protocol_version` field).

**Compatibility Rules**:
1. Service maintains compatibility matrix mapping client versions to service versions
2. Service validates `client_version` during request processing
3. Clients should call `GetVersion()` on connection initialization
4. Breaking changes require new protocol version (v2, v3, etc.)
5. Non-breaking changes preserve existing field numbers

**Example Compatibility Matrix**:

| Client Version | Service Version | Compatible | Notes |
|---------------|----------------|------------|-------|
| v1.0.0 | v0.2.1 | ✅ | Initial release |
| v1.0.0 | v0.3.0 | ✅ | Backward compatible |
| v2.0.0 | v0.2.1 | ❌ | Requires service upgrade |

---

## Serialization Overhead Analysis

**Target**: <10% overhead vs local execution (SC-003)

**Overhead Sources**:
1. Protobuf serialization (audio buffers): ~2-3% for large buffers
2. gRPC metadata encoding: <1%
3. Network transmission: Variable (not counted in serialization overhead)

**Optimization Strategies**:
- Use `bytes` fields for audio samples (zero-copy where possible)
- Avoid repeated serialization of large static data (e.g., node registries)
- Compress audio data for network transmission (optional, client-configured)

**Measurement**: `ExecutionMetrics.serialization_time_ms` tracks time spent in protobuf encode/decode.

---

## References

- **Rust Runtime Types**: `runtime/src/manifest/mod.rs`, `runtime/src/audio/mod.rs`
- **Performance Targets**: `specs/003-rust-grpc-service/spec.md` (SC-001, SC-003, SC-004)
- **Protocol Evolution**: Version negotiation ensures backward compatibility (FR-015, FR-016)
