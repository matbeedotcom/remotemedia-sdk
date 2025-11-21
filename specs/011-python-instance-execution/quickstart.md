# Quickstart: Python Instance Execution

**Feature**: Python Instance Execution in FFI
**Date**: 2025-11-20
**For**: Developers using RemoteMedia SDK

## Overview

This guide shows you how to use the new Python Instance Execution feature, which allows you to pass Node instances directly to the Rust runtime instead of serializing to JSON manifests.

---

## Before You Start

### Prerequisites

- Python 3.11+ installed
- RemoteMedia SDK installed (`pip install remotemedia`)
- Rust runtime available (`remotemedia.runtime` module)

### Verify Installation

```python
import remotemedia
from remotemedia import is_rust_runtime_available

print(f"RemoteMedia version: {remotemedia.__version__}")
print(f"Rust runtime available: {is_rust_runtime_available()}")
```

Expected output:
```
RemoteMedia version: 0.x.x
Rust runtime available: True
```

---

## Basic Usage

### Option 1: Pass List of Node Instances (Simplest)

```python
import asyncio
from remotemedia.runtime import execute_pipeline
from remotemedia.nodes import PassThroughNode, CalculatorNode

async def main():
    # Create node instances with configuration
    nodes = [
        PassThroughNode(name="pass"),
        CalculatorNode(name="add10", operation="add", operand=10)
    ]

    # Execute directly - no manifest needed!
    result = await execute_pipeline(nodes)
    print(f"Result: {result}")

asyncio.run(main())
```

**Key Point**: You can pass a list of Node instances directly. No need to create a Pipeline or serialize to JSON.

---

### Option 2: Pass Pipeline Instance

```python
import asyncio
from remotemedia.runtime import execute_pipeline
from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes import PassThroughNode, CalculatorNode

async def main():
    # Create pipeline and add nodes
    pipeline = Pipeline("my-pipeline")
    pipeline.add_node(PassThroughNode(name="pass"))
    pipeline.add_node(CalculatorNode(name="add10", operation="add", operand=10))

    # Execute the pipeline instance
    result = await execute_pipeline(pipeline)
    print(f"Result: {result}")

asyncio.run(main())
```

---

### Option 3: Use Pipeline.run() Method (Automatic)

```python
import asyncio
from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes import PassThroughNode, CalculatorNode

async def main():
    # Create pipeline
    pipeline = Pipeline("my-pipeline")
    pipeline.add_node(PassThroughNode(name="pass"))
    pipeline.add_node(CalculatorNode(name="add10", operation="add", operand=10))

    # Run automatically uses Rust runtime with instances
    result = await pipeline.run(input_data=[1, 2, 3])
    print(f"Results: {result}")

asyncio.run(main())
```

**Output**: `Results: [11, 12, 13]`

---

## Processing with Input Data

### Pass Input Data to Instances

```python
import asyncio
from remotemedia.runtime import execute_pipeline_with_input
from remotemedia.nodes import CalculatorNode

async def main():
    # Create node instance
    nodes = [CalculatorNode(name="multiply", operation="multiply", operand=2)]

    # Process multiple inputs
    input_data = [1, 2, 3, 4, 5]
    results = await execute_pipeline_with_input(nodes, input_data)

    print(f"Inputs:  {input_data}")
    print(f"Results: {results}")

asyncio.run(main())
```

**Output**:
```
Inputs:  [1, 2, 3, 4, 5]
Results: [2, 4, 6, 8, 10]
```

---

## Advanced: Nodes with Complex State

### Example: ML Model Node

