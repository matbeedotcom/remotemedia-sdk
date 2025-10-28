# Implementation Plan: Native Rust gRPC Service for Remote Execution

**Branch**: `003-rust-grpc-service` | **Date**: 2025-10-28 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/003-rust-grpc-service/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Create a high-performance gRPC service that executes audio processing pipelines using the native Rust runtime (v0.2.1) without Python FFI overhead. The service accepts pipeline manifests via gRPC, executes them directly in Rust, and returns results via protocol buffer serialization. Target performance: 10x faster than current Python-based remote execution, <5ms latency for simple operations, support for 1000+ concurrent connections.

## Technical Context

**Language/Version**: Rust 1.75+ (stable)  
**Primary Dependencies**: 
- tonic 0.10+ (gRPC framework)
- prost 0.12+ (Protocol Buffers)
- tokio 1.35+ (async runtime)
- serde_json 1.0+ (JSON manifest parsing)
- remotemedia_runtime v0.2.1 (existing Rust pipeline executor)
- tracing/tracing-subscriber (structured logging)
- prometheus 0.13+ (metrics)

**Storage**: In-memory execution state only (stateless service), configuration from environment variables/config files  
**Testing**: cargo test (unit), integration tests via gRPC client, load testing with ghz/k6  
**Target Platform**: Linux server (primary), cross-platform support (macOS, Windows)  
**Project Type**: Single server application (gRPC + HTTP metrics endpoint)  
**Performance Goals**: 
- <5ms p50 latency for simple operations (1-second audio resample)
- <10ms p95 latency
- 1000+ concurrent connections
- <10MB memory per concurrent execution
- 10x faster than Python-based remote execution

**Constraints**: 
- Zero Python FFI overhead (pure Rust execution path)
- <10% serialization overhead vs local execution
- 99.9% uptime (graceful degradation under load)
- Backward compatible protocol (version negotiation)
- Single instance per environment (future mesh-ready architecture)

**Scale/Scope**: 
- Initial: Single gRPC service binary
- ~2000-3000 LoC (service layer + proto definitions)
- 4 RPC methods (ExecutePipeline, StreamPipeline, GetVersion, GetMetrics)
- Reuses existing runtime (~23K LoC from v0.2.1)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Constitution Status**: Template constitution detected (no project-specific gates defined). Proceeding with best practices gates:

✅ **Library-First Principle**: Service wraps existing remotemedia_runtime library (v0.2.1) without modifications. Runtime remains independently testable.

✅ **Test-First Development**: Will generate contract tests from protobuf schemas, integration tests for each user story, unit tests for authentication/validation logic.

✅ **Observability**: Structured JSON logging to stdout, Prometheus metrics endpoint, request tracing with correlation IDs.

✅ **Versioning**: Protocol version in every request/response, compatibility matrix published, semver for service releases.

⚠️ **Complexity Justification Required**: Adding new network service alongside existing Python gRPC service - see Complexity Tracking section.

## Project Structure

### Documentation (this feature)

```text
specs/003-rust-grpc-service/
├── plan.md              # This file
├── research.md          # Phase 0: Technology choices, patterns
├── data-model.md        # Phase 1: Protocol buffer schemas
├── quickstart.md        # Phase 1: Getting started guide
├── contracts/           # Phase 1: .proto files
│   ├── execution.proto  # ExecutePipeline RPC
│   ├── streaming.proto  # StreamPipeline RPC
│   └── common.proto     # Shared types
└── tasks.md             # Phase 2: Implementation tasks (NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
runtime/
├── src/
│   ├── grpc_service/          # NEW: gRPC service implementation
│   │   ├── mod.rs
│   │   ├── server.rs          # tonic server setup
│   │   ├── execution.rs       # ExecutePipeline handler
│   │   ├── streaming.rs       # StreamPipeline handler
│   │   ├── auth.rs            # Token authentication
│   │   ├── limits.rs          # Resource limit enforcement
│   │   ├── metrics.rs         # Prometheus metrics
│   │   └── version.rs         # Version negotiation
│   ├── executor/              # EXISTING: Rust runtime (unchanged)
│   ├── nodes/                 # EXISTING: Audio nodes (unchanged)
│   └── lib.rs
├── protos/                    # NEW: Protocol buffer definitions
│   ├── execution.proto
│   ├── streaming.proto
│   └── common.proto
├── build.rs                   # UPDATED: Add prost-build for protos
├── Cargo.toml                 # UPDATED: Add gRPC dependencies
└── tests/
    ├── grpc_integration/      # NEW: gRPC service tests
    │   ├── test_execution.rs
    │   ├── test_streaming.rs
    │   ├── test_auth.rs
    │   ├── test_limits.rs
    │   └── test_version.rs
    └── ...                    # EXISTING: Runtime tests (unchanged)

runtime/bin/                   # NEW: Service binary
└── grpc_server.rs             # Main entry point, CLI args, config

nodejs-client/                 # EXISTING: Will add gRPC client
├── src/
│   └── grpc_client.ts         # NEW: TypeScript gRPC client wrapper

python-client/                 # EXISTING: Will add gRPC client  
├── remotemedia/
│   └── grpc_client.py         # NEW: Python gRPC client wrapper
```

**Structure Decision**: Integrate gRPC service into existing `runtime/` crate to share the pipeline executor directly. This avoids creating a separate binary project and keeps service code co-located with the runtime it wraps. Client libraries (Python, TypeScript) get new gRPC client modules alongside existing SDK code.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Second network service (Rust gRPC alongside Python gRPC) | Eliminates Python FFI overhead for 10x performance improvement. Current Python service has 8+ serialization steps; Rust→Rust has 2 (protobuf only). Critical for SC-004 (10x faster) and SC-001 (<5ms latency). | Extending Python service: Still requires FFI crossings (Python→Rust→Python), adds 400μs overhead per request. Performance target unachievable with FFI in path. |
| Protocol versioning complexity (compatibility matrix) | Required for zero-downtime upgrades and future service mesh migration (clarification Q1). Enables rolling deployments without breaking existing clients. | Strict version matching: Forces simultaneous client/server upgrades, violates SC-006 (99.9% uptime). Backward compatibility guarantee: Unsustainable maintenance burden, blocks protocol evolution. |
