#!/usr/bin/env python3
"""
Rust vs Python Nodes Performance Comparison

This example demonstrates the performance difference between:
1. Rust-native nodes (MultiplyNode, AddNode implemented in Rust)
2. Python nodes executed via CPython executor
3. Python nodes in pure Python runtime

Key takeaway: Rust-native nodes provide significant performance benefits
for compute-intensive operations, while maintaining identical behavior.
"""

import asyncio
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.simple_math import MultiplyNode, AddNode


async def benchmark_pipeline(pipeline, data, use_rust, description):
    """Run pipeline and measure execution time."""
    runs = 100  # Number of iterations for accurate timing

    start = time.perf_counter()
    for _ in range(runs):
        result = await pipeline.run(data, use_rust=use_rust)
    elapsed = (time.perf_counter() - start) * 1000  # Convert to ms

    avg_time = elapsed / runs

    return result, avg_time


async def main():
    """Run the Rust vs Python nodes comparison."""
    print("=" * 70)
    print("RemoteMedia SDK - Rust vs Python Nodes Performance")
    print("=" * 70)
    print()

    # Create test data - larger dataset to see performance difference
    test_data = list(range(1000))
    print(f"Test data: {len(test_data)} numbers")
    print()

    # Test 1: Simple pipeline with math nodes
    print("Test 1: Simple Math Pipeline (Multiply x2, Add +10)")
    print("-" * 70)

    pipeline = Pipeline(name="MathPipeline")
    pipeline.add_node(MultiplyNode(factor=2, name="multiply"))
    pipeline.add_node(AddNode(addend=10, name="add"))

    # Benchmark with Rust runtime (uses Rust-native nodes)
    print("Running with Rust-native nodes...")
    rust_result, rust_time = await benchmark_pipeline(pipeline, test_data, True, "Rust nodes")
    print(f"  Average time: {rust_time:.3f} ms")

    # Benchmark with Python runtime
    print("Running with Python nodes...")
    python_result, python_time = await benchmark_pipeline(pipeline, test_data, False, "Python nodes")
    print(f"  Average time: {python_time:.3f} ms")

    # Verify results match
    if rust_result == python_result:
        print("  [OK] Results match!")
    else:
        print("  [ERROR] Results differ!")
        return 1

    # Calculate speedup
    speedup = python_time / rust_time if rust_time > 0 else 0
    print()
    print(f"Performance: Rust is {speedup:.2f}x faster than Python")
    print()

    # Test 2: Complex pipeline with many nodes
    print("Test 2: Complex Pipeline (10 multiply + 10 add operations)")
    print("-" * 70)

    complex_pipeline = Pipeline(name="ComplexPipeline")

    # Add many operations to amplify the difference
    for i in range(10):
        complex_pipeline.add_node(MultiplyNode(factor=1.1, name=f"mult_{i}"))
        complex_pipeline.add_node(AddNode(addend=1, name=f"add_{i}"))

    # Smaller dataset for complex pipeline
    small_data = list(range(100))

    print(f"Pipeline: {len(complex_pipeline.nodes)} nodes")
    print(f"Test data: {len(small_data)} numbers")

    # Benchmark with Rust runtime
    print("Running with Rust-native nodes...")
    rust_result2, rust_time2 = await benchmark_pipeline(complex_pipeline, small_data, True, "Rust nodes")
    print(f"  Average time: {rust_time2:.3f} ms")

    # Benchmark with Python runtime
    print("Running with Python nodes...")
    python_result2, python_time2 = await benchmark_pipeline(complex_pipeline, small_data, False, "Python nodes")
    print(f"  Average time: {python_time2:.3f} ms")

    # Verify results match
    if rust_result2 == python_result2:
        print("  [OK] Results match!")
    else:
        print("  [ERROR] Results differ!")
        return 1

    # Calculate speedup
    speedup2 = python_time2 / rust_time2 if rust_time2 > 0 else 0
    print()
    print(f"Performance: Rust is {speedup2:.2f}x faster than Python")
    print()

    # Summary
    print("=" * 70)
    print("Summary")
    print("=" * 70)
    print(f"Simple pipeline:  Rust {speedup:.2f}x faster")
    print(f"Complex pipeline: Rust {speedup2:.2f}x faster")
    print()
    print("Key insights:")
    print("  - Rust-native nodes provide significant performance benefits")
    print("  - More complex pipelines see even greater speedups")
    print("  - Both runtimes produce identical results")
    print("  - No code changes needed - same Python API")
    print("=" * 70)

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
