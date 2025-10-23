# Python-Rust Interop Specification

## ADDED Requirements

### Requirement: FFI Boundary Layer
The system SHALL provide a Foreign Function Interface (FFI) layer enabling Python SDK to invoke Rust runtime seamlessly.

#### Scenario: Call Rust runtime from Python
- **GIVEN** a Python Pipeline object with nodes
- **WHEN** user calls `pipeline.run()`
- **THEN** Python SHALL invoke Rust runtime via FFI without requiring manual binding code

#### Scenario: Return results to Python
- **GIVEN** Rust runtime completes pipeline execution
- **WHEN** results are ready
- **THEN** Rust SHALL marshal results back to Python as native Python objects

#### Scenario: Handle FFI errors gracefully
- **GIVEN** an error occurs in Rust runtime
- **WHEN** crossing FFI boundary
- **THEN** error SHALL be converted to Python exception with preserved stack trace

### Requirement: Data Marshaling
The system SHALL marshal data between Python and Rust efficiently and correctly.

#### Scenario: Marshal Python dict to Rust
- **GIVEN** a Python dictionary as node input
- **WHEN** passing to Rust runtime
- **THEN** system SHALL convert to Rust HashMap without data loss

#### Scenario: Marshal numpy arrays
- **GIVEN** a numpy array (audio/video data)
- **WHEN** passing between Python and Rust
- **THEN** system SHALL use zero-copy shared memory where possible

#### Scenario: Marshal complex Python objects
- **GIVEN** a Python object with nested structures (dataclass, list of dicts)
- **WHEN** serializing for Rust
- **THEN** system SHALL use CloudPickle and preserve type information

#### Scenario: Handle serialization failures
- **GIVEN** a Python object that cannot be serialized
- **WHEN** attempting to marshal to Rust
- **THEN** system SHALL return clear error indicating unsupported type

### Requirement: RustPython VM Management
The system SHALL manage RustPython virtual machine lifecycle and execution context.

#### Scenario: Initialize RustPython VM
- **GIVEN** Rust runtime starts
- **WHEN** first Python node needs execution
- **THEN** system SHALL create RustPython VM with appropriate Python path and imports

#### Scenario: Reuse VM across nodes
- **GIVEN** multiple Python nodes in same pipeline
- **WHEN** executing sequentially
- **THEN** system SHALL reuse same RustPython VM instance for performance

#### Scenario: Isolate VMs for concurrent execution
- **GIVEN** nodes executing in parallel
- **WHEN** running on different threads
- **THEN** each thread SHALL have isolated RustPython VM instance

#### Scenario: Clean up VM resources
- **GIVEN** pipeline execution completes
- **WHEN** VM is no longer needed
- **THEN** system SHALL release VM and associated memory

### Requirement: Python Module Loading in RustPython
The system SHALL load and import Python modules within RustPython environment.

#### Scenario: Import SDK modules
- **GIVEN** a node importing `from remotemedia.core import Node`
- **WHEN** executing in RustPython
- **THEN** system SHALL successfully import and use SDK classes

#### Scenario: Import third-party dependencies
- **GIVEN** a node importing `import numpy as np`
- **WHEN** executing in RustPython
- **THEN** system SHALL load numpy (if pure-Python) or return clear error if C extension

#### Scenario: Handle missing modules
- **GIVEN** a node importing unavailable module
- **WHEN** execution starts
- **THEN** system SHALL return ImportError with module name and suggestion

#### Scenario: Inject custom modules
- **GIVEN** runtime needs to provide custom module (e.g., logging bridge)
- **WHEN** RustPython VM initializes
- **THEN** system SHALL inject module into Python sys.modules

### Requirement: Python-Rust Type Compatibility
The system SHALL define clear type mappings between Python and Rust types.

#### Scenario: Map primitive types
- **GIVEN** Python int, float, str, bool
- **WHEN** marshaling to Rust
- **THEN** system SHALL use i64, f64, String, bool respectively

#### Scenario: Map collection types
- **GIVEN** Python list, tuple, dict, set
- **WHEN** marshaling to Rust
- **THEN** system SHALL use Vec, Vec (immutable), HashMap, HashSet

#### Scenario: Map None/null
- **GIVEN** Python None value
- **WHEN** marshaling to Rust
- **THEN** system SHALL use Option::None

#### Scenario: Handle custom classes
- **GIVEN** Python custom class instance
- **WHEN** marshaling to Rust
- **THEN** system SHALL serialize via CloudPickle and store as bytes

### Requirement: Node State Preservation
The system SHALL preserve Python node state across method calls.

#### Scenario: Maintain instance variables
- **GIVEN** a Python node with __init__ setting self.counter = 0
- **WHEN** process() method increments self.counter
- **THEN** subsequent calls SHALL see incremented value

