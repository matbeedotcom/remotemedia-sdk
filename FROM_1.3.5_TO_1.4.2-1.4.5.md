# From Task 1.3.5 to Tasks 1.4.2-1.4.5

**Completed:** Task 1.3.5 - Node Lifecycle Management
**Next:** Tasks 1.4.2-1.4.5 - Python-Rust FFI Integration
**Date:** 2025-10-22

---

## Overview

**Goal:** Enable Python code to call the Rust runtime and execute pipelines.

**What we're building:**
```python
# Python code (user writes this)
pipeline = Pipeline(name="test")
pipeline.add_node(PassThroughNode(name="pass1"))
pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=5))

# This should call Rust runtime transparently
results = await pipeline.run()  # ← Calls Rust!
```

**Behind the scenes:**
1. Python serializes pipeline to JSON manifest
2. Python calls Rust via FFI (PyO3)
3. Rust executes pipeline with our executor
4. Rust returns results to Python
5. User gets results as Python objects

---

## Tasks Overview

### Task 1.4.2: Implement Pipeline.run() FFI Wrapper
**Goal:** Create Python method that calls Rust executor

### Task 1.4.3: Create Rust FFI Entry Points ✅ (Already done)
**Status:** PyO3 chosen, basic setup exists in `runtime/src/python/mod.rs`

### Task 1.4.4: Implement Data Marshaling (Python → Rust)
**Goal:** Convert Python objects to Rust `Value` types

### Task 1.4.5: Implement Result Marshaling (Rust → Python)
**Goal:** Convert Rust results back to Python objects

---

## Current State

### What We Have ✅
- Rust runtime with working executor
- Manifest schema and parser
- Node lifecycle system
- PyO3 dependency added
- Basic Python module scaffold in `runtime/src/python/mod.rs`

### What We Need
- Python extension module built and importable
- `Pipeline.run()` that calls Rust
- Data marshaling in both directions
- Async bridge (Python asyncio ↔ Rust tokio)

---

## Implementation Plan

## Task 1.4.2: Pipeline.run() FFI Wrapper

### Step 1: Build Rust Extension Module

**Modify:** `runtime/Cargo.toml`

```toml
[lib]
name = "remotemedia_runtime"
crate-type = ["cdylib", "rlib"]  # Add cdylib for Python extension

[dependencies]
pyo3 = { version = "0.20", features = ["extension-module", "abi3-py38"] }
pyo3-asyncio = { version = "0.20", features = ["tokio-runtime"] }
```

### Step 2: Implement Rust FFI Functions

**Modify:** `runtime/src/python/mod.rs`

```rust
use pyo3::prelude::*;
use pyo3_asyncio::tokio::future_into_py;
use crate::executor::Executor;
use crate::manifest::parse_manifest;

/// Execute a pipeline from a JSON manifest
#[pyfunction]
fn execute_pipeline<'py>(
    py: Python<'py>,
    manifest_json: String,
) -> PyResult<&'py PyAny> {
    // Convert to async Python future
    future_into_py(py, async move {
        // Parse manifest
        let manifest = parse_manifest(&manifest_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Failed to parse manifest: {}", e)
            ))?;

        // Execute
        let executor = Executor::new();
        let result = executor.execute(&manifest).await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Execution failed: {}", e)
            ))?;

        // Convert results to Python (Task 1.4.5)
        Ok(Python::with_gil(|py| {
            result.results.into_py(py)
        }))
    })
}

/// Execute pipeline with input data
#[pyfunction]
fn execute_pipeline_with_input<'py>(
    py: Python<'py>,
    manifest_json: String,
    input_data: Vec<PyObject>,
) -> PyResult<&'py PyAny> {
    future_into_py(py, async move {
        let manifest = parse_manifest(&manifest_json)?;

        // Convert Python input to Rust Values (Task 1.4.4)
        let rust_input: Vec<serde_json::Value> = Python::with_gil(|py| {
            input_data.iter()
                .map(|obj| python_to_json(py, obj))
                .collect::<PyResult<Vec<_>>>()
        })?;

        let executor = Executor::new();
        let result = executor.execute_with_input(&manifest, rust_input).await?;

        // Convert back to Python (Task 1.4.5)
        Ok(Python::with_gil(|py| {
            result.results.into_py(py)
        }))
    })
}

/// Python module initialization
#[pymodule]
fn _remotemedia_runtime(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(execute_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(execute_pipeline_with_input, m)?)?;
    Ok(())
}
```

