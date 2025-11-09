# RemoteMedia SDK - Quick Build Reference

Fast reference for common build commands.

## Core Library (runtime-core)

```bash
# Default (multiprocess + VAD)
cargo build -p remotemedia-runtime-core

# Minimal (no optional features)
cargo build -p remotemedia-runtime-core --no-default-features

# Only multiprocess
cargo build -p remotemedia-runtime-core --no-default-features --features multiprocess

# Only VAD
cargo build -p remotemedia-runtime-core --no-default-features --features silero-vad
```

## gRPC Transport

```bash
# Library only (no server binary)
cargo build -p remotemedia-grpc --no-default-features

# Server (default) - includes grpc-server binary
cargo build -p remotemedia-grpc

# Server + WebRTC signaling
cargo build -p remotemedia-grpc --features webrtc-signaling

# All features
cargo build -p remotemedia-grpc --features full
```

## Run gRPC Server

```bash
# Default server (pipeline execution + streaming)
cargo run --bin grpc-server --release

# With WebRTC signaling support
cargo run --bin grpc-server --release --features webrtc-signaling

# Custom configuration
GRPC_BIND_ADDRESS="0.0.0.0:50051" \
GRPC_REQUIRE_AUTH=false \
RUST_LOG=debug \
cargo run --bin grpc-server --release
```

## WebRTC Transport (when available)

```bash
# Minimal (signaling only)
cargo build -p remotemedia-webrtc --features signaling-client

# Audio only
cargo build -p remotemedia-webrtc --features audio-codecs,signaling-client

# Video only
cargo build -p remotemedia-webrtc --features video-codecs,signaling-client

# Full WebRTC
cargo build -p remotemedia-webrtc --features full
```

## Python FFI Transport

```bash
# Python extension module
cargo build -p remotemedia-ffi --release --features extension-module

# Library with Python bindings
cargo build -p remotemedia-ffi --features python-bindings

# Library only (no Python)
cargo build -p remotemedia-ffi --no-default-features
```

## Common Workflows

### Development (fast compilation)

```bash
cargo build -p remotemedia-grpc
cargo test -p remotemedia-grpc
```

### Production (optimized)

```bash
cargo build -p remotemedia-grpc --release
```

### Testing

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p remotemedia-grpc

# With features
cargo test -p remotemedia-grpc --features webrtc-signaling

# With logging
RUST_LOG=debug cargo test -p remotemedia-grpc -- --nocapture
```

### Clean Build

```bash
cargo clean
cargo build -p remotemedia-grpc --release
```

## Feature Matrix

| Crate | Feature | Includes |
|-------|---------|----------|
| **runtime-core** | `multiprocess` | iceoryx2 IPC |
| **runtime-core** | `silero-vad` | ONNX Runtime VAD |
| **remotemedia-grpc** | `server` | grpc-server binary deps |
| **remotemedia-grpc** | `webrtc-signaling` | WebRTC signaling service |
| **remotemedia-grpc** | `full` | All features |
| **remotemedia-ffi** | `python-bindings` | PyO3 + numpy |
| **remotemedia-ffi** | `extension-module` | Python package |
| **remotemedia-webrtc** | `audio-codecs` | Opus |
| **remotemedia-webrtc** | `video-codecs` | VP9, H.264 |
| **remotemedia-webrtc** | `signaling-client` | WebSocket |
| **remotemedia-webrtc** | `full` | All WebRTC features |

## Size Comparison

| Build | Approximate Size |
|-------|------------------|
| runtime-core (minimal) | ~5-10 MB |
| remotemedia-grpc (lib only) | ~10-15 MB |
| remotemedia-grpc (server) | ~15-20 MB |
| remotemedia-grpc (server + webrtc-signaling) | ~20-25 MB |
| remotemedia-ffi (extension) | ~10-15 MB |
| remotemedia-webrtc (full) | ~25-35 MB |
| Entire workspace (all features) | ~50-80 MB |

---

For detailed documentation, see [BUILD_CONFIGURATION.md](BUILD_CONFIGURATION.md)