```python
import asyncio
from remotemedia.core.node import Node
from remotemedia.runtime import execute_pipeline

class SentimentNode(Node):
    """Node that holds a pre-loaded ML model."""

    def __init__(self, model_path: str, **kwargs):
        super().__init__(**kwargs)
        self.model_path = model_path
        self.model = None  # Loaded in initialize()

    def initialize(self):
        """Load model before processing."""
        super().initialize()
        # Simulate loading a model
        print(f"Loading model from {self.model_path}")
        self.model = {"loaded": True, "path": self.model_path}

    def process(self, text: str) -> dict:
        """Run inference with loaded model."""
        if not self.model:
            raise RuntimeError("Model not initialized")

        # Simulate inference
        sentiment = "positive" if len(text) > 10 else "negative"
        return {
            "text": text,
            "sentiment": sentiment,
            "confidence": 0.95
        }

    def cleanup(self):
        """Unload model after processing."""
        super().cleanup()
        print(f"Unloading model")
        self.model = None

async def main():
    # Create node with complex state
    node = SentimentNode(
        name="sentiment",
        model_path="/path/to/model.pkl"
    )

    # Execute - model stays loaded throughout
    result = await execute_pipeline([node])
    print(f"Result: {result}")

asyncio.run(main())
```

**Key Point**: The model is loaded once (in `initialize()`) and reused for all processing. State is preserved throughout execution.

---

## Backward Compatibility

### Old Way (Still Works)

```python
import asyncio
from remotemedia.runtime import execute_pipeline
from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes import CalculatorNode

async def main():
    # Create pipeline
    pipeline = Pipeline("calc-pipeline")
    pipeline.add_node(CalculatorNode(name="add", operation="add", operand=5))

    # Manually serialize to manifest JSON
    manifest_json = pipeline.serialize()

    # Execute with manifest (old way)
    result = await execute_pipeline(manifest_json)
    print(f"Result: {result}")

asyncio.run(main())
```

**Key Point**: All existing code using manifest JSON continues to work unchanged.

---

## Comparison: Before vs After

### Before (Manifest-Based)

```python
# Create pipeline
pipeline = Pipeline("my-pipeline")
pipeline.add_node(MyNode(param="value"))

# Serialize to JSON
manifest_json = pipeline.serialize()

# Execute with JSON
result = await execute_pipeline(manifest_json)
```

**Limitations**:
- Node configuration must be JSON-serializable
- Complex state (loaded models) lost during serialization
- Extra step to serialize

---

### After (Instance-Based)

```python
# Create node with complex state
node = MyNode(param=ComplexObject(), model=LoadedModel())

# Execute directly
result = await execute_pipeline([node])
```

**Benefits**:
- ✅ No serialization needed
- ✅ Complex state preserved
- ✅ More ergonomic API
- ✅ Simpler code

---

## Enabling Metrics

### Track Performance

```python
import asyncio
from remotemedia.runtime import execute_pipeline
from remotemedia.nodes import CalculatorNode

async def main():
    nodes = [CalculatorNode(name="calc", operation="add", operand=10)]

    # Enable metrics collection
    result = await execute_pipeline(nodes, enable_metrics=True)

    # Access outputs and metrics
    print(f"Outputs: {result['outputs']}")
    print(f"Metrics: {result['metrics']}")

asyncio.run(main())
```

**Output**:
```
Outputs: 15
Metrics: {
    "total_duration_us": 1234,
    "node_metrics": [
        {"node_id": "calc_0", "avg_duration_us": 456, ...}
    ]
}
```

---

## Error Handling

### Handling Serialization Errors (Multiprocess Nodes)

```python
import asyncio
from remotemedia.runtime import execute_pipeline
from remotemedia.core.node import Node

class BadNode(Node):
    """Node with non-serializable state."""

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        # File handle - not serializable!
        self.file_handle = open("/tmp/data.txt", "w")

    def process(self, data):
        return data

async def main():
    node = BadNode(name="bad")

    try:
        # This will fail if multiprocess execution is used
        result = await execute_pipeline([node])
    except Exception as e:
        print(f"Error: {e}")
        # Error: Cannot serialize Node 'bad': attribute 'file_handle' is not serializable.
        #        Suggestion: Call node.cleanup() before serialization...

asyncio.run(main())
```

**Solution**: Implement `cleanup()` to close file handles before serialization:

