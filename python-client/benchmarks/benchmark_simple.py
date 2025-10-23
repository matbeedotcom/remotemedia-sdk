"""
Simple benchmark comparing Python node execution with Rust runtime.
Direct 1:1 comparison without pipeline overhead.

Updated: 2025-10-23
Now includes:
- CPython baseline (direct node execution)
- Rust runtime (via Pipeline.run())
- Three-way comparison table
"""

import time
import statistics
import asyncio
import sys
from pathlib import Path

# Add parent to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.base import PassThroughNode
from remotemedia.nodes.calculator import CalculatorNode


def benchmark_simple(iterations=100, items=100):
    """Benchmark 3-node chain: PassThrough -> Calculator -> PassThrough"""
    print("\n" + "="*60)
    print(f"SIMPLE BENCHMARK ({iterations} iterations, {items} items)")
    print("="*60)

    # Create nodes
    pass1 = PassThroughNode(name="pass_0")
    calc = CalculatorNode(name="calc", operation="add", operand=1)
    pass2 = PassThroughNode(name="pass_2")

    times = []

    # Warmup
    print("Warming up...", end=" ", flush=True)
    for i in range(items):
        r = pass1.process(i)
        r = calc.process(r)
        r = pass2.process(r)
    print("[OK]")

    # Benchmark
    print(f"Running {iterations} iterations...", end=" ", flush=True)
    for _ in range(iterations):
        start = time.perf_counter()
        for i in range(items):
            r = pass1.process(i)
            r = calc.process(r)
            r = pass2.process(r)
        end = time.perf_counter()
        times.append(end - start)
    print("[DONE]")

    # Stats
    mean = statistics.mean(times)
    median = statistics.median(times)
    stddev = statistics.stdev(times) if len(times) > 1 else 0

    print(f"\nResults:")
    print(f"  Mean:   {mean*1e6:.2f} µs  ({mean*1000:.2f} ms)")
    print(f"  Median: {median*1e6:.2f} µs")
    print(f"  Stddev: {stddev*1e6:.2f} µs")
    print(f"  Min:    {min(times)*1e6:.2f} µs")
    print(f"  Max:    {max(times)*1e6:.2f} µs")

    return mean


def benchmark_complex(iterations=100, items=100):
    """Benchmark 10-node chain alternating PassThrough and Calculator"""
    print("\n" + "="*60)
    print(f"COMPLEX BENCHMARK ({iterations} iterations, {items} items)")
    print("="*60)

    # Create 10 nodes
    nodes = []
    for i in range(10):
        if i % 2 == 0:
            nodes.append(PassThroughNode(name=f"pass_{i}"))
        else:
            nodes.append(CalculatorNode(name=f"calc_{i}", operation="add", operand=1))

    times = []

    # Warmup
    print("Warming up...", end=" ", flush=True)
    for i in range(items):
        r = i
        for node in nodes:
            r = node.process(r)
    print("[OK]")

    # Benchmark
    print(f"Running {iterations} iterations...", end=" ", flush=True)
    for _ in range(iterations):
        start = time.perf_counter()
        for i in range(items):
            r = i
            for node in nodes:
                r = node.process(r)
        end = time.perf_counter()
        times.append(end - start)
    print("[DONE]")

    # Stats
    mean = statistics.mean(times)
    median = statistics.median(times)
    stddev = statistics.stdev(times) if len(times) > 1 else 0

    print(f"\nResults:")
    print(f"  Mean:   {mean*1e6:.2f} µs  ({mean*1000:.2f} ms)")
    print(f"  Median: {median*1e6:.2f} µs")
    print(f"  Stddev: {stddev*1e6:.2f} µs")
    print(f"  Min:    {min(times)*1e6:.2f} µs")
    print(f"  Max:    {max(times)*1e6:.2f} µs")

    return mean


def benchmark_scaling():
    """Test with 10, 100, 1000 items"""
    print("\n" + "="*60)
    print("SCALING BENCHMARK")
    print("="*60)

    pass1 = PassThroughNode(name="pass_0")
    calc = CalculatorNode(name="calc", operation="add", operand=1)
    pass2 = PassThroughNode(name="pass_2")

    for size in [10, 100, 1000]:
        times = []
        for _ in range(100):
            start = time.perf_counter()
            for i in range(size):
                r = pass1.process(i)
                r = calc.process(r)
                r = pass2.process(r)
            end = time.perf_counter()
            times.append(end - start)

        mean = statistics.mean(times)
        print(f"\n  {size:4d} items: {mean*1e6:8.2f} µs")


async def benchmark_pipeline_rust(iterations=100, items=100):
    """Benchmark 3-node pipeline using Rust runtime via Pipeline.run()"""
    print("\n" + "="*60)
    print(f"RUST RUNTIME BENCHMARK ({iterations} iterations, {items} items)")
    print("="*60)

    # Create pipeline
    pipeline = Pipeline(name="simple-rust-benchmark")
    pipeline.add_node(PassThroughNode(name="pass_0"))
    pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=1))
    pipeline.add_node(PassThroughNode(name="pass_2"))

    input_data = list(range(items))
    times = []

    # Warmup
    print("Warming up...", end=" ", flush=True)
    try:
        await pipeline.run(input_data, use_rust=True)
        print("[OK]")
    except Exception as e:
        print(f"[FAIL: {e}]")
        print("Rust runtime not available or failed")
        return None

    # Benchmark
    print(f"Running {iterations} iterations...", end=" ", flush=True)
    for _ in range(iterations):
        start = time.perf_counter()
        await pipeline.run(input_data, use_rust=True)
        end = time.perf_counter()
        times.append(end - start)
    print("[DONE]")

    # Stats
    mean = statistics.mean(times)
    median = statistics.median(times)
    stddev = statistics.stdev(times) if len(times) > 1 else 0

    print(f"\nResults:")
    print(f"  Mean:   {mean*1e6:.2f} µs  ({mean*1000:.2f} ms)")
    print(f"  Median: {median*1e6:.2f} µs")
    print(f"  Stddev: {stddev*1e6:.2f} µs")
    print(f"  Min:    {min(times)*1e6:.2f} µs")
    print(f"  Max:    {max(times)*1e6:.2f} µs")

    return mean


