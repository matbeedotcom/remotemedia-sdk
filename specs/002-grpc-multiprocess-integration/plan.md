# Implementation Plan: gRPC Multiprocess Integration

**Branch**: `002-grpc-multiprocess-integration` | **Date**: 2025-11-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-grpc-multiprocess-integration/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Enable the gRPC service to execute pipelines containing Python nodes using the multiprocess executor (from spec 001), allowing clients to submit manifest.v1.json format pipelines that benefit from concurrent Python execution without GIL contention. This integrates the existing multiprocess executor with the gRPC service's manifest parsing and execution flow, maintaining backward compatibility while unlocking 10+ second to <500ms latency improvements for speech-to-speech pipelines.

## Technical Context

**Language/Version**: Rust 1.75+ (runtime service), Python 3.11+ (node processes)
**Primary Dependencies**: tonic (gRPC), tokio (async runtime), iceoryx2 (shared memory IPC), PyO3 (Python-Rust bridge)
**Storage**: N/A (in-memory execution only)
**Testing**: cargo test (Rust integration tests), pytest (Python node tests)
**Target Platform**: Linux x64, Windows x64 (iceoryx2 supported platforms)
**Project Type**: single (extends existing runtime library and gRPC service)
**Performance Goals**: <100ms manifest parsing, <150ms process spawn overhead, <2ms executor boundary latency
**Constraints**: Zero breaking changes to manifest.v1.json schema, must integrate with existing ExecutionServiceImpl in execution.rs, multiprocess executor already implemented (spec 001)
**Scale/Scope**: 10+ concurrent sessions with multiprocess pipelines, 5+ Python nodes per session

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Note**: Project constitution not fully defined. Applying standard engineering principles from existing codebase patterns.

- ✅ **Testability**: Feature includes integration tests for manifest execution with multiprocess nodes
- ✅ **Performance**: Clear targets (<100ms parsing, <150ms spawn, <2ms latency) with measurable criteria
- ✅ **Reliability**: Process lifecycle management with cleanup guarantees (5s max) and error handling
- ✅ **Scalability**: Designed for 10+ concurrent sessions with configurable resource limits
- ✅ **Simplicity**: Single responsibility (executor routing), clear extension points in existing gRPC service

## Project Structure

### Documentation (this feature)

```text
specs/002-grpc-multiprocess-integration/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
runtime/
├── src/
│   ├── grpc_service/
│   │   ├── execution.rs         # [MODIFY] Add multiprocess executor routing
│   │   ├── streaming.rs         # [MODIFY] Add multiprocess support for streaming
│   │   ├── executor_registry.rs # [NEW] Map node types to executors
│   │   ├── manifest_parser.rs   # [NEW] Enhanced parsing with multiprocess config
│   │   └── mod.rs               # [MODIFY] Export new modules
│   │
│   ├── executor/
│   │   ├── executor_bridge.rs   # [NEW] Bridge between gRPC and multiprocess executor
│   │   └── data_conversion.rs   # [NEW] Convert between native and IPC formats
│   │
│   └── python/
│       └── multiprocess/        # [EXISTING from spec 001]
│           └── multiprocess_executor.rs
│
├── tests/
│   └── integration/
│       ├── grpc_multiprocess_test.rs  # [NEW] Integration tests
│       └── mixed_executor_test.rs     # [NEW] Test Rust + Python nodes
│
└── Cargo.toml                   # [MODIFY] Ensure multiprocess feature enabled
```

**Structure Decision**: Single project structure extending existing runtime and gRPC service. The multiprocess integration is implemented as additional modules in the gRPC service layer (`executor_registry`, `manifest_parser`) and a bridge layer (`executor_bridge`, `data_conversion`) that connects the gRPC service to the multiprocess executor.

## Complexity Tracking

> No constitution violations - feature maintains simplicity and clear separation of concerns.

## Phase Status

### Phase 0: Research ✅ Complete
- Generated: `research.md` - Technical decisions for executor routing, manifest parsing, data conversion

### Phase 1: Design & Contracts ✅ Complete
- Generated: `data-model.md` - Core entities for executor routing and session management
- Generated: `contracts/grpc-service-extension.md` - Extended service behavior for multiprocess
- Generated: `contracts/executor-bridge-api.md` - Bridge layer API specification
- Generated: `quickstart.md` - Usage guide for clients
- Updated: Agent context with multiprocess integration patterns

### Phase 2: Tasks (Next Step)
- Run `/openspec:speckit.tasks` to generate implementation tasks
