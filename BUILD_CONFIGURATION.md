# RemoteMedia SDK - Build Configuration Guide

This document explains the modular build system for RemoteMedia SDK, allowing you to compile only the components you need.

## Architecture Overview

RemoteMedia SDK follows a **modular transport architecture**:

```
┌─────────────────────────────────────────────────────┐
│  runtime-core (required)                            │
│  - Transport-agnostic pipeline execution            │
│  - Node registry and executor                       │
│  - RuntimeData types                                │
│  - Manifest parsing and validation                  │
└─────────────────────────────────────────────────────┘
                      ↑
                      │
        ┌─────────────┼─────────────┬─────────────┐
        │             │             │             │
┌───────┴──────┐ ┌────┴────┐ ┌─────┴──────┐ ┌────┴────┐
│ grpc         │ │ ffi     │ │ webrtc     │ │ runtime │
│ transport    │ │ transport│ │ transport  │ │ (legacy)│
│ (optional)   │ │(optional)│ │ (optional) │ │         │
└──────────────┘ └──────────┘ └────────────┘ └─────────┘
```

Each transport is **completely optional** and can be included/excluded at compile time.

---

## Feature Flags by Crate

### 1. runtime-core (Transport-Agnostic Core)

**Location**: `runtime-core/Cargo.toml`

**Features**:
```toml
default = ["multiprocess", "silero-vad"]

# IPC for multiprocess Python nodes
multiprocess = ["iceoryx2", "iceoryx2-bb-log", "iceoryx2-bb-container", "iceoryx2-bb-elementary"]

# Silero VAD using ONNX Runtime
silero-vad = ["ort"]
```

**Build Examples**:
```bash
# Default build (multiprocess + silero-vad)
cargo build -p remotemedia-runtime-core

# Minimal build (no optional features)
cargo build -p remotemedia-runtime-core --no-default-features

# With only multiprocess support
cargo build -p remotemedia-runtime-core --no-default-features --features multiprocess

# With only VAD support
cargo build -p remotemedia-runtime-core --no-default-features --features silero-vad
```

---

### 2. remotemedia-grpc (gRPC Transport)

**Location**: `transports/remotemedia-grpc/Cargo.toml`

**Features**:
```toml
default = ["server"]

# Server feature: includes dependencies for grpc-server binary
server = ["ctrlc", "num_cpus"]

# WebRTC signaling service (gRPC-based signaling)
webrtc-signaling = []

# Full feature: everything enabled
full = ["server", "webrtc-signaling"]
```

**Build Examples**:
```bash
# Library only (for embedding in applications)
cargo build -p remotemedia-grpc --no-default-features

# Library with WebRTC signaling support
cargo build -p remotemedia-grpc --no-default-features --features webrtc-signaling

# Full server (default)
cargo build -p remotemedia-grpc

# Full server with WebRTC signaling
cargo build -p remotemedia-grpc --features webrtc-signaling

# Run server binary
cargo run --bin grpc-server --release
```

**Use Cases**:
- **`--no-default-features`**: Embed gRPC client/services in your own application
- **`server`**: Run standalone gRPC server binary
- **`webrtc-signaling`**: Add gRPC-based WebRTC signaling to server
- **`full`**: Enable all gRPC features

---

### 3. remotemedia-ffi (Python FFI Transport)

**Location**: `transports/remotemedia-ffi/Cargo.toml`

**Features**:
```toml
default = ["python-bindings"]

# Python FFI bindings (PyO3)
python-bindings = ["pyo3", "pyo3-async-runtimes", "numpy"]

# Extension module (cdylib for Python import)
extension-module = ["python-bindings", "pyo3/extension-module"]
```

**Build Examples**:
```bash
# Python extension module (for `pip install`)
cargo build -p remotemedia-ffi --release --features extension-module

# Library only (no Python bindings)
cargo build -p remotemedia-ffi --no-default-features

# Python bindings for embedding
cargo build -p remotemedia-ffi --features python-bindings
```

**Use Cases**:
- **`extension-module`**: Build Python package (`.so`/`.pyd`) for distribution
- **`python-bindings`**: Embed Python runtime in Rust application
- **`--no-default-features`**: Use FFI transport without Python

---

### 4. remotemedia-webrtc (WebRTC Transport)

**Location**: `transports/remotemedia-webrtc/Cargo.toml`

**Status**: In development (see `specs/001-webrtc-multi-peer-transport/`)

**Planned Features**:
```toml
default = []

# Audio codec support (Opus)
audio-codecs = ["opus"]

# Video codec support (VP9, H.264)
video-codecs = ["vpx", "openh264"]

# All codecs
codecs = ["audio-codecs", "video-codecs"]

# WebSocket signaling client
signaling-client = ["tokio-tungstenite"]

# Full WebRTC transport with all features
full = ["codecs", "signaling-client"]
```

**Planned Build Examples**:
```bash
# Minimal build (signaling only, no media)
cargo build -p remotemedia-webrtc --features signaling-client

# Audio-only (for voice conferences)
cargo build -p remotemedia-webrtc --features audio-codecs,signaling-client

# Video-only (for video processing)
cargo build -p remotemedia-webrtc --features video-codecs,signaling-client

# Full WebRTC (audio + video + signaling)
cargo build -p remotemedia-webrtc --features full
```

