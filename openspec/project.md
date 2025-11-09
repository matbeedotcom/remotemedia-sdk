# Project Context

## Purpose

RemoteMedia SDK is a distributed audio/video/data processing framework that enables developers to build complex, real-time processing applications with transparent remote offloading capabilities. The SDK provides seamless execution of computationally intensive tasks on remote servers while maintaining a simple, intuitive API and preserving object state across network boundaries.

**Key Goals:**
- Enable transparent remote execution of Python objects and processing nodes
- Provide a rich library of pre-built audio/video/data processing nodes
- Support real-time streaming with WebRTC and bidirectional gRPC
- Maintain cross-language interoperability (Python and TypeScript/Node.js)
- Offer secure, sandboxed execution environments for custom code
- Support ML model integration (Transformers, Kokoro TTS, Qwen3, Ultravox)

## Tech Stack

### Primary Languages
- **Python 3.9+**: Core SDK, service backend, examples (primary language)
- **Rust**: Language-neutral runtime, performance-critical execution engine
- **TypeScript/JavaScript**: Node.js client library
- **Protocol Buffers (proto3)**: gRPC service definitions and data serialization

### Core Technologies
- **gRPC**: Remote Procedure Call framework for client-server communication
- **Protocol Buffers**: Language-agnostic data serialization
- **aiortc**: WebRTC implementation for real-time audio/video streaming
- **aiohttp**: Async HTTP server framework
- **CloudPickle**: Serialization of complex Python objects (classes, functions, closures)
- **asyncio**: Asynchronous programming throughout the codebase

### Rust Runtime Stack
- **Tokio**: Async runtime for concurrent execution
- **PyO3 0.26**: Python FFI bindings with Bound API and freethreaded support
- **RustPython**: Embedded Python interpreter for node execution
- **rust-numpy**: Zero-copy numpy array marshaling
- **Wasmtime**: WASM runtime for sandboxed execution (Phase 3)
- **webrtc-rs**: WebRTC implementation for peer-to-peer transport (Phase 2)
- **serde**: Serialization framework for manifest parsing

### Audio/Video Processing
- **PyAV (av)**: Python bindings for FFmpeg
- **librosa**: Audio analysis and processing
- **soundfile**: Audio file I/O
- **numpy**: Numerical computing for audio/video data

### Machine Learning
- **Transformers (Hugging Face)**: NLP and ML model integration
- **Kokoro TTS**: Text-to-speech synthesis
- **Qwen3**: Multimodal AI models
- **Ultravox**: Voice AI pipeline

### Build & Development Tools
- **setuptools**: Python package management
- **pytest**: Testing framework
- **npm/workspaces**: Node.js monorepo management
- **Docker**: Containerization support
- **ruff**: Python linting
- **black**: Python code formatting
- **mypy**: Static type checking

## Project Conventions

### Code Style

**Python:**
- Line length: 88 characters (Black default)
- Target version: Python 3.9+
- Follow PEP 8 naming conventions:
  - Classes: `PascalCase` (e.g., `AudioTransform`, `RemoteExecutionNode`)
  - Functions/methods: `snake_case` (e.g., `process()`, `add_node()`)
  - Constants: `UPPER_SNAKE_CASE` (e.g., `_SENTINEL`, `_EMPTY`)
  - Private members: prefix with single underscore (e.g., `_is_initialized`)
- Use type hints for all function signatures
- Docstrings: Google-style docstrings for all public classes and methods
- Linting: ruff with select rules ["E", "F", "W", "I", "N"], ignore E501
- Formatting: black (automatic)
- Type checking: mypy with strict settings (disallow_untyped_defs = true)

**TypeScript/Node.js:**
- Follow ESLint and Prettier configurations
- Use TypeScript 5.0+ features
- Export types alongside implementations
- Prefer `async/await` over callbacks

**Rust:**
- Edition: 2021, Rust version 1.70+
- Formatting: rustfmt (automatic via cargo fmt)
- Linting: clippy (cargo clippy)
- Naming conventions:
  - Types/Traits: `PascalCase` (e.g., `Executor`, `PythonVm`)
  - Functions/methods: `snake_case` (e.g., `execute_pipeline`, `json_to_python`)
  - Constants: `UPPER_SNAKE_CASE` (e.g., `MAX_RETRIES`)
  - Modules: `snake_case` (e.g., `python/marshal.rs`)
