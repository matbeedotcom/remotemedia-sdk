# Quickstart: Transport Layer Decoupling

**Feature**: 003-transport-decoupling
**Target Audience**: Developers implementing or using transports
**Prerequisites**: Rust 1.75+, familiarity with async/await

## For Transport Users

### Using runtime-core Directly

If you want to use RemoteMedia runtime without any specific transport:

```toml
# Cargo.toml
[dependencies]
remotemedia-runtime-core = "0.4"
tokio = { version = "1.35", features = ["full"] }
```

```rust
use remotemedia_runtime_core::transport::PipelineRunner;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create runner
    let runner = PipelineRunner::new()?;

    // Load manifest
    let manifest_json = r#"{ ... }"#;
    let manifest = Arc::new(Manifest::from_json(manifest_json)?);

    // Create input
    let input = TransportData::new(RuntimeData::Audio {
        samples: vec![...],
        sample_rate: 16000,
        channels: 1,
    });

    // Execute
    let output = runner.execute_unary(manifest, input).await?;

    println!("Result: {:?}", output.data);
    Ok(())
}
```

### Using gRPC Transport

If you want to deploy a gRPC server:

```toml
# Cargo.toml
[dependencies]
remotemedia-grpc = "0.4"
tokio = { version = "1.35", features = ["full"] }
```

```rust
use remotemedia_grpc::GrpcServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = GrpcServer::builder()
        .bind_address("0.0.0.0:50051")
        .build()?;

    server.serve().await?;
    Ok(())
}
```

### Using FFI Transport (Python)

If you want to call runtime from Python:

```bash
pip install remotemedia
```

```python
import asyncio
from remotemedia import execute_pipeline

async def main():
    manifest = {
        "version": "v1",
        "nodes": [...],
        "connections": [...]
    }

    input_data = {
        "audio": {
            "samples": [...],
            "sample_rate": 16000
        }
    }

    result = await execute_pipeline(manifest, input_data)
    print(result)

asyncio.run(main())
```

## For Custom Transport Developers

### Implementing a Custom Transport

Create a new crate that depends on `remotemedia-runtime-core`:

```toml
# Cargo.toml
[package]
name = "remotemedia-kafka"
version = "0.1.0"
edition = "2021"

[dependencies]
remotemedia-runtime-core = "0.4"
async-trait = "0.1"
tokio = { version = "1.35", features = ["full"] }
# Your transport-specific dependencies
kafka = "0.8"
```

Implement the `PipelineTransport` trait:

```rust
// src/lib.rs
use remotemedia_runtime_core::transport::{
    PipelineTransport, TransportData, StreamSession, PipelineRunner
};
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

pub struct KafkaTransport {
    runner: PipelineRunner,
    producer: KafkaProducer,
    consumer: KafkaConsumer,
}

impl KafkaTransport {
    pub fn new(config: KafkaConfig) -> Result<Self> {
        Ok(Self {
            runner: PipelineRunner::new()?,
            producer: KafkaProducer::new(config.clone())?,
            consumer: KafkaConsumer::new(config)?,
        })
    }
}

#[async_trait]
impl PipelineTransport for KafkaTransport {
    async fn execute(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        // Execute via runner
        let output = self.runner.execute_unary(manifest, input).await?;

        // Optionally publish result to Kafka
        self.producer.send_result(&output).await?;

        Ok(output)
    }

    async fn stream(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn StreamSession>> {
        // Create session via runner
        let session = self.runner.create_stream_session(manifest).await?;

        // Wrap in Kafka-specific session if needed
        Ok(Box::new(session))
    }
}
```

### Using Your Custom Transport

```rust
use remotemedia_kafka::KafkaTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = KafkaTransport::new(config)?;

    let manifest = Arc::new(Manifest::from_json(json)?);
    let mut session = transport.stream(manifest).await?;

    // Read from Kafka, process through pipeline
    while let Some(message) = consumer.recv().await {
        let input = TransportData::new(parse_message(message)?);
        session.send_input(input).await?;

        while let Some(output) = session.recv_output().await? {
            producer.send(output).await?;
        }
    }

    session.close().await?;
    Ok(())
}
```

## For Core Runtime Developers

### Project Structure After Refactoring

```text
remotemedia-sdk/
├── Cargo.toml          # Workspace root
├── runtime-core/       # Your main workspace
├── transports/
│   ├── remotemedia-grpc/
│   ├── remotemedia-ffi/
│   └── remotemedia-webrtc/
```

### Working in runtime-core

```bash
# Build just the core
cd runtime-core
cargo build

# Run core tests (no transport deps needed)
cargo test

# Check dependencies (should not include tonic, pyo3, etc.)
cargo tree | grep -E '(tonic|prost|pyo3|tower|hyper)'
# Expected: no matches

# Build time should be <45s
time cargo build --release
```

### Working on a Transport

```bash
# Build specific transport
cd transports/remotemedia-grpc
cargo build

# Run transport tests
cargo test

# Check that it depends on runtime-core
cargo tree | grep remotemedia-runtime-core
# Expected: remotemedia-runtime-core vX.Y.Z
```

