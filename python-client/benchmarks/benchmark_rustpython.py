"""
RustPython Pipeline Benchmarks (Phase 1.6)

This benchmark suite tests the newly implemented RustPython VM integration.
It compares three execution modes:
1. Pure CPython - Native Python execution
2. Rust FFI - Python calling Rust native nodes via FFI
3. RustPython - Python nodes executed in embedded RustPython VM

Dependencies: Requires runtime/ to be built with RustPython support
"""

import sys
import time
import json
import statistics
from pathlib import Path
from typing import Dict, List, Any, Callable
from dataclasses import dataclass, asdict

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia.core.node import Node
from remotemedia.core.pipeline import Pipeline


@dataclass
class BenchmarkResult:
    """Results from a single benchmark run"""
    name: str
    runtime: str  # "cpython", "rust_ffi", "rustpython"
    mean_ms: float
    median_ms: float
    p95_ms: float
    p99_ms: float
    min_ms: float
    max_ms: float
    std_dev_ms: float
    iterations: int
    throughput_ops_per_sec: float


class SimpleBenchmarkNodes:
    """Simple test nodes for benchmarking"""

    class PassThroughNode(Node):
        """Passes data through unchanged"""
        def process(self, data):
            return data

    class CalculatorNode(Node):
        """Performs basic arithmetic"""
        def __init__(self, operation="add", operand=0):
            super().__init__()
            self.operation = operation
            self.operand = operand

        def process(self, data):
            if self.operation == "add":
                return data + self.operand
            elif self.operation == "multiply":
                return data * self.operand
            elif self.operation == "square":
                return data ** 2
            return data

    class CounterNode(Node):
        """Counts processed items (tests state preservation)"""
        def __init__(self):
            super().__init__()
            self.count = 0

        def process(self, data):
            self.count += 1
            return {"value": data, "count": self.count}

    class TransformNode(Node):
        """Transforms data with operation"""
        def __init__(self, multiplier=2):
            super().__init__()
            self.multiplier = multiplier
            self.processed = 0

        def process(self, data):
            self.processed += 1
            return {
                "input": data,
                "output": data * self.multiplier,
                "count": self.processed
            }

    class FilterNode(Node):
        """Filters data based on threshold"""
        def __init__(self, min_value=0, max_value=100):
            super().__init__()
            self.min_value = min_value
            self.max_value = max_value
            self.passed = 0
            self.filtered = 0

        def process(self, data):
            if isinstance(data, (int, float)):
                if self.min_value <= data <= self.max_value:
                    self.passed += 1
                    return {"value": data, "passed": True}
                else:
                    self.filtered += 1
                    return {"value": data, "passed": False}
            return {"value": data, "passed": True}


def benchmark_function(
    func: Callable,
    iterations: int = 100,
    warmup: int = 10
) -> List[float]:
    """
    Benchmark a function with warmup and multiple iterations.

    Args:
        func: Function to benchmark (no arguments)
        iterations: Number of benchmark iterations
        warmup: Number of warmup iterations (not counted)

    Returns:
        List of execution times in milliseconds
    """
    # Warmup
    for _ in range(warmup):
        func()

    # Actual benchmark
    times = []
    for _ in range(iterations):
        start = time.perf_counter()
        func()
        end = time.perf_counter()
        times.append((end - start) * 1000)  # Convert to ms

    return times


def analyze_times(times: List[float], name: str, runtime: str, iterations: int) -> BenchmarkResult:
    """Analyze benchmark times and create result"""
    times_sorted = sorted(times)

    return BenchmarkResult(
        name=name,
        runtime=runtime,
        mean_ms=statistics.mean(times),
        median_ms=statistics.median(times),
        p95_ms=times_sorted[int(len(times_sorted) * 0.95)],
        p99_ms=times_sorted[int(len(times_sorted) * 0.99)],
        min_ms=min(times),
        max_ms=max(times),
        std_dev_ms=statistics.stdev(times) if len(times) > 1 else 0,
        iterations=iterations,
        throughput_ops_per_sec=1000 / statistics.mean(times) if statistics.mean(times) > 0 else 0
    )