- Use `Result<T>` and `?` operator for error propagation
- Document public APIs with `///` doc comments
- Use `tracing` crate for structured logging
- Prefer `&str` over `String` for function parameters
- Use `async fn` with tokio for async operations
- PyO3 0.26 Bound API patterns:
  - Use `Bound<'py, PyAny>` for Python object references
  - Use `.into_bound_py_any()` for Rust → Python conversions
  - Use `Python::with_gil()` for GIL access (auto-initialize feature enabled)

**General:**
- Use logging extensively with appropriate levels (DEBUG, INFO, WARNING, ERROR)
- Logger naming: `logger = logging.getLogger(__name__)`
- All errors should have descriptive messages with context

### Architecture Patterns

**Node-Based Processing:**
- All processing units inherit from `Node` base class
- Nodes implement a `process()` method for synchronous operations
- Nodes can implement `aprocess()` for async operations
- Nodes support both local and remote execution transparently
- Use TypedDict for structured error outputs

**Pipeline Pattern:**
- Pipelines orchestrate sequences of nodes
- Support method chaining: `pipeline.add_node(node1).add_node(node2)`
- Pipelines are immutable once initialized
- Support both sync and async execution modes
- Streaming support for real-time processing

**Remote Execution:**
- Use gRPC for client-server communication
- Serialize Python objects with CloudPickle for remote execution
- Maintain session state across multiple remote calls
- Support bidirectional streaming for real-time data

**Dependency Injection:**
- Configuration via `ServiceConfig` class
- Custom nodes registered via dependency injection in `TaskExecutor`
- Node discovery system for dynamic loading of extensions

**Error Handling:**
- Nodes return error dictionaries instead of raising exceptions
- Use structured error types (TypedDict) with `error`, `input`, `processed_by` fields
- Log all errors with context before returning error objects
- Service-level errors use standard Python exceptions

**Async-First Design:**
- Use `asyncio` throughout the codebase
- Prefer async generators for streaming operations
- Support both sync and async APIs for compatibility
- Use context managers for resource cleanup

### Testing Strategy

**Test Organization:**
- Test files located in `tests/`, `python-client/tests/`, `service/tests/`
- Naming: `test_*.py` or `*_test.py`
- Test classes: `Test*` prefix
- Test functions: `test_*` prefix

**Testing Tools:**
- pytest for test execution
- pytest-asyncio for async test support
- pytest-grpc for gRPC service testing
- cargo test for Rust unit and integration tests
- criterion for Rust performance benchmarks

**Testing Requirements:**
- All new nodes should have unit tests
- Test both success and error cases
- Test async and sync execution paths
- Integration tests for gRPC services
- Test remote execution with mock servers
- Rust: Test all public APIs, include doctests for examples
- Rust: Marshaling tests must verify round-trip data integrity

### Git Workflow

**Branching:**
- Main branch: `main`
- Development happens on feature branches
- Branch naming: descriptive names (e.g., `custom-server-discovery`, `webrtc-support`)

**Commit Conventions:**
- Write descriptive commit messages
- Focus on "why" rather than "what"
- Recent commit style examples:
  - "Add custom server and node discovery for enhanced RemoteMedia service"
  - "Configure flexible installation patterns with client SDK as default"
  - "Restructure mono-repo with shared protobuf definitions"

**Repository Structure:**
- Monorepo with multiple packages: `python-client/`, `service/`, `nodejs-client/`, `runtime/`
- Shared protobuf definitions in `remotemedia/protos/`
- Examples in `examples/` and `webrtc-example/`
- Rust runtime workspace in `runtime/` with cdylib for Python FFI

## Domain Context

**RemoteMedia Processing Concepts:**
- **Node**: A processing unit that transforms input to output
- **Pipeline**: A sequence of nodes that processes data through multiple stages
- **Remote Execution**: Offloading node execution to a server via gRPC
- **SDK Nodes**: Pre-built processing nodes shipped with the SDK
- **Custom Nodes**: User-defined nodes loaded dynamically by the service
- **Streaming**: Real-time processing of audio/video/data streams
- **Sentinel Values**: Special objects like `_SENTINEL` and `_EMPTY` for flow control

