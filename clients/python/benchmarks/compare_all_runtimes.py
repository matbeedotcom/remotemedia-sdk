#!/usr/bin/env python
"""
Comprehensive Runtime Comparison Benchmark

Compares three execution strategies:
1. CPython - Pure Python execution (baseline)
2. Rust Runtime - Rust executor with FFI (current)
3. RustPython - Python in RustPython VM (future - Phase 1.5)

This benchmark uses Pipeline.run() to test the integrated runtime.
"""

import asyncio
import time
import statistics
import json
import sys
from pathlib import Path
from typing import List, Dict, Any, Optional
from dataclasses import dataclass, asdict

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.base import PassThroughNode
from remotemedia.nodes.calculator import CalculatorNode


@dataclass
class RuntimeResult:
    """Results from a single runtime execution."""
    runtime: str  # "cpython", "rust", or "rustpython"
    mean_ms: float
    median_ms: float
    stddev_ms: float
    min_ms: float
    max_ms: float
    p95_ms: float
    p99_ms: float
    throughput_ops_sec: float
    iterations: int
    available: bool = True
    error: Optional[str] = None


@dataclass
class BenchmarkComparison:
    """Comparison of multiple runtimes for a benchmark."""
    name: str
    description: str
    cpython: RuntimeResult
    rust: Optional[RuntimeResult]
    rustpython: Optional[RuntimeResult]

    def speedup_rust_vs_cpython(self) -> Optional[float]:
        """Calculate Rust speedup over CPython."""
        if self.rust and self.rust.available:
            return self.cpython.mean_ms / self.rust.mean_ms
        return None

    def speedup_rustpython_vs_cpython(self) -> Optional[float]:
        """Calculate RustPython speedup over CPython."""
        if self.rustpython and self.rustpython.available:
            return self.cpython.mean_ms / self.rustpython.mean_ms
        return None

    def winner(self) -> str:
        """Determine which runtime is fastest."""
        results = []
        if self.cpython.available:
            results.append(("CPython", self.cpython.mean_ms))
        if self.rust and self.rust.available:
            results.append(("Rust", self.rust.mean_ms))
        if self.rustpython and self.rustpython.available:
            results.append(("RustPython", self.rustpython.mean_ms))

        if not results:
            return "None"

        return min(results, key=lambda x: x[1])[0]


