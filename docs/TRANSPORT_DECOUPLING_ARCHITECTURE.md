# Transport Decoupling Architecture

## Problem Statement

**Current Architecture (Tightly Coupled):**
```
┌─────────────────────────────────────────────────────┐
│         remotemedia-runtime (single crate)          │
│                                                      │
│  ┌────────────┐  ┌──────────┐  ┌────────────────┐  │
│  │ gRPC       │  │ WebRTC   │  │ FFI (PyO3)     │  │
│  │ Service    │  │ (planned)│  │                │  │
│  └─────┬──────┘  └────┬─────┘  └──────┬─────────┘  │
│        │              │                │            │
│        └──────────────┼────────────────┘            │
│                       │                             │
│  ┌────────────────────▼────────────────────────┐   │
│  │         Core Runtime Engine                 │   │
│  │  (Executor, SessionRouter, Nodes, etc.)     │   │
│  └─────────────────────────────────────────────┘   │
│                                                      │
│  Dependencies: tonic, prost, pyo3, tower, hyper...  │
└─────────────────────────────────────────────────────┘
```

**Issues:**
- Runtime has transitive dependencies on transport crates (tonic, pyo3, etc.)
- Cannot use runtime without pulling in unused transport code
- Transports cannot evolve independently
- Testing requires mocking transport layers
- Violates Single Responsibility Principle

## Proposed Architecture (Decoupled)

```
┌───────────────────────────────────────────────────────────────────────────┐
│                    TRANSPORT LAYER (separate crates)                       │
│                                                                            │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────────┐   │
│  │ remotemedia-grpc │  │ remotemedia-     │  │ remotemedia-ffi      │   │
│  │                  │  │ webrtc           │  │                      │   │
│  │ Depends on:      │  │                  │  │ Depends on:          │   │
│  │ • runtime-core ✓ │  │ Depends on:      │  │ • runtime-core ✓     │   │
│  │ • tonic          │  │ • runtime-core ✓ │  │ • pyo3               │   │
│  │ • prost          │  │ • webrtc         │  │                      │   │
│  └────────┬─────────┘  └────────┬─────────┘  └──────────┬───────────┘   │
│           │                     │                        │               │
│           └─────────────────────┼────────────────────────┘               │
│                                 │                                        │
└─────────────────────────────────┼────────────────────────────────────────┘
                                  │ Implements
                                  │ PipelineTransport trait
                                  ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                    ABSTRACTION LAYER (traits)                              │
│                                                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐ │
│  │  pub trait PipelineTransport {                                       │ │
│  │      async fn execute(manifest, input) -> Result<Output>            │ │
│  │      async fn stream(manifest) -> Result<StreamHandle>              │ │
│  │  }                                                                   │ │
│  │                                                                      │ │
│  │  pub trait StreamHandle {                                            │ │
│  │      async fn send_input(data) -> Result<()>                        │ │
│  │      async fn recv_output() -> Result<Data>                         │ │
│  │  }                                                                   │ │
│  │                                                                      │ │
│  │  pub trait DataFormat {                                              │ │
│  │      fn serialize(RuntimeData) -> Self::Wire                        │ │
│  │      fn deserialize(Self::Wire) -> RuntimeData                      │ │
│  │  }                                                                   │ │
│  └─────────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                    CORE LAYER (runtime-core crate)                         │
│                                                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐ │
│  │                     Core Runtime Engine                              │ │
│  │                                                                      │ │
│  │  • Executor              • SessionRouter                            │ │
│  │  • Node Registry         • Multiprocess Executor                    │ │
│  │  • RuntimeData           • IPC Layer (iceoryx2)                     │ │
│  │  • Manifest Parser       • Metrics                                  │ │
│  │  • Audio Nodes           • Error Handling                           │ │
│  │                                                                      │ │
│  │  Dependencies: tokio, serde, iceoryx2, rubato, etc.                 │ │
│  │  NO transport dependencies!                                         │ │
│  └─────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────┘
```

## Key Changes Required

### 1. Create Core Abstraction Layer

**New file: `runtime-core/src/transport/mod.rs`**

