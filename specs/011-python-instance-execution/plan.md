# Implementation Plan: Python Instance Execution in FFI

**Branch**: `011-python-instance-execution` | **Date**: 2025-11-20 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/011-python-instance-execution/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Enable RemoteMedia FFI to accept Python Node instances directly as arguments to `execute_pipeline()` and `execute_pipeline_with_input()`, bypassing JSON manifest serialization. This allows developers to pass pre-configured Node objects (with complex state like loaded ML models) directly to the Rust runtime while maintaining backward compatibility with manifest-based execution. For multiprocess execution, instances are serialized using existing `cleanup()`/`initialize()` lifecycle methods.

## Technical Context

**Language/Version**: Python 3.11+, Rust 1.75+ (dual-language FFI project)
**Primary Dependencies**: PyO3 (Rust-Python FFI), cloudpickle (Python object serialization), iceoryx2 (IPC), remotemedia-runtime-core
**Storage**: N/A (ephemeral pipeline execution)
**Testing**: pytest (Python), cargo test (Rust)
**Target Platform**: Linux/macOS/Windows (wherever PyO3 + iceoryx2 supported)
**Project Type**: Single library (FFI transport layer)
**Performance Goals**: No additional overhead beyond cloudpickle serialization (~1-5ms for typical nodes), maintain existing pipeline execution performance
**Constraints**: Must maintain backward compatibility with manifest-based execution; PyO3 GIL constraints; iceoryx2 !Send types require dedicated threads
**Scale/Scope**: Support pipelines with 1-100 nodes; handle Node instances up to ~100MB serialized size; support concurrent multiprocess execution

**Key Unknowns Requiring Research**:
- NEEDS CLARIFICATION: Best practices for PyO3 object lifetime management when holding Python object references in Rust
- NEEDS CLARIFICATION: cloudpickle limitations and edge cases for Node serialization
- NEEDS CLARIFICATION: How to extend manifest schema to support instance references while maintaining backward compatibility

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Status**: ✅ PASS (No project constitution defined - using default gates)

**Default Gates**:
- ✅ Backward Compatibility: FR-012 ensures existing manifest-based pipelines continue to work
- ✅ Testing Strategy: pytest and cargo test frameworks specified
- ✅ Clear Scope: Feature limited to Node instances only (non-Node objects out of scope)
- ✅ Performance: No degradation goal specified (SC-003)

**Notes**: Project does not have a custom constitution file. Standard software engineering practices apply.

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
transports/remotemedia-ffi/
├── src/
│   ├── lib.rs                     # PyO3 module definition (modify)
│   ├── api.rs                     # FFI functions execute_pipeline(), execute_pipeline_with_input() (modify)
│   ├── marshal.rs                 # Python ↔ JSON conversion (modify for instances)
│   ├── numpy_bridge.rs            # Zero-copy numpy integration (no changes)
│   └── instance_handler.rs        # NEW: Instance detection, validation, serialization
│
├── python/
│   └── remotemedia/
│       └── __init__.py            # Python package (minimal changes)
│
└── tests/
    ├── test_instance_execution.rs # NEW: Rust tests for instance handling
    └── test_ffi_instances.py      # NEW: Python integration tests

python-client/
├── remotemedia/
│   ├── core/
│   │   ├── pipeline.py            # Pipeline.run() method (modify to support instances)
│   │   └── node.py                # Node base class (no changes, already has lifecycle methods)
│   └── __init__.py                # Module exports (add is_rust_runtime_available if needed)
│
└── tests/
    └── test_instance_pipelines.py # NEW: Python tests for instance execution

runtime/
└── src/
    └── transport/
        └── mod.rs                 # PipelineRunner (may need minor changes)
```

**Structure Decision**: This is a cross-cutting feature affecting the FFI transport layer (`transports/remotemedia-ffi`) and Python client (`python-client`). Primary changes are in the FFI layer to accept and handle Python object references via PyO3. Minimal changes to existing runtime-core since PipelineRunner already supports execution modes.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

N/A - No constitution violations