---

### 5. runtime (Legacy Monolithic Runtime)

**Location**: `runtime/Cargo.toml`

**Status**: Legacy, being phased out. Includes all transports bundled together.

**Features**:
```toml
default = ["webrtc-transport", "wasmtime-runtime", "python-async", "native-numpy", "silero-vad", "multiprocess"]

webrtc-transport = ["webrtc"]
python-async = ["pyo3-async-runtimes"]
native-numpy = ["numpy"]
whisper = ["rwhisper", "rodio"]
whisper-wasm = ["whisper-rs"]
wasmtime-runtime = ["wasmtime", "wasmtime-wasi"]
silero-vad = ["ort"]
multiprocess = ["iceoryx2", ...]
wasm = []  # WASM-specific code paths
```

**Migration Strategy**: Prefer using `runtime-core` + individual transport crates instead of `runtime`.

---

## Common Build Scenarios

### Scenario 1: Minimal Core (No Transports)

**Use Case**: Embed pipeline execution in your own application with custom transport.

```bash
# Build only runtime-core with minimal features
cargo build -p remotemedia-runtime-core --no-default-features

# Size: ~5-10 MB
# Dependencies: Minimal (tokio, serde, basic audio processing)
```

### Scenario 2: gRPC Server Only (No WebRTC, No FFI)

**Use Case**: Run RemoteMedia as a gRPC service for remote pipeline execution.

```bash
# Build gRPC transport server
cargo build -p remotemedia-grpc --release --bin grpc-server

# Run server
./target/release/grpc-server

# Size: ~15-20 MB
# Port: 50051 (default)
```

### Scenario 3: gRPC Server with WebRTC Signaling

**Use Case**: Unified server for both pipeline execution and WebRTC signaling.

```bash
# Build with WebRTC signaling support
cargo build -p remotemedia-grpc --release --features webrtc-signaling --bin grpc-server

# Run server
GRPC_BIND_ADDRESS="0.0.0.0:50051" ./target/release/grpc-server

# Services available:
# - PipelineExecutionService (port 50051)
# - StreamingPipelineService (port 50051)
# - WebRtcSignalingService (port 50051)
```

### Scenario 4: Python Package Only (No gRPC, No WebRTC)

**Use Case**: Distribute RemoteMedia as a Python pip package.

```bash
# Build Python extension module
cd transports/remotemedia-ffi
cargo build --release --features extension-module

# Install in Python environment
pip install -e .

# Size: ~10-15 MB
# Usage: import remotemedia_runtime
```

### Scenario 5: WebRTC Peer-to-Peer (No gRPC Server)

**Use Case**: Direct peer-to-peer media streaming with pipeline processing.

```bash
# Build WebRTC transport with all codecs (when available)
cargo build -p remotemedia-webrtc --release --features full

# Embedded in application:
use remotemedia_webrtc::WebRtcTransport;
let transport = WebRtcTransport::new(config)?;
```

### Scenario 6: All Transports (Development/Testing)

**Use Case**: Full feature testing, development environment.

```bash
# Build entire workspace with all features
cargo build --workspace --all-features --release

# Size: ~50-80 MB (includes all transports + codecs)
```

### Scenario 7: WASM Target (Browser Execution)

**Use Case**: Run pipelines in browser via WebAssembly.

```bash
# Build WASM binary from legacy runtime
cd runtime
cargo build --target wasm32-wasip1 \
  --bin pipeline_executor_wasm \
  --no-default-features \
  --features wasm \
  --release

# Output: target/wasm32-wasip1/release/pipeline_executor_wasm.wasm
# Size: ~5-10 MB
```

---

## Environment Variables

Each transport can be configured via environment variables:

### gRPC Transport

```bash
# Server configuration
GRPC_BIND_ADDRESS="0.0.0.0:50051"  # Bind address
GRPC_REQUIRE_AUTH=true              # Enable authentication
GRPC_AUTH_TOKENS="token1,token2"    # API tokens
GRPC_MAX_MEMORY_MB=200              # Max memory per execution
GRPC_MAX_TIMEOUT_SEC=10             # Max execution timeout
GRPC_JSON_LOGGING=true              # JSON structured logging

# Logging
RUST_LOG=info  # Options: trace, debug, info, warn, error
```

### WebRTC Transport (Planned)

```bash
# Signaling server
WEBRTC_SIGNALING_URL="ws://localhost:8080"

# STUN/TURN servers
WEBRTC_STUN_SERVERS="stun:stun.l.google.com:19302"
WEBRTC_TURN_SERVERS="turn:turn.example.com:3478"

# Media configuration
WEBRTC_MAX_PEERS=10
WEBRTC_JITTER_BUFFER_MS=50
WEBRTC_ENABLE_DATA_CHANNEL=true
```

### Python FFI Transport

```bash
# Python environment
PYTHON_PATH="/path/to/venv/bin/python"

# Multiprocess configuration
ICEORYX2_SERVICE_NAME="remotemedia"
```

---

