# Feature Specification: Python Instance Execution in FFI

**Feature Branch**: `011-python-instance-execution`
**Created**: 2025-11-20
**Status**: Draft
**Input**: User description: "transports/remotemedia-ffi needs to accept python class instances in its "execute_pipeline" and "execute_pipeline_with_input". Instead of basic JSON manifest, it should allow for the rust runtime to execute python class instances in its Pipeline. Not just by creating python classes by string."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Direct Node Instance Execution (Priority: P1)

Developers can pass pre-configured Python Node instances directly to the Rust runtime's pipeline execution functions, eliminating the need to serialize node configurations to JSON manifests and reconstruct them on the Rust side.

**Why this priority**: This is the core functionality that enables the most natural Python developer experience. It allows developers to use Python's full expressiveness (complex objects, closures, etc.) when configuring nodes, rather than being limited to JSON-serializable configuration.

**Independent Test**: Can be fully tested by creating a Node instance in Python (e.g., `node = CustomNode(param=obj)`), passing it to `execute_pipeline([node])`, and verifying the pipeline executes with the exact instance provided, delivering processed output.

**Acceptance Scenarios**:

1. **Given** a Python script creates a Node instance with complex configuration, **When** the instance is passed to `execute_pipeline()`, **Then** the Rust runtime executes the exact instance without reconstructing it
2. **Given** a Pipeline object containing pre-initialized Node instances, **When** `run()` is called with `use_rust=True`, **Then** the Rust runtime uses the existing instances rather than creating new ones from manifests
3. **Given** a Node instance with in-memory state (e.g., loaded ML model), **When** passed to `execute_pipeline_with_input()`, **Then** the state is preserved and used during execution

---

### User Story 2 - Mixed Manifest and Instance Pipelines (Priority: P2)

Developers can create pipelines that mix JSON-defined nodes (by class name string) with direct Node instances, allowing flexibility in pipeline construction approaches.

**Why this priority**: This enables gradual migration from manifest-based pipelines to instance-based pipelines, and allows developers to use the most appropriate approach for each node (simple nodes via manifest, complex nodes via instances).

**Independent Test**: Can be tested independently by creating a pipeline with both manifest node definitions (`{"node_type": "PassThroughNode"}`) and direct instances (`CustomNode()`), executing it, and verifying all nodes process data correctly in sequence.

**Acceptance Scenarios**:

1. **Given** a pipeline with 3 nodes where node 1 is a manifest definition and nodes 2-3 are instances, **When** executed, **Then** all nodes process data in the correct order
2. **Given** a manifest with a mix of string-based node types and serialized instance references, **When** parsed by Rust runtime, **Then** appropriate execution paths are selected for each node type
3. **Given** a pipeline where some nodes require complex initialization (instances) and others are simple (manifests), **When** executed, **Then** both types integrate seamlessly

---

### User Story 3 - Instance Serialization for IPC (Priority: P3)

When Python Node instances are passed to the Rust runtime for multiprocess execution, the system automatically serializes the instance state for transfer to the subprocess and deserializes it correctly in the target process.

**Why this priority**: This is necessary infrastructure for making P1 work with multiprocess execution mode, but is less critical than the core functionality since many use cases work with single-process execution.

**Independent Test**: Can be tested by creating a Node instance with specific state, marking it for multiprocess execution, passing to `execute_pipeline()`, and verifying the subprocess receives and uses the correct state.

**Acceptance Scenarios**:

1. **Given** a Node instance marked for multiprocess execution, **When** passed to the Rust runtime, **Then** the instance is serialized, transferred via IPC, and reconstructed in the subprocess
2. **Given** a Node instance with non-serializable state (e.g., file handles), **When** serialization is attempted, **Then** a clear error message identifies the problematic attribute
3. **Given** a successfully deserialized Node instance in subprocess, **When** it processes data, **Then** it behaves identically to the original instance

---

### Edge Cases

