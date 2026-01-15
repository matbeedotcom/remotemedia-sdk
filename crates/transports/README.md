# RemoteMedia Transports

This directory contains transport **library** crates that depend on `remotemedia-core`.

## Available Transports

- **remotemedia-grpc**: gRPC transport library for remote pipeline execution
- **remotemedia-http**: HTTP/REST transport library with SSE streaming
- **remotemedia-ffi**: Python FFI transport for Python SDK integration
- **remotemedia-webrtc**: WebRTC transport library for real-time media streaming

## Server Binaries

Server binaries are located in `crates/services/`:

```bash
# gRPC server
cargo run -p remotemedia-grpc-server

# HTTP server
cargo run -p remotemedia-http-server

# WebRTC server
cargo run -p remotemedia-webrtc-server
```

## Architecture

Each transport is an independent **library** crate that:
1. Depends on `remotemedia-core`
2. Implements the `PipelineTransport` trait
3. Handles its own serialization format
4. Can be independently versioned and deployed

Server binaries in `crates/services/` depend on these transport libraries and provide CLI entry points.

See `docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md` for details.

## Validation Error Format

All transports provide consistent validation error reporting when node parameters fail schema validation. The runtime validates parameters against JSON Schema definitions before execution.

### Error Structure

Each validation error contains:

| Field | Type | Description |
|-------|------|-------------|
| `node_id` | string | Node ID from the manifest |
| `node_type` | string | Node type (e.g., "SileroVAD") |
| `path` | string | JSON pointer to invalid parameter (e.g., "/threshold") |
| `constraint` | string | Constraint that was violated |
| `expected` | string | Expected value description |
| `received` | string | Actual value received |
| `message` | string | Human-readable error message |

### Constraint Types

- `type` - Value has wrong JSON type (e.g., string instead of number)
- `required` - Required parameter is missing
- `minimum` / `maximum` - Numeric value out of range
- `exclusive_minimum` / `exclusive_maximum` - Exclusive range bounds
- `enum` - Value not in allowed set
- `pattern` - String doesn't match regex pattern
- `min_length` / `max_length` - String length out of range
- `min_items` / `max_items` - Array length out of range

### Transport-Specific Behavior

#### gRPC

Validation errors return `ErrorType::Validation` (value 1) with:
- `message`: JSON array of `ValidationError` objects
- `context`: Human-readable summary (e.g., "3 validation error(s) in node parameters")

```protobuf
message ErrorResponse {
  ErrorType error_type = 1;  // Validation = 1
  string message = 2;        // JSON array of errors
  string context = 4;        // Summary message
}
```

#### HTTP

Validation errors return HTTP 400 Bad Request with JSON body:

```json
{
  "error_type": "validation",
  "message": "3 validation error(s) in node parameters",
  "validation_errors": [
    {
      "node_id": "vad",
      "node_type": "SileroVAD",
      "path": "/threshold",
      "constraint": "maximum",
      "expected": "1.0",
      "received": "1.5",
      "message": "Node 'vad' (SileroVAD): parameter 'threshold' must be <= 1.0, got 1.5"
    }
  ]
}
```

#### Python FFI

Validation errors raise `ValueError` with formatted message:

```python
try:
    await execute_pipeline(manifest)
except ValueError as e:
    # e.args[0] contains:
    # "Parameter validation failed (3 error(s)):
    # [
    #   {
    #     "node_id": "vad",
    #     "node_type": "SileroVAD",
    #     ...
    #   }
    # ]"
```

### Example Error Messages

Type mismatch:
```
Node 'vad_node' (SileroVAD): parameter 'threshold' expected type 'number', got "high"
```

Missing required parameter:
```
Node 'tts' (KokoroTTSNode): required parameter 'voice' is missing
```

Range violation:
```
Node 'vad' (SileroVAD): parameter 'threshold' must be <= 1.0, got 1.5
```

Enum violation:
```
Node 'encoder' (AudioEncoder): parameter 'format' must be one of [wav, mp3, flac], got "invalid"
```
