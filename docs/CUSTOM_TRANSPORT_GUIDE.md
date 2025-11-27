# Custom Transport Implementation Guide

This guide shows how to create a custom transport for RemoteMedia using only `remotemedia-runtime-core`, without any transport dependencies.

## Overview

The transport decoupling architecture enables you to:
- Use runtime-core as a standalone library
- Implement custom transports (Kafka, Redis, HTTP, WebSocket, etc.)
- Avoid pulling in unused transport dependencies (gRPC, FFI)
- Create production-ready transports in ~100 lines of code

## Quick Start

### 1. Create a New Crate

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
kafka = "0.10"  # Example
```

### 2. Implement PipelineTransport Trait

```rust
use remotemedia_runtime_core::transport::{
    PipelineTransport, PipelineRunner, StreamSession, TransportData,
};
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::Result;
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
        Ok(Box::new(session))
    }
}
```

### 3. Use Your Transport

```rust
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

## Core API Reference

### PipelineTransport Trait

Your transport must implement this trait:

```rust
#[async_trait]
pub trait PipelineTransport: Send + Sync {
    async fn execute(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData>;

    async fn stream(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn StreamSession>>;
}
```

**Thread Safety**: Must be `Send + Sync` for concurrent access

**Execution Modes**:
- `execute()`: Unary (single request → single response)
- `stream()`: Streaming (multiple requests/responses)

### PipelineRunner

The core execution engine:

```rust
pub struct PipelineRunner {
    pub fn new() -> Result<Self>;
    pub async fn execute_unary(manifest, input) -> Result<TransportData>;
    pub async fn create_stream_session(manifest) -> Result<StreamSessionHandle>;
}
```

**Usage Pattern**:
1. Create once: `PipelineRunner::new()`
2. Reuse for all executions
3. Cloning is cheap (Arc-wrapped)

### StreamSession Trait

For streaming operations:

```rust
#[async_trait]
pub trait StreamSession: Send + Sync {
    fn session_id(&self) -> &str;
    async fn send_input(&mut self, data: TransportData) -> Result<()>;
    async fn recv_output(&mut self) -> Result<Option<TransportData>>;
    async fn close(&mut self) -> Result<()>;
    fn is_active(&self) -> bool;
}
```

**Lifecycle**:
1. Created by `PipelineTransport::stream()`
2. Use `send_input()` / `recv_output()` repeatedly
3. Call `close()` when done
4. Check `is_active()` for status

### TransportData

Container for pipeline data:

```rust
pub struct TransportData {
    pub data: RuntimeData,
    pub sequence: Option<u64>,
    pub metadata: HashMap<String, String>,
}

impl TransportData {
    pub fn new(data: RuntimeData) -> Self;
    pub fn with_sequence(self, seq: u64) -> Self;
    pub fn with_metadata(self, key: String, value: String) -> Self;
}
```

**Builder Pattern**:
```rust
let data = TransportData::new(RuntimeData::Audio { ... })
    .with_sequence(1)
    .with_metadata("client_id".into(), "abc123".into());
```

## Common Patterns

### Pattern 1: Simple Pass-Through Transport

Minimal transport that just uses PipelineRunner:

```rust
pub struct SimpleTransport {
    runner: PipelineRunner,
}

#[async_trait]
impl PipelineTransport for SimpleTransport {
    async fn execute(&self, manifest: Arc<Manifest>, input: TransportData)
        -> Result<TransportData>
    {
        self.runner.execute_unary(manifest, input).await
    }

    async fn stream(&self, manifest: Arc<Manifest>)
        -> Result<Box<dyn StreamSession>>
    {
        let session = self.runner.create_stream_session(manifest).await?;
        Ok(Box::new(session))
    }
}
```

### Pattern 2: Transport with Input/Output Transformation

Add custom serialization:

```rust
pub struct JsonTransport {
    runner: PipelineRunner,
}

impl JsonTransport {
    fn json_to_runtime_data(&self, json: &str) -> Result<RuntimeData> {
        // Custom deserialization
    }

    fn runtime_data_to_json(&self, data: &RuntimeData) -> Result<String> {
        // Custom serialization
    }
}

#[async_trait]
impl PipelineTransport for JsonTransport {
    async fn execute(&self, manifest: Arc<Manifest>, input: TransportData)
        -> Result<TransportData>
    {
        let output = self.runner.execute_unary(manifest, input).await?;
        // Could log, transform, or publish output here
        Ok(output)
    }

    async fn stream(&self, manifest: Arc<Manifest>)
        -> Result<Box<dyn StreamSession>>
    {
        Ok(Box::new(self.runner.create_stream_session(manifest).await?))
    }
}
```

