# Feature Specification: Transport Layer Decoupling

**Feature Branch**: `003-transport-decoupling`
**Created**: 2025-01-06
**Status**: Draft
**Input**: User description: "Decouple runtime core from transport implementations by extracting gRPC, FFI, and WebRTC into separate crates that depend on a pure runtime-core library"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - SDK Developer Uses Core Without Transports (Priority: P1)

A developer building a custom transport (e.g., message queue, HTTP REST API, custom IPC) needs to integrate the RemoteMedia runtime without pulling in unused gRPC or FFI dependencies.

**Why this priority**: This is the foundational use case that validates the decoupling architecture. If developers cannot use runtime-core independently, the entire refactoring fails its primary goal.

**Independent Test**: Can be fully tested by creating a minimal Rust project that depends only on `remotemedia-runtime-core`, implements the `PipelineTransport` trait for a mock transport, and successfully executes a pipeline without any gRPC or PyO3 dependencies in the dependency tree.

**Acceptance Scenarios**:

1. **Given** a new Rust project with only `remotemedia-runtime-core` as a dependency, **When** the developer runs `cargo build`, **Then** the build succeeds without pulling in tonic, prost, pyo3, tower, or hyper
2. **Given** runtime-core is available, **When** a developer implements a custom `PipelineTransport` trait, **Then** they can execute pipelines using only the core abstractions
3. **Given** the developer has implemented a custom transport, **When** they run a pipeline with audio nodes, **Then** the pipeline executes successfully using the custom transport

---

### User Story 2 - Service Operator Deploys gRPC Server (Priority: P2)

A service operator wants to deploy a gRPC server for RemoteMedia pipelines without concern that core runtime bugs will require transport code changes, or vice versa.

**Why this priority**: Independent deployment and maintenance is a key benefit of decoupling. This validates that transports can evolve separately from core.

**Independent Test**: Can be fully tested by updating `remotemedia-grpc` crate to a new version, rebuilding the gRPC server binary, and verifying that existing pipeline functionality continues working without any changes to `runtime-core`.

**Acceptance Scenarios**:

1. **Given** a deployed gRPC server using `remotemedia-grpc`, **When** the operator updates only the gRPC transport crate (e.g., to tonic 1.0), **Then** the server continues processing pipelines without runtime-core changes
2. **Given** runtime-core receives a bug fix, **When** the operator updates only runtime-core, **Then** the gRPC server benefits from the fix without recompiling transport code
3. **Given** the operator wants to add WebSocket transport, **When** they create a new `remotemedia-websocket` crate, **Then** they can implement it without modifying runtime-core or existing transports

---

### User Story 3 - Python SDK User Integrates Runtime (Priority: P2)

A Python developer using the RemoteMedia Python SDK wants faster installation and smaller footprint without gRPC dependencies if they're only using the direct FFI interface.

**Why this priority**: This demonstrates the practical benefit of reduced dependencies for end users, improving developer experience and installation time.

**Independent Test**: Can be fully tested by measuring Python package installation time and size before/after decoupling, and verifying that `pip install remotemedia` (FFI-only) does not trigger compilation of gRPC-related native code.

**Acceptance Scenarios**:

1. **Given** a Python developer installs the remotemedia package, **When** they use only FFI functions (not gRPC client), **Then** the installation does not compile or download gRPC dependencies
2. **Given** the Python SDK is installed, **When** the developer imports and uses the runtime, **Then** the import time is reduced by at least 30% compared to the monolithic version
3. **Given** the FFI transport receives a performance optimization, **When** the Python package is rebuilt, **Then** the optimization is available without requiring gRPC transport changes

---

### User Story 4 - Contributor Tests Core Logic (Priority: P3)

A contributor wants to test core pipeline execution logic without setting up real gRPC servers or Python FFI environments.

**Why this priority**: This improves development velocity and test reliability, but is less critical than the architectural goals (P1) and deployment scenarios (P2).

