# Implementation Plan: Model Registry and Shared Memory Tensors

**Branch**: `006-model-sharing` | **Date**: 2025-01-08 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/006-model-sharing/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Enable efficient model sharing across nodes through three core capabilities: process-local model registry for in-process sharing, cross-process model workers for GPU-efficient serving, and shared memory tensors with zero-copy transfers. This reduces memory usage by 60%, enables sub-100ms model access, and achieves 10GB/s tensor transfer throughput.

## Technical Context

**Language/Version**: Rust 1.75+ (core runtime), Python 3.9+ (ML nodes)  
**Primary Dependencies**: tokio (async runtime), PyO3 (Python bindings), shared_memory (cross-platform SHM)  
**Storage**: In-memory registry with optional disk caching for model weights  
**Testing**: cargo test (Rust), pytest (Python integration tests)  
**Target Platform**: Linux/Windows/macOS servers with GPU support  
**Project Type**: Library extension to runtime-core  
**Performance Goals**: <100ms model access, 10GB/s tensor transfer, 100 concurrent requests  
**Constraints**: <5% overhead for zero-copy ops, automatic cleanup within 30s  
**Scale/Scope**: Support 10+ models per process, 100+ concurrent sessions, TB-scale tensor transfers

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- ✅ **Library-First**: Feature is designed as extensions to runtime-core library
- ✅ **Testability**: All components are independently testable (registry, workers, SHM)
- ✅ **Simplicity**: Starts with process-local sharing (P1), adds complexity incrementally
- ✅ **Observability**: Includes metrics for memory usage, cache hits, and performance
- ✅ **Backward Compatibility**: Fallback mechanisms ensure existing code continues working

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
runtime-core/
├── src/
│   ├── model_registry/
│   │   ├── mod.rs              # ModelRegistry trait and implementation
│   │   ├── handle.rs           # ModelHandle with reference counting
│   │   └── cache.rs            # LRU cache and eviction policies
│   ├── model_worker/
│   │   ├── mod.rs              # Worker process management
│   │   ├── client.rs           # Client for connecting to workers
│   │   └── protocol.rs         # IPC protocol definitions
│   ├── tensor/
│   │   ├── mod.rs              # Enhanced TensorBuffer with storage backends
│   │   ├── shared_memory.rs    # Cross-platform SHM implementation
│   │   └── dlpack.rs           # DLPack interface for Python
│   └── lib.rs                  # Public API exports

python-client/remotemedia/
├── core/
│   ├── model_registry.py       # Python registry mirroring Rust
│   └── tensor_bridge.py        # NumPy/PyTorch zero-copy bridge
└── nodes/
    └── ml/                     # Updated ML nodes to use registry

tests/
├── integration/
│   ├── test_model_sharing.rs   # Process-local sharing tests
│   ├── test_model_worker.rs    # Cross-process worker tests
│   └── test_shm_tensors.rs     # Shared memory tensor tests
└── python/
    └── test_zero_copy.py       # Python integration tests
```

**Structure Decision**: Library extension pattern - new modules added to runtime-core with corresponding Python bindings. Maintains separation between Rust core and Python integration layer.

## Complexity Tracking

> No violations - all constitution checks pass.
