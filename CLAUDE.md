# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

## Build Commands

### Rust Runtime

```bash
# Build runtime (native with all features)
cd runtime
cargo build --release

# Build for development (faster compilation)
cargo build

# Build WASM target
rustup target add wasm32-wasip1
cargo build --target wasm32-wasip1 \
  --bin pipeline_executor_wasm \
  --no-default-features \
  --features wasm \
  --release

# Build gRPC server
cargo build --bin grpc_server --release

# Run gRPC server
cargo run --bin grpc_server --release
```

### Testing

```bash
cd runtime

# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run tests matching pattern
cargo test multiprocess

# Run integration tests only
cargo test --test test_grpc_service

# Run with specific features
cargo test --features multiprocess
cargo test --no-default-features --features grpc-transport
```

### Benchmarking

```bash
cd runtime

# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench audio_nodes

# List available benchmarks
cargo bench -- --list
```

### Python Client

```bash
cd python-client

# Install in development mode (without deps for faster iteration)
pip install -e . --no-deps

# Build and link Rust runtime (for remotemedia.runtime)
cd ../transports/ffi
./dev-install.sh

# Run tests
cd ../../python-client
pytest

# Lint
ruff check .
```

**Note**: For editable installs, use `dev-install.sh` to create a symlink to the Rust runtime. For production, just `pip install` both packages normally.

## Architecture Overview

### High-Level System Design

RemoteMedia SDK is a **multi-runtime, multi-transport pipeline execution framework** with three execution modes:

1. **Native Rust Runtime** - In-process node execution with 2-16x speedup for audio
2. **Multiprocess Python Runtime** - Process-isolated Python nodes with zero-copy IPC via iceoryx2
3. **WASM Runtime** - Browser execution with hybrid Rust+Pyodide support

### Core Data Flow: Session Router + Multiprocess IPC Architecture

**Critical concept:** The system uses a **session-level async router** that runs persistently throughout the streaming session, continuously routing data packets between nodes. For multiprocess Python nodes, this is combined with **dedicated IPC threads** to handle iceoryx2's !Send types.

#### Session Router (`runtime/src/grpc_service/session_router.rs`)

The session router is the **central message broker** for all streaming pipelines:

```
┌─────────────────────────────────────────────────────────┐
│  gRPC Client Stream                                     │
│    ↓ (audio chunks)                                     │
│  SessionRouter (single persistent task per session)     │
│    │                                                     │
│    ├─→ Node Task 1 ──→ outputs ──→ router_tx ──┐       │
│    │                                             │       │
│    ├─→ Node Task 2 ──→ outputs ──→ router_tx ──┤       │
│    │                                             │       │
│    └─→ Node Task N ──→ outputs ──→ router_tx ──┘       │
│         ↓                                               │
│    Router Main Loop (collects all outputs)              │
│         ↓                                               │
│    client_tx (stream back to gRPC client)               │
└─────────────────────────────────────────────────────────┘
```