**Independent Test**: Can be fully tested by writing a unit test in `runtime-core/tests/` that uses a mock `PipelineTransport` implementation to verify executor behavior, without any transport dependencies.

**Acceptance Scenarios**:

1. **Given** a contributor writes a test for pipeline execution, **When** they use a mock transport implementation, **Then** the test runs in under 1 second without network or subprocess overhead
2. **Given** core runtime logic changes, **When** the contributor runs `cargo test` in runtime-core, **Then** all tests pass without requiring transport crates to be available
3. **Given** the contributor wants to debug a routing issue, **When** they create a minimal test case with synthetic data, **Then** they can reproduce and fix the issue using only runtime-core

---

### Edge Cases

- What happens when a transport implementation violates the `PipelineTransport` trait contract (e.g., sends invalid data)?
- How does the system handle version mismatches between runtime-core and transport crates?
- What happens if a custom transport blocks indefinitely in an async method?
- How does the system handle multiple transports trying to use the same session ID?
- What happens when runtime-core is updated with breaking changes to the trait API?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a trait-based abstraction (`PipelineTransport`) that all transports implement
- **FR-002**: Runtime-core MUST NOT depend on any transport-specific crates (tonic, prost, pyo3, webrtc)
- **FR-003**: Runtime-core MUST provide a `PipelineRunner` that transports can use to execute pipelines
- **FR-004**: System MUST support both unary (single request/response) and streaming execution modes via the trait abstraction
- **FR-005**: Each transport MUST be in a separate crate that depends on runtime-core
- **FR-006**: Runtime-core MUST define a transport-agnostic data container (`TransportData`) that wraps `RuntimeData`
- **FR-007**: System MUST maintain backward compatibility during migration via feature flags and re-exports
- **FR-008**: Runtime-core MUST expose all necessary types (SessionRouter, Executor, Node registries) for transports to use
- **FR-009**: System MUST support independent versioning of runtime-core and transport crates
- **FR-010**: Each transport MUST handle its own serialization format (Protobuf for gRPC, Python objects for FFI)
- **FR-011**: System MUST provide a `StreamSession` trait for stateful streaming operations
- **FR-012**: Runtime-core MUST manage session lifecycle (creation, active processing, cleanup) independently of transport
- **FR-013**: System MUST allow multiple transport implementations to coexist in the same workspace
- **FR-014**: Migration MUST be incremental, allowing old monolithic code to coexist with new decoupled code for at least 2 release cycles

### Key Entities

- **PipelineTransport**: Trait that defines the interface all transports must implement (execute and stream methods)
- **StreamSession**: Trait that defines the interface for stateful streaming pipeline sessions (send_input, recv_output, close)
- **TransportData**: Transport-agnostic data container that wraps RuntimeData with optional metadata (sequence numbers, transport-specific headers)
- **PipelineRunner**: Core executor that transports use to run pipelines, hiding internal implementation details
- **StreamSessionHandle**: Concrete implementation of StreamSession provided by runtime-core
- **Transport Crate**: Independent crate (remotemedia-grpc, remotemedia-ffi) that implements PipelineTransport and depends on runtime-core

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Runtime-core builds in under 45 seconds (reduced from 60+ seconds with all transports)
- **SC-002**: A minimal custom transport implementation requires fewer than 100 lines of Rust code
- **SC-003**: Runtime-core has zero dependencies on transport-specific crates (verified by `cargo tree`)
- **SC-004**: Migration completes within 4 weeks with all existing integration tests passing
- **SC-005**: Python SDK installation time reduces by at least 30% when only FFI transport is used
- **SC-006**: Developers can create and test a new transport without modifying runtime-core
- **SC-007**: All three transports (gRPC, FFI, WebRTC placeholder) can be independently versioned and released
- **SC-008**: Build times for individual transport crates are under 30 seconds each
- **SC-009**: Test suite for runtime-core runs without any transport dependencies present
- **SC-010**: Documentation provides clear examples of implementing custom transports using the trait API
