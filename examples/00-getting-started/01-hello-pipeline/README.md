# Hello Pipeline

**Your first RemoteMedia SDK pipeline in 5 minutes!**

## What You'll Learn

- How to create a pipeline from a YAML manifest
- Basic pipeline structure (nodes, connections, outputs)
- Running a simple text processing pipeline
- Understanding pipeline execution flow

## Prerequisites

**SDK Version**: v0.4.0 or later

**System Requirements**:
- OS: Windows/Linux/macOS
- RAM: 1GB minimum
- Python: 3.9+

**Required Dependencies**:
```bash
pip install remotemedia>=0.4.0
```

**Optional Dependencies**: None - this is a minimal example!

**External Services**: None required

## Quick Start

### 1. Install Dependencies

```bash
pip install -r requirements.txt
```

### 2. Run the Example

```bash
python main.py
```

### 3. Expected Output

```
============================================================
🎯 Hello Pipeline - Your First RemoteMedia Example
============================================================

Step 1: Loading pipeline from pipeline.yaml...
✅ Pipeline loaded successfully!

Step 2: Preparing input data...
   Input: 'Hello, RemoteMedia SDK!'

Step 3: Running the pipeline...
✅ Pipeline execution complete!

Step 4: Results:
------------------------------------------------------------
   Output: 'PROCESSED: HELLO, REMOTEMEDIA SDK!'
------------------------------------------------------------

============================================================
🎉 Congratulations! You've run your first RemoteMedia pipeline!
============================================================
```

## Detailed Usage

### Basic Usage

The example demonstrates the minimal code needed to run a pipeline:

```python
from remotemedia.core.pipeline import Pipeline

# Load pipeline from YAML
pipeline = await Pipeline.from_yaml_file("pipeline.yaml")

# Run with input data
result = await pipeline.run({
    "text": "Hello, RemoteMedia SDK!"
})

# Access the output
print(result['processed_text'])
```

### Advanced Options

**Option 1: Modify Input Text**

Edit `main.py` and change the input:
```python
input_text = "Your custom message here!"
```

**Option 2: Change Processing Behavior**

Edit `pipeline.yaml` to modify the node parameters:
```yaml
params:
  operation: lowercase  # Change from uppercase
  prefix: ">>> "        # Change the prefix
```

**Option 3: Add Pipeline Metrics**

Enable performance tracking:
```python
pipeline = await Pipeline.from_yaml_file(
    "pipeline.yaml",
    enable_metrics=True
)
result = await pipeline.run({"text": input_text})

# Get metrics
metrics = pipeline.get_metrics()
print(f"Processing time: {metrics['duration_ms']}ms")
```

## Understanding the Pipeline

### Pipeline Structure

```yaml
# pipeline.yaml
version: v1

nodes:
  - id: text_processor        # Unique identifier for this node
    node_type: TextProcessorNode  # Type of node to create
    params:                     # Configuration for the node
      operation: uppercase
      prefix: "PROCESSED: "
    executor: python            # Run in Python runtime

connections: []               # No connections (single node)

outputs:
  - node_id: text_processor   # Which node's output to return
    output_key: processed_text
```

**Key Components**:
- **version**: Pipeline manifest format version
- **nodes**: List of processing units (just one in this example)
- **connections**: How data flows between nodes (empty for single node)
- **outputs**: What the pipeline returns

### How It Works

1. **Loading**: Pipeline reads `pipeline.yaml` and creates a `TextProcessorNode`
2. **Initialization**: Node prepares for processing
3. **Execution**: Input text flows through the node
4. **Processing**: Node converts to uppercase and adds prefix
5. **Output**: Processed text is returned in the result dictionary

**Data Flow**:
```
Input: "Hello, RemoteMedia SDK!"
   ↓
TextProcessorNode
   ├─ Convert to uppercase: "HELLO, REMOTEMEDIA SDK!"
   └─ Add prefix: "PROCESSED: "
   ↓
Output: "PROCESSED: HELLO, REMOTEMEDIA SDK!"
```