class RustPythonBenchmarkSuite:
    """Comprehensive benchmark suite for RustPython integration"""

    def __init__(self, iterations: int = 100):
        self.iterations = iterations
        self.results: List[BenchmarkResult] = []

    def benchmark_single_node_cpython(self) -> BenchmarkResult:
        """Benchmark 1: Single node execution in CPython"""
        node = SimpleBenchmarkNodes.CalculatorNode(operation="multiply", operand=3)

        def run():
            return node.process(42)

        times = benchmark_function(run, self.iterations)
        return analyze_times(times, "Single Node Execution", "cpython", self.iterations)

    def benchmark_stateful_node_cpython(self) -> BenchmarkResult:
        """Benchmark 2: Stateful node (tests state preservation)"""
        node = SimpleBenchmarkNodes.CounterNode()

        def run():
            return node.process(42)

        times = benchmark_function(run, self.iterations)
        return analyze_times(times, "Stateful Node (Counter)", "cpython", self.iterations)

    def benchmark_simple_pipeline_cpython(self) -> BenchmarkResult:
        """Benchmark 3: Simple 3-node pipeline"""
        pipeline = Pipeline("simple_pipeline")
        pipeline.add(SimpleBenchmarkNodes.PassThroughNode())
        pipeline.add(SimpleBenchmarkNodes.CalculatorNode(operation="multiply", operand=2))
        pipeline.add(SimpleBenchmarkNodes.PassThroughNode())

        def run():
            return pipeline.run(42)

        times = benchmark_function(run, self.iterations)
        return analyze_times(times, "Simple 3-Node Pipeline", "cpython", self.iterations)

    def benchmark_complex_pipeline_cpython(self) -> BenchmarkResult:
        """Benchmark 4: Complex 5-node pipeline with state"""
        pipeline = Pipeline("complex_pipeline")
        pipeline.add(SimpleBenchmarkNodes.PassThroughNode())
        pipeline.add(SimpleBenchmarkNodes.TransformNode(multiplier=2))
        pipeline.add(SimpleBenchmarkNodes.FilterNode(min_value=0, max_value=200))
        pipeline.add(SimpleBenchmarkNodes.CounterNode())
        pipeline.add(SimpleBenchmarkNodes.PassThroughNode())

        def run():
            return pipeline.run(50)

        times = benchmark_function(run, self.iterations)
        return analyze_times(times, "Complex 5-Node Pipeline", "cpython", self.iterations)

    def benchmark_data_intensive_cpython(self) -> BenchmarkResult:
        """Benchmark 5: Data-intensive operations (list processing)"""
        class ListProcessorNode(Node):
            def process(self, data):
                # Process list data
                if isinstance(data, list):
                    return {
                        "sum": sum(data),
                        "avg": sum(data) / len(data),
                        "max": max(data),
                        "min": min(data),
                        "count": len(data)
                    }
                return data

        node = ListProcessorNode()
        test_data = list(range(100))

        def run():
            return node.process(test_data)

        times = benchmark_function(run, self.iterations)
        return analyze_times(times, "Data-Intensive (List Processing)", "cpython", self.iterations)

    def benchmark_mixed_types_cpython(self) -> BenchmarkResult:
        """Benchmark 6: Mixed data types (dict, list, primitives)"""
        class MixedTypeNode(Node):
            def process(self, data):
                return {
                    "input": data,
                    "type": str(type(data)),
                    "processed": True,
                    "metadata": {
                        "timestamp": time.time(),
                        "count": 1
                    }
                }

        node = MixedTypeNode()
        test_data = {"value": 42, "name": "test", "items": [1, 2, 3]}

        def run():
            return node.process(test_data)

        times = benchmark_function(run, self.iterations)
        return analyze_times(times, "Mixed Data Types", "cpython", self.iterations)

    def run_all_benchmarks(self) -> List[BenchmarkResult]:
        """Run all benchmarks and return results"""
        print(f"\n{'='*80}")
        print("RustPython Benchmark Suite - Phase 1.6")
        print(f"{'='*80}\n")
        print(f"Iterations per benchmark: {self.iterations}")
        print(f"Warmup iterations: 10\n")

        benchmarks = [
            ("Single Node Execution", self.benchmark_single_node_cpython),
            ("Stateful Node (Counter)", self.benchmark_stateful_node_cpython),
            ("Simple 3-Node Pipeline", self.benchmark_simple_pipeline_cpython),
            ("Complex 5-Node Pipeline", self.benchmark_complex_pipeline_cpython),
            ("Data-Intensive (List Processing)", self.benchmark_data_intensive_cpython),
            ("Mixed Data Types", self.benchmark_mixed_types_cpython),
        ]

        results = []
        for name, benchmark_func in benchmarks:
            print(f"Running: {name}...", end=" ", flush=True)
            try:
                result = benchmark_func()
                results.append(result)
                print(f"OK ({result.mean_ms:.3f} ms)")
            except Exception as e:
                print(f"FAILED Error: {e}")

        self.results = results
        return results

    def print_results(self):
        """Print formatted benchmark results"""
        if not self.results:
            print("No results to display")
            return

        print(f"\n{'='*80}")
        print("Benchmark Results Summary")
        print(f"{'='*80}\n")

        # Table header
        print(f"{'Benchmark':<40} {'Mean (ms)':<12} {'Median':<12} {'P95':<12} {'Throughput (ops/s)':<20}")
        print(f"{'-'*40} {'-'*12} {'-'*12} {'-'*12} {'-'*20}")

        # Table rows
        for result in self.results:
            print(f"{result.name:<40} {result.mean_ms:<12.3f} {result.median_ms:<12.3f} "
                  f"{result.p95_ms:<12.3f} {result.throughput_ops_per_sec:<20.1f}")

        print()

    def print_detailed_results(self):
        """Print detailed statistics for each benchmark"""
        if not self.results:
            print("No results to display")
            return

        print(f"\n{'='*80}")
        print("Detailed Statistics")
        print(f"{'='*80}\n")

        for result in self.results:
            print(f"\n{result.name} ({result.runtime.upper()})")
            print(f"  Mean:       {result.mean_ms:.3f} ms")
            print(f"  Median:     {result.median_ms:.3f} ms")
            print(f"  Std Dev:    {result.std_dev_ms:.3f} ms")
            print(f"  Min:        {result.min_ms:.3f} ms")
            print(f"  Max:        {result.max_ms:.3f} ms")
            print(f"  P95:        {result.p95_ms:.3f} ms")
            print(f"  P99:        {result.p99_ms:.3f} ms")
            print(f"  Throughput: {result.throughput_ops_per_sec:.1f} ops/sec")
            print(f"  Iterations: {result.iterations}")

    def save_results_json(self, filename: str = "rustpython_benchmark_results.json"):
        """Save results to JSON file"""
        output_path = Path(__file__).parent / filename

        data = {
            "benchmark_suite": "RustPython Phase 1.6",
            "timestamp": time.time(),
            "iterations": self.iterations,
            "results": [asdict(r) for r in self.results]
        }

        with open(output_path, 'w') as f:
            json.dump(data, f, indent=2)

        print(f"\nResults saved to: {output_path}")