- What happens when a Node instance contains non-serializable state (open file handles, database connections)?
- How does the system handle circular references in Node instance configurations?
- What happens when a Node instance is passed but the class definition is not available in the subprocess?
- How are Node instances with complex dependencies (imported modules, global state) handled during serialization?
- What happens when mixing Python 2-style and Python 3-style Node instances in the same pipeline?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST accept Python Node instances as direct arguments to `execute_pipeline()` and `execute_pipeline_with_input()` FFI functions
- **FR-002**: System MUST preserve exact instance state when executing Node instances (no reconstruction from manifest)
- **FR-003**: System MUST support mixed pipelines containing both manifest-defined nodes (class name strings) and direct Node instances
- **FR-004**: System MUST automatically detect whether pipeline input contains instances vs. manifest definitions
- **FR-005**: System MUST serialize Node instances for IPC transfer when multiprocess execution is required
- **FR-006**: System MUST call `cleanup()` on Node instances before serialization to release external resources
- **FR-007**: System MUST call `initialize()` on Node instances after deserialization in subprocess to recreate resources
- **FR-008**: System MUST deserialize Node instances in subprocess environments with identical behavior to original instance
- **FR-009**: Users MUST be able to pass Pipeline objects directly to FFI functions (extracting nodes automatically)
- **FR-010**: System MUST validate Node instances before execution (check for required methods: `process`, `initialize`)
- **FR-011**: System MUST provide clear error messages when Node instance serialization fails
- **FR-012**: System MUST maintain backward compatibility with existing manifest-based pipeline execution

### Key Entities *(include if feature involves data)*

- **Node Instance**: A Python object (instance of a Node subclass) with complete state, configuration, and methods, passed directly to the runtime rather than reconstructed from JSON
- **Instance Manifest**: An extended manifest format that includes references to Python objects alongside traditional JSON-serializable node definitions
- **Serialized Instance State**: A pickle or cloudpickle-serialized representation of a Node instance for IPC transfer
- **Execution Context**: Runtime environment information (process ID, IPC channels) required to route Node instance execution to correct subprocess

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Developers can create and execute pipelines with direct Node instances in under 10 lines of Python code
- **SC-002**: Node instances with complex state (e.g., pre-loaded ML models) execute without requiring JSON serialization of configuration
- **SC-003**: Pipeline execution with Node instances completes with same or better performance compared to manifest-based execution (no additional overhead beyond initial serialization)
- **SC-004**: 100% of existing manifest-based pipelines continue to work without modification
- **SC-005**: Error messages for serialization failures include specific attribute names and reasons within 2 seconds of failure

## Assumptions *(optional)*

- Python developers are familiar with Node base class and subclass patterns
- Node instances passed to FFI are intended for immediate execution, not long-term storage
- Most Node instances will be serializable using cloudpickle (which handles more cases than standard pickle)
- Developers using this feature understand Python object lifecycle and garbage collection implications
- The Rust FFI layer can hold references to Python objects during execution via PyO3

## Dependencies *(optional)*

- PyO3 library for Rust-Python interop (already in use)
- cloudpickle library for advanced Python object serialization (may need to be added as dependency)
- Existing PipelineRunner and Manifest infrastructure in runtime-core
- Python multiprocessing/iceoryx2 IPC mechanisms for subprocess communication

## Out of Scope *(optional)*

- Support for non-Node Python objects (only Node subclass instances accepted)
- Persistent storage of Node instances across process restarts
- Distributed execution of Node instances across network boundaries
- Automatic detection and handling of all non-serializable state (developers responsible for making instances serializable)
- GUI or visual pipeline builder for Node instances
- Type checking or validation of Node instance method signatures beyond basic existence checks

## Open Questions *(optional)*

**Resolved**: External resource dependencies are handled by leveraging the existing Node lifecycle methods (`cleanup()` and `initialize()`). Before serialization for IPC transfer, the system calls `cleanup()` to release resources in the parent process. After deserialization in the subprocess, `initialize()` is called to recreate resources. This approach uses the established Node contract and requires no additional infrastructure.

## Technical Notes *(optional)*

- Current implementation uses JSON manifest (`manifest.v1.json` schema) exclusively
- PyO3 allows Rust to hold `Py<PyAny>` references to Python objects
- Existing `to_manifest()` method on Node class could be bypassed for direct instance execution
- iceoryx2 IPC uses binary serialization - cloudpickle output can be sent as binary payload
- Rust PipelineRunner would need to distinguish between manifest-based and instance-based execution modes
- Node base class already implements `initialize()` and `cleanup()` lifecycle methods (python-client/remotemedia/core/node.py:334, 362)
- StateManager cleanup is already integrated with Node.cleanup() (line 372-374)
- Serialization workflow for multiprocess: `cleanup()` → pickle → IPC transfer → unpickle → `initialize()`