### Step 3: Build System Setup

**Create:** `runtime/setup.py`

```python
from setuptools import setup
from setuptools_rust import Binding, RustExtension

setup(
    name="remotemedia-runtime",
    version="0.1.0",
    rust_extensions=[
        RustExtension(
            "remotemedia._remotemedia_runtime",
            binding=Binding.PyO3,
            debug=False,
        )
    ],
    zip_safe=False,
)
```

**Or use maturin (recommended):**

```bash
cd runtime
pip install maturin
maturin develop  # Build and install in development mode
```

### Step 4: Python Integration

**Modify:** `python-client/remotemedia/core/pipeline.py`

```python
import asyncio
from typing import Any, List, Optional

class Pipeline:
    # ... existing code ...

    async def run(
        self,
        input_data: Optional[List[Any]] = None,
        use_rust: bool = True
    ) -> List[Any]:
        """
        Execute the pipeline.

        Args:
            input_data: Optional input data for the pipeline
            use_rust: Use Rust runtime (default: True)

        Returns:
            List of results
        """
        if use_rust:
            return await self._run_rust(input_data)
        else:
            return await self._run_python(input_data)

    async def _run_rust(self, input_data: Optional[List[Any]] = None) -> List[Any]:
        """Execute using Rust runtime."""
        try:
            from remotemedia._remotemedia_runtime import (
                execute_pipeline,
                execute_pipeline_with_input
            )
        except ImportError:
            logger.warning("Rust runtime not available, falling back to Python")
            return await self._run_python(input_data)

        # Serialize to manifest
        manifest_json = self.serialize()

        # Call Rust
        if input_data is None:
            results = await execute_pipeline(manifest_json)
        else:
            results = await execute_pipeline_with_input(manifest_json, input_data)

        return results

    async def _run_python(self, input_data: Optional[List[Any]] = None) -> List[Any]:
        """Execute using Python runtime (existing implementation)."""
        # ... existing Python execution code ...
        pass
```

---

## Task 1.4.4: Data Marshaling (Python → Rust)

**Goal:** Convert Python objects to Rust `serde_json::Value`

**Create:** `runtime/src/python/marshal.rs`

```rust
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyBool, PyFloat, PyInt, PyString, PyNone};
use serde_json::Value;

/// Convert Python object to JSON Value
pub fn python_to_json(py: Python, obj: &PyObject) -> PyResult<Value> {
    // None
    if obj.is_none(py) {
        return Ok(Value::Null);
    }

    // Boolean
    if let Ok(val) = obj.extract::<bool>(py) {
        return Ok(Value::Bool(val));
    }

    // Integer
    if let Ok(val) = obj.extract::<i64>(py) {
        return Ok(Value::Number(val.into()));
    }

    // Float
    if let Ok(val) = obj.extract::<f64>(py) {
        if let Some(num) = serde_json::Number::from_f64(val) {
            return Ok(Value::Number(num));
        }
    }

    // String
    if let Ok(val) = obj.extract::<String>(py) {
        return Ok(Value::String(val));
    }

    // List
    if let Ok(list) = obj.downcast::<PyList>(py) {
        let mut vec = Vec::new();
        for item in list.iter() {
            vec.push(python_to_json(py, &item.into())?);
        }
        return Ok(Value::Array(vec));
    }

    // Dict
    if let Ok(dict) = obj.downcast::<PyDict>(py) {
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key_str = key.extract::<String>()?;
            map.insert(key_str, python_to_json(py, &value.into())?);
        }
        return Ok(Value::Object(map));
    }

    // Fallback: Try to serialize with pickle or return error
    Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
        format!("Cannot convert Python type {} to JSON", obj.as_ref(py).get_type().name()?)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marshal_primitives() {
        Python::with_gil(|py| {
            // Integer
            let py_int = 42.into_py(py);
            assert_eq!(python_to_json(py, &py_int).unwrap(), Value::from(42));

            // String
            let py_str = "hello".into_py(py);
            assert_eq!(python_to_json(py, &py_str).unwrap(), Value::from("hello"));

            // Boolean
            let py_bool = true.into_py(py);
            assert_eq!(python_to_json(py, &py_bool).unwrap(), Value::Bool(true));

            // None
            let py_none = py.None();
            assert_eq!(python_to_json(py, &py_none).unwrap(), Value::Null);
        });
    }

    #[test]
    fn test_marshal_collections() {
        Python::with_gil(|py| {
            // List
            let py_list = vec![1, 2, 3].into_py(py);
            let result = python_to_json(py, &py_list).unwrap();
            assert_eq!(result, Value::Array(vec![
                Value::from(1),
                Value::from(2),
                Value::from(3),
            ]));

            // Dict
            let py_dict = [("a", 1), ("b", 2)].into_py_dict(py).into();
            let result = python_to_json(py, &py_dict).unwrap();
            assert!(result.is_object());
        });
    }
}
```

