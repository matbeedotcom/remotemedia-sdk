# Rust FFI Contract: Instance Execution

**Feature**: Python Instance Execution in FFI
**Version**: v1
**Date**: 2025-11-20

## Overview

This contract defines the Rust FFI layer's responsibilities for handling Python Node instances passed from Python code. The Rust layer uses PyO3 to hold references to Python objects and call their methods during pipeline execution.

---

## Module: `instance_handler.rs` (NEW)

### Struct: `InstanceExecutor`

Wraps a Python Node instance and provides execution interface compatible with Rust runtime.

```rust
use pyo3::prelude::*;
use remotemedia_runtime_core::data::RuntimeData;
use std::sync::Arc;

/// Wrapper for executing Python Node instances from Rust
pub struct InstanceExecutor {
    /// Python Node instance reference (GIL-independent)
    node_instance: Py<PyAny>,

    /// Node identifier for logging/debugging
    node_id: String,

    /// Whether this node supports streaming
    is_streaming: bool,
}
```

### Methods

#### `new(node_instance: Py<PyAny>, node_id: String) -> PyResult<Self>`

Create a new instance executor from a Python Node object.

**Parameters**:
- `node_instance`: Python Node instance (must be `Py<PyAny>` for storage)
- `node_id`: Unique identifier for this node

**Returns**: `PyResult<InstanceExecutor>`

**Validation**:
- ✅ Verify `node_instance` has `process` method
- ✅ Verify `node_instance` has `initialize` method
- ✅ Check for `is_streaming` attribute (default: false)

**Example**:
```rust
pub fn new(node_instance: Py<PyAny>, node_id: String) -> PyResult<Self> {
    // Validate required methods exist
    Python::with_gil(|py| {
        let node_ref = node_instance.bind(py);

        // Check for process method
        if !node_ref.hasattr("process")? {
            return Err(PyErr::new::<pyo3::exceptions::PyAttributeError, _>(
                format!("Node '{}' missing required process() method", node_id)
            ));
        }

        // Check for initialize method
        if !node_ref.hasattr("initialize")? {
            return Err(PyErr::new::<pyo3::exceptions::PyAttributeError, _>(
                format!("Node '{}' missing required initialize() method", node_id)
            ));
        }

        // Check if streaming (optional attribute)
        let is_streaming = node_ref
            .getattr("is_streaming")
            .map(|v| v.extract::<bool>().unwrap_or(false))
            .unwrap_or(false);

        Ok(InstanceExecutor {
            node_instance,
            node_id,
            is_streaming,
        })
    })
}
```

---

#### `initialize(&self) -> PyResult<()>`

Initialize the Python Node instance before processing.

**Behavior**:
- Acquires GIL
- Calls `node_instance.initialize()` Python method
- Handles exceptions from Python side
- Logs initialization status

**Example**:
```rust
pub fn initialize(&self) -> PyResult<()> {
    Python::with_gil(|py| {
        self.node_instance
            .call_method0(py, "initialize")
            .map_err(|e| {
                tracing::error!("Failed to initialize node '{}': {}", self.node_id, e);
                e
            })?;

        tracing::debug!("Initialized node: {}", self.node_id);
        Ok(())
    })
}
```

---

#### `process(&self, input: RuntimeData) -> PyResult<Vec<RuntimeData>>`

Process input data through the Python Node instance.

**Parameters**:
- `input`: Runtime data to process

**Returns**: `PyResult<Vec<RuntimeData>>` (vec to support multiple outputs)

**Behavior**:
- Acquires GIL
- Converts `RuntimeData` to Python object
- Calls `node_instance.process(data)` Python method
- Converts Python result back to `RuntimeData`
- Handles None returns (filters out)

**Example**:
```rust
pub fn process(&self, input: RuntimeData) -> PyResult<Vec<RuntimeData>> {
    Python::with_gil(|py| {
        // Convert RuntimeData to Python object
        let py_input = runtime_data_to_python(py, &input)?;

        // Call process method
        let result = self.node_instance
            .call_method1(py, "process", (py_input,))
            .map_err(|e| {
                tracing::error!("Node '{}' process() failed: {}", self.node_id, e);
                e
            })?;

        // Convert result back to RuntimeData
        if result.is_none(py) {
            Ok(vec![])  // None = no output
        } else {
            let runtime_data = python_to_runtime_data(py, result.bind(py))?;
            Ok(vec![runtime_data])
        }
    })
}
```

