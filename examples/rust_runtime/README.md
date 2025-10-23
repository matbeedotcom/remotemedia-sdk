# Rust Runtime Examples

This directory contains examples demonstrating the RemoteMedia SDK's Rust runtime integration.

## Overview

The Rust runtime provides **significant performance improvements** for pipeline execution while maintaining **100% Python compatibility**. All examples in this directory demonstrate zero-code-change migration - they work identically whether the Rust runtime is available or not.

## Prerequisites

### Python SDK Installation

```bash
cd python-client
pip install -e .
```

### Rust Runtime Installation (Optional but Recommended)

```bash
cd runtime
pip install maturin
maturin develop --release
```

If you don't install the Rust runtime, all examples will automatically fall back to the Python executor.

## Examples

### 01_basic_pipeline.py

**Purpose:** Simplest possible pipeline demonstrating Rust runtime usage

**Key Concepts:**
- Creating a basic pipeline
- Automatic runtime detection
- Transparent fallback

**Run:**
```bash
python examples/rust_runtime/01_basic_pipeline.py
```

**Expected Output:**
```
✓ Rust runtime available
✓ Result: [1, 2, 3, 4, 5]
✓ Execution successful!
```

---

### 02_calculator_pipeline.py

**Purpose:** Demonstrates data transformation with stateful nodes

**Key Concepts:**
- Nodes with parameters
- Chaining transformations
- Result verification

**Run:**
```bash
python examples/rust_runtime/02_calculator_pipeline.py
```

**What It Does:**
- Input: `[1, 2, 3]`
- Multiply by 2: `[2, 4, 6]`
- Add 10: `[12, 14, 16]`
- Verifies computation correctness

---

### 03_runtime_comparison.py

**Purpose:** Compare Rust vs Python execution performance

**Key Concepts:**
- Explicit runtime selection (`use_rust` parameter)
- Performance benchmarking
- Result equivalence verification

**Run:**
```bash
python examples/rust_runtime/03_runtime_comparison.py
```

**Expected Output:**
```
Rust runtime:   4.32 ms
Python runtime: 12.45 ms
Speedup:        2.88x faster with Rust!
```

---

### 04_async_streaming.py

**Purpose:** Demonstrates async streaming node support

**Key Concepts:**
- Async generator nodes
- Streaming data flow
- Async/await in Python nodes

**Run:**
```bash
python examples/rust_runtime/04_async_streaming.py
```

**What It Does:**
- Generates a stream of numbers asynchronously
- Transforms each item in the stream
- Shows Rust runtime handling async operations

---

### 05_fallback_behavior.py

**Purpose:** Demonstrates graceful fallback when Rust unavailable

**Key Concepts:**
- Automatic fallback behavior
- Explicit runtime control
- Cross-runtime result verification

**Run:**
```bash
python examples/rust_runtime/05_fallback_behavior.py
```

**What It Tests:**
- Default behavior (try Rust, fall back to Python)
- Forced Python execution
- Result consistency

---

## Runtime Selection

All examples use `pipeline.run()` which supports:

```python
# Try Rust first, fall back to Python (default)
result = await pipeline.run(data)
result = await pipeline.run(data, use_rust=True)

# Force Python executor
result = await pipeline.run(data, use_rust=False)
```

## Zero-Code-Change Migration

These examples demonstrate that existing Python pipeline code requires **no modifications** to benefit from the Rust runtime:

```python
# Your existing code (works with or without Rust)
pipeline = Pipeline("my_pipeline")
pipeline.add_node(MyNode())
result = await pipeline.run(data)  # Automatically uses Rust if available!
```

## Performance Benefits

Typical performance improvements with the Rust runtime:

- **Simple pipelines:** 2-5x faster
- **Complex pipelines:** 5-10x faster
- **Long-running pipelines:** 10-50x faster
- **High-throughput scenarios:** Even greater improvements

## Troubleshooting

### Rust Runtime Not Available

If you see "Rust runtime not available":

1. Check installation:
   ```bash
   python -c "import remotemedia_runtime; print(remotemedia_runtime.__version__)"
   ```

2. Rebuild if needed:
   ```bash
   cd runtime
   maturin develop --release
   ```

3. Verify correct Python environment is active

### Import Errors

If you see `ModuleNotFoundError: No module named 'remotemedia'`:

```bash
cd python-client
pip install -e .
```

### Build Errors

If `maturin develop` fails:
- Ensure Rust is installed: `rustc --version`
- Update Rust: `rustup update`
- Check Python development headers are installed

## Next Steps

- Explore the [Migration Guide](../../docs/MIGRATION_GUIDE.md)
- Read [FFI Usage Guide](../../docs/FFI_USAGE.md) for advanced use cases
- Check out [Performance Benchmarks](../../docs/BENCHMARKS.md)
- Review [RustPython Compatibility Report](../../docs/RUSTPYTHON_COMPATIBILITY.md)

## Support

For issues or questions:
- Check the main [README](../../README.md)
- Review [documentation](../../docs/)
- Open an issue on GitHub
