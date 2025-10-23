"""
Simple benchmark comparing Python node execution with Rust runtime.
Direct 1:1 comparison without pipeline overhead.
"""

import time
import statistics

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


if __name__ == "__main__":
    print("="*60)
    print("PYTHON BASELINE BENCHMARK")
    print("Direct node execution (1:1 comparison with Rust)")
    print("="*60)

    simple = benchmark_simple(100, 100)
    complex_time = benchmark_complex(100, 100)
    benchmark_scaling()

    print("\n" + "="*60)
    print("COMPARISON WITH RUST")
    print("="*60)
    print("\nPython:")
    print(f"  Simple  (3 nodes, 100 items): {simple*1e6:8.2f} µs")
    print(f"  Complex (10 nodes, 100 items): {complex_time*1e6:8.2f} µs")
    print("\nRust (from cargo bench):")
    print(f"  Simple  (3 nodes, 100 items):    38.04 µs")
    print(f"  Complex (10 nodes, 100 items):  313.48 µs")
    print("\nSpeedup (Rust over Python):")
    if simple > 0:
        print(f"  Simple:  {simple*1e6/38.04:.2f}x")
    if complex_time > 0:
        print(f"  Complex: {complex_time*1e6/313.48:.2f}x")
    print()