---

#### `cleanup(&self) -> PyResult<()>`

Clean up resources held by the Python Node instance.

**Behavior**:
- Acquires GIL
- Calls `node_instance.cleanup()` Python method
- Logs cleanup status
- Does NOT drop `Py<PyAny>` reference (Rust Drop trait handles that)

**Example**:
```rust
pub fn cleanup(&self) -> PyResult<()> {
    Python::with_gil(|py| {
        self.node_instance
            .call_method0(py, "cleanup")
            .map_err(|e| {
                tracing::warn!("Failed to cleanup node '{}': {}", self.node_id, e);
                e
            })?;

        tracing::debug!("Cleaned up node: {}", self.node_id);
        Ok(())
    })
}
```

---

### Trait Implementation: `Drop`

Ensure Python reference is properly released when InstanceExecutor is dropped.

```rust
impl Drop for InstanceExecutor {
    fn drop(&mut self) {
        // Py<PyAny> automatically decrements Python refcount when dropped
        // But we should explicitly cleanup if not already done
        if let Err(e) = self.cleanup() {
            tracing::warn!("Cleanup during drop failed for '{}': {}", self.node_id, e);
        }
    }
}
```

---

## Module: `api.rs` (MODIFICATIONS)

### Modified Function: `execute_pipeline`

**Current Signature**:
```rust
#[pyfunction]
pub fn execute_pipeline(
    py: Python<'_>,
    manifest_json: String,
    enable_metrics: Option<bool>,
) -> PyResult<Bound<'_, PyAny>>
```

**Modifications**: Add instance detection and handling

**New Behavior**:
1. Accept Python input that may be Pipeline instance, list of Nodes, or manifest JSON
2. Detect input type:
   - If has `.serialize()` method → call it to get manifest JSON
   - If list → convert to Pipeline and serialize
   - If string → use as manifest JSON
3. For instance-based input, store references and create InstanceExecutor wrappers
4. Execute using existing PipelineRunner

**Note**: Based on research, we implement detection at Python wrapper layer, not in Rust. Rust FFI continues to accept manifest JSON only. This simplifies the Rust contract.

**No changes required to Rust FFI signature** - Python wrapper handles conversion.

---

## Module: `marshal.rs` (MODIFICATIONS)

### New Function: `runtime_data_to_python`

Convert Rust `RuntimeData` to Python object for passing to Node instances.

**Signature**:
```rust
pub fn runtime_data_to_python(py: Python<'_>, data: &RuntimeData) -> PyResult<PyObject>
```

**Behavior**:
- Convert `RuntimeData::Audio` → Python dict with `{"type": "audio", "samples": [...], ...}`
- Convert `RuntimeData::Text` → Python string
- Convert `RuntimeData::Json` → Python dict (already JSON-compatible)
- Convert `RuntimeData::Binary` → Python bytes
- Other types → appropriate Python equivalents

**Example**:
```rust
pub fn runtime_data_to_python(py: Python<'_>, data: &RuntimeData) -> PyResult<PyObject> {
    match data {
        RuntimeData::Audio { samples, sample_rate, channels } => {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("type", "audio")?;
            dict.set_item("samples", samples.as_slice())?;
            dict.set_item("sample_rate", sample_rate)?;
            dict.set_item("channels", channels)?;
            Ok(dict.into())
        }
        RuntimeData::Text(s) => Ok(s.to_object(py)),
        RuntimeData::Json(v) => json_to_python(py, v),
        RuntimeData::Binary(b) => Ok(pyo3::types::PyBytes::new(py, b).into()),
        // ... other variants
    }
}
```

---

### New Function: `python_to_runtime_data`

Convert Python object to Rust `RuntimeData` for pipeline processing.

**Signature**:
```rust
pub fn python_to_runtime_data(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<RuntimeData>
```

**Behavior**:
- Detect Python type and convert to appropriate `RuntimeData` variant
- Support dict with "type" field for structured data
- Support raw types (str → Text, bytes → Binary)
- Use existing `python_to_json` for complex objects