```rust
//! Transport abstraction layer
//!
//! This module defines traits that transport implementations must satisfy.
//! The core runtime knows nothing about specific transports (gRPC, WebRTC, FFI).

use crate::data::RuntimeData;
use crate::manifest::Manifest;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Transport-agnostic pipeline execution interface
#[async_trait]
pub trait PipelineTransport: Send + Sync {
    /// Execute a pipeline with the given manifest and input data (unary)
    async fn execute(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData>;

    /// Start a streaming pipeline session
    async fn stream(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn StreamSession>>;
}

/// Streaming session handle
#[async_trait]
pub trait StreamSession: Send + Sync {
    /// Get unique session identifier
    fn session_id(&self) -> &str;

    /// Send input data to the pipeline
    async fn send_input(&mut self, data: TransportData) -> Result<()>;

    /// Receive output data from the pipeline (blocking)
    async fn recv_output(&mut self) -> Result<Option<TransportData>>;

    /// Close the session gracefully
    async fn close(&mut self) -> Result<()>;

    /// Check if session is still active
    fn is_active(&self) -> bool;
}

/// Transport-agnostic data container
///
/// This wraps RuntimeData and provides metadata needed by transports
pub struct TransportData {
    /// Core data payload
    pub data: RuntimeData,

    /// Optional sequence number (for ordering in streams)
    pub sequence: Option<u64>,

    /// Optional metadata (transport-specific)
    pub metadata: std::collections::HashMap<String, String>,
}

impl TransportData {
    pub fn new(data: RuntimeData) -> Self {
        Self {
            data,
            sequence: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_sequence(mut self, seq: u64) -> Self {
        self.sequence = Some(seq);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Pipeline executor that transports can use
///
/// This is the main entry point for transport implementations
pub struct PipelineRunner {
    // Internal state (executor, registries, etc.)
    // Hidden from transport implementations
    inner: Arc<PipelineRunnerInner>,
}

impl PipelineRunner {
    /// Create new pipeline runner
    pub fn new() -> Result<Self> {
        // Initialize core runtime
        Ok(Self {
            inner: Arc::new(PipelineRunnerInner::new()?),
        })
    }

    /// Execute unary pipeline
    pub async fn execute_unary(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        // Implementation delegates to internal executor
        self.inner.execute_unary(manifest, input).await
    }

    /// Create streaming session
    pub async fn create_stream_session(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<StreamSessionHandle> {
        self.inner.create_stream_session(manifest).await
    }
}

/// Concrete streaming session implementation
pub struct StreamSessionHandle {
    session_id: String,
    // Internal channels and state
    inner: Arc<StreamSessionInner>,
}

#[async_trait]
impl StreamSession for StreamSessionHandle {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn send_input(&mut self, data: TransportData) -> Result<()> {
        self.inner.send_input(data).await
    }

    async fn recv_output(&mut self) -> Result<Option<TransportData>> {
        self.inner.recv_output().await
    }

    async fn close(&mut self) -> Result<()> {
        self.inner.close().await
    }

    fn is_active(&self) -> bool {
        self.inner.is_active()
    }
}
```

### 2. Restructure Project Layout

**New workspace structure:**

```
remotemedia-sdk/
├── Cargo.toml                    # Workspace root
│
├── runtime-core/                 # Core runtime (no transport deps)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── executor/             # Executor, SessionRouter
│   │   ├── nodes/                # Node registry, audio nodes
│   │   ├── data/                 # RuntimeData types
│   │   ├── manifest/             # Manifest parsing
│   │   ├── python/               # Multiprocess executor
│   │   │   └── multiprocess/     # IPC, process management
│   │   ├── transport/            # NEW: Transport abstractions
│   │   │   ├── mod.rs            # PipelineTransport trait
│   │   │   ├── runner.rs         # PipelineRunner
│   │   │   └── session.rs        # StreamSession trait
│   │   └── error.rs
│   └── tests/
│
├── transports/                   # Transport implementations
│   │
│   ├── remotemedia-grpc/         # gRPC transport (separate crate)
│   │   ├── Cargo.toml            # Depends: runtime-core, tonic, prost
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── server.rs         # Tonic server setup
│   │   │   ├── service.rs        # gRPC service impl
│   │   │   ├── streaming.rs      # Streaming handler
│   │   │   ├── execution.rs      # Unary handler
│   │   │   ├── adapters.rs       # RuntimeData ↔ Protobuf
│   │   │   ├── auth.rs
│   │   │   ├── metrics.rs
│   │   │   └── generated/        # Protobuf types
│   │   ├── protos/
│   │   ├── build.rs
│   │   └── bin/
│   │       └── grpc-server.rs    # Binary entry point
│   │
│   ├── remotemedia-ffi/          # Python FFI transport
│   │   ├── Cargo.toml            # Depends: runtime-core, pyo3
│   │   ├── src/
│   │   │   ├── lib.rs            # PyO3 module definition
│   │   │   ├── api.rs            # Python-facing API
│   │   │   ├── marshal.rs        # Python ↔ RuntimeData
│   │   │   └── numpy_bridge.rs   # Zero-copy numpy
│   │   └── python/
│   │       └── remotemedia/
│   │           └── __init__.py
│   │
│   └── remotemedia-webrtc/       # WebRTC transport (future)
│       ├── Cargo.toml            # Depends: runtime-core, webrtc
│       └── src/
│           └── lib.rs
│
├── python-client/                # Python SDK (uses FFI transport)
│   └── remotemedia/
│       └── runtime.py            # Import remotemedia-ffi
│
└── examples/
    ├── grpc-server/              # Example using remotemedia-grpc
    ├── python-sdk/               # Example using remotemedia-ffi
    └── custom-transport/         # Example custom transport
```