**Key behaviors:**
- One router per session, lives for entire streaming duration
- Each node runs in its own tokio task with dedicated input channel
- All node outputs flow back through a shared `router_tx` channel
- Router's main loop continuously: receives inputs → routes to nodes → collects outputs → sends to client
- Nodes process inputs in **pipelined fashion** (spawned as background tasks, don't block on completion)

#### Multiprocess IPC Architecture (`runtime/src/python/multiprocess/multiprocess_executor.rs`)

For Python nodes running in separate processes, data flows through **dedicated IPC threads**:

```
┌────────────────────────────────────────────────────────────────┐
│  Rust Runtime (async tokio)                                    │
│                                                                 │
│  SessionRouter Task                                            │
│    │                                                            │
│    ├─→ sends input via send_data_to_node()                    │
│    │      ↓                                                     │
│    │   IPC Thread (dedicated OS thread per node)               │
│    │      │                                                     │
│    │      ├─ Persistent iceoryx2 Publisher (!Send)             │
│    │      │    ↓ (zero-copy shared memory)                     │
│    │      │  Python Process (node_id)                          │
│    │      │    ↓ (yields outputs continuously)                 │
│    │      ├─ Persistent iceoryx2 Subscriber (!Send)            │
│    │      │                                                     │
│    │      └─ Continuous polling loop (yield_now when idle)     │
│    │           ↓ (forwards outputs immediately)                │
│    │      output_callback_tx                                   │
│    │           ↓                                                │
│    └─── Background Draining Task                               │
│           ↓ (converts IPCRuntimeData → RuntimeData)            │
│         router_tx ──→ Router Main Loop ──→ client              │
└────────────────────────────────────────────────────────────────┘
```

**Why dedicated IPC threads?**
- iceoryx2 `Publisher`/`Subscriber` are **!Send** (cannot cross thread boundaries)
- Must live on a single OS thread for their entire lifetime
- Async Rust code communicates via channels: `mpsc::Sender<IpcCommand>` → IPC thread

**Critical optimizations (Dec 2024):**
- IPC thread polling uses `std::thread::yield_now()` instead of sleep(1ms) to minimize latency
- Python polling uses `asyncio.sleep(0)` instead of sleep(10ms)
- These changes eliminated ~20ms of artificial latency for real-time audio streaming

**Global session storage:**
- Sessions stored in `GLOBAL_SESSIONS` static (`OnceLock<Arc<RwLock<HashMap>>>`)
- Allows session_router to find IPC threads across executor instances
- Maps: `session_id` → `node_id` → `mpsc::Sender<IpcCommand>`

**Continuous output draining:**
- Background task registered via `register_output_callback()` continuously drains outputs
- Runs independently of input processing (fire-and-forget architecture)
- Ensures outputs always reach client even during model initialization

### Module Structure

```
runtime/src/
├── grpc_service/          # gRPC server and streaming services
│   ├── session_router.rs  # Persistent session-level routing (CRITICAL)
│   ├── streaming.rs       # Bidirectional streaming RPC handler
│   ├── execution.rs       # Unary RPC handler
│   ├── server.rs          # Tonic server setup with middleware
│   └── generated/         # Protobuf-generated code
│
├── python/
│   ├── multiprocess/      # Process-isolated Python execution (spec 002)
│   │   ├── multiprocess_executor.rs  # IPC thread architecture (CRITICAL)
│   │   ├── process_manager.rs        # Process lifecycle
│   │   ├── ipc_channel.rs            # iceoryx2 channel registry
│   │   ├── data_transfer.rs          # IPC serialization
│   │   └── health_monitor.rs         # Process health tracking
│   │
│   ├── cpython_executor.rs  # DEPRECATED (use multiprocess instead)
│   ├── ffi.rs               # Python FFI entry points
│   ├── marshal.rs           # Python↔Rust data conversion
│   └── runtime_data_py.rs   # RuntimeData Python bindings
│
├── executor/
│   ├── scheduler.rs       # Topological sorting and execution order
│   ├── graph.rs           # Pipeline graph construction
│   ├── retry.rs           # Exponential backoff, circuit breaker
│   ├── metrics.rs         # Performance tracking (29μs overhead)
│   └── runtime_selector.rs # Rust vs Python runtime selection
│
├── nodes/
│   ├── audio/             # Native Rust audio nodes (2-16x faster)
│   │   ├── resample.rs    # High-quality resampling (rubato)
│   │   ├── vad.rs         # Voice activity detection (Silero VAD)
│   │   └── format.rs      # Zero-copy format conversion
│   │
│   ├── python_streaming.rs # Python node wrapper for streaming
│   └── registry.rs         # Node type registration
│
├── data/
│   ├── runtime_data.rs    # Core data types (Audio, Text, Image, etc.)
│   └── conversion.rs      # Protobuf ↔ RuntimeData conversion
│
└── manifest/
    ├── manifest.rs        # Pipeline definition (YAML/JSON)
    └── validation.rs      # Schema validation
```

### Key Execution Flows

#### 1. gRPC Streaming Pipeline

```rust
// Client opens bidirectional stream
StreamPipeline(request_stream) -> response_stream

// Server creates session + router
let session = StreamSession::new(manifest);
let (router, shutdown_tx) = SessionRouter::new(session_id, registry, session, client_tx);

// Router starts persistent background task
tokio::spawn(router.run());

// For each input chunk from client:
router.send_input(data)
  -> routes to first node
  -> node processes asynchronously
  -> outputs flow back via router_tx
  -> router forwards to client_tx
```

#### 2. Multiprocess Python Node Execution

```rust
// Initialization (once per session)
executor.initialize(ctx, session_id)
  -> spawn Python process with session_id
  -> create iceoryx2 channels: {session_id}_{node_id}_input/output
  -> spawn_ipc_thread() creates dedicated OS thread with persistent pub/sub
  -> register in GLOBAL_SESSIONS
  -> register_output_callback() for continuous draining

// During execution (per chunk)
send_data_to_node(node_id, session_id, data)
  -> lookup IPC thread from global sessions
  -> send IpcCommand::SendData via mpsc channel
  -> IPC thread publishes to iceoryx2 (zero-copy)
  -> Python receives, processes, yields outputs
  -> IPC thread subscriber receives outputs
  -> forwards via output_callback_tx
  -> background draining task converts and sends to router_tx
```

#### 3. Session-Scoped Channel Naming

All iceoryx2 channels are prefixed with `session_id` to avoid conflicts:

```rust
// Rust side
let input_channel = format!("{}_{}_input", session_id, node_id);
let output_channel = format!("{}_{}_output", session_id, node_id);

// Python side (runner.py)
input_channel_name = f"{session_id}_{node_id}_input"
output_channel_name = f"{session_id}_{node_id}_output"
```

This prevents routing failures when iceoryx2 fails to clean up node/service files.

### Python Integration Details

#### FFI Layer (`runtime/src/python/ffi.rs`)

Exposes Rust runtime to Python via PyO3:

```rust
#[pyfunction]
fn execute_pipeline(manifest_json: &str, input_data: PyObject) -> PyResult<PyObject>

#[pyfunction]
fn is_rust_runtime_available() -> PyResult<bool>
```

Python calls via:
```python
from remotemedia_runtime import execute_pipeline, is_rust_runtime_available
```

#### Zero-Copy Audio Transfer (`runtime/src/python/numpy_marshal.rs`)

Uses rust-numpy for zero-copy data transfer between Python numpy arrays and Rust:

```rust
// Python → Rust (zero-copy view)
let numpy_array: &PyArray1<f32> = array.extract()?;
let rust_slice: &[f32] = numpy_array.readonly().as_slice()?;

// Rust → Python (zero-copy)
let py_array = PyArray1::from_slice(py, rust_slice);
```

**Critical for performance:** Audio samples are never copied between Rust and Python.

#### Multiprocess Data Serialization (`runtime/src/python/multiprocess/data_transfer.rs`)

Custom binary format for iceoryx2 IPC:

```
Format: type (1 byte) | session_len (2 bytes) | session_id | timestamp (8 bytes) | payload_len (4 bytes) | payload

Audio: payload = f32 samples (little-endian)
Text:  payload = UTF-8 bytes
```

Python deserializes in `node.py:_receive_input()`.

## Common Patterns

### Adding a New Native Rust Node

1. Create in `runtime/src/nodes/` (e.g., `new_node.rs`)
2. Implement `StreamingNode` trait:
   ```rust
   #[async_trait]
   impl StreamingNode for NewNode {
       async fn process_streaming(&self, input: RuntimeData) -> Result<Vec<RuntimeData>>
   }
   ```
3. Register in `runtime/src/nodes/registry.rs`:
   ```rust
   registry.register("NewNode", Arc::new(NewNodeFactory));
   ```

### Adding a Multiprocess Python Node

1. Create Python class in `python-client/remotemedia/nodes/`
2. Extend `MultiprocessNode` base class:
   ```python
   class NewNode(MultiprocessNode):
       async def process(self, data: RuntimeData) -> RuntimeData:
           # For streaming, use async generator:
           async def process(self, data: RuntimeData):
               yield output1
               yield output2
   ```
3. Register in manifest:
   ```yaml
   nodes:
     - id: new_node
       node_type: NewNode
       executor: multiprocess
   ```

### Performance Debugging

Critical performance knobs:

1. **IPC Thread Polling** (`multiprocess_executor.rs:1107`):
   ```rust
   std::thread::yield_now();  // NOT sleep(1ms)!
   ```

2. **Python Processing Loop** (`node.py:431`):
   ```python
   await asyncio.sleep(0)  # NOT sleep(0.01)!
   ```

3. **Session Router Pipelining** (`session_router.rs:448`):
   ```rust
   tokio::spawn(async move {
       node.process_streaming_async(...).await  // Don't await in main loop
   });
   ```

### Error Handling

All errors flow through `crate::Error` enum:

```rust
pub enum Error {
    Execution(String),      // Node execution failures
    InvalidData(String),    // Data validation errors
    IpcError(String),       // iceoryx2 IPC failures
    ProcessError(String),   // Python process crashes
    // ...
}
```

Python exceptions are caught and converted to `Error::Execution` at FFI boundary.

## Important Constraints

### iceoryx2 !Send Types

**Never** try to store `iceoryx2::Publisher` or `Subscriber` in async contexts:
- They are `!Send` (cannot cross thread boundaries)
- Must live on a dedicated OS thread
- Communicate via channels from async code

See detailed explanation in `multiprocess_executor.rs:24-50`.

### Session Lifecycle

Sessions are created per gRPC stream and persist until:
1. Client closes stream, OR
2. Error occurs, OR
3. `terminate_session()` is explicitly called

Always clean up in `terminate_session()`:
```rust
// Send shutdown to IPC threads
ipc_thread.command_tx.send(IpcCommand::Shutdown).await?;

// Remove from global storage
global_sessions().write().await.remove(session_id);
```

### Python Process Communication

Python processes communicate **only** via iceoryx2 IPC, not via Python multiprocessing:
- stdin/stdout/stderr for logging only
- Control channel for READY signal
- Input/output channels for data transfer

## Platform-Specific Notes

### Windows
- Use `powershell.exe` scripts in `.specify/scripts/powershell/`
- Memory measurement via `windows` crate APIs
- Process signals use `ctrlc` crate (no Unix signals)

### Linux/macOS
- Process signals via `nix` crate
- Memory from `/proc` filesystem (Linux) or system APIs (macOS)
- iceoryx2 requires `libc` for shared memory

## Related Documentation

For deeper architectural context, see:
- `docs/NATIVE_ACCELERATION.md` - Rust acceleration architecture
- `docs/PERFORMANCE_TUNING.md` - Optimization strategies
- `specs/001-native-rust-acceleration/` - Original design specs
- `specs/002-grpc-multiprocess-integration/` - IPC architecture specs