async def benchmark_pipeline_python(iterations=100, items=100):
    """Benchmark 3-node pipeline using Python executor via Pipeline.run()"""
    print("\n" + "="*60)
    print(f"PYTHON PIPELINE BENCHMARK ({iterations} iterations, {items} items)")
    print("="*60)

    # Create pipeline
    pipeline = Pipeline(name="simple-python-benchmark")
    pipeline.add_node(PassThroughNode(name="pass_0"))
    pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=1))
    pipeline.add_node(PassThroughNode(name="pass_2"))

    input_data = list(range(items))
    times = []

    # Warmup
    print("Warming up...", end=" ", flush=True)
    await pipeline.run(input_data, use_rust=False)
    print("[OK]")

    # Benchmark
    print(f"Running {iterations} iterations...", end=" ", flush=True)
    for _ in range(iterations):
        start = time.perf_counter()
        await pipeline.run(input_data, use_rust=False)
        end = time.perf_counter()
        times.append(end - start)
    print("[DONE]")

    # Stats
    mean = statistics.mean(times)
    median = statistics.median(times)
    stddev = statistics.stdev(times) if len(times) > 1 else 0

    print(f"\nResults:")
    print(f"  Mean:   {mean*1e6:.2f} µs  ({mean*1000:.2f} ms)")
    print(f"  Median: {median*1e6:.2f} µs")
    print(f"  Stddev: {stddev*1e6:.2f} µs")
    print(f"  Min:    {min(times)*1e6:.2f} µs")
    print(f"  Max:    {max(times)*1e6:.2f} µs")

    return mean


async def main():
    print("="*60)
    print("THREE-WAY RUNTIME COMPARISON")
    print("Direct node execution vs Pipeline.run() execution")
    print("="*60)

    # Benchmark 1: CPython direct execution
    simple_direct = benchmark_simple(100, 100)
    complex_direct = benchmark_complex(100, 100)

    # Benchmark 2: Pipeline with Python executor
    simple_pipeline_py = await benchmark_pipeline_python(100, 100)

    # Benchmark 3: Pipeline with Rust runtime
    simple_pipeline_rust = await benchmark_pipeline_rust(100, 100)

    # Scaling test
    benchmark_scaling()

    # Final comparison
    print("\n" + "="*60)
    print("THREE-WAY COMPARISON SUMMARY")
    print("="*60)

    print(f"\n{'Runtime':<25} {'Simple (3n, 100i)':<20} {'Complex (10n, 100i)':<20}")
    print("-" * 70)

    # CPython direct
    print(f"{'CPython (direct)':<25} {simple_direct*1e6:8.2f} µs          {complex_direct*1e6:8.2f} µs")

    # Pipeline Python
    if simple_pipeline_py:
        print(f"{'Python (Pipeline.run)':<25} {simple_pipeline_py*1e6:8.2f} µs          N/A")

    # Pipeline Rust
    if simple_pipeline_rust:
        speedup = simple_direct / simple_pipeline_rust if simple_pipeline_rust else 0
        print(f"{'Rust (Pipeline.run)':<25} {simple_pipeline_rust*1e6:8.2f} µs ({speedup:.2f}x)  N/A")
    else:
        print(f"{'Rust (Pipeline.run)':<25} NOT AVAILABLE      N/A")

    # Rust native (from cargo bench)
    rust_native_simple = 38.04
    rust_native_complex = 313.48
    speedup_simple = simple_direct*1e6 / rust_native_simple if rust_native_simple else 0
    speedup_complex = complex_direct*1e6 / rust_native_complex if rust_native_complex else 0

    print(f"{'Rust (native/cargo)':<25} {rust_native_simple:8.2f} µs ({speedup_simple:.2f}x)  {rust_native_complex:8.2f} µs ({speedup_complex:.2f}x)")

    print("\n" + "="*60)
    print("NOTES")
    print("="*60)
    print("• CPython (direct) = Baseline (direct node.process() calls)")
    print("• Python (Pipeline.run) = Pipeline.run(use_rust=False)")
    print("• Rust (Pipeline.run) = Pipeline.run(use_rust=True) [FFI overhead]")
    print("• Rust (native/cargo) = Pure Rust execution [from cargo bench]")
    print()
    print("Expected:")
    print("  - Rust (native) fastest for all scenarios")
    print("  - Rust (Pipeline.run) may be slower due to FFI + fallback")
    print("  - After Phase 1.5 (RustPython): Rust (Pipeline.run) ≈ Rust (native)")
    print()


if __name__ == "__main__":
    asyncio.run(main())