**Workspace Cargo.toml:**

```toml
[workspace]
members = [
    "runtime-core",
    "transports/grpc",
    "transports/ffi",
    "transports/webrtc",
]
resolver = "2"

[workspace.dependencies]
# Shared dependencies
tokio = { version = "1.35", features = ["sync", "macros", "rt", "time"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
async-trait = "0.1"
tracing = "0.1"
```

### 3. Example gRPC Transport Implementation

**transports/grpc/src/service.rs:**

```rust
//! gRPC service implementation using runtime-core

use remotemedia_runtime_core::transport::{PipelineRunner, TransportData};
use remotemedia_runtime_core::manifest::Manifest;
use tonic::{Request, Response, Status};
use std::sync::Arc;

pub struct GrpcPipelineService {
    runner: Arc<PipelineRunner>,
}

impl GrpcPipelineService {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            runner: Arc::new(PipelineRunner::new()?),
        })
    }
}

#[tonic::async_trait]
impl PipelineExecutionService for GrpcPipelineService {
    async fn execute_pipeline(
        &self,
        request: Request<ExecutePipelineRequest>,
    ) -> Result<Response<ExecutePipelineResponse>, Status> {
        let req = request.into_inner();

        // Parse manifest
        let manifest = Manifest::from_json(&req.manifest_json)
            .map_err(|e| Status::invalid_argument(format!("Invalid manifest: {}", e)))?;

        // Convert protobuf AudioBuffer to TransportData
        let input_data = self.proto_to_transport_data(req.input_audio)?;

        // Execute via core runner
        let output = self.runner
            .execute_unary(Arc::new(manifest), input_data)
            .await
            .map_err(|e| Status::internal(format!("Execution failed: {}", e)))?;

        // Convert back to protobuf
        let response = self.transport_data_to_proto(output)?;

        Ok(Response::new(response))
    }

    type StreamPipelineStream = tokio_stream::wrappers::ReceiverStream<Result<StreamResponse, Status>>;

    async fn stream_pipeline(
        &self,
        request: Request<Streaming<StreamRequest>>,
    ) -> Result<Response<Self::StreamPipelineStream>, Status> {
        let mut in_stream = request.into_inner();
        let runner = Arc::clone(&self.runner);

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            // Get manifest from first message
            let first_msg = match in_stream.message().await {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    let _ = tx.send(Err(Status::invalid_argument("Empty stream"))).await;
                    return;
                }
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(format!("Stream error: {}", e)))).await;
                    return;
                }
            };

            // Parse manifest
            let manifest = match Manifest::from_json(&first_msg.manifest_json) {
                Ok(m) => Arc::new(m),
                Err(e) => {
                    let _ = tx.send(Err(Status::invalid_argument(format!("Invalid manifest: {}", e)))).await;
                    return;
                }
            };

            // Create streaming session via core runner
            let mut session = match runner.create_stream_session(manifest).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(format!("Failed to create session: {}", e)))).await;
                    return;
                }
            };

            // Send ready response
            let _ = tx.send(Ok(StreamResponse {
                response: Some(stream_response::Response::Ready(StreamReady {
                    session_id: session.session_id().to_string(),
                })),
            })).await;

            // Process incoming stream
            while let Some(msg) = in_stream.message().await.transpose() {
                match msg {
                    Ok(request) => {
                        // Convert protobuf to TransportData
                        let input_data = proto_to_transport_data(request.audio);

                        // Send to session
                        if let Err(e) = session.send_input(input_data).await {
                            let _ = tx.send(Err(Status::internal(format!("Send failed: {}", e)))).await;
                            break;
                        }

                        // Receive outputs
                        while let Ok(Some(output)) = session.recv_output().await {
                            let proto_response = transport_data_to_proto(output);
                            let _ = tx.send(Ok(proto_response)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(format!("Stream error: {}", e)))).await;
                        break;
                    }
                }
            }

            // Cleanup
            let _ = session.close().await;
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}
```

### 4. Example FFI Transport Implementation

**transports/ffi/src/api.rs:**

```rust
//! Python FFI API using runtime-core

use remotemedia_runtime_core::transport::{PipelineRunner, TransportData};
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::data::RuntimeData;
use pyo3::prelude::*;
use std::sync::Arc;

#[pyfunction]
fn execute_pipeline(
    py: Python<'_>,
    manifest_json: String,
    input_data: PyObject,
) -> PyResult<Bound<'_, PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        // Parse manifest
        let manifest = Manifest::from_json(&manifest_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid manifest: {}", e)))?;

        // Convert Python data to RuntimeData
        let runtime_data = py_to_runtime_data(input_data)?;
        let transport_data = TransportData::new(runtime_data);

        // Create runner and execute
        let runner = PipelineRunner::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to create runner: {}", e)))?;

        let output = runner
            .execute_unary(Arc::new(manifest), transport_data)
            .await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Execution failed: {}", e)))?;

        // Convert back to Python
        Python::attach(|py| runtime_data_to_py(py, output.data))
    })
}

/// Python module definition
#[pymodule]
fn remotemedia_runtime(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(execute_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(is_rust_available, m)?)?;
    Ok(())
}
```