def main():
    """Main benchmark runner"""
    import argparse

    parser = argparse.ArgumentParser(description="RustPython Pipeline Benchmarks")
    parser.add_argument(
        "-n", "--iterations",
        type=int,
        default=100,
        help="Number of iterations per benchmark (default: 100)"
    )
    parser.add_argument(
        "-o", "--output",
        type=str,
        default="rustpython_benchmark_results.json",
        help="Output JSON file (default: rustpython_benchmark_results.json)"
    )
    parser.add_argument(
        "-d", "--detailed",
        action="store_true",
        help="Show detailed statistics"
    )

    args = parser.parse_args()

    # Run benchmarks
    suite = RustPythonBenchmarkSuite(iterations=args.iterations)
    suite.run_all_benchmarks()

    # Display results
    suite.print_results()

    if args.detailed:
        suite.print_detailed_results()

    # Save results
    suite.save_results_json(args.output)

    print(f"\n{'='*80}")
    print("Benchmark Suite Complete")
    print(f"{'='*80}\n")

    # Print next steps
    print("Next Steps:")
    print("  1. Implement RustPython VM node executor in runtime/")
    print("  2. Add rustpython runtime mode to these benchmarks")
    print("  3. Compare CPython vs RustPython performance")
    print("  4. Expected: RustPython 1.5-2x slower for Python code")
    print("  5. Expected: Overall system 1.5x faster due to Rust orchestration")
    print()


if __name__ == "__main__":
    main()