## Cross-Compilation

### Build for Windows (from Linux/macOS)

```bash
# Install Windows target
rustup target add x86_64-pc-windows-gnu

# Build gRPC server for Windows
cargo build -p remotemedia-grpc --target x86_64-pc-windows-gnu --release
```

### Build for Linux (from macOS/Windows)

```bash
# Install Linux target
rustup target add x86_64-unknown-linux-gnu

# Build
cargo build -p remotemedia-grpc --target x86_64-unknown-linux-gnu --release
```

### Build for ARM (Raspberry Pi, etc.)

```bash
# Install ARM target
rustup target add aarch64-unknown-linux-gnu

# Build with cross
cross build -p remotemedia-grpc --target aarch64-unknown-linux-gnu --release
```

---

## Performance Optimization

### Release Builds

All crates use aggressive optimization for release builds:

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

**Build Time**: ~10-20 minutes for full workspace
**Binary Size**: Reduced by ~30-40% with LTO
**Runtime Performance**: 2-5x faster than debug builds

### Development Builds

For faster iteration during development:

```bash
# Dev build (no optimization, faster compile)
cargo build -p remotemedia-grpc

# Build time: ~2-5 minutes
# Runtime performance: Slower, but acceptable for testing
```

---

## Dependency Tree

Visualize dependencies for any crate:

```bash
# Install cargo-tree
cargo install cargo-tree

# Show dependency tree for gRPC transport
cargo tree -p remotemedia-grpc

# Show only top-level dependencies
cargo tree -p remotemedia-grpc --depth 1

# Find why a specific crate is included
cargo tree -p remotemedia-grpc -i tokio
```

---

## Testing

### Run Tests for Specific Transport

```bash
# Test gRPC transport only
cargo test -p remotemedia-grpc

# Test with specific features
cargo test -p remotemedia-grpc --features webrtc-signaling

# Test all transports
cargo test --workspace
```

### Run Integration Tests

```bash
# Run integration tests for gRPC service
cargo test -p remotemedia-grpc --test '*'

# Run with logging
RUST_LOG=debug cargo test -p remotemedia-grpc -- --nocapture
```

---

## CI/CD Configuration

### GitHub Actions Example

```yaml
# .github/workflows/build.yml
name: Build Matrix

on: [push, pull_request]

jobs:
  build-minimal:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Build runtime-core (minimal)
        run: cargo build -p remotemedia-runtime-core --no-default-features

  build-grpc:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Build gRPC transport
        run: cargo build -p remotemedia-grpc --release

  build-grpc-webrtc:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Build gRPC with WebRTC signaling
        run: cargo build -p remotemedia-grpc --features webrtc-signaling

  build-full:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Build all transports
        run: cargo build --workspace --all-features
```

---

## Troubleshooting

### Issue: Build fails with missing dependencies

**Solution**: Ensure you've installed system dependencies:

```bash
# Ubuntu/Debian
sudo apt-get install cmake pkg-config libssl-dev

# macOS
brew install cmake pkg-config openssl

# Windows (via vcpkg)
vcpkg install openssl
```

### Issue: Cannot find `grpc-server` binary

**Solution**: The binary requires the `server` feature:

```bash
# Build with server feature (default)
cargo build -p remotemedia-grpc --bin grpc-server

# Or explicitly enable
cargo build -p remotemedia-grpc --features server --bin grpc-server
```

### Issue: WebRTC signaling not available in gRPC server

**Solution**: Enable the `webrtc-signaling` feature:

```bash
cargo build -p remotemedia-grpc --features webrtc-signaling
```

---

## Summary: Feature Flag Reference

| Crate | Feature | Description | Dependencies |
|-------|---------|-------------|--------------|
| **runtime-core** | `multiprocess` | IPC for Python nodes | iceoryx2 |
| **runtime-core** | `silero-vad` | Voice activity detection | ort (ONNX Runtime) |
| **remotemedia-grpc** | `server` | Server binary dependencies | ctrlc, num_cpus |
| **remotemedia-grpc** | `webrtc-signaling` | gRPC-based WebRTC signaling | None (uses existing gRPC) |
| **remotemedia-ffi** | `python-bindings` | Python FFI | pyo3, numpy |
| **remotemedia-ffi** | `extension-module` | Python package (.so/.pyd) | pyo3/extension-module |
| **remotemedia-webrtc** | `audio-codecs` | Opus audio encoding | opus |
| **remotemedia-webrtc** | `video-codecs` | VP9/H.264 encoding | vpx, openh264 |
| **remotemedia-webrtc** | `signaling-client` | WebSocket signaling | tokio-tungstenite |

---

## Next Steps

1. **Choose your transport(s)** based on your use case
2. **Build with minimal features** for production deployments
3. **Enable optional features** as needed
4. **Test your build** with `cargo test -p <crate>`
5. **Deploy** using release builds (`--release`)

For more information:
- [Project README](../README.md)
- [gRPC Transport](../transports/remotemedia-grpc/README.md)
- [WebRTC Transport Spec](../specs/001-webrtc-multi-peer-transport/spec.md)
- [FFI Transport](../transports/remotemedia-ffi/README.md)