---

## Task 1.4.5: Result Marshaling (Rust → Python)

**Goal:** Convert Rust results back to Python

**Add to:** `runtime/src/python/marshal.rs`

```rust
use pyo3::types::{PyDict, PyList};

/// Convert JSON Value to Python object
pub fn json_to_python(py: Python, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),

        Value::Bool(b) => Ok(b.into_py(py)),

        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_py(py))
            } else {
                Ok(py.None())
            }
        }

        Value::String(s) => Ok(s.into_py(py)),

        Value::Array(arr) => {
            let py_list = PyList::empty(py);
            for item in arr {
                py_list.append(json_to_python(py, item)?)?;
            }
            Ok(py_list.into())
        }

        Value::Object(obj) => {
            let py_dict = PyDict::new(py);
            for (key, value) in obj {
                py_dict.set_item(key, json_to_python(py, value)?)?;
            }
            Ok(py_dict.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unmarshal_primitives() {
        Python::with_gil(|py| {
            // Integer
            let json_val = Value::from(42);
            let py_obj = json_to_python(py, &json_val).unwrap();
            assert_eq!(py_obj.extract::<i64>(py).unwrap(), 42);

            // String
            let json_val = Value::from("hello");
            let py_obj = json_to_python(py, &json_val).unwrap();
            assert_eq!(py_obj.extract::<String>(py).unwrap(), "hello");
        });
    }

    #[test]
    fn test_round_trip() {
        Python::with_gil(|py| {
            let original = vec![1, 2, 3].into_py(py);

            // Python → Rust
            let json_val = python_to_json(py, &original).unwrap();

            // Rust → Python
            let result = json_to_python(py, &json_val).unwrap();

            // Verify
            assert_eq!(
                result.extract::<Vec<i64>>(py).unwrap(),
                vec![1, 2, 3]
            );
        });
    }
}
```

---

## Build and Test Process

### Step 1: Build Rust Extension

```bash
cd runtime

# Using maturin (recommended)
pip install maturin
maturin develop --release

# Or using setuptools-rust
pip install setuptools-rust
python setup.py develop
```

### Step 2: Test Import

```python
# Test in Python
python -c "from remotemedia._remotemedia_runtime import execute_pipeline; print('Success!')"
```

### Step 3: Integration Test

**Create:** `python-client/tests/test_rust_integration.py`

```python
import pytest
import asyncio
from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.base import PassThroughNode
from remotemedia.nodes.calculator import CalculatorNode


@pytest.mark.asyncio
async def test_rust_pipeline_execution():
    """Test executing pipeline via Rust runtime."""
    # Create pipeline
    pipeline = Pipeline(name="test")
    pipeline.add_node(PassThroughNode(name="pass1"))
    pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=5))
    pipeline.add_node(PassThroughNode(name="pass2"))

    # Execute with Rust
    results = await pipeline.run(input_data=[1, 2, 3], use_rust=True)

    # Verify (Calculator adds 5 to each)
    assert results == [6, 7, 8]


@pytest.mark.asyncio
async def test_rust_fallback_to_python():
    """Test fallback to Python when Rust not available."""
    pipeline = Pipeline(name="test")
    pipeline.add_node(PassThroughNode(name="pass1"))

    # This should work even if Rust not available
    results = await pipeline.run(input_data=[1, 2, 3])
    assert len(results) > 0
```

