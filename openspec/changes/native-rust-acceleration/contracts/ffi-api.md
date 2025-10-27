# FFI API Contract

**Interface**: Rust → Python FFI boundary  
**Protocol**: PyO3  
**Version**: 1.0.0

## Overview

Defines the C-compatible FFI functions exposed by the Rust runtime for Python SDK integration.

---

## Functions

### `execute_pipeline`

Execute a pipeline from JSON manifest.

**Signature**:
```rust
#[pyfunction]
pub fn execute_pipeline(
    py: Python,
    manifest_json: &str,
) -> PyResult<Py<PyDict>>
```

**Parameters**:
- `manifest_json` (str): JSON-serialized pipeline manifest

**Returns**:
- `dict`: Execution result with keys:
  - `success` (bool): Whether execution succeeded
  - `result` (Any): Pipeline output data
  - `metrics` (dict): Performance metrics
  - `error` (str | None): Error message if failed

**Errors**:
- `ValueError`: Invalid manifest JSON
- `RuntimeError`: Execution failure

**Example**:
```python
from remotemedia_runtime import execute_pipeline

manifest = '{"version":"v1","nodes":[...] }'
result = execute_pipeline(manifest)

if result["success"]:
    print(f"Output: {result['result']}")
    print(f"Time: {result['metrics']['total_time_ms']}ms")
else:
    print(f"Error: {result['error']}")
```

---

### `execute_pipeline_with_input`

Execute a pipeline with explicit input data.

**Signature**:
```rust
#[pyfunction]
pub fn execute_pipeline_with_input(
    py: Python,
    manifest_json: &str,
    input_data: &PyAny,
) -> PyResult<Py<PyDict>>
```

**Parameters**:
- `manifest_json` (str): JSON-serialized pipeline manifest
- `input_data` (Any): Input data for first node (numpy array, dict, list, etc.)

**Returns**:
- `dict`: Same as `execute_pipeline`

**Example**:
```python
import numpy as np
from remotemedia_runtime import execute_pipeline_with_input

audio = np.random.rand(16000).astype(np.float32)
manifest = '{"version":"v1","nodes":[{"id":"vad","node_type":"VADNode"}],"connections":[]}'

result = execute_pipeline_with_input(manifest, audio)
segments = result["result"]
```

---

### `validate_manifest`

Validate a pipeline manifest without executing.

**Signature**:
```rust
#[pyfunction]
pub fn validate_manifest(manifest_json: &str) -> PyResult<bool>
```

**Parameters**:
- `manifest_json` (str): JSON-serialized pipeline manifest

**Returns**:
- `bool`: True if valid

**Errors**:
- `ValueError`: Invalid manifest with detailed error message

**Example**:
```python
from remotemedia_runtime import validate_manifest

try:
    validate_manifest(manifest_json)
    print("Manifest is valid")
except ValueError as e:
    print(f"Invalid manifest: {e}")
```

---

### `get_available_nodes`

List all available node types.

**Signature**:
```rust
#[pyfunction]
pub fn get_available_nodes() -> PyResult<Vec<String>>
```

**Returns**:
- `list[str]`: List of node type names

**Example**:
```python
from remotemedia_runtime import get_available_nodes

nodes = get_available_nodes()
print(f"Available nodes: {', '.join(nodes)}")
# Output: Available nodes: VADNode, ResampleNode, FormatConverterNode, ...
```

---

### `get_node_metadata`

Get metadata for a specific node type.

**Signature**:
```rust
#[pyfunction]
pub fn get_node_metadata(node_type: &str) -> PyResult<Py<PyDict>>
```

**Parameters**:
- `node_type` (str): Node type name

**Returns**:
- `dict`: Node metadata with keys:
  - `name` (str): Node type name
  - `version` (str): Node version
  - `capabilities` (list[str]): Required capabilities
  - `parameters` (dict): Parameter schema

**Example**:
```python
from remotemedia_runtime import get_node_metadata

meta = get_node_metadata("VADNode")
print(f"Parameters: {meta['parameters']}")
# Output: Parameters: {'threshold': {'type': 'float', 'default': -30.0}, ...}
```

