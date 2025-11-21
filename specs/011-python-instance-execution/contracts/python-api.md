# Python API Contract: Instance Execution

**Feature**: Python Instance Execution in FFI
**Version**: v1
**Date**: 2025-11-20

## Overview

This contract defines the Python API for executing pipelines with Node instances in the RemoteMedia FFI layer.

---

## Function: `execute_pipeline`

Execute a pipeline using the Rust runtime with support for Node instances.

### Signature

```python
async def execute_pipeline(
    pipeline_or_manifest: Union[Pipeline, List[Node], str, dict],
    enable_metrics: bool = False
) -> Any
```

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `pipeline_or_manifest` | `Union[Pipeline, List[Node], str, dict]` | Yes | - | Pipeline instance, list of Node instances, JSON manifest string, or manifest dict |
| `enable_metrics` | `bool` | No | `False` | Enable performance metrics collection |

### Returns

| Type | Description |
|------|-------------|
| `Any` | Pipeline execution result (type depends on final node output) |
| `Dict[str, Any]` | If `enable_metrics=True`, returns `{"outputs": <result>, "metrics": <metrics_dict>}` |

### Raises

| Exception | Condition |
|-----------|-----------|
| `TypeError` | Invalid input type (not Pipeline, list, str, or dict) |
| `ValueError` | Invalid manifest format or empty pipeline |
| `SerializationError` | Node instance cannot be serialized (for multiprocess execution) |
| `RuntimeError` | Rust runtime execution failed |
| `ImportError` | remotemedia.runtime module not available |

### Behavior

1. **Type Detection**:
   - If `Pipeline` instance → call `.serialize()` to get manifest JSON
   - If `List[Node]` → create `Pipeline(nodes=<list>).serialize()`
   - If `dict` → convert to JSON string
   - If `str` → use as-is (assume valid manifest JSON)

2. **Execution**:
   - Pass manifest JSON to Rust FFI via `remotemedia.runtime.execute_pipeline()`
   - Rust runtime parses manifest and executes pipeline
   - Returns final output from last node

3. **Metrics** (if `enable_metrics=True`):
   - Collect execution time, memory usage, per-node metrics
   - Return dict with `outputs` and `metrics` keys

### Example Usage

```python
import asyncio
from remotemedia.runtime import execute_pipeline
from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes import PassThroughNode, CalculatorNode

# Option 1: Pass Pipeline instance
pipeline = Pipeline("my-pipeline")
pipeline.add_node(PassThroughNode(name="pass"))
pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=10))
result = await execute_pipeline(pipeline)
print(result)  # Processed output

# Option 2: Pass list of Node instances
nodes = [
    PassThroughNode(name="pass"),
    CalculatorNode(name="calc", operation="add", operand=10)
]
result = await execute_pipeline(nodes)

# Option 3: Pass manifest dict
manifest = {
    "version": "v1",
    "metadata": {"name": "my-pipeline"},
    "nodes": [
        {"id": "pass_0", "node_type": "PassThroughNode", "params": {}},
        {"id": "calc_1", "node_type": "CalculatorNode", "params": {"operation": "add", "operand": 10}}
    ],
    "connections": [{"from": "pass_0", "to": "calc_1"}]
}
result = await execute_pipeline(manifest)

# Option 4: Pass manifest JSON string
import json
manifest_json = json.dumps(manifest)
result = await execute_pipeline(manifest_json)

# With metrics
result = await execute_pipeline(pipeline, enable_metrics=True)
print(result['outputs'])
print(result['metrics'])
```

### Contract Guarantees

- ✅ **Backward Compatibility**: All existing manifest-based code continues to work (FR-012)
- ✅ **Type Safety**: Invalid input types raise `TypeError` immediately
- ✅ **State Preservation**: Node instance state is preserved when passed directly (FR-002)
- ✅ **Error Messages**: Clear error messages for serialization failures (FR-011, SC-005)

---

## Function: `execute_pipeline_with_input`

Execute a pipeline with input data, supporting Node instances.

### Signature