class RuntimeBenchmark:
    """Benchmark harness for comparing all three runtimes."""

    def __init__(self, iterations: int = 100, warmup: int = 10):
        self.iterations = iterations
        self.warmup = warmup
        self.comparisons: List[BenchmarkComparison] = []

    async def benchmark_runtime(
        self,
        pipeline: Pipeline,
        input_data: List[Any],
        runtime: str
    ) -> RuntimeResult:
        """
        Benchmark a single runtime execution.

        Args:
            pipeline: Pipeline to execute
            input_data: Input data
            runtime: "cpython", "rust", or "rustpython"

        Returns:
            RuntimeResult with timing statistics
        """
        times = []

        # Determine use_rust flag
        if runtime == "cpython":
            use_rust = False
        elif runtime == "rust":
            use_rust = True
        elif runtime == "rustpython":
            # Future: This will use a different flag
            # For now, treat as unavailable
            return RuntimeResult(
                runtime=runtime,
                mean_ms=0,
                median_ms=0,
                stddev_ms=0,
                min_ms=0,
                max_ms=0,
                p95_ms=0,
                p99_ms=0,
                throughput_ops_sec=0,
                iterations=0,
                available=False,
                error="RustPython VM not yet implemented (Phase 1.5)"
            )
        else:
            raise ValueError(f"Unknown runtime: {runtime}")

        # Warmup runs
        print(f"    Warmup ({self.warmup} runs)...", end=" ", flush=True)
        try:
            for _ in range(self.warmup):
                await pipeline.run(input_data, use_rust=use_rust)
            print("[OK]")
        except Exception as e:
            print(f"[FAIL: {e}]")
            return RuntimeResult(
                runtime=runtime,
                mean_ms=0,
                median_ms=0,
                stddev_ms=0,
                min_ms=0,
                max_ms=0,
                p95_ms=0,
                p99_ms=0,
                throughput_ops_sec=0,
                iterations=0,
                available=False,
                error=str(e)
            )

        # Benchmark runs
        print(f"    Running {self.iterations} iterations...", end=" ", flush=True)
        for i in range(self.iterations):
            if (i + 1) % 20 == 0:
                print(f"{i+1}", end="", flush=True)
            elif (i + 1) % 10 == 0:
                print(".", end="", flush=True)

            start = time.perf_counter()
            try:
                await pipeline.run(input_data, use_rust=use_rust)
                elapsed = (time.perf_counter() - start) * 1000  # Convert to ms
                times.append(elapsed)
            except Exception as e:
                print(f"[FAIL: {e}]")
                return RuntimeResult(
                    runtime=runtime,
                    mean_ms=0,
                    median_ms=0,
                    stddev_ms=0,
                    min_ms=0,
                    max_ms=0,
                    p95_ms=0,
                    p99_ms=0,
                    throughput_ops_sec=0,
                    iterations=len(times),
                    available=False,
                    error=str(e)
                )

        print(" [DONE]")

        # Calculate statistics
        times_sorted = sorted(times)
        p95_idx = int(len(times_sorted) * 0.95)
        p99_idx = int(len(times_sorted) * 0.99)

        mean_ms = statistics.mean(times)
        total_time_sec = sum(times) / 1000  # Convert ms to seconds

        return RuntimeResult(
            runtime=runtime,
            mean_ms=mean_ms,
            median_ms=statistics.median(times),
            stddev_ms=statistics.stdev(times) if len(times) > 1 else 0,
            min_ms=min(times),
            max_ms=max(times),
            p95_ms=times_sorted[p95_idx] if p95_idx < len(times_sorted) else times_sorted[-1],
            p99_ms=times_sorted[p99_idx] if p99_idx < len(times_sorted) else times_sorted[-1],
            throughput_ops_sec=self.iterations / total_time_sec if total_time_sec > 0 else 0,
            iterations=self.iterations,
            available=True
        )

    async def compare_all(
        self,
        name: str,
        description: str,
        pipeline: Pipeline,
        input_data: List[Any]
    ) -> BenchmarkComparison:
        """
        Run the same benchmark on all three runtimes.

        Args:
            name: Benchmark name
            description: Benchmark description
            pipeline: Pipeline to execute
            input_data: Input data

        Returns:
            BenchmarkComparison with results from all runtimes
        """
        print(f"\n{'=' * 70}")
        print(f"{name}")
        print(f"{description}")
        print(f"{'=' * 70}")

        # Benchmark CPython
        print("\n  [1/3] CPython (Python-only executor)")
        cpython_result = await self.benchmark_runtime(pipeline, input_data, "cpython")
        self._print_result(cpython_result)

        # Benchmark Rust
        print("\n  [2/3] Rust (Rust runtime via FFI)")
        rust_result = await self.benchmark_runtime(pipeline, input_data, "rust")
        self._print_result(rust_result)

        # Benchmark RustPython (future)
        print("\n  [3/3] RustPython (RustPython VM - Phase 1.5)")
        rustpython_result = await self.benchmark_runtime(pipeline, input_data, "rustpython")
        self._print_result(rustpython_result)

        comparison = BenchmarkComparison(
            name=name,
            description=description,
            cpython=cpython_result,
            rust=rust_result if rust_result.available else None,
            rustpython=rustpython_result if rustpython_result.available else None
        )

        self.comparisons.append(comparison)
        self._print_comparison(comparison)

        return comparison

    def _print_result(self, result: RuntimeResult):
        """Print individual runtime result."""
        if not result.available:
            print(f"    Status: NOT AVAILABLE")
            print(f"    Reason: {result.error}")
            return

        print(f"    Mean:       {result.mean_ms:8.2f} ms")
        print(f"    Median:     {result.median_ms:8.2f} ms")
        print(f"    Std Dev:    {result.stddev_ms:8.2f} ms")
        print(f"    Min:        {result.min_ms:8.2f} ms")
        print(f"    Max:        {result.max_ms:8.2f} ms")
        print(f"    P95:        {result.p95_ms:8.2f} ms")
        print(f"    P99:        {result.p99_ms:8.2f} ms")
        print(f"    Throughput: {result.throughput_ops_sec:8.1f} ops/sec")

    def _print_comparison(self, comp: BenchmarkComparison):
        """Print comparison summary."""
        print(f"\n  Comparison:")
        print(f"  {'-' * 66}")

        # Print each runtime
        runtimes = [
            ("CPython", comp.cpython),
            ("Rust", comp.rust),
            ("RustPython", comp.rustpython)
        ]

        baseline = comp.cpython.mean_ms

        for name, result in runtimes:
            if result is None or not result.available:
                print(f"  {name:12s}: NOT AVAILABLE")
                continue

            speedup = baseline / result.mean_ms
            speedup_str = f"{speedup:.2f}x" if speedup != 1.0 else "baseline"

            if speedup > 1.0:
                symbol = "[OK]  "
            elif speedup < 1.0:
                symbol = "[WARN]"
            else:
                symbol = "      "

            print(f"  {symbol} {name:12s}: {result.mean_ms:8.2f} ms  ({speedup_str})")

        print(f"  {'-' * 66}")
        print(f"  Winner: {comp.winner()}")

    def print_summary(self):
        """Print summary of all benchmarks."""
        print(f"\n{'=' * 70}")
        print("BENCHMARK SUMMARY")
        print(f"{'=' * 70}\n")

        # Summary table
        print(f"{'Benchmark':<35} {'CPython':>10} {'Rust':>10} {'RustPy':>10}")
        print(f"{'-' * 70}")

        for comp in self.comparisons:
            cpython_str = f"{comp.cpython.mean_ms:.2f}ms"

            rust_str = "N/A"
            if comp.rust and comp.rust.available:
                speedup = comp.speedup_rust_vs_cpython()
                rust_str = f"{comp.rust.mean_ms:.2f}ms ({speedup:.2f}x)"

            rustpy_str = "N/A"
            if comp.rustpython and comp.rustpython.available:
                speedup = comp.speedup_rustpython_vs_cpython()
                rustpy_str = f"{comp.rustpython.mean_ms:.2f}ms ({speedup:.2f}x)"

            name = comp.name[:33]
            print(f"{name:<35} {cpython_str:>10} {rust_str:>10} {rustpy_str:>10}")

        print(f"{'-' * 70}\n")

        # Overall statistics
        rust_speedups = [
            comp.speedup_rust_vs_cpython()
            for comp in self.comparisons
            if comp.rust and comp.rust.available and comp.speedup_rust_vs_cpython()
        ]

        if rust_speedups:
            avg_speedup = statistics.mean(rust_speedups)
            print(f"Average Rust Speedup: {avg_speedup:.2f}x")

            if avg_speedup >= 1.5:
                print("[OK] Rust runtime meets >=1.5x speedup target!")
            elif avg_speedup >= 1.0:
                print("[WARN] Rust runtime faster but below 1.5x target")
            else:
                print("[WARN] CPython currently faster (FFI overhead for small pipelines)")

        print()

    def save_results(self, filename: str = "runtime_comparison.json"):
        """Save results to JSON file."""
        data = {
            "benchmark_config": {
                "iterations": self.iterations,
                "warmup": self.warmup
            },
            "comparisons": [
                {
                    "name": comp.name,
                    "description": comp.description,
                    "cpython": asdict(comp.cpython),
                    "rust": asdict(comp.rust) if comp.rust else None,
                    "rustpython": asdict(comp.rustpython) if comp.rustpython else None,
                    "speedup_rust": comp.speedup_rust_vs_cpython(),
                    "speedup_rustpython": comp.speedup_rustpython_vs_cpython(),
                    "winner": comp.winner()
                }
                for comp in self.comparisons
            ]
        }

        output_path = Path(__file__).parent / filename
        with open(output_path, 'w') as f:
            json.dump(data, f, indent=2)

        print(f"[OK] Results saved to {output_path}")