```python
class GoodNode(Node):
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.file_path = "/tmp/data.txt"
        self.file_handle = None

    def initialize(self):
        super().initialize()
        self.file_handle = open(self.file_path, "w")

    def process(self, data):
        self.file_handle.write(str(data))
        return data

    def cleanup(self):
        super().cleanup()
        if self.file_handle:
            self.file_handle.close()
            self.file_handle = None
```

---

## Common Patterns

### Pattern 1: Reusable Node Configuration

```python
# Define node configuration once
sentiment_config = {
    "model_path": "/models/sentiment.pkl",
    "threshold": 0.5,
    "language": "en"
}

# Create multiple pipelines with same config
pipeline1 = Pipeline("pipeline1")
pipeline1.add_node(SentimentNode(**sentiment_config))

pipeline2 = Pipeline("pipeline2")
pipeline2.add_node(SentimentNode(**sentiment_config))

# Execute both
result1 = await execute_pipeline(pipeline1)
result2 = await execute_pipeline(pipeline2)
```

---

### Pattern 2: Dynamic Pipeline Construction

```python
def build_pipeline(num_stages: int) -> list:
    """Build pipeline with N processing stages."""
    nodes = []
    for i in range(num_stages):
        nodes.append(CalculatorNode(
            name=f"stage_{i}",
            operation="add",
            operand=i * 10
        ))
    return nodes

# Build and execute
pipeline_nodes = build_pipeline(3)
result = await execute_pipeline(pipeline_nodes)
```

---

### Pattern 3: Conditional Node Inclusion

```python
def build_conditional_pipeline(include_filter: bool) -> list:
    """Build pipeline with optional filtering stage."""
    nodes = [PassThroughNode(name="input")]

    if include_filter:
        nodes.append(FilterNode(name="filter", threshold=0.5))

    nodes.append(OutputNode(name="output"))
    return nodes

# Execute with or without filter
result = await execute_pipeline(build_conditional_pipeline(include_filter=True))
```

---

## Next Steps

1. **Read the specification**: [spec.md](spec.md) for complete feature details
2. **Review API contracts**: [contracts/python-api.md](contracts/python-api.md) for full API documentation
3. **Explore data model**: [data-model.md](data-model.md) for understanding internals
4. **Check examples**: See `python-client/tests/test_instance_pipelines.py` for more examples

---

## FAQ

**Q: Do I need to change my existing code?**
A: No. All existing manifest-based code continues to work. Instance execution is an optional feature.

**Q: Can I mix instances and manifest definitions?**
A: Not yet (P2 feature). Currently, you can pass either all instances or a manifest, but not both.

**Q: What happens to node state during execution?**
A: When you pass instances directly, their state is preserved. When using manifests, nodes are reconstructed from scratch.

**Q: Does this work with multiprocess execution?**
A: Yes, but nodes are serialized using cloudpickle. Ensure your nodes implement proper `cleanup()`/`initialize()` methods.

**Q: How do I debug instance execution?**
A: Enable logging with `logging.basicConfig(level=logging.DEBUG)` to see detailed execution flow.

**Q: Is this slower than manifest-based execution?**
A: No. Performance is the same or better (no extra serialization overhead for simple cases).

---

## Troubleshooting

### Issue: "Rust runtime not available"

**Solution**: Install remotemedia-ffi package:
```bash
cd transports/remotemedia-ffi
./dev-install.sh
```

### Issue: "Node missing process() method"

**Solution**: Ensure your custom node extends `Node` base class and implements `process()`:
```python
from remotemedia.core.node import Node

class MyNode(Node):
    def process(self, data):
        # Your logic here
        return data
```

### Issue: "SerializationError: Cannot serialize..."

**Solution**: Implement `cleanup()` to release non-serializable resources:
```python
def cleanup(self):
    super().cleanup()
    # Close files, release connections, unload models
    if hasattr(self, 'file'):
        self.file.close()
```

---

## Support

- **Issues**: Report bugs at [GitHub Issues](https://github.com/your-org/remotemedia-sdk/issues)
- **Documentation**: [CLAUDE.md](../../CLAUDE.md)
- **Examples**: `python-client/tests/` directory