```python
async def execute_pipeline_with_input(
    pipeline_or_manifest: Union[Pipeline, List[Node], str, dict],
    input_data: List[Any],
    enable_metrics: bool = False
) -> Any
```

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `pipeline_or_manifest` | `Union[Pipeline, List[Node], str, dict]` | Yes | - | Pipeline instance, list of Node instances, JSON manifest string, or manifest dict |
| `input_data` | `List[Any]` | Yes | - | List of input items to process through the pipeline |
| `enable_metrics` | `bool` | No | `False` | Enable performance metrics collection |

### Returns

| Type | Description |
|------|-------------|
| `List[Any]` | List of pipeline execution results (one per input item) |
| `Dict[str, Any]` | If `enable_metrics=True`, returns `{"outputs": <results>, "metrics": <metrics_dict>}` |

### Raises

| Exception | Condition |
|-----------|-----------|
| `TypeError` | Invalid input type or empty `input_data` |
| `ValueError` | Invalid manifest format or empty pipeline |
| `SerializationError` | Node instance cannot be serialized (for multiprocess execution) |
| `RuntimeError` | Rust runtime execution failed |
| `ImportError` | remotemedia.runtime module not available |

### Behavior

1. **Type Detection**: Same as `execute_pipeline`
2. **Input Processing**: Each item in `input_data` is processed sequentially through the pipeline
3. **Output Collection**: Results from all inputs collected into a list
4. **Metrics**: Aggregates metrics across all input items if `enable_metrics=True`

### Example Usage

```python
from remotemedia.runtime import execute_pipeline_with_input
from remotemedia.nodes import CalculatorNode

# Execute with Node instances
nodes = [CalculatorNode(name="calc", operation="multiply", operand=2)]
input_data = [1, 2, 3, 4, 5]
results = await execute_pipeline_with_input(nodes, input_data)
print(results)  # [2, 4, 6, 8, 10]

# Execute with manifest
manifest = {"version": "v1", "nodes": [...], "connections": [...]}
results = await execute_pipeline_with_input(manifest, input_data)

# With metrics
results = await execute_pipeline_with_input(nodes, input_data, enable_metrics=True)
print(results['outputs'])  # [2, 4, 6, 8, 10]
print(results['metrics'])  # {"total_duration_us": 1234, ...}
```

### Contract Guarantees

- ✅ **Batch Processing**: Handles lists of input data efficiently
- ✅ **State Preservation**: Node state preserved across all inputs in the batch
- ✅ **Error Handling**: Individual input failures don't crash entire batch (based on error handling strategy)

---

## Function: `Pipeline.run` (Modified)

Convenience method for executing pipelines with automatic Rust runtime detection.

### Signature

```python
async def run(
    self,
    input_data: Optional[Any] = None,
    use_rust: bool = True
) -> Any
```

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input_data` | `Optional[Any]` | No | `None` | Optional input data to feed into pipeline |
| `use_rust` | `bool` | No | `True` | Whether to try using Rust runtime |

### Returns

| Type | Description |
|------|-------------|
| `Any` | Pipeline execution result |

### Raises

| Exception | Condition |
|-----------|-----------|
| `PipelineError` | Pipeline not initialized or execution failed |
| `ImportError` | Rust runtime not available (falls back to Python executor) |

### Behavior

1. **Runtime Selection**:
   - If `use_rust=True` and Rust runtime available → use `execute_pipeline()` with instance
   - If `use_rust=False` or Rust runtime unavailable → fall back to Python executor
   - Automatic fallback ensures backward compatibility

2. **Instance Execution**:
   - When using Rust runtime, passes `self` (Pipeline instance) directly to `execute_pipeline()`
   - Node instances preserved with their state
   - No manual serialization required

### Example Usage

```python
from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes import PassThroughNode, CalculatorNode

# Create pipeline with Node instances
pipeline = Pipeline("my-pipeline")
pipeline.add_node(PassThroughNode(name="pass"))
pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=10))

# Execute with Rust runtime (automatically uses instances)
result = await pipeline.run(input_data=[1, 2, 3], use_rust=True)
print(result)  # Processed outputs