# Benchmark Scenarios

def create_simple_pipeline() -> Pipeline:
    """Simple 3-node pipeline."""
    pipeline = Pipeline(name="simple-3-node")
    pipeline.add_node(PassThroughNode(name="pass1"))
    pipeline.add_node(PassThroughNode(name="pass2"))
    pipeline.add_node(PassThroughNode(name="pass3"))
    return pipeline


def create_medium_pipeline() -> Pipeline:
    """Medium 5-node pipeline with calculation."""
    pipeline = Pipeline(name="medium-5-node")
    pipeline.add_node(PassThroughNode(name="pass1"))
    pipeline.add_node(CalculatorNode(name="calc1", operation="add", operand=5))
    pipeline.add_node(PassThroughNode(name="pass2"))
    pipeline.add_node(CalculatorNode(name="calc2", operation="multiply", operand=2))
    pipeline.add_node(PassThroughNode(name="pass3"))
    return pipeline


def create_complex_pipeline() -> Pipeline:
    """Complex 10-node pipeline."""
    pipeline = Pipeline(name="complex-10-node")
    for i in range(10):
        if i % 3 == 0:
            pipeline.add_node(CalculatorNode(name=f"calc{i}", operation="add", operand=1))
        else:
            pipeline.add_node(PassThroughNode(name=f"pass{i}"))
    return pipeline


