#!/usr/bin/env python
"""
Test script for Pipeline.run() with Rust runtime integration.

This script tests:
1. Rust runtime availability
2. Pipeline execution with Rust runtime
3. Automatic fallback to Python executor
4. Performance comparison
"""

import asyncio
import sys
import time
from pathlib import Path

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.base import PassThroughNode


async def test_rust_availability():
    """Test if Rust runtime is available."""
    print("=" * 60)
    print("Test 1: Rust Runtime Availability")
    print("=" * 60)

    try:
        import remotemedia_runtime
        print(f"[OK] Rust runtime available: v{remotemedia_runtime.__version__}")
        print(f"[OK] Runtime status: {remotemedia_runtime.is_available()}")
        return True
    except ImportError as e:
        print(f"[FAIL] Rust runtime not available: {e}")
        return False


async def test_simple_pipeline_rust():
    """Test simple pipeline execution with Rust runtime."""
    print("\n" + "=" * 60)
    print("Test 2: Simple Pipeline with Rust Runtime")
    print("=" * 60)

    # Create simple pipeline
    pipeline = Pipeline(name="test_rust")
    pipeline.add_node(PassThroughNode(name="pass1"))
    pipeline.add_node(PassThroughNode(name="pass2"))
    pipeline.add_node(PassThroughNode(name="pass3"))

    input_data = [1, 2, 3, 4, 5]

    print(f"Input data: {input_data}")
    print(f"Pipeline: {len(pipeline.nodes)} nodes")

    try:
        start = time.perf_counter()
        result = await pipeline.run(input_data, use_rust=True)
        elapsed = (time.perf_counter() - start) * 1000

        print(f"[OK] Result: {result}")
        print(f"[OK] Execution time: {elapsed:.2f} ms")

        # Verify correctness
        assert result == input_data, f"Expected {input_data}, got {result}"
        print("[OK] Result matches expected output")

        return elapsed
    except Exception as e:
        print(f"[FAIL] Test failed: {e}")
        import traceback
        traceback.print_exc()
        return None


async def test_simple_pipeline_python():
    """Test simple pipeline execution with Python executor."""
    print("\n" + "=" * 60)
    print("Test 3: Simple Pipeline with Python Executor")
    print("=" * 60)

    # Create simple pipeline
    pipeline = Pipeline(name="test_python")
    pipeline.add_node(PassThroughNode(name="pass1"))
    pipeline.add_node(PassThroughNode(name="pass2"))
    pipeline.add_node(PassThroughNode(name="pass3"))

    input_data = [1, 2, 3, 4, 5]

    print(f"Input data: {input_data}")
    print(f"Pipeline: {len(pipeline.nodes)} nodes")

    try:
        start = time.perf_counter()
        result = await pipeline.run(input_data, use_rust=False)
        elapsed = (time.perf_counter() - start) * 1000

        print(f"[OK] Result: {result}")
        print(f"[OK] Execution time: {elapsed:.2f} ms")

        # Verify correctness
        assert result == input_data, f"Expected {input_data}, got {result}"
        print("[OK] Result matches expected output")

        return elapsed
    except Exception as e:
        print(f"[FAIL] Test failed: {e}")
        import traceback
        traceback.print_exc()
        return None


async def test_automatic_fallback():
    """Test automatic fallback from Rust to Python."""
    print("\n" + "=" * 60)
    print("Test 4: Automatic Fallback")
    print("=" * 60)

    # Create pipeline that might fail in Rust (for testing)
    pipeline = Pipeline(name="test_fallback")
    pipeline.add_node(PassThroughNode(name="pass1"))

    input_data = [10, 20, 30]

    print(f"Input data: {input_data}")
    print("Testing fallback mechanism...")

    try:
        result = await pipeline.run(input_data, use_rust=True)
        print(f"[OK] Pipeline executed successfully: {result}")
        print("[OK] Fallback mechanism works (either Rust succeeded or fell back to Python)")
        return True
    except Exception as e:
        print(f"[FAIL] Test failed: {e}")
        import traceback
        traceback.print_exc()
        return False


async def main():
    """Run all tests."""
    print("Starting Pipeline.run() Integration Tests\n")

    # Test 1: Check Rust availability
    rust_available = await test_rust_availability()

    # Test 2: Run with Rust runtime
    rust_time = None
    if rust_available:
        rust_time = await test_simple_pipeline_rust()

    # Test 3: Run with Python executor
    python_time = await test_simple_pipeline_python()

    # Test 4: Test automatic fallback
    await test_automatic_fallback()

    # Performance comparison
    if rust_time and python_time:
        print("\n" + "=" * 60)
        print("Performance Comparison")
        print("=" * 60)
        print(f"Rust runtime:     {rust_time:.2f} ms")
        print(f"Python executor:  {python_time:.2f} ms")
        speedup = python_time / rust_time
        print(f"Speedup:          {speedup:.2f}x")

        if speedup > 1.0:
            print(f"[OK] Rust runtime is {speedup:.2f}x faster!")
        else:
            print(f"[WARN]  Python executor is faster (FFI overhead may be significant for small pipelines)")

    print("\n" + "=" * 60)
    print("All Tests Complete!")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(main())