# Execute with Python runtime (fallback)
result = await pipeline.run(input_data=[1, 2, 3], use_rust=False)
```

### Contract Guarantees

- ✅ **Automatic Instance Handling**: Pipeline instances automatically serialized for Rust runtime (FR-001)
- ✅ **Fallback Graceful**: Falls back to Python executor if Rust unavailable
- ✅ **State Preservation**: Node instance state preserved during execution (FR-002)

---

## Validation Contract

All functions MUST validate inputs according to FR-010:

### Node Instance Validation

```python
def validate_node_instance(obj: Any) -> bool:
    """Validate that object is a valid Node instance."""
    # Check if subclass of Node
    from remotemedia.core.node import Node
    if not isinstance(obj, Node):
        raise TypeError(f"Expected Node instance, got {type(obj).__name__}")

    # Check required methods exist
    if not hasattr(obj, 'process') or not callable(obj.process):
        raise ValueError(f"Node {obj.name} missing required process() method")

    if not hasattr(obj, 'initialize') or not callable(obj.initialize):
        raise ValueError(f"Node {obj.name} missing required initialize() method")

    return True
```

### Manifest Validation

```python
def validate_manifest(manifest: Union[str, dict]) -> bool:
    """Validate manifest format."""
    if isinstance(manifest, str):
        try:
            manifest = json.loads(manifest)
        except json.JSONDecodeError as e:
            raise ValueError(f"Invalid JSON manifest: {e}")

    if not isinstance(manifest, dict):
        raise TypeError(f"Manifest must be dict or JSON string, got {type(manifest)}")

    # Validate required fields
    if 'version' not in manifest:
        raise ValueError("Manifest missing required 'version' field")

    if 'nodes' not in manifest or not isinstance(manifest['nodes'], list):
        raise ValueError("Manifest missing required 'nodes' list")

    if len(manifest['nodes']) == 0:
        raise ValueError("Manifest must contain at least one node")

    return True
```

---

## Error Handling Contract

All errors MUST include helpful messages per FR-011 and SC-005:

### Serialization Error Format

```python
class SerializationError(Exception):
    """Raised when Node instance cannot be serialized."""
    def __init__(self, node_name: str, attr_name: str, reason: str):
        message = (
            f"Cannot serialize Node '{node_name}': "
            f"attribute '{attr_name}' is not serializable.\n"
            f"Reason: {reason}\n"
            f"Suggestion: Call node.cleanup() before serialization or "
            f"implement __getstate__/__setstate__ to handle this attribute."
        )
        super().__init__(message)
```

### Example Error Messages

```
✗ SerializationError: Cannot serialize Node 'ml_model': attribute '_torch_model' is not serializable.
  Reason: PyTorch models require special handling.
  Suggestion: Call node.cleanup() before serialization or implement __getstate__/__setstate__ to handle this attribute.

✗ TypeError: Expected Pipeline, list of Nodes, dict, or str, got int

✗ ValueError: Node 'custom_node' missing required process() method

✗ RuntimeError: Rust pipeline execution failed: Node 'resample' not found in registry
```

---

## Performance Contract

Per SC-003 and SC-005:

- ✅ **No Additional Overhead**: Pipeline execution with instances completes in same or better time as manifest-based
- ✅ **Serialization Budget**: cloudpickle serialization must complete within 5ms for nodes <10MB
- ✅ **Error Response Time**: Serialization failures detected and reported within 2 seconds

---

## Backward Compatibility Contract

Per FR-012 and SC-004:

- ✅ **100% Manifest Compatibility**: All existing manifest-based code works without modification
- ✅ **Optional Feature**: Instance execution is opt-in; manifest execution remains default
- ✅ **No Breaking Changes**: API additions only, no removals or signature changes
- ✅ **Version Agnostic**: Works with existing Rust runtime, no version coupling

---

## Type Hints

Full type signature for IDE support:

```python
from typing import Union, List, Dict, Any, Optional, overload
from remotemedia.core.pipeline import Pipeline
from remotemedia.core.node import Node

# Overloads for execute_pipeline
@overload
async def execute_pipeline(
    pipeline: Pipeline,
    enable_metrics: bool = False
) -> Any: ...

@overload
async def execute_pipeline(
    nodes: List[Node],
    enable_metrics: bool = False
) -> Any: ...

@overload
async def execute_pipeline(
    manifest: Union[str, Dict[str, Any]],
    enable_metrics: bool = False
) -> Any: ...

# Implementation
async def execute_pipeline(
    pipeline_or_manifest: Union[Pipeline, List[Node], str, Dict[str, Any]],
    enable_metrics: bool = False
) -> Any:
    """Execute a pipeline using the Rust runtime."""
    ...
```