### Pattern 3: Transport with External I/O

Integrate with external systems:

```rust
pub struct RedisTransport {
    runner: PipelineRunner,
    redis: RedisClient,
}

#[async_trait]
impl PipelineTransport for RedisTransport {
    async fn execute(&self, manifest: Arc<Manifest>, input: TransportData)
        -> Result<TransportData>
    {
        // Execute pipeline
        let output = self.runner.execute_unary(manifest, input).await?;

        // Publish result to Redis
        self.redis.publish("results", &output).await?;

        Ok(output)
    }

    async fn stream(&self, manifest: Arc<Manifest>)
        -> Result<Box<dyn StreamSession>>
    {
        Ok(Box::new(self.runner.create_stream_session(manifest).await?))
    }
}
```

## Testing

### Unit Testing with Mock Data

```rust
#[tokio::test]
async fn test_custom_transport() {
    let transport = MyTransport::new().unwrap();
    let manifest = Arc::new(Manifest::from_json(r#"{...}"#).unwrap());
    let input = TransportData::new(RuntimeData::Text("test".into()));

    let output = transport.execute(manifest, input).await.unwrap();
    assert_eq!(output.data, RuntimeData::Text("test".into()));
}
```

### Integration Testing

```rust
#[tokio::test]
async fn test_streaming_session() {
    let transport = MyTransport::new().unwrap();
    let manifest = Arc::new(Manifest::from_json(r#"{...}"#).unwrap());

    let mut session = transport.stream(manifest).await.unwrap();

    // Send input
    let input = TransportData::new(RuntimeData::Audio { ... });
    session.send_input(input).await.unwrap();

    // Receive output
    let output = session.recv_output().await.unwrap();
    assert!(output.is_some());

    session.close().await.unwrap();
}
```

## Examples

See `examples/custom-transport/` for complete working examples:
- `src/lib.rs` - ConsoleTransport implementation (~80 lines)
- `src/main.rs` - Unary execution demo
- `examples/streaming.rs` - Streaming execution demo

## Verification Checklist

Before deploying your custom transport:

- [ ] Implements `PipelineTransport` trait
- [ ] Only depends on `remotemedia-runtime-core` (no gRPC/FFI deps)
- [ ] Handles errors appropriately
- [ ] Cleans up resources in `close()`
- [ ] Thread-safe (`Send + Sync`)
- [ ] Tested with both unary and streaming modes
- [ ] Documentation includes usage examples

## Troubleshooting

### Build Errors

**Issue**: "Cannot find trait `PipelineTransport`"

**Solution**: Add `async-trait` and use the macro:
```rust
use async_trait::async_trait;

#[async_trait]
impl PipelineTransport for MyTransport { ... }
```

**Issue**: "Session closed unexpectedly"

**Solution**: Don't call methods after `close()`:
```rust
session.send_input(data).await?;  // OK
session.close().await?;
session.send_input(data).await?;  // ERROR
```

### Dependency Issues

**Issue**: Pulling in unwanted transport dependencies

**Solution**: Check `cargo tree` and ensure you only depend on `remotemedia-runtime-core`:
```bash
cargo tree | grep -E '(tonic|prost|pyo3|numpy)'
# Should return empty
```

## Performance Considerations

- **Build time**: Custom transports build in <10s (minimal dependencies)
- **Runtime overhead**: Trait dispatch is negligible (~1ns per call)
- **Memory**: PipelineRunner clones are cheap (Arc-wrapped)
- **Concurrency**: Multiple sessions can run concurrently

## Reference Implementations

For production examples, see:
- `transports/grpc/` - gRPC transport (tonic/prost)
- `transports/ffi/` - Python FFI transport (pyo3)

## Support

- Architecture docs: `docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md`
- API contracts: `specs/003-transport-decoupling/contracts/`
- System diagrams: `runtime/SYSTEM_DIAGRAM.md`
- GitHub issues: https://github.com/matbeedotcom/remotemedia-sdk/issues