**Rust Runtime Architecture:**
- **Manifest-Driven Execution**: Pipelines serialize to JSON manifests for language-neutral execution
- **Dual Python Runtimes**: RustPython (embedded) for node execution, PyO3 (FFI) for SDK integration
- **Data Marshaling**: Bidirectional conversion between Python and Rust with zero-copy numpy support
- **Async Execution**: Tokio-based async runtime for concurrent pipeline execution
- **VM Pooling**: Reusable RustPython VM instances for efficient node execution
- **FFI Functions**: `execute_pipeline()` and `execute_pipeline_with_input()` exposed to Python
- **Module Organization**:
  - `executor/`: Pipeline orchestration and execution engine
  - `manifest/`: JSON manifest parsing and validation
  - `python/ffi`: PyO3 FFI functions for Python SDK
  - `python/marshal`: Data type conversion (Python ↔ Rust)
  - `python/numpy_marshal`: Zero-copy numpy array handling
  - `python/vm`: RustPython VM lifecycle management
  - `python/node_executor`: High-level Python node execution

**Processing Modes:**
- **Local Execution**: Nodes run in the client process
- **Remote Execution**: Nodes run on a gRPC server
- **Hybrid Pipelines**: Mix of local and remote nodes in the same pipeline
- **WebRTC Streaming**: Real-time audio/video through browser connections

**Data Flow:**
1. Client creates a pipeline with nodes
2. Pipeline.run() or Pipeline.arun() executes the sequence
3. Each node processes data and passes to the next
4. Remote nodes transparently execute on the server
5. Results stream back to the client

**Serialization:**
- Simple types: Protocol Buffers
- Complex Python objects: CloudPickle
- Audio/Video: PyAV frames or numpy arrays
- Text: UTF-8 strings

## Important Constraints

**Technical Constraints:**
- Python 3.9+ required (uses modern type hints and asyncio features)
- Node.js 14.0+ required for TypeScript client
- gRPC version compatibility: 1.50.0+
- Protocol Buffer proto3 syntax required
- Windows/Linux/macOS support (cross-platform)
- Custom nodes must inherit from `Node` base class
- Remote execution requires CloudPickle-serializable objects

**Performance Constraints:**
- Real-time audio processing requires low latency
- WebRTC streaming has strict timing requirements
- Large audio/video buffers can cause memory pressure
- gRPC message size limits (default 4MB, configurable)

**Security Constraints:**
- Sandbox execution environment for untrusted code
- Resource limits enforced by SandboxManager
- No automatic execution of arbitrary code without explicit client request
- gRPC uses insecure channels by default (add TLS for production)

**Development Constraints:**
- Monorepo structure requires careful dependency management
- Hugging Face cache directory: Configurable via `HF_HOME` env var
- Node discovery requires YAML configuration
- Generated protobuf code must stay in sync across packages

## External Dependencies

**ML Models & Services:**
- **Hugging Face Hub**: Model downloads and transformers pipelines
- **Kokoro TTS**: Text-to-speech synthesis models
- **Qwen3**: Multimodal AI models from Alibaba
- **Ultravox**: Voice AI pipeline for transcription/understanding

**System Libraries:**
- **FFmpeg**: Required by PyAV for audio/video codec support
- **PortAudio**: May be required for audio I/O on some platforms
- **libsoundfile**: Required by soundfile library

**Development Services:**
- **GitHub**: Source code repository (https://github.com/matbeeDOTcom/remotemedia-sdk)
- **npm Registry**: For publishing @remotemedia/nodejs-client package

**Optional Dependencies:**
- **MQTT Broker**: For async messaging (asyncio-mqtt)
- **Docker Registry**: For container deployments
- **gRPC Health Checking**: Standard gRPC health protocol

**Protocol Buffer Definitions:**
- Shared across Python and Node.js clients
- Located in `remotemedia/protos/`: `execution.proto`, `types.proto`
- Auto-generated stubs must be regenerated when .proto files change
