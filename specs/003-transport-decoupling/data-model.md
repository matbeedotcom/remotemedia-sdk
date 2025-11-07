# Data Model: Transport Layer Decoupling

**Feature**: 003-transport-decoupling
**Date**: 2025-01-06

## Overview

This document defines the core abstractions and data structures for the transport decoupling architecture. These types form the contract between runtime-core and transport implementations.

## Core Traits

### PipelineTransport

**Purpose**: Defines the interface that all transport implementations must satisfy

**Contract**:
```rust
#[async_trait]
pub trait PipelineTransport: Send + Sync {
    /// Execute a pipeline with unary semantics (single request → single response)
    async fn execute(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData>;

    /// Start a streaming pipeline session (multiple requests/responses)
    async fn stream(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn StreamSession>>;
}
```

**Implementers**: remotemedia-grpc, remotemedia-ffi, custom user transports

**Guarantees**:
- Thread-safe (Send + Sync)
- Async methods for non-blocking I/O
- Manifest-based pipeline configuration
- Transport-agnostic data exchange

### StreamSession

**Purpose**: Manages stateful streaming interactions between transport and core

**Contract**:
```rust
#[async_trait]
pub trait StreamSession: Send + Sync {
    /// Unique identifier for this session
    fn session_id(&self) -> &str;

    /// Send input data to the pipeline
    async fn send_input(&mut self, data: TransportData) -> Result<()>;

    /// Receive output data from the pipeline (blocks until available)
    async fn recv_output(&mut self) -> Result<Option<TransportData>>;

    /// Close the session gracefully
    async fn close(&mut self) -> Result<()>;

    /// Check if session is still active
    fn is_active(&self) -> bool;
}
```

**Lifecycle**:
1. Created by `PipelineTransport::stream()`
2. Transport calls `send_input()` with client data
3. Transport calls `recv_output()` to get results
4. Transport calls `close()` when client disconnects
5. Core cleans up resources

**State Transitions**:
- `Created` → `Active` (after first send_input)
- `Active` → `Closed` (after close() or error)
- `Closed` → terminal state

## Data Types

### TransportData

**Purpose**: Transport-agnostic container for pipeline data with metadata

**Structure**:
```rust
pub struct TransportData {
    /// Core data payload (audio, text, image, etc.)
    pub data: RuntimeData,

    /// Optional sequence number for ordering in streams
    pub sequence: Option<u64>,

    /// Transport-specific metadata (headers, tags, etc.)
    pub metadata: HashMap<String, String>,
}
```

**Fields**:
- **data**: Core payload using existing RuntimeData enum (Audio, Text, Image, Binary)
- **sequence**: For stream ordering, managed by transport or core
- **metadata**: Extensible key-value pairs for transport-specific info

**Methods**:
```rust
impl TransportData {
    pub fn new(data: RuntimeData) -> Self;
    pub fn with_sequence(self, seq: u64) -> Self;
    pub fn with_metadata(self, key: String, value: String) -> Self;
}
```

### PipelineRunner

**Purpose**: Core execution engine exposed to transports

**Structure** (opaque to transports):
```rust
pub struct PipelineRunner {
    inner: Arc<PipelineRunnerInner>,  // Implementation details hidden
}
```

**Public API**:
```rust
impl PipelineRunner {
    pub fn new() -> Result<Self>;

    pub async fn execute_unary(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData>;

    pub async fn create_stream_session(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<StreamSessionHandle>;
}
```

**Responsibilities**:
- Manages Executor, SessionRouter, Node registries
- Hides multiprocess executor details
- Provides clean API for transports

### StreamSessionHandle

**Purpose**: Concrete implementation of StreamSession trait

**Structure** (implementation detail):
```rust
pub struct StreamSessionHandle {
    session_id: String,
    inner: Arc<StreamSessionInner>,  // Internal channels, state
}
```

**Implements**: `StreamSession` trait

**Internal State**:
- Session ID (UUID)
- Input channel to SessionRouter
- Output channel from SessionRouter
- Shutdown signal
- Active flag

## Entity Relationships

