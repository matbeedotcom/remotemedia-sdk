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

## Using Transports as a Library

Each transport crate provides a **builder API** so you can add transport support to your own application with minimal code. Add the crate as a dependency and use the builder:

### gRPC

```toml
[dependencies]
remotemedia-grpc = { path = "crates/transports/grpc" }
```

```rust
use remotemedia_grpc::GrpcServerBuilder;
use remotemedia_core::transport::PipelineExecutor;
use std::sync::Arc;

let executor = Arc::new(PipelineExecutor::new()?);
GrpcServerBuilder::new()
    .bind("0.0.0.0:50051")
    .executor(executor)
    .auth_tokens(vec!["my-token".into()])
    .build()?
    .run()
    .await?;
```

Or configure entirely from environment variables:

```rust
GrpcServerBuilder::new()
    .from_env()  // reads GRPC_BIND_ADDRESS, GRPC_AUTH_TOKENS, etc.
    .build()?
    .run()
    .await?;
```

### HTTP

```toml
[dependencies]
remotemedia-http = { path = "crates/transports/http" }
```

```rust
use remotemedia_http::HttpServerBuilder;

HttpServerBuilder::new()
    .bind("0.0.0.0:8080")
    .build().await?
    .run().await?;
```

### WebRTC

```toml
[dependencies]
remotemedia-webrtc = { path = "crates/transports/webrtc" }
```

**WebSocket client mode** (connects to an existing signaling server):

```rust
use remotemedia_webrtc::WebRtcServerBuilder;

WebRtcServerBuilder::new()
    .signaling_url("ws://localhost:8080")
    .stun_servers(vec!["stun:stun.l.google.com:19302".into()])
    .max_peers(10)
    .build()?
    .run()
    .await?;
```

**gRPC signaling server mode** (requires `grpc-signaling` feature):

```toml
[dependencies]
remotemedia-webrtc = { path = "crates/transports/webrtc", features = ["grpc-signaling"] }
```

```rust
use remotemedia_webrtc::WebRtcSignalingServerBuilder;

WebRtcSignalingServerBuilder::new()
    .bind("0.0.0.0:50051")
    .manifest_from_file("pipeline.json")?
    .build()?
    .run()
    .await?;
```

### CLI Integration

Each transport also provides optional **clap argument structs** behind a `cli` feature, so you can embed transport-specific CLI args in your own clap-based application:

```toml
[dependencies]
remotemedia-grpc = { path = "crates/transports/grpc", features = ["cli"] }
```

```rust
use clap::Parser;
use remotemedia_grpc::GrpcServeArgs;

#[derive(Parser)]
struct MyCli {
    #[command(flatten)]
    grpc: GrpcServeArgs,
}

let cli = MyCli::parse();
cli.grpc.run().await?;
```

## Architecture

Each transport is an independent **library** crate that:
1. Depends on `remotemedia-core`
2. Implements the `PipelineTransport` trait
3. Provides a builder API for easy integration
4. Optionally provides clap CLI args (behind `cli` feature)
5. Handles its own serialization format
6. Can be independently versioned and deployed

Server binaries in `crates/services/` depend on these transport libraries and use the builder API.

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