#### Scenario: Preserve closures
- **GIVEN** a Python node with lambda or closure
- **WHEN** executing in RustPython
- **THEN** system SHALL maintain closure scope and variables

#### Scenario: Handle stateful generators
- **GIVEN** a streaming node returning generator
- **WHEN** yielding values over time
- **THEN** system SHALL preserve generator state between yields

### Requirement: Python Exception Handling
The system SHALL handle Python exceptions raised in RustPython and propagate to Rust runtime.

#### Scenario: Catch Python exception
- **GIVEN** a Python node raising ValueError
- **WHEN** executing in RustPython
- **THEN** Rust runtime SHALL catch exception and convert to Result::Err

#### Scenario: Preserve exception details
- **GIVEN** a Python exception with message and traceback
- **WHEN** caught by Rust runtime
- **THEN** error SHALL include exception type, message, and Python traceback

#### Scenario: Propagate through pipeline
- **GIVEN** a Python node exception in middle of pipeline
- **WHEN** error occurs
- **THEN** runtime SHALL stop pipeline, cleanup, and return structured error

### Requirement: Python Standard Library Compatibility
The system SHALL document and test which Python stdlib modules work in RustPython.

#### Scenario: Test core modules
- **GIVEN** nodes using json, sys, os.path, collections
- **WHEN** executing in RustPython
- **THEN** all SHALL work correctly

#### Scenario: Test async modules
- **GIVEN** nodes using asyncio, async/await
- **WHEN** executing in RustPython
- **THEN** system SHALL support async execution

#### Scenario: Identify unsupported modules
- **GIVEN** nodes using multiprocessing, ctypes, C extensions
- **WHEN** attempting to import in RustPython
- **THEN** system SHALL provide clear error with alternatives

#### Scenario: Document compatibility matrix
- **GIVEN** RustPython stdlib implementation
- **WHEN** building documentation
- **THEN** system SHALL generate compatibility table (module → supported/unsupported)

### Requirement: Performance Optimization for Interop
The system SHALL optimize data passing between Python and Rust for minimal overhead.

#### Scenario: Use zero-copy for large arrays
- **GIVEN** a 10MB numpy array
- **WHEN** passing from Python to Rust
- **THEN** system SHALL use shared memory without copying data

#### Scenario: Batch small data transfers
- **GIVEN** streaming node producing many small results
- **WHEN** returning to Python
- **THEN** system SHALL batch results to reduce FFI overhead

#### Scenario: Cache serialized objects
- **GIVEN** same Python object passed multiple times
- **WHEN** marshaling to Rust
- **THEN** system MAY cache serialized form for reuse

### Requirement: Debugging and Introspection
The system SHALL provide debugging tools for Python-Rust boundary issues.

#### Scenario: Log FFI calls
- **GIVEN** debug mode enabled
- **WHEN** Python calls Rust or vice versa
- **THEN** system SHALL log function name, arguments, and timing

#### Scenario: Inspect marshaled data
- **GIVEN** data marshaling between Python and Rust
- **WHEN** debug logging enabled
- **THEN** system SHALL show before/after representations

#### Scenario: Profile interop overhead
- **GIVEN** pipeline execution
- **WHEN** profiling enabled
- **THEN** system SHALL measure time spent in FFI vs actual computation

### Requirement: RustPython Compatibility Testing
The system SHALL provide automated testing for RustPython compatibility with existing nodes.

#### Scenario: Test all SDK nodes
- **GIVEN** all built-in remotemedia nodes
- **WHEN** running compatibility test suite
- **THEN** system SHALL execute each node in both CPython and RustPython and compare results

#### Scenario: Test example pipelines
- **GIVEN** all example pipelines in repository
- **WHEN** running in RustPython
- **THEN** system SHALL produce identical results to CPython (within numerical tolerance)

#### Scenario: Generate compatibility report
- **GIVEN** compatibility tests complete
- **WHEN** generating report
- **THEN** system SHALL list: working nodes, broken nodes, reason for failures, workarounds

### Requirement: Fallback to CPython
The system SHALL support falling back to CPython for incompatible nodes.

#### Scenario: Detect RustPython incompatibility
- **GIVEN** a node that fails in RustPython
- **WHEN** runtime detects repeated failures
- **THEN** system MAY automatically retry with CPython subprocess

#### Scenario: Explicit CPython mode
- **GIVEN** user sets REMOTEMEDIA_PYTHON_RUNTIME=cpython
- **WHEN** pipeline runs
- **THEN** system SHALL use CPython subprocess instead of RustPython

#### Scenario: Mixed execution
- **GIVEN** pipeline with some RustPython-compatible and some incompatible nodes
- **WHEN** executing
- **THEN** system SHALL use RustPython where possible and CPython for incompatible nodes
