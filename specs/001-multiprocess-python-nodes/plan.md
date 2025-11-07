# Implementation Plan: Multi-Process Node Execution

**Branch**: `001-multiprocess-python-nodes` | **Date**: 2025-11-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-multiprocess-python-nodes/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Enable concurrent execution of multiple Python-based pipeline nodes with independent GILs using iceoryx2 for zero-copy shared memory IPC and PyO3 for Python-Rust interop. This eliminates the current GIL bottleneck where multiple AI models (LFM2-Audio, VibeVoice TTS) cannot run concurrently, reducing end-to-end latency from 10+ seconds to under 500ms for speech-to-speech pipelines.

## Technical Context

**Language/Version**: Rust 1.75+ (runtime), Python 3.11+ (nodes)
**Primary Dependencies**: iceoryx2 (IPC), PyO3 (Python-Rust binding), tokio (async runtime)
**Storage**: N/A (in-memory shared buffers only)
**Testing**: cargo test (Rust), pytest (Python nodes), integration tests (end-to-end)
**Target Platform**: Linux x64, Windows x64 (cross-platform IPC)
**Project Type**: single (library with runtime integration)
**Performance Goals**: <1ms inter-node transfer latency, <500ms end-to-end speech-to-speech
**Constraints**: <100µs IPC overhead, zero-copy data transfer, event-driven health monitoring
**Scale/Scope**: 10+ concurrent sessions, 5+ Python nodes per session (configurable limit)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Note**: Project constitution not fully defined. Applying standard engineering principles.

- ✅ **Testability**: Feature includes comprehensive unit, integration, and end-to-end test requirements
- ✅ **Performance**: Clear latency targets (<1ms IPC, <500ms e2e) with measurable criteria
- ✅ **Reliability**: Event-driven monitoring, clean failure handling with pipeline termination
- ✅ **Scalability**: Configurable process limits, supports 10+ concurrent sessions
- ✅ **Simplicity**: Single responsibility (process isolation), clear boundaries (shared memory IPC)

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
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
│   ├── python/
│   │   ├── process_manager.rs      # Process spawning & lifecycle
│   │   ├── ipc_channel.rs          # iceoryx2 shared memory channels
│   │   ├── data_transfer.rs        # Zero-copy RuntimeData transfer
│   │   └── health_monitor.rs       # Event-driven process monitoring
│   └── lib.rs                      # Public API surface
│
├── tests/
│   ├── integration/
│   │   ├── multiprocess_test.rs    # End-to-end pipeline tests
│   │   └── crash_recovery_test.rs  # Failure handling tests
│   └── unit/
│       ├── ipc_test.rs             # IPC channel tests
│       └── process_test.rs         # Process management tests
│
└── benches/
    └── latency_bench.rs            # Performance benchmarks

python-client/
├── remotemedia.core/
│   └── multiprocess/
│       ├── __init__.py             # Python API
│       ├── node_wrapper.py         # Process node wrapper
│       └── shared_memory.py        # Shared memory interface
└── tests/
    └── test_multiprocess.py        # Python integration tests
```

**Structure Decision**: Single project structure extending the existing runtime and python-client modules. The multiprocess functionality integrates into the current architecture as a new execution mode for Python nodes, maintaining backward compatibility while enabling concurrent execution.

## Complexity Tracking

> No constitution violations - feature maintains simplicity with clear single responsibility.

## Phase Status

### Phase 0: Research ✅ Complete
- Generated: `research.md` - Technical decisions for IPC, process management, error handling

### Phase 1: Design & Contracts ✅ Complete
- Generated: `data-model.md` - Core entities and state transitions
- Generated: `contracts/rust-api.md` - Rust API surface
- Generated: `contracts/python-api.md` - Python node API
- Generated: `contracts/multiprocess-executor.md` - NodeExecutor trait implementation
- Generated: `quickstart.md` - Usage guide and examples
- Updated: Agent context with new technologies

### Phase 2: Tasks (Next Step)
- Run `/openspec:speckit.tasks` to generate implementation tasks
