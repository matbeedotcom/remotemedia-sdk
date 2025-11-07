# Implementation Plan: Transport Layer Decoupling

**Branch**: `003-transport-decoupling` | **Date**: 2025-01-06 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/003-transport-decoupling/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Decouple the RemoteMedia runtime core from transport implementations (gRPC, FFI, WebRTC) by creating a trait-based abstraction layer. Runtime-core will be a pure library with zero transport dependencies, and each transport (gRPC, FFI) will be extracted into separate crates that implement the `PipelineTransport` trait. This enables independent evolution, faster builds, reduced dependencies, and a plugin architecture for custom transports.

## Technical Context

**Language/Version**: Rust 1.75+ (stable)
**Primary Dependencies**:
- Runtime-core: tokio, serde, iceoryx2, rubato, rustfft (NO transport deps)
- Transport crates: tonic/prost (gRPC), pyo3 (FFI), each depending on runtime-core

**Storage**: N/A (stateless service)
**Testing**: cargo test, integration tests via mock transports
**Target Platform**: Multi-platform (native Linux/macOS/Windows, WASM for browser)
**Project Type**: Workspace with multiple crates (runtime-core + transport crates)
**Performance Goals**:
- Runtime-core build time <45s (from 60s+)
- Transport crate build time <30s each
- Zero dependency overhead when using core only

**Constraints**:
- MUST maintain backward compatibility during migration (2 release cycles minimum)
- MUST NOT break existing gRPC/FFI functionality
- Runtime-core MUST have zero dependencies on tonic, prost, pyo3, tower, hyper
- Migration timeline: 4 weeks maximum

**Scale/Scope**:
- 3 transport implementations initially (gRPC, FFI, WebRTC placeholder)
- ~50+ files affected in runtime/ directory
- Workspace restructuring with 4+ crates

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Note**: No project constitution file exists yet (`constitution.md` is empty template). The following checks are based on the existing codebase patterns and the TRANSPORT_DECOUPLING_ARCHITECTURE.md document already created:

### Architecture Principles (from existing codebase)

- ✅ **Dependency Inversion**: Transport abstractions follow DIP - core defines traits, transports implement
- ✅ **Single Responsibility**: Each crate has one purpose (core runtime OR specific transport)
- ✅ **Open/Closed**: System is open for extension (new transports) without modifying core
- ✅ **Independent Deployment**: Transports can be updated without core changes

### Testing Requirements

- ✅ **Unit Testing**: Mock transports enable pure unit testing of core logic
- ⚠️ **Integration Testing**: Requires strategy for testing transport implementations against core
- ✅ **Contract Testing**: Trait boundaries provide clear contracts to validate

### Performance Standards

- ✅ **Build Performance**: Explicit goal of <45s for core, <30s per transport
- ✅ **Runtime Performance**: No performance degradation (trait dispatch has negligible overhead)

### Breaking Change Management

- ✅ **Incremental Migration**: FR-014 requires 2-release backward compatibility
- ✅ **Feature Flags**: FR-007 requires feature flag support during migration
- ✅ **Semantic Versioning**: Each crate independently versioned per FR-009

**Constitution Check Result**: ✅ **PASS** - All applicable principles satisfied

**Post-Design Re-check**: ✅ **PASS** (after Phase 1 design artifacts)

After generating contracts, data model, and quickstart guide, re-validated against architecture principles:

- ✅ **API Contracts**: Traits provide clear, testable contracts (see `contracts/` directory)
- ✅ **Separation of Concerns**: TransportData cleanly separates core payload from transport metadata
- ✅ **Encapsulation**: PipelineRunner hides implementation details from transports
- ✅ **Testability**: Mock implementations demonstrated in quickstart.md
- ✅ **Documentation**: Comprehensive API docs in contracts with examples

**No new violations introduced** during design phase.

## Project Structure

### Documentation (this feature)

