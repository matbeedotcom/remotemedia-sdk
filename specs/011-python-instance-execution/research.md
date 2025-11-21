# Research: Python Instance Execution in FFI

**Date**: 2025-11-20
**Feature**: Python Instance Execution in FFI
**Phase**: 0 - Technical Research

## Overview

This document consolidates research findings for enabling Python Node instance execution in the RemoteMedia FFI layer. Research covers three key areas: PyO3 object lifetime management, cloudpickle serialization capabilities, and manifest schema extension strategies.

---

## Decision: PyO3 Object Lifetime Management

**Chosen Approach**: Use `Py<PyAny>` for storing Python Node references in Rust structs, with `Python::with_gil()` for method calls

**Rationale**:
For the RemoteMedia FFI feature where Python Node instances must be stored in Rust structs and called during pipeline execution, `Py<T>` is the correct choice because:

1. **Storage in Structs**: `#[pyclass]` types and regular Rust structs cannot have lifetime parameters, so `Py<T>` is the only way to store Python objects that need to persist beyond a single function scope
2. **GIL-Independent**: `Py<T>` is not bound to the `'py` lifetime, allowing it to be stored, cloned, and passed between async tasks without GIL constraints
3. **Send + Sync**: `Py<PyAny>` implements `Send`, making it safe to pass between async Rust tasks (tokio) - critical for the async pipeline execution architecture
4. **Reference Counting**: Works like `Arc<T>` - cloning is cheap and only increments Python's reference count

**Key Practices**:

1. **Storage Pattern**:
   ```rust
   use pyo3::prelude::*;

   struct NodeExecutor {
       node_instance: Py<PyAny>,  // NOT Bound<'py, PyAny>
   }
   ```

2. **Calling Python Methods from Rust**:
   ```rust
   // From async Rust code
   Python::with_gil(|py| {
       let result = self.node_instance
           .call_method1(py, "process", (input_data,))?;
       Ok(result)
   })
   ```

3. **Reference Counting**:
   - Prefer `Py::clone_ref(&obj, py)` over `obj.clone()` when GIL is already held (faster, safer)
   - Use `obj.clone()` only when crossing thread boundaries without GIL access
   - Both properly increment Python's reference count

4. **Passing Between Async Tasks**:
   ```rust
   let node_ref = self.node_instance.clone();  // Safe - increments refcount
   tokio::spawn(async move {
       Python::with_gil(|py| {
           node_ref.call_method0(py, "initialize")?;
           Ok(())
       })
   });
   ```

5. **GIL Management in Async Context**:
   - Minimize GIL hold time - acquire only when accessing Python objects
   - Release GIL during CPU-intensive Rust operations with `py.allow_threads(|| { ... })`
   - For async operations, avoid holding GIL across `.await` points
   - Pattern: `Python::with_gil` → quick Python call → release → continue async work

6. **Integration with pyo3-async-runtimes** (for calling async Python methods):
   ```rust
   use pyo3_async_runtimes::tokio::into_future;

   let coroutine = Python::with_gil(|py| {
       self.node_instance.call_method0(py, "async_process")
   })?;
   let future = pyo3_async_runtimes::tokio::into_future(coroutine)?;
   let result = future.await?;
   ```

7. **Garbage Collection for Complex Objects**:
   - If Python nodes hold circular references, they will be handled by Python's GC
   - `Py<T>` automatically decrements refcount on drop (when GIL is next acquired)
   - No manual cleanup needed - Rust's drop semantics + Python's GC work together

**Alternatives Considered**:

- **`Bound<'py, PyAny>`**: Cannot be stored in structs without lifetimes. Only suitable for temporary references within a single function scope where the `Python<'py>` token is available. Not viable for async task boundaries.

- **`&PyAny` (GIL References)**: Deprecated as of PyO3 0.21+ and gated behind `gil-refs` feature flag. Unsound and less performant. Will be removed in next major version.

- **`PyObject` (type alias)**: `PyObject` is just a type alias for `Py<PyAny>`, so functionally equivalent. Using `Py<PyAny>` explicitly is more idiomatic in modern PyO3.

- **Custom Python wrapper struct**: Could wrap Python objects in a Rust struct with interior mutability, but adds unnecessary complexity. `Py<T>` already provides all needed functionality.

