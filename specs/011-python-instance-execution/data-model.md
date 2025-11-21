# Data Model: Python Instance Execution in FFI

**Date**: 2025-11-20
**Feature**: Python Instance Execution in FFI
**Phase**: 1 - Design

## Overview

This document defines the data structures and their relationships for the Python Instance Execution feature. The feature extends the existing manifest-based pipeline execution to support direct Python Node instance execution.

---

## Key Entities

### 1. Node Instance

**Description**: A Python object (instance of a Node subclass) with complete state, configuration, and methods, passed directly to the runtime rather than reconstructed from JSON.

**Attributes**:
| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `str` | Yes | Node identifier |
| `config` | `Dict[str, Any]` | No | Configuration parameters |
| `_is_initialized` | `bool` | Yes | Initialization state flag |
| `state` | `StateManager` | No | Session state manager (if `enable_state=True`) |
| `_current_session_id` | `Optional[str]` | No | Current session context |

**Methods**:
| Method | Signature | Description |
|--------|-----------|-------------|
| `process` | `process(data: Any) -> Any` | Core processing logic (abstract) |
| `initialize` | `initialize() -> None` | Setup resources before processing |
| `cleanup` | `cleanup() -> None` | Release resources after processing |
| `serialize` | `serialize() -> str` | Convert to manifest JSON (existing method) |
| `to_manifest` | `to_manifest() -> Dict[str, Any]` | Convert to manifest dict |

**Validation Rules**:
- Must be subclass of `remotemedia.core.node.Node`
- Must implement `process()` method
- Must implement `initialize()` method
- Should be serializable with cloudpickle (warning if not)

**State Transitions**:
```
Created → Initialized → Processing → Cleaned Up
   ↓           ↓            ↓            ↓
   ↓      (Serialized) → Transferred → Deserialized → Re-Initialized
```

**Relationships**:
- Contained in: `Pipeline` (via `pipeline.nodes` list)
- Serialized to: `Serialized Instance State` (for IPC)
- Converted to: `Instance Manifest` (for Rust runtime)

---

### 2. Instance Manifest

**Description**: An extended manifest format that includes references to Python objects alongside traditional JSON-serializable node definitions.

**Note**: Per research decision, we do NOT extend the JSON schema. Instead, we serialize instances to standard manifest format at the Python FFI boundary.

**Attributes** (Standard Manifest Format):
| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `id` | `str` | Yes | Unique node identifier |
| `node_type` | `str` | Yes | Node class name |
| `params` | `Dict[str, Any]` | No | Configuration parameters from `node.config` |
| `is_streaming` | `bool` | No | Streaming execution flag |
| `capabilities` | `Dict[str, Any]` | No | Resource requirements |
| `host` | `str` | No | Remote execution endpoint (format: "host:port") |

**Conversion from Node Instance**:
```python
manifest = {
    "id": node.name,
    "node_type": node.__class__.__name__,
    "params": node._extract_params(),
    # Optional fields based on node attributes
}
```

**Validation Rules**:
- Must conform to manifest.v1.json schema
- `node_type` must be importable Python class name
- `params` must be JSON-serializable
- If instance has custom state, it's lost in conversion (use serialization instead)

**Relationships**:
- Converted from: `Node Instance` (via `to_manifest()`)
- Consumed by: Rust PipelineRunner
- Part of: Full Pipeline Manifest

---

### 3. Serialized Instance State

**Description**: A pickle or cloudpickle-serialized representation of a Node instance for IPC transfer to multiprocess execution environments.

**Attributes**:
| Attribute | Type | Description |
|-----------|------|-------------|
| `serialized_bytes` | `bytes` | cloudpickle output |
| `node_class_name` | `str` | Class name (for error messages) |
| `python_version` | `str` | Python version used for serialization |
| `cloudpickle_version` | `str` | cloudpickle version |
| `size_bytes` | `int` | Serialized data size |

**Serialization Workflow**:
```python
# Before serialization
node.cleanup()  # Release resources

# Serialize
import cloudpickle
serialized_bytes = cloudpickle.dumps(node)

# Transfer via IPC
send_via_iceoryx2(serialized_bytes)

# Deserialize
node = cloudpickle.loads(serialized_bytes)

# After deserialization
node.initialize()  # Recreate resources
```

**Validation Rules**:
- Size limit: ~100MB (configurable)
- Python versions must match on both ends
- All node dependencies must be available in target environment
- Non-serializable attributes must be cleaned up before serialization

**Error Conditions**:
- `SerializationError`: Non-serializable attribute detected
- `SizeLimitError`: Serialized size exceeds limit
- `VersionMismatchError`: Python/cloudpickle version incompatible
- `ImportError`: Node class not available in target environment

**Relationships**:
- Serialized from: `Node Instance`
- Transferred via: iceoryx2 IPC channels
- Deserialized to: `Node Instance` (in subprocess)

---

### 4. Execution Context

**Description**: Runtime environment information required to route Node instance execution to the correct subprocess for multiprocess execution.