def create_calculation_heavy_pipeline() -> Pipeline:
    """Calculation-heavy pipeline (7 calculator nodes)."""
    pipeline = Pipeline(name="calc-heavy")
    operations = ["add", "subtract", "multiply", "divide", "add", "multiply", "subtract"]
    operands = [5, 2, 3, 2, 10, 2, 5]

    for i, (op, operand) in enumerate(zip(operations, operands)):
        pipeline.add_node(CalculatorNode(name=f"calc{i}", operation=op, operand=operand))

    return pipeline


async def main():
    """Run all benchmark comparisons."""
    print("=" * 70)
    print("REMOTEMEDIA RUNTIME COMPARISON BENCHMARK")
    print("=" * 70)
    print()
    print("This benchmark compares three execution strategies:")
    print("  1. CPython - Pure Python execution (baseline)")
    print("  2. Rust - Rust executor via FFI (current)")
    print("  3. RustPython - RustPython VM (future - Phase 1.5)")
    print()
    print("Using Pipeline.run() with automatic runtime selection")
    print("=" * 70)

    bench = RuntimeBenchmark(iterations=100, warmup=10)

    # Benchmark 1: Simple pipeline with small dataset
    await bench.compare_all(
        name="Simple Pipeline (3 nodes, 10 items)",
        description="Minimal pipeline to measure overhead",
        pipeline=create_simple_pipeline(),
        input_data=list(range(10))
    )

    # Benchmark 2: Simple pipeline with medium dataset
    await bench.compare_all(
        name="Simple Pipeline (3 nodes, 100 items)",
        description="Standard dataset size",
        pipeline=create_simple_pipeline(),
        input_data=list(range(100))
    )

    # Benchmark 3: Medium pipeline
    await bench.compare_all(
        name="Medium Pipeline (5 nodes, 100 items)",
        description="Mixed PassThrough and Calculator nodes",
        pipeline=create_medium_pipeline(),
        input_data=list(range(100))
    )

    # Benchmark 4: Complex pipeline
    await bench.compare_all(
        name="Complex Pipeline (10 nodes, 100 items)",
        description="Large pipeline with multiple stages",
        pipeline=create_complex_pipeline(),
        input_data=list(range(100))
    )

    # Benchmark 5: Calculation-heavy
    await bench.compare_all(
        name="Calculation-Heavy (7 calc nodes, 100 items)",
        description="Pure calculation workload",
        pipeline=create_calculation_heavy_pipeline(),
        input_data=list(range(1, 101))  # Start at 1 to avoid division by zero
    )

    # Benchmark 6: Large dataset
    await bench.compare_all(
        name="Large Dataset (3 nodes, 1000 items)",
        description="Stress test with large input",
        pipeline=create_simple_pipeline(),
        input_data=list(range(1000))
    )

    # Print summary and save results
    bench.print_summary()
    bench.save_results("runtime_comparison.json")

    print("\n" + "=" * 70)
    print("NEXT STEPS")
    print("=" * 70)
    print()
    print("Phase 1.5 (RustPython VM Integration):")
    print("  1. Embed RustPython VM in Rust runtime")
    print("  2. Implement Python node execution in VM")
    print("  3. Re-run this benchmark to compare all three runtimes")
    print()
    print("Expected Results After Phase 1.5:")
    print("  - Rust (native nodes): 2-5x faster")
    print("  - RustPython (Python nodes): 1.5-2x faster")
    print("  - Mixed execution: Optimal performance")
    print()


if __name__ == "__main__":
    asyncio.run(main())