**Example**:
```rust
pub fn python_to_runtime_data(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<RuntimeData> {
    // Check if it's a dict with "type" field
    if let Ok(dict) = obj.downcast::<pyo3::types::PyDict>() {
        if let Ok(Some(type_val)) = dict.get_item("type") {
            let type_str: String = type_val.extract()?;
            match type_str.as_str() {
                "audio" => {
                    let samples: Vec<f32> = dict.get_item("samples")?.unwrap().extract()?;
                    let sample_rate: u32 = dict.get_item("sample_rate")?.unwrap().extract()?;
                    let channels: u16 = dict.get_item("channels")?.unwrap().extract()?;
                    return Ok(RuntimeData::Audio { samples, sample_rate, channels });
                }
                "text" => {
                    let data: String = dict.get_item("data")?.unwrap().extract()?;
                    return Ok(RuntimeData::Text(data));
                }
                _ => {}  // Fall through to JSON conversion
            }
        }
    }

    // Try as string
    if let Ok(s) = obj.extract::<String>() {
        return Ok(RuntimeData::Text(s));
    }

    // Try as bytes
    if let Ok(b) = obj.extract::<Vec<u8>>() {
        return Ok(RuntimeData::Binary(b));
    }

    // Default: Convert to JSON
    let json_val = python_to_json(py, obj)?;
    Ok(RuntimeData::Json(json_val))
}
```

---

## Performance Contract

### GIL Acquisition
- ✅ Minimize GIL hold time - acquire only for Python calls
- ✅ Release GIL during Rust-side processing
- ✅ Pattern: `Python::with_gil(|| { quick call })` → release → continue

### Memory Management
- ✅ `Py<PyAny>` reference counting is automatic
- ✅ Cloning is cheap (increments refcount only)
- ✅ Drop releases reference when GIL next acquired

### Error Handling
- ✅ All PyErr converted to descriptive Rust errors
- ✅ Include node ID in all error messages
- ✅ Log errors at appropriate levels (error, warn, debug)

---

## Testing Contract

All instance handling code must be tested with:

### Unit Tests
- ✅ Valid Node instance → successful execution
- ✅ Missing `process()` method → validation error
- ✅ Missing `initialize()` method → validation error
- ✅ Python exception in `process()` → propagated correctly
- ✅ None return from `process()` → empty output list

### Integration Tests
- ✅ End-to-end instance execution via FFI
- ✅ Multiple instances in pipeline
- ✅ Mixed instance + manifest nodes (future)
- ✅ Instance with complex state (ML model)
- ✅ Cleanup called on drop

### Example Test
```rust
#[test]
fn test_instance_executor_lifecycle() -> PyResult<()> {
    pyo3::prepare_freethreaded_python();

    Python::with_gil(|py| {
        // Create a Python Node instance
        let code = r#"
class TestNode:
    def __init__(self):
        self.initialized = False

    def initialize(self):
        self.initialized = True

    def process(self, data):
        return data + 10

    def cleanup(self):
        self.initialized = False
"#;
        py.run(code, None, None)?;
        let test_node = py.eval("TestNode()", None, None)?;

        // Create InstanceExecutor
        let executor = InstanceExecutor::new(test_node.unbind(), "test".to_string())?;

        // Initialize
        executor.initialize()?;

        // Process
        let input = RuntimeData::Text("5".to_string());
        let output = executor.process(input)?;
        assert_eq!(output.len(), 1);

        // Cleanup
        executor.cleanup()?;

        Ok(())
    })
}
```

---

## Contract Guarantees

- ✅ **No Breaking Changes**: Existing Rust FFI API unchanged (manifest JSON still accepted)
- ✅ **Memory Safe**: PyO3 ensures safe Python object lifecycle management
- ✅ **Thread Safe**: `Py<PyAny>` is Send, can cross async task boundaries
- ✅ **GIL Correct**: All Python access protected by `Python::with_gil()`
- ✅ **Error Propagation**: All Python exceptions converted to PyResult/Rust errors
- ✅ **Resource Cleanup**: Drop trait ensures cleanup always called
