# RemoteMedia Runtime Core

Transport-agnostic execution engine for RemoteMedia pipelines.

## Overview

`remotemedia-runtime-core` is a pure library that provides pipeline execution functionality **without any transport dependencies**. It defines trait-based abstractions that transport implementations use.

## Features

- ✅ **Zero transport dependencies** - No tonic, prost, pyo3, or other transport crates
- ✅ **Trait-based extensibility** - Implement `PipelineTransport` for custom transports
- ✅ **Fast builds** - Minimal dependency tree, builds in <45s
- ✅ **Full functionality** - All core features (executor, nodes, session routing)
- ✅ **Plugin architecture** - Add custom transports without modifying core

## Installation

```toml
[dependencies]
remotemedia-runtime-core = "0.4"
async-trait = "0.1"
tokio = { version = "1.35", features = ["full"] }
```

## Usage

### Direct Pipeline Execution

```rust
use remotemedia_runtime_core::transport::{PipelineRunner, TransportData};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runner = PipelineRunner::new()?;

    let manifest = Arc::new(Manifest::from_json(json)?);
    let input = TransportData::new(RuntimeData::Text("hello".into()));

    let output = runner.execute_unary(manifest, input).await?;
    println!("Result: {:?}", output.data);
    Ok(())
}
```

### Streaming Execution

```rust
let runner = PipelineRunner::new()?;
let manifest = Arc::new(Manifest::from_json(json)?);

let mut session = runner.create_stream_session(manifest).await?;

// Send inputs
for chunk in audio_chunks {
    let data = TransportData::new(RuntimeData::Audio { ... });
    session.send_input(data).await?;

    // Receive outputs
    while let Some(output) = session.recv_output().await? {
        process_output(output);
    }
}

session.close().await?;
```

### Creating a Custom Transport

```rust
use remotemedia_runtime_core::transport::PipelineTransport;
use async_trait::async_trait;

pub struct MyTransport {
    runner: PipelineRunner,
}

#[async_trait]
impl PipelineTransport for MyTransport {
    async fn execute(&self, manifest: Arc<Manifest>, input: TransportData)
        -> Result<TransportData>
    {
        self.runner.execute_unary(manifest, input).await
    }

    async fn stream(&self, manifest: Arc<Manifest>)
        -> Result<Box<dyn StreamSession>>
    {
        Ok(Box::new(self.runner.create_stream_session(manifest).await?))
    }
}
```

See `examples/custom-transport/` for a complete working example (~80 lines).

## Architecture

```
┌─────────────────────────────────────────┐
│  Your Transport (separate crate)        │
│  implements PipelineTransport           │
└─────────────────┬───────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────┐
│  remotemedia-runtime-core               │
│  ├─ transport/ (traits & abstractions)  │
│  ├─ executor/ (pipeline execution)      │
│  ├─ nodes/ (audio processing nodes)     │
│  ├─ data/ (RuntimeData types)           │
│  └─ manifest/ (configuration parsing)   │
│                                          │
│  NO dependencies on:                    │
│  ❌ tonic, prost (gRPC)                 │
│  ❌ pyo3, numpy (FFI)                   │
│  ❌ webrtc (WebRTC)                     │
└─────────────────────────────────────────┘
```

## API Documentation

### Core Types

- **`PipelineTransport`** - Trait for transport implementations
- **`StreamSession`** - Trait for streaming sessions
- **`PipelineRunner`** - Core execution engine
- **`TransportData`** - Transport-agnostic data container
- **`RuntimeData`** - Core data types (Audio, Text, Binary)
- **`Manifest`** - Pipeline configuration

### Key Traits

```rust
// Implement this for your transport
pub trait PipelineTransport: Send + Sync {
    async fn execute(...) -> Result<TransportData>;
    async fn stream(...) -> Result<Box<dyn StreamSession>>;
}

// Session interface for streaming
pub trait StreamSession: Send + Sync {
    fn session_id(&self) -> &str;
    async fn send_input(...) -> Result<()>;
    async fn recv_output() -> Result<Option<TransportData>>;
    async fn close() -> Result<()>;
    fn is_active(&self) -> bool;
}
```

## Examples

- **Custom Transport**: `examples/custom-transport/` - Console-based transport demonstrating the API
- **Unary Execution**: `examples/custom-transport/src/main.rs`
- **Streaming**: `examples/custom-transport/examples/streaming.rs`

## Documentation

- **Custom Transport Guide**: `docs/CUSTOM_TRANSPORT_GUIDE.md`
- **Architecture Overview**: `docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md`
- **System Diagrams**: `runtime/SYSTEM_DIAGRAM.md`
- **API Contracts**: `specs/003-transport-decoupling/contracts/`

## Transport Implementations

Official transport crates (separate repositories):
- **`remotemedia-grpc`** - gRPC transport (tonic/prost)
- **`remotemedia-ffi`** - Python FFI transport (pyo3)
- **`remotemedia-webrtc`** - WebRTC transport (planned)

## Testing

```bash
# Run tests
cargo test

# Check dependencies
cargo tree | grep -E '(tonic|prost|pyo3)'
# Should return empty

# Build time
cargo build --release
# Should complete in <45s
```

## Contributing

This is part of the transport decoupling initiative (spec 003). See:
- Specification: `specs/003-transport-decoupling/spec.md`
- Implementation plan: `specs/003-transport-decoupling/plan.md`
- Tasks: `specs/003-transport-decoupling/tasks.md`

## License

MIT OR Apache-2.0