**Attributes**:
| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `session_id` | `str` | Yes | Unique session identifier |
| `node_id` | `str` | Yes | Node identifier within session |
| `process_id` | `int` | Yes | Target subprocess PID |
| `ipc_input_channel` | `str` | Yes | IPC channel name for input (format: `{session_id}_{node_id}_input`) |
| `ipc_output_channel` | `str` | Yes | IPC channel name for output (format: `{session_id}_{node_id}_output`) |
| `executor_type` | `str` | Yes | "multiprocess" or "native" |

**Lifecycle**:
```
Session Created → Context Allocated → Node Execution → Context Released
```

**Validation Rules**:
- `session_id` must be unique per pipeline execution
- `node_id` must be unique within session
- IPC channel names must follow naming convention
- Process must be alive and responsive

**Relationships**:
- Created for: Each node in multiprocess execution mode
- Stored in: `GLOBAL_SESSIONS` registry (Rust side)
- Used by: IPC threads for message routing
- Cleaned up: On session termination

---

## Relationships Diagram

```
┌────────────────┐
│ Pipeline       │
│ Instance       │
└───────┬────────┘
        │ contains
        ▼
┌────────────────┐      to_manifest()      ┌────────────────┐
│ Node Instance  │─────────────────────────▶│ Instance       │
│                │                           │ Manifest       │
│  - name        │                           │ (JSON)         │
│  - config      │                           └────────────────┘
│  - state       │                                  │
└────────┬───────┘                                  │
         │                                           │
         │ cleanup() → serialize → IPC              │
         ▼                                           │
┌────────────────┐                                  │
│ Serialized     │        Consumed by Rust          ▼
│ Instance State │        PipelineRunner    ┌────────────────┐
│ (bytes)        │◀─────────────────────────│ Full Pipeline  │
└────────┬───────┘        Or                │ Manifest       │
         │                                   └────────────────┘
         │ Transfer via IPC
         ▼
┌────────────────┐      Associated with     ┌────────────────┐
│ Subprocess     │───────────────────────────▶│ Execution      │
│ Node Instance  │                            │ Context        │
│ (deserialized) │                            │                │
└────────────────┘                            │  - session_id  │
         │                                     │  - node_id     │
         │ initialize() → process()            │  - IPC channels│
         ▼                                     └────────────────┘
     Output Data
```

---

## Data Flow

### Flow 1: Direct Instance Execution (No Multiprocess)

```
Python Code:
  node = MyNode(param="value")
     ↓
  execute_pipeline([node])
     ↓
  FFI Boundary Type Detection
     ↓
  Convert to manifest: node.to_manifest()
     ↓
  Pass manifest JSON to Rust
     ↓
  Rust PipelineRunner creates Python executor
     ↓
  Python executor imports class by name
     ↓
  Creates new instance from params (NOT using original instance)
     ↓
  Executes pipeline
```

**Issue**: Original instance state is lost. **Solution**: Use Py<PyAny> to hold reference to original instance (see Flow 2).

### Flow 2: Instance Execution with PyO3 Reference Holding

```
Python Code:
  node = MyNode(param="value")
     ↓
  execute_pipeline([node])
     ↓
  FFI Boundary: Detect instance
     ↓
  Store Py<PyAny> reference in Rust
     ↓
  Rust creates InstanceExecutor wrapper
     ↓
  On process():
    Python::with_gil(|py| {
        node_ref.call_method1(py, "process", (data,))
    })
     ↓
  Uses ORIGINAL instance with preserved state
```

### Flow 3: Multiprocess Instance Execution

```
Python Code:
  node = MyNode(model=loaded_model)
     ↓
  execute_pipeline([node], executor="multiprocess")
     ↓
  FFI Boundary: Detect instance + multiprocess
     ↓
  Call node.cleanup()
     ↓
  Serialize: cloudpickle.dumps(node)
     ↓
  Create Execution Context (session_id, IPC channels)
     ↓
  Transfer bytes via iceoryx2
     ↓
  Subprocess receives bytes
     ↓
  Deserialize: cloudpickle.loads(bytes)
     ↓
  Call node.initialize()
     ↓
  Execute with restored state
```

---

## Implementation Notes

1. **State Preservation**: Use PyO3 `Py<PyAny>` to hold references to original instances, avoiding manifest round-trip that loses state

2. **Serialization Trigger**: Only serialize when multiprocess execution is explicitly requested or detected

3. **Validation Points**:
   - At FFI boundary: Validate instance is a Node subclass
   - Before serialization: Validate cleanup() was called
   - After deserialization: Validate initialize() succeeds
   - During execution: Validate process() method exists

4. **Error Propagation**: All serialization/deserialization errors should include:
   - Node class name
   - Specific attribute that failed
   - Suggested fix (call cleanup(), remove non-serializable field, etc.)

5. **Performance Considerations**:
   - Manifest conversion: ~1μs per node (negligible)
   - cloudpickle serialization: 1-5ms for typical nodes
   - IPC transfer: ~100μs for <1MB payloads
   - Total overhead: <10ms per instance (acceptable per SC-003)