```
┌─────────────────────────────────────────────────┐
│  Transport Crate (e.g., remotemedia-grpc)       │
│                                                  │
│  ┌────────────────────────────────────────────┐ │
│  │  GrpcTransport                             │ │
│  │  implements PipelineTransport              │ │
│  │                                             │ │
│  │  execute(manifest, input) ────────────────►│ │
│  │  stream(manifest) ─────────────────────┐   │ │
│  └────────────────────────────────────────┼───┘ │
│                                           │     │
└───────────────────────────────────────────┼─────┘
                                            │
                                            ▼
┌─────────────────────────────────────────────────┐
│  Runtime-Core                                    │
│                                                  │
│  ┌────────────────────────────────────────────┐ │
│  │  PipelineRunner                            │ │
│  │                                             │ │
│  │  create_stream_session() returns           │ │
│  │  ┌──────────────────────────────────────┐ │ │
│  │  │  StreamSessionHandle                  │ │ │
│  │  │  implements StreamSession             │ │ │
│  │  │                                        │ │ │
│  │  │  session_id: String                   │ │ │
│  │  │  send_input(TransportData)            │ │ │
│  │  │  recv_output() -> TransportData       │ │ │
│  │  │  close()                               │ │ │
│  │  └──────────────────────────────────────┘ │ │
│  └────────────────────────────────────────────┘ │
│           │                                     │
│           ▼                                     │
│  ┌────────────────────────────────────────────┐ │
│  │  SessionRouter (internal)                  │ │
│  │  Manages node tasks, routing               │ │
│  └────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

## Data Flow

### Unary Execution

```
1. Transport receives request with manifest + input data
2. Transport creates TransportData from input
3. Transport calls PipelineRunner::execute_unary(manifest, data)
4. Core:
   - Parses manifest
   - Initializes pipeline
   - Executes nodes
   - Returns TransportData output
5. Transport converts TransportData to transport format
6. Transport sends response
```

### Streaming Execution

```
1. Transport receives stream connection with manifest
2. Transport calls PipelineRunner::create_stream_session(manifest)
3. Core returns StreamSessionHandle
4. Loop:
   a. Transport receives chunk from client
   b. Transport converts to TransportData
   c. Transport calls session.send_input(data)
   d. Transport calls session.recv_output()
   e. Core processes through pipeline
   f. Core returns TransportData output
   g. Transport converts and sends to client
5. Client closes connection
6. Transport calls session.close()
7. Core cleans up session resources
```

## Validation Rules

### TransportData

- `data` field is REQUIRED
- `sequence` SHOULD be set for streaming sessions
- `metadata` keys MUST be valid UTF-8 strings
- `metadata` values MUST be valid UTF-8 strings

### Session Lifecycle

- `session_id()` MUST return unique ID
- `send_input()` MUST NOT be called after `close()`
- `recv_output()` MUST return `None` after session closed
- `is_active()` MUST return false after `close()`

### PipelineTransport

- `execute()` MUST be idempotent (same input → same output)
- `stream()` MUST return new session each call
- Methods MUST handle cancellation gracefully (tokio::select!)

## Error Handling

### Core Error Types

```rust
pub enum Error {
    // Manifest errors
    InvalidManifest(String),

    // Execution errors
    NodeExecutionFailed { node_id: String, message: String },
    SessionNotFound(String),

    // Transport errors
    TransportError(String),

    // Data errors
    InvalidData(String),
    SerializationError(String),
}
```

### Transport Error Conversion

Each transport defines its own error type and converts from core errors:

```rust
// In remotemedia-grpc
pub enum GrpcError {
    Core(remotemedia_runtime_core::Error),
    Tonic(tonic::Status),
    InvalidRequest(String),
}

impl From<remotemedia_runtime_core::Error> for GrpcError {
    fn from(e: remotemedia_runtime_core::Error) -> Self {
        GrpcError::Core(e)
    }
}
```

## Extension Points

### Custom Transports

Users can create custom transports by:
1. Adding dependency: `remotemedia-runtime-core = "0.4"`
2. Implementing `PipelineTransport` trait
3. Using `PipelineRunner` for execution
4. Implementing transport-specific serialization

**Example**:
```rust
struct KafkaTransport {
    runner: PipelineRunner,
    producer: KafkaProducer,
}

#[async_trait]
impl PipelineTransport for KafkaTransport {
    async fn execute(&self, manifest: Arc<Manifest>, input: TransportData)
        -> Result<TransportData>
    {
        let result = self.runner.execute_unary(manifest, input).await?;
        self.producer.send_result(result).await?;
        Ok(result)
    }

    async fn stream(&self, manifest: Arc<Manifest>)
        -> Result<Box<dyn StreamSession>>
    {
        Ok(Box::new(
            self.runner.create_stream_session(manifest).await?
        ))
    }
}
```

## Backward Compatibility

During migration (v0.3.x - v0.4.x):

1. **Re-exports**: Old paths continue to work
   ```rust
   // runtime/src/lib.rs
   #[deprecated(since = "0.4.0")]
   pub use remotemedia_grpc::*;
   ```

2. **Feature flags**: Legacy code behind `legacy-grpc` feature
   ```toml
   [features]
   legacy-grpc = ["remotemedia-grpc"]
   ```

3. **Semantic versioning**: Core and transports independently versioned

After v0.5.0: Deprecated code removed, only new trait-based API supported.