### 5. Migration Strategy

#### Phase 1: Create Core Abstractions (Week 1)

1. Create `runtime-core/src/transport/mod.rs` with traits
2. Create `PipelineRunner` and `StreamSessionHandle`
3. Refactor existing `SessionRouter` to work with `TransportData`
4. Add tests for core abstractions

#### Phase 2: Extract gRPC Transport (Week 2)

1. Create `transports/grpc/` crate
2. Move `grpc_service/*` → `remotemedia-grpc/src/`
3. Implement `PipelineTransport` trait for gRPC
4. Create adapter layer: Protobuf ↔ `TransportData`
5. Update `bin/grpc_server.rs` to use new crate
6. Run full integration test suite

#### Phase 3: Extract FFI Transport (Week 3)

1. Create `transports/ffi/` crate
2. Move `python/ffi.rs` → `remotemedia-ffi/src/api.rs`
3. Move marshaling code
4. Update Python client to import from new crate
5. Run compatibility tests

#### Phase 4: Cleanup & Documentation (Week 4)

1. Remove `grpc-transport` feature from runtime-core
2. Remove `python-async` feature from runtime-core
3. Update all documentation
4. Create migration guide for users
5. Update CI/CD pipelines

## Benefits

### 1. **Clear Separation of Concerns**
```rust
// Runtime-core knows nothing about transports
runtime-core/
  └── No dependencies on: tonic, prost, pyo3, tower, hyper

// Each transport is self-contained
remotemedia-grpc/
  └── Depends on: runtime-core (✓), tonic, prost

remotemedia-ffi/
  └── Depends on: runtime-core (✓), pyo3
```

### 2. **Independent Evolution**
- Update gRPC to tonic 1.0 without touching core
- Add HTTP/REST transport without modifying runtime
- Experiment with new transports (WebSocket, IPC) independently

### 3. **Reduced Build Times**
```bash
# Building only core (no transport deps)
cd runtime-core
cargo build  # ~30s instead of ~60s

# Building only gRPC transport
cd transports/grpc
cargo build  # Only rebuilds when transport changes
```

### 4. **Easier Testing**
```rust
// Mock transport for testing
struct MockTransport;

#[async_trait]
impl PipelineTransport for MockTransport {
    async fn execute(&self, manifest: Arc<Manifest>, input: TransportData)
        -> Result<TransportData>
    {
        // Test logic without real gRPC/network
        Ok(TransportData::new(RuntimeData::Text("test".into())))
    }
}

#[tokio::test]
async fn test_pipeline_execution() {
    let transport = MockTransport;
    let result = transport.execute(manifest, input).await.unwrap();
    assert_eq!(result.data, expected);
}
```

### 5. **Plugin Architecture**
```rust
// Users can create custom transports
struct CustomKafkaTransport {
    runner: PipelineRunner,
    kafka_producer: KafkaProducer,
}

#[async_trait]
impl PipelineTransport for CustomKafkaTransport {
    async fn execute(&self, manifest: Arc<Manifest>, input: TransportData)
        -> Result<TransportData>
    {
        // Custom logic
        let result = self.runner.execute_unary(manifest, input).await?;
        self.kafka_producer.send(result).await?;
        Ok(result)
    }
}
```

## Backward Compatibility

To maintain backward compatibility during migration:

1. **Feature flags** - Keep old code behind `legacy-grpc` feature
2. **Re-exports** - Re-export new types from old locations
3. **Deprecation warnings** - Annotate old APIs with `#[deprecated]`
4. **Migration period** - Support both architectures for 2-3 releases

```rust
// runtime/src/lib.rs (during migration)
#[deprecated(
    since = "0.4.0",
    note = "Use `remotemedia-grpc` crate instead. This will be removed in 0.5.0"
)]
#[cfg(feature = "legacy-grpc")]
pub mod grpc_service {
    pub use remotemedia_grpc::*;
}
```

## Conclusion

This architecture achieves **true decoupling** by:

1. ✅ Runtime-core has **zero** transport dependencies
2. ✅ Transports depend on runtime-core (correct direction)
3. ✅ Clean abstraction via `PipelineTransport` trait
4. ✅ Each transport is independently versioned and maintained
5. ✅ Enables plugin architecture for custom transports

The migration can be done incrementally over 4 weeks with minimal disruption to users.