---

## Module Initialization

**Module name**: `remotemedia_runtime`

**Initialization**:
```rust
#[pymodule]
fn remotemedia_runtime(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(execute_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(execute_pipeline_with_input, m)?)?;
    m.add_function(wrap_pyfunction!(validate_manifest, m)?)?;
    m.add_function(wrap_pyfunction!(get_available_nodes, m)?)?;
    m.add_function(wrap_pyfunction!(get_node_metadata, m)?)?;
    Ok(())
}
```

---

## Error Handling

### Error Types

All errors are converted to Python exceptions:

| Rust Error | Python Exception | When |
|------------|------------------|------|
| `ManifestError` | `ValueError` | Invalid JSON or schema |
| `GraphError` | `ValueError` | Invalid pipeline graph |
| `CycleError` | `ValueError` | Cycle detected in graph |
| `NodeNotFound` | `KeyError` | Unknown node type |
| `NodeExecutionError` | `RuntimeError` | Node execution failed |
| `PythonError` | `RuntimeError` | Python node raised exception |
| `MarshalingError` | `TypeError` | Data type conversion failed |

### Error Context

All errors include:
- Human-readable message
- Context (node ID, operation)
- Stack trace (for Python errors)

**Example**:
```python
try:
    result = execute_pipeline(manifest)
except RuntimeError as e:
    print(f"Execution failed: {e}")
    # Output: Execution failed: Node execution failed: whisper: Model file not found
```

---

## Performance Guarantees

| Operation | Latency | Notes |
|-----------|---------|-------|
| **FFI call overhead** | <1μs | Measured via criterion |
| **Manifest parsing** | <5ms | For typical pipelines (5-10 nodes) |
| **Graph construction** | <1ms | Topological sort |
| **Metrics collection** | <100μs | Negligible overhead |

---

## Thread Safety

- ✅ **Thread-safe**: All FFI functions can be called from multiple Python threads
- ✅ **GIL management**: Rust releases GIL during computation-heavy operations
- ✅ **Memory safety**: PyO3 handles reference counting automatically

---

## Versioning

**Semantic versioning**: MAJOR.MINOR.PATCH

- **MAJOR**: Breaking API changes (function signature changes)
- **MINOR**: New features (new functions, new node types)
- **PATCH**: Bug fixes, performance improvements

**Current version**: 1.0.0

---

## Testing Contract

### Unit Tests

All FFI functions must have:
- Happy path test (valid input → expected output)
- Error path test (invalid input → expected error)
- Edge case tests (empty manifest, single node, etc.)

### Integration Tests

- Python SDK integration (call from Python, verify results)
- Performance tests (benchmark FFI overhead)
- Memory leak tests (valgrind)

---

## Example Integration (Python SDK)

```python
# python-client/remotemedia/runtime/executor.py

from remotemedia_runtime import (
    execute_pipeline_with_input,
    validate_manifest,
    get_available_nodes,
)

class RustExecutor:
    """Wrapper for Rust runtime FFI."""
    
    def execute(self, manifest: dict, input_data: Any) -> dict:
        """Execute pipeline using Rust runtime."""
        manifest_json = json.dumps(manifest)
        
        # Validate first
        validate_manifest(manifest_json)
        
        # Execute
        result = execute_pipeline_with_input(manifest_json, input_data)
        
        if not result["success"]:
            raise RuntimeError(result["error"])
        
        return result
    
    @staticmethod
    def available_nodes() -> list[str]:
        """Get list of available node types."""
        return get_available_nodes()
```

**Usage**:
```python
from remotemedia import Pipeline, AudioResampleNode

# User code (unchanged)
p = Pipeline("audio")
p.add_node(AudioResampleNode(target_rate=16000))
result = p.run(audio_data)

# Behind the scenes:
# 1. Pipeline.run() calls pipeline.serialize()
# 2. Rust executor receives manifest JSON
# 3. Rust executes pipeline
# 4. Results returned to Python
# User sees transparent acceleration!
```