---

## Files to Create/Modify

### New Files
1. `runtime/src/python/marshal.rs` - Data marshaling
2. `runtime/setup.py` or `pyproject.toml` - Build config
3. `python-client/tests/test_rust_ffi.py` - FFI tests

### Modified Files
1. `runtime/Cargo.toml` - Add cdylib, pyo3-asyncio
2. `runtime/src/python/mod.rs` - FFI functions
3. `runtime/src/lib.rs` - Export marshal module
4. `python-client/remotemedia/core/pipeline.py` - Add run() with Rust option

---

## Challenges and Solutions

### Challenge 1: Async Bridge (asyncio ↔ tokio)
**Solution:** Use `pyo3-asyncio` crate with `tokio-runtime` feature

```rust
use pyo3_asyncio::tokio::future_into_py;

#[pyfunction]
fn execute_pipeline<'py>(py: Python<'py>, manifest: String) -> PyResult<&'py PyAny> {
    future_into_py(py, async move {
        // Rust async code here
    })
}
```

### Challenge 2: GIL (Global Interpreter Lock)
**Solution:** Release GIL during Rust execution

```rust
py.allow_threads(|| {
    // Heavy computation here (GIL released)
})
```

### Challenge 3: Error Propagation
**Solution:** Convert Rust errors to Python exceptions

```rust
.map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
    format!("Execution failed: {}", e)
))
```

### Challenge 4: Complex Python Types (numpy, pandas)
**Solution:** Phase 1 - support primitives and collections. Phase 2 - add numpy/pandas support

---

## Acceptance Criteria

**Tasks 1.4.2-1.4.5 Complete When:**
- [ ] Rust extension module builds successfully
- [ ] Python can import `remotemedia._remotemedia_runtime`
- [ ] `execute_pipeline()` function works
- [ ] `execute_pipeline_with_input()` function works
- [ ] Data marshaling works for primitives (int, float, str, bool, None)
- [ ] Data marshaling works for collections (list, dict)
- [ ] Round-trip test passes (Python → Rust → Python)
- [ ] `Pipeline.run(use_rust=True)` executes via Rust
- [ ] Fallback to Python works when Rust unavailable
- [ ] All tests passing
- [ ] Documentation updated

---

## Testing Strategy

### Unit Tests (Rust)
- Marshaling functions (python_to_json, json_to_python)
- Round-trip conversions
- Error cases

### Unit Tests (Python)
- Import test
- FFI function calls
- Error handling

### Integration Tests
- Full pipeline execution via FFI
- Data flow validation
- Performance comparison

### Manual Testing
```bash
# Build
cd runtime
maturin develop --release

# Test
cd ../python-client
python -m pytest tests/test_rust_ffi.py -v

# Run example
python examples/rust_execution_example.py
```

---

## Next Steps After 1.4.2-1.4.5

**Remaining in Phase 1.4:**
- **1.4.6:** Error handling across FFI boundary
- **1.4.7:** Test FFI with simple pipeline (2-3 nodes)
- **1.4.8:** Optimize FFI overhead (zero-copy for numpy arrays)

**Then Phase 1.5:** RustPython VM Integration

---

## Estimated Timeline

- **Session 1:** Tasks 1.4.2-1.4.3 (Build system + FFI functions) - 1-2 hours
- **Session 2:** Task 1.4.4 (Python → Rust marshaling) - 1 hour
- **Session 3:** Task 1.4.5 (Rust → Python marshaling) - 1 hour
- **Session 4:** Integration and testing - 1 hour

**Total:** 4-5 hours / 2-3 sessions

---

## Ready to Start?

**First step:** Set up the build system and create basic FFI functions.

1. Update `Cargo.toml` with cdylib and pyo3-asyncio
2. Implement `execute_pipeline()` in `runtime/src/python/mod.rs`
3. Build with `maturin develop`
4. Test import in Python

Let's do it! 🚀
