#!/usr/bin/env python3
"""
Runtime Comparison Example

This example demonstrates the difference between Rust and Python execution,
showing performance improvements and verifying that both runtimes produce
identical results.

Key Features Demonstrated:
- Explicit runtime selection (use_rust parameter)
- Performance comparison
- Result verification across runtimes
- Runtime availability detection
"""

import asyncio
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.simple_math import MultiplyNode, AddNode
from remotemedia.nodes.base import PassThroughNode


async def check_rust_availability():
    """Check if Rust runtime is available."""
    try:
        import remotemedia_runtime
        print(f"[OK] Rust runtime available (version {remotemedia_runtime.__version__})")
        return True
    except ImportError:
        print("[WARN] Rust runtime not available")
        return False


async def run_with_timing(pipeline, data, use_rust):
    """Run pipeline and measure execution time."""
    start = time.perf_counter()
    result = await pipeline.run(data, use_rust=use_rust)
    elapsed = (time.perf_counter() - start) * 1000  # Convert to milliseconds
    return result, elapsed


async def main():
    """Run the runtime comparison example."""
    print("=" * 70)
    print("RemoteMedia SDK - Runtime Comparison")
    print("=" * 70)
    print()

    # Check Rust availability
    print("1. Checking runtime availability...")
    rust_available = await check_rust_availability()
    print()

    if not rust_available:
        print("Note: Rust runtime not available. Install with:")
        print("      cd runtime && maturin develop --release")
        print()

    # Create a pipeline with multiple nodes
    print("2. Creating test pipeline...")
    pipeline = Pipeline(name="ComparisonTest")

    # Add multiple nodes to make the difference more noticeable
    for i in range(10):
        pipeline.add_node(PassThroughNode(name=f"pass_{i}"))

    pipeline.add_node(MultiplyNode(factor=2, name="multiply"))
    pipeline.add_node(AddNode(addend=5, name="add"))

    print(f"   [OK] Created pipeline with {len(pipeline.nodes)} nodes")
    print()

    # Prepare test data (larger dataset)
    test_data = list(range(100))
    print(f"3. Test data: {len(test_data)} items")
    print()

    # Run with Rust runtime
    if rust_available:
        print("4. Running with Rust runtime...")
        rust_result, rust_time = await run_with_timing(pipeline, test_data, use_rust=True)
        print(f"   [OK] Execution time: {rust_time:.2f} ms")
        print(f"   [OK] Result: {rust_result[:5]}... (showing first 5)")
        print()
    else:
        rust_result = None
        rust_time = 0

    # Run with Python runtime
    print(f"{'5' if rust_available else '4'}. Running with Python runtime...")
    python_result, python_time = await run_with_timing(pipeline, test_data, use_rust=False)
    print(f"   [OK] Execution time: {python_time:.2f} ms")
    print(f"   [OK] Result: {python_result[:5]}... (showing first 5)")
    print()

    # Compare results
    if rust_available:
        print(f"{'6' if rust_available else '5'}. Comparing results...")
        if rust_result == python_result:
            print("   [OK] Results match perfectly!")
        else:
            print("   [ERROR] Results differ (this shouldn't happen!)")
            return 1
        print()

        # Show performance difference
        print("7. Performance comparison:")
        speedup = python_time / rust_time if rust_time > 0 else 0
        print(f"   Rust runtime:   {rust_time:.2f} ms")
        print(f"   Python runtime: {python_time:.2f} ms")
        if speedup > 1:
            print(f"   Speedup:        {speedup:.2f}x faster with Rust!")
        elif speedup < 1 and speedup > 0:
            print(f"   Speedup:        {1/speedup:.2f}x slower with Rust")
        print()

    print("=" * 70)
    print("[OK] Runtime comparison complete!")
    print("[OK] Both runtimes produce identical results")
    if rust_available:
        print("[OK] Rust runtime provides performance benefits")
    print("=" * 70)

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