**References**:

- [PyO3 Python Object Types Documentation](https://pyo3.rs/v0.23.3/types.html) - Comprehensive guide on `Py<T>` vs `Bound<'py, T>`
- [PyO3 Thread Safety Guide](https://pyo3.rs/v0.23.4/class/thread-safety.html) - Passing Python objects between Rust threads
- [PyO3 Calling Python from Rust](https://pyo3.rs/v0.27.1/python-from-rust.html) - Using `Python::with_gil()`
- [PyO3 Parallelism Guide](https://pyo3.rs/main/parallelism) - GIL management and async integration
- [PyO3 Memory Management](https://pyo3.rs/v0.21.2/memory.html) - Reference counting and garbage collection
- [PyO3 `Py<T>` API Documentation](https://docs.rs/pyo3/latest/pyo3/struct.Py.html) - Methods and usage patterns
- [pyo3-async-runtimes GitHub](https://github.com/PyO3/pyo3-async-runtimes) - Integrating Python async with tokio

---

## Decision: cloudpickle for Node Serialization

**Chosen Approach**: Use cloudpickle with custom `__getstate__`/`__setstate__` methods and pre/post-serialization hooks

**Rationale**:
- cloudpickle extends standard pickle to serialize functions, classes, and closures by value rather than reference
- Ideal for distributed computing where code is shipped across processes/network
- Already used successfully in the codebase (`code_packager.py`) for serializing Python objects
- Widely adopted in distributed frameworks (Dask, Ray, Apache Spark, IPython Parallel)
- Drop-in replacement for pickle with same API

### Capabilities

**✅ What cloudpickle CAN serialize:**
- Lambda functions and closures with captured variables
- Functions and classes defined interactively in `__main__` module
- Functions with dynamically generated code
- Classes defined in notebooks/REPL/scripts
- Standard Python data types (dict, list, tuple, etc.)
- NumPy arrays (though slower than pickle)
- Objects from modules registered with `register_pickle_by_value()`
- Monkey-patched objects (serializes members directly)
- Custom classes with `__getstate__`/`__setstate__` methods

**❌ What cloudpickle CANNOT serialize (or has limitations):**
- ML models with native state (PyTorch, TensorFlow) - requires model-specific serialization
- File handles and open file objects
- Weak references (requires dill)
- Thread locks, threading primitives
- Network connections, database connections
- CUDA/GPU tensors in memory
- Objects referencing unavailable modules in target environment
- Self-referencing circular objects (edge case issues)

**⚠️ Performance Limitations:**
- 2-8x slower than standard pickle for serialization
- Very slow for large Python collections (dict/list with millions of items)
- Not suitable for long-term storage (version-dependent)
- Requires exact same Python version on both ends
- Can produce larger serialized sizes for complex closures

### Best Practices for Node Serialization

**1. Implement Custom Serialization Hooks**
```python
class Node:
    def __getstate__(self):
        """Called before pickling - clean up non-serializable state."""
        state = self.__dict__.copy()
        # Remove non-serializable attributes
        state.pop('_lock', None)  # Threading locks
        state.pop('_file_handle', None)  # Open files
        state.pop('_logger', None)  # Loggers (can be recreated)
        # For ML models, save to bytes and restore in __setstate__
        if hasattr(self, 'model'):
            state['_model_bytes'] = self._serialize_model()
            state.pop('model', None)
        return state

    def __setstate__(self, state):
        """Called after unpickling - restore non-serializable state."""
        self.__dict__.update(state)
        # Recreate non-serializable objects
        self._logger = logging.getLogger(self.__class__.__name__)
        if '_model_bytes' in state:
            self.model = self._deserialize_model(state['_model_bytes'])
```

**2. Use cleanup() Before Pickling**
- Call `node.cleanup()` to release resources (close files, clear GPU memory, stop threads)
- Clean up state manager sessions if not needed for transfer
- Close network connections and database handles

**3. Use initialize() After Unpickling**
- Call `node.initialize()` after deserialization to restore runtime state
- Reload ML models from serialized bytes
- Re-establish connections if needed
- Recreate thread pools, loggers, etc.

**4. Test Round-Trip Serialization**
```python
def test_node_serialization(node):
    """Verify node can be serialized and deserialized."""
    node.cleanup()  # Clean up first

    # Serialize
    import cloudpickle
    serialized = cloudpickle.dumps(node)

    # Deserialize
    restored_node = cloudpickle.loads(serialized)

    # Initialize restored node
    restored_node.initialize()

    # Verify behavior matches
    test_data = create_test_data()
    original_output = node.process(test_data)
    restored_output = restored_node.process(test_data)

    assert_equal(original_output, restored_output)
```

**5. Handle ML Models Specially**
```python
# For PyTorch models
def _serialize_model(self):
    import io, torch
    buffer = io.BytesIO()
    torch.save(self.model.state_dict(), buffer)
    return buffer.getvalue()

def _deserialize_model(self, model_bytes):
    import io, torch
    buffer = io.BytesIO(model_bytes)
    state_dict = torch.load(buffer)
    self.model.load_state_dict(state_dict)
    return self.model
```

### Error Handling Strategy

**1. Detect Serialization Failures Early**
```python
def safe_serialize(node):
    """Attempt serialization with helpful error messages."""
    import cloudpickle

    try:
        # First, try to identify non-serializable attributes
        node.cleanup()
        serialized = cloudpickle.dumps(node)
        return serialized
    except Exception as e:
        # Provide detailed error context
        raise SerializationError(
            f"Failed to serialize {node.__class__.__name__}: {str(e)}\n"
            f"Check for non-serializable attributes: file handles, locks, "
            f"connections, or unserializable ML model objects.\n"
            f"Ensure cleanup() was called before serialization."
        ) from e
```

**2. Provide Helpful Error Messages**
```python
class Node:
    def validate_serializable(self):
        """Check if node is ready for serialization."""
        non_serializable = []

        if hasattr(self, '_lock'):
            non_serializable.append('threading locks')
        if hasattr(self, '_file_handle'):
            non_serializable.append('open file handles')
        if self._is_initialized:
            non_serializable.append('initialized state (call cleanup() first)')

        if non_serializable:
            raise ValueError(
                f"Node {self.name} has non-serializable state: "
                f"{', '.join(non_serializable)}"
            )
```

**Alternatives Considered**:

- **Standard pickle**: Cannot serialize lambdas, closures, or interactive classes. Insufficient for Node instances with complex state.
- **dill**: More capabilities but slower than cloudpickle. cloudpickle sufficient for our needs.
- **MessagePack / Protocol Buffers / JSON**: Cannot serialize Python code (functions, classes). Limited to data structures.
- **Custom Binary Protocol**: High development cost, no ecosystem support.

**References**:
- [cloudpickle GitHub](https://github.com/cloudpipe/cloudpickle)
- [cloudpickle PyPI](https://pypi.org/project/cloudpickle/)
- [Python pickle module](https://docs.python.org/3/library/pickle.html)
- Existing implementation: `python-client/remotemedia/packaging/code_packager.py`

---

## Decision: Manifest Schema Extension Strategy

**Chosen Approach**: Dual-Mode Input Acceptance with Runtime Detection (No Schema Extension Required)

**Rationale**: After analyzing the codebase and researching schema evolution patterns, the optimal approach is to support both instance-based and manifest-based inputs **without modifying the JSON schema**. Instead, we detect the input type at the Python FFI boundary and handle each appropriately:

1. **Instance Detection at FFI Boundary**: The Python layer detects whether the input is:
   - A `Pipeline` instance (has `.serialize()` method)
   - A `Node` instance (convert to single-node pipeline)
   - A list of `Node` instances (convert to pipeline)
   - A JSON string manifest
   - A dict manifest

2. **Automatic Serialization**: If an instance is detected, automatically call `.serialize()` to convert it to manifest JSON before passing to Rust

3. **Backward Compatibility**: Existing manifest-based code continues to work unchanged

### Detection Strategy

In `transports/remotemedia-ffi/src/api.rs`:

```python
# Python-side detection (preferred approach)
# In Python wrapper before calling FFI:
def execute_pipeline(pipeline_or_manifest, enable_metrics=False):
    # Detect input type
    if hasattr(pipeline_or_manifest, 'serialize'):
        # It's a Pipeline instance
        manifest_json = pipeline_or_manifest.serialize()
    elif isinstance(pipeline_or_manifest, list):
        # It's a list of Node instances - create Pipeline
        from remotemedia.core.pipeline import Pipeline
        pipeline = Pipeline(nodes=pipeline_or_manifest)
        manifest_json = pipeline.serialize()
    elif isinstance(pipeline_or_manifest, dict):
        # It's a manifest dict
        manifest_json = json.dumps(pipeline_or_manifest)
    elif isinstance(pipeline_or_manifest, str):
        # It's already JSON
        manifest_json = pipeline_or_manifest
    else:
        raise TypeError(f"Expected Pipeline, list of Nodes, dict, or str, got {type(pipeline_or_manifest)}")

    # Call Rust FFI with manifest JSON
    return await _remotemedia_runtime.execute_pipeline(manifest_json, enable_metrics)
```

### Runtime Type Checking Approach

Based on research, use Python's built-in mechanisms:

1. **`hasattr()` for duck typing**: Check if object has `.serialize()` method
2. **`isinstance()` for type validation**: Verify dict/str types
3. **Type hints with runtime validation**: Use Python type annotations for IDE support

```python
from typing import Union, List, overload
from remotemedia.core.pipeline import Pipeline
from remotemedia.core.node import Node

@overload
def execute_pipeline(pipeline: Pipeline, enable_metrics: bool = False) -> Any: ...

@overload
def execute_pipeline(nodes: List[Node], enable_metrics: bool = False) -> Any: ...

@overload
def execute_pipeline(manifest: Union[str, dict], enable_metrics: bool = False) -> Any: ...

def execute_pipeline(pipeline_or_manifest: Union[Pipeline, List[Node], str, dict], enable_metrics: bool = False) -> Any:
    """
    Execute a pipeline using the Rust runtime.

    Args:
        pipeline_or_manifest: Either a Pipeline instance, list of Node instances,
                              JSON manifest string, or manifest dict
        enable_metrics: Enable performance metrics collection

    Returns:
        Pipeline execution results
    """
    # Implementation as shown above
```

### Backward Compatibility

1. **Existing manifest-based code**: Works unchanged
   ```python
   # Old code (still works)
   manifest_json = pipeline.serialize()
   await execute_pipeline(manifest_json)
   ```

2. **New instance-based code**: Simpler API
   ```python
   # New code (more ergonomic)
   pipeline = Pipeline("my-pipeline")
   pipeline.add_node(MyNode(param1="value"))
   await execute_pipeline(pipeline)  # Automatic serialization

   # Or even simpler
   await execute_pipeline([Node1(), Node2(), Node3()])
   ```

3. **Migration Path**: No migration required - both approaches work simultaneously

**Alternatives Considered**:

- **Extend Schema with `instance_ref` Field**: Requires schema versioning, complicates deserializer, breaks language-neutrality
- **Separate Schema Version (v2)**: Increases maintenance burden, no clear benefit
- **Binary Serialization (Pickle/MessagePack)**: Loses human-readable format, security concerns
- **Global Instance Registry**: Complex lifetime management, thread-safety concerns

**References**:
- [JSON Schema Versioning Best Practices](https://json-schema.org/blog/posts/future-of-json-schema)
- **Similar Systems**: PyArrow (deprecated custom serialization), Pydantic, FastAPI, Airflow
- [Runtime Type Checking in Python](https://stassajin.medium.com/performing-runtime-type-checking-in-python-b46ced88ef2e)

### Key Advantages of Chosen Approach

1. **Zero Breaking Changes**: 100% backward compatible (FR-012 satisfied)
2. **No Schema Pollution**: Keeps manifest format language-neutral
3. **Ergonomic API**: Users can pass instances directly
4. **Simple Implementation**: Type detection at single point (FFI boundary)
5. **Maintainable**: No dual schema/parser support needed
6. **Future-Proof**: Easy to extend with more input types if needed

---

## Summary

All three technical unknowns have been resolved:

1. **PyO3 Object Lifetimes**: Use `Py<PyAny>` with `Python::with_gil()` for GIL-safe async execution
2. **cloudpickle Serialization**: Leverage existing `cleanup()`/`initialize()` lifecycle with cloudpickle for IPC
3. **Manifest Schema**: No schema changes needed - runtime type detection at FFI boundary

These decisions enable the feature implementation while maintaining backward compatibility and leveraging existing infrastructure.