### Running Full Integration Tests

```bash
# From workspace root
cargo test --workspace

# Run only gRPC integration tests
cargo test --package remotemedia-grpc --test integration

# Run benchmarks
cargo bench --workspace
```

## Migration Guide (for Existing Code)

### Before (Monolithic)

```rust
// Old: Everything in runtime crate
use remotemedia_runtime::grpc_service::GrpcServer;
use remotemedia_runtime::executor::Executor;
```

### After (Decoupled)

```rust
// New: Import from specific crates
use remotemedia_grpc::GrpcServer;
use remotemedia_runtime_core::executor::Executor;
```

### Compatibility Shim (During Migration)

For backward compatibility (v0.3.x - v0.4.x):

```rust
// Still works but deprecated
#[deprecated(since = "0.4.0", note = "Use remotemedia-grpc crate")]
use remotemedia_runtime::grpc_service::GrpcServer;
```

### Feature Flags

```toml
# Old way (still supported in v0.3.x)
[dependencies]
remotemedia-runtime = { version = "0.3", features = ["grpc-transport"] }

# New way (v0.4.x+)
[dependencies]
remotemedia-runtime-core = "0.4"
remotemedia-grpc = "0.4"  # Only if you need gRPC
```

## Testing Your Implementation

### Unit Testing with Mock Transport

```rust
use remotemedia_runtime_core::transport::{PipelineTransport, TransportData};

struct MockTransport;

#[async_trait]
impl PipelineTransport for MockTransport {
    async fn execute(&self, _manifest: Arc<Manifest>, input: TransportData)
        -> Result<TransportData>
    {
        // Return mock output for testing
        Ok(TransportData::new(RuntimeData::Text("mock output".into())))
    }

    async fn stream(&self, _manifest: Arc<Manifest>)
        -> Result<Box<dyn StreamSession>>
    {
        // Return mock session
        Ok(Box::new(MockSession))
    }
}

#[tokio::test]
async fn test_custom_transport() {
    let transport = MockTransport;
    let manifest = Arc::new(Manifest::from_json(r#"{...}"#).unwrap());
    let input = TransportData::new(RuntimeData::Text("test".into()));

    let output = transport.execute(manifest, input).await.unwrap();
    assert_eq!(output.data, RuntimeData::Text("mock output".into()));
}
```

### Integration Testing

```rust
#[tokio::test]
async fn test_end_to_end_streaming() {
    let runner = PipelineRunner::new().unwrap();
    let manifest = Arc::new(load_test_manifest());

    let mut session = runner.create_stream_session(manifest).await.unwrap();

    // Send test data
    let input = TransportData::new(RuntimeData::Audio {
        samples: vec![0.0; 16000],
        sample_rate: 16000,
        channels: 1,
    });
    session.send_input(input).await.unwrap();

    // Receive and validate output
    let output = session.recv_output().await.unwrap().unwrap();
    assert!(matches!(output.data, RuntimeData::Audio { .. }));

    session.close().await.unwrap();
}
```

## Debugging

### Check Dependency Tree

```bash
# Verify runtime-core has no transport deps
cargo tree --package remotemedia-runtime-core \
    --invert tonic
# Should show "not found"

# Check what depends on tonic
cargo tree --package remotemedia-grpc \
    --invert tonic
# Should show remotemedia-grpc
```

### Build Time Analysis

```bash
# Profile build with timings
cargo build --timings

# Open target/cargo-timings/cargo-timing.html
# Look for:
# - runtime-core: should be <45s
# - Transport crates: should be <30s each
```

### Logging

```rust
// Enable tracing for debugging
env_logger::init();

// Or with specific level
RUST_LOG=remotemedia_runtime_core=debug cargo run
```

## Common Issues

### Issue: "Cannot find trait `PipelineTransport`"

**Solution**: Add `async-trait` dependency and use `#[async_trait]` macro

```toml
[dependencies]
async-trait = "0.1"
```

```rust
use async_trait::async_trait;

#[async_trait]
impl PipelineTransport for MyTransport { ... }
```

### Issue: "Session closed unexpectedly"

**Solution**: Check that you're not calling methods after `close()`

```rust
session.send_input(data).await?;  // OK
session.close().await?;
session.send_input(data).await?;  // ERROR: SessionClosed
```

### Issue: Build takes longer than expected

**Solution**: Use incremental compilation and sccache

```bash
# Enable sccache
export RUSTC_WRAPPER=sccache

# Build with incremental
cargo build --release
```

## Next Steps

- Read detailed architecture: `docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md`
- Review API contracts: `specs/003-transport-decoupling/contracts/`
- See example implementations: `examples/custom-transport/`
- Run benchmarks: `cargo bench --package remotemedia-grpc`

## Support

- GitHub Issues: https://github.com/matbeedotcom/remotemedia-sdk/issues
- Documentation: https://docs.rs/remotemedia-runtime-core
- Examples: `examples/` directory in repository