```text
specs/003-transport-decoupling/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
│   ├── PipelineTransport.trait.rs    # Core transport trait
│   ├── StreamSession.trait.rs        # Streaming session trait
│   └── TransportData.rs               # Data container type
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

**Current Structure** (before refactoring):
```text
runtime/
├── src/
│   ├── grpc_service/          # To be extracted → transports/remotemedia-grpc
│   ├── python/
│   │   ├── ffi.rs             # To be extracted → transports/remotemedia-ffi
│   │   └── multiprocess/      # Stays in core
│   ├── executor/              # Stays in core
│   ├── nodes/                 # Stays in core
│   ├── data/                  # Stays in core
│   └── manifest/              # Stays in core
└── Cargo.toml                 # Becomes workspace root
```

**Target Structure** (after refactoring):
```text
remotemedia-sdk/
├── Cargo.toml                        # Workspace root
│
├── runtime-core/                     # NEW: Pure runtime library
│   ├── Cargo.toml                    # NO transport dependencies
│   ├── src/
│   │   ├── lib.rs
│   │   ├── executor/                 # SessionRouter, Executor
│   │   ├── nodes/                    # Node registry, audio nodes
│   │   ├── data/                     # RuntimeData types
│   │   ├── manifest/                 # Manifest parsing
│   │   ├── python/multiprocess/      # IPC, process management
│   │   ├── transport/                # NEW: Transport abstractions
│   │   │   ├── mod.rs                # PipelineTransport trait
│   │   │   ├── runner.rs             # PipelineRunner impl
│   │   │   ├── session.rs            # StreamSession trait
│   │   │   └── data.rs               # TransportData type
│   │   └── error.rs
│   └── tests/
│       └── mock_transport.rs         # Mock for testing
│
├── transports/                       # NEW: Transport implementations
│   │
│   ├── remotemedia-grpc/             # Extracted from runtime/grpc_service
│   │   ├── Cargo.toml                # Depends: runtime-core, tonic, prost
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── server.rs             # Tonic server setup
│   │   │   ├── service.rs            # Service implementation
│   │   │   ├── streaming.rs          # Streaming handler
│   │   │   ├── execution.rs          # Unary handler
│   │   │   ├── adapters.rs           # RuntimeData ↔ Protobuf
│   │   │   ├── auth.rs
│   │   │   ├── metrics.rs
│   │   │   └── generated/            # Protobuf types
│   │   ├── protos/
│   │   ├── build.rs
│   │   └── bin/
│   │       └── grpc-server.rs        # Binary entry point
│   │
│   ├── remotemedia-ffi/              # Extracted from runtime/python/ffi.rs
│   │   ├── Cargo.toml                # Depends: runtime-core, pyo3
│   │   ├── src/
│   │   │   ├── lib.rs                # PyO3 module definition
│   │   │   ├── api.rs                # Python-facing API
│   │   │   ├── marshal.rs            # Python ↔ RuntimeData
│   │   │   └── numpy_bridge.rs       # Zero-copy numpy
│   │   └── python/
│   │       └── remotemedia/
│   │           └── __init__.py
│   │
│   └── remotemedia-webrtc/           # Placeholder for future
│       ├── Cargo.toml                # Depends: runtime-core, webrtc
│       └── src/
│           └── lib.rs
│
├── python-client/                    # Updated to use FFI transport
│   └── remotemedia/
│       └── runtime.py                # Import remotemedia-ffi
│
└── examples/
    ├── grpc-server/                  # Example using remotemedia-grpc
    ├── python-sdk/                   # Example using remotemedia-ffi
    └── custom-transport/             # Example custom transport
```

**Structure Decision**: Selected **Workspace with Multiple Crates** structure. This is appropriate because:

1. **Clear Separation**: Each transport is truly independent with its own Cargo.toml and dependency tree
2. **Independent Versioning**: Each crate can have its own version number (FR-009)
3. **Selective Compilation**: Users only compile what they need
4. **Workspace Benefits**: Shared dependency resolution and unified build commands

The structure follows Rust best practices for multi-crate projects where components have different dependency requirements.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No violations to justify. The workspace structure and trait-based architecture are standard Rust patterns for dependency decoupling.