## Expected Output

### Console Output

When you run `python main.py`, you should see:

```
============================================================
🎯 Hello Pipeline - Your First RemoteMedia Example
============================================================

Step 1: Loading pipeline from pipeline.yaml...
✅ Pipeline loaded successfully!

Step 2: Preparing input data...
   Input: 'Hello, RemoteMedia SDK!'

Step 3: Running the pipeline...
✅ Pipeline execution complete!

Step 4: Results:
------------------------------------------------------------
   Output: 'PROCESSED: HELLO, REMOTEMEDIA SDK!'
------------------------------------------------------------

============================================================
🎉 Congratulations! You've run your first RemoteMedia pipeline!
============================================================

What you just did:
  ✅ Loaded a pipeline from YAML configuration
  ✅ Sent data through a processing node
  ✅ Received and displayed the output

Next steps:
  📚 Try ../02-basic-audio/ for audio processing
  🔬 Experiment by changing the input text above
  📖 Read the README.md to understand how it works
```

### Output Files

No files are created - this example only processes data in memory.

### Performance Benchmarks

**Expected Performance** (on typical hardware):
- Pipeline load time: <100ms
- Processing time: <10ms for short text
- Memory usage: ~50MB

This is a Python-only example. For faster processing, see advanced examples with Rust acceleration.

## Related Examples

### Prerequisites

This is the starting point - no prerequisites needed!

### Next Steps

After completing this example, try:
- [Basic Audio Processing](../02-basic-audio/) - Process audio files
- [Python-Rust Interop](../03-python-rust-interop/) - See the performance difference

### Related Features

- [Advanced Text Processing](../../01-advanced/text-pipelines/) - More complex text operations
- [Multiprocess Nodes](../../01-advanced/multiprocess-nodes/) - Process isolation

## Troubleshooting

### Common Issues

#### Issue: "Module not found: remotemedia"

**Solution**: Install the SDK:
```bash
pip install remotemedia>=0.4.0
```

#### Issue: "FileNotFoundError: pipeline.yaml"

**Solution**: Make sure you're running from the example directory:
```bash
cd examples/00-getting-started/01-hello-pipeline/
python main.py
```

#### Issue: "TextProcessorNode not found"

**Solution**: This node should be built into the SDK. If it's missing, you may need to update:
```bash
pip install --upgrade remotemedia
```

If the issue persists, the node may need to be created. See [CONTRIBUTING.md](../../../CONTRIBUTING.md) for creating custom nodes.

### Performance Issues

This example should run instantly (<100ms total). If it's slow:
1. Check Python version: `python --version` (should be 3.9+)
2. Check SDK installation: `pip show remotemedia`
3. Try running with verbose logging to see what's happening

### Getting Help

- [Documentation](https://docs.remotemedia.dev)
- [GitHub Issues](https://github.com/org/remotemedia-sdk/issues)
- [Example Discussion](https://github.com/org/remotemedia-sdk/discussions)

## Additional Resources

### Documentation
- [Pipeline Manifest Format](../../../docs/pipeline-manifests.md)
- [Node Types Reference](../../../docs/node-types.md)
- [API Reference](https://docs.remotemedia.dev/api)

### Source Code
- [Pipeline definition](pipeline.yaml) - YAML configuration
- [Main script](main.py) - Python execution code
- [Example README template](../../../specs/001-repo-cleanup/contracts/example-readme-template.md)

### Concepts to Understand
- **Pipeline**: A sequence of processing nodes connected together
- **Node**: A single processing unit (like a function)
- **Manifest**: YAML configuration describing the pipeline
- **Executor**: The runtime that runs the node (python, rust, or multiprocess)

---

**Last Tested**: 2025-11-07 with SDK v0.4.0
**Platform**: All (Windows/Linux/macOS)
**Complexity**: Getting Started (⭐)
**Category**: Text Processing
**Estimated Time**: 5 minutes
