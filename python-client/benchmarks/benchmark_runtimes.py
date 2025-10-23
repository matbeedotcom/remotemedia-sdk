"""
Benchmark comparing Python native execution vs Rust runtime.

This script measures the performance difference between:
1. Python native execution (baseline)
2. Rust runtime with manifest execution (when available)
3. (Future) Rust runtime with RustPython VM

Benchmark scenarios:
- Simple linear pipeline (3-5 nodes)
- Complex pipeline (10+ nodes)
- Data-intensive operations
- Stateful processing
"""

import time
import json
import asyncio
import statistics
from typing import List, Dict, Any
from dataclasses import dataclass

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.calculator import CalculatorNode
from remotemedia.nodes.io_nodes import DataSourceNode, DataSinkNode
from remotemedia.nodes.base import PassThroughNode, BufferNode
from remotemedia.nodes.transform import TextTransformNode


@dataclass
class BenchmarkResult:
    """Results from a benchmark run."""
    name: str
    runtime: str
    iterations: int
    times: List[float]
    mean: float
    median: float
    stddev: float
    min_time: float
    max_time: float

    def speedup_vs(self, baseline: 'BenchmarkResult') -> float:
        """Calculate speedup compared to baseline."""
        return baseline.mean / self.mean


class PipelineBenchmark:
    """Benchmark harness for pipeline execution."""

    def __init__(self, iterations: int = 100):
        self.iterations = iterations
        self.results: List[BenchmarkResult] = []

    async def benchmark_pipeline(
        self,
        name: str,
        pipeline: Pipeline,
        input_data: List[Any],
        runtime: str = "python"
    ) -> BenchmarkResult:
        """
        Benchmark a pipeline execution.

        Args:
            name: Benchmark name
            pipeline: Pipeline to benchmark
            input_data: Input data stream
            runtime: Runtime type (python, rust, rustpython)

        Returns:
            BenchmarkResult with timing statistics
        """
        print(f"\n{'='*60}")
        print(f"Benchmarking: {name} ({runtime})")
        print(f"Iterations: {self.iterations}")
        print(f"{'='*60}")

        # Warmup
        print("Warming up... ", end="", flush=True)
        await self._run_pipeline(pipeline, input_data, warmup=True)
        print("[OK]")

        # Benchmark iterations
        times = []
        print(f"Running {self.iterations} iterations: ", end="", flush=True)

        for i in range(self.iterations):
            if (i + 1) % 10 == 0:
                print(f"{i+1}", end="", flush=True)
            elif (i + 1) % 5 == 0:
                print(".", end="", flush=True)

            elapsed = await self._run_pipeline(pipeline, input_data)
            times.append(elapsed)

        print(" [DONE]")

        # Calculate statistics
        result = BenchmarkResult(
            name=name,
            runtime=runtime,
            iterations=self.iterations,
            times=times,
            mean=statistics.mean(times),
            median=statistics.median(times),
            stddev=statistics.stdev(times) if len(times) > 1 else 0,
            min_time=min(times),
            max_time=max(times)
        )

        self.results.append(result)
        self._print_result(result)

        return result

    async def _run_pipeline(
        self,
        pipeline: Pipeline,
        input_data: List[Any],
        warmup: bool = False
    ) -> float:
        """Run pipeline and measure execution time."""
        # Initialize pipeline if not already done
        if not pipeline.is_initialized:
            await pipeline.initialize()

        # Get the first node (DataSourceNode) and push data
        source_node = pipeline.nodes[0]

        async def push_and_process():
            """Push data into source and process through pipeline."""
            # Push data in background task
            async def push_data():
                for item in input_data:
                    await source_node.push_data(item)
                from remotemedia.core.types import _SENTINEL
                await source_node.push_data(_SENTINEL)

            # Start pushing data
            push_task = asyncio.create_task(push_data())

            # Process through remaining nodes (skip DataSourceNode)
            # DataSourceNode.process() generates the stream from its queue
            results = []
            if len(pipeline.nodes) > 1:
                # Create input stream from DataSourceNode
                async for result in pipeline.process(source_node.process()):
                    results.append(result)
            else:
                # Only DataSourceNode, just consume its output
                async for result in source_node.process():
                    results.append(result)

            await push_task
            return results

        start = time.perf_counter()
        results = await push_and_process()
        end = time.perf_counter()

        # Cleanup after warmup
        if warmup:
            await pipeline.cleanup()

        return end - start

    def _print_result(self, result: BenchmarkResult):
        """Print benchmark results."""
        print(f"\nResults:")
        print(f"  Mean:   {result.mean*1000:.2f} ms")
        print(f"  Median: {result.median*1000:.2f} ms")
        print(f"  Stddev: {result.stddev*1000:.2f} ms")
        print(f"  Min:    {result.min_time*1000:.2f} ms")
        print(f"  Max:    {result.max_time*1000:.2f} ms")
        print(f"  Throughput: {result.iterations/sum(result.times):.1f} runs/sec")

    def print_comparison(self):
        """Print comparison of all benchmark results."""
        if len(self.results) < 2:
            return

        print("\n" + "="*60)
        print("BENCHMARK COMPARISON")
        print("="*60)

        # Group by benchmark name
        by_name: Dict[str, List[BenchmarkResult]] = {}
        for result in self.results:
            if result.name not in by_name:
                by_name[result.name] = []
            by_name[result.name].append(result)

        # Print comparison for each benchmark
        for name, results in by_name.items():
            print(f"\n{name}:")
            print("-" * 60)

            # Find baseline (Python)
            baseline = next((r for r in results if r.runtime == "python"), results[0])

            for result in sorted(results, key=lambda r: r.mean):
                speedup = baseline.mean / result.mean
                speedup_str = f"{speedup:.2f}x" if result.runtime != baseline.runtime else "baseline"

                print(f"  {result.runtime:15s}: {result.mean*1000:7.2f} ms  ({speedup_str})")

        print("\n" + "="*60)

    def save_results(self, filename: str = "benchmark_results.json"):
        """Save benchmark results to JSON file."""
        data = {
            "benchmarks": [
                {
                    "name": r.name,
                    "runtime": r.runtime,
                    "iterations": r.iterations,
                    "mean_ms": r.mean * 1000,
                    "median_ms": r.median * 1000,
                    "stddev_ms": r.stddev * 1000,
                    "min_ms": r.min_time * 1000,
                    "max_ms": r.max_time * 1000,
                }
                for r in self.results
            ]
        }

        with open(filename, 'w') as f:
            json.dump(data, f, indent=2)

        print(f"\n[OK] Results saved to {filename}")


# Benchmark scenarios

def create_simple_pipeline() -> Pipeline:
    """Create a simple 3-node pipeline."""
    pipeline = Pipeline(name="simple-pipeline")
    pipeline.add_node(DataSourceNode(name="input"))
    pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=5))
    pipeline.add_node(DataSinkNode(name="output"))
    return pipeline


def create_complex_pipeline() -> Pipeline:
    """Create a complex 10-node pipeline."""
    pipeline = Pipeline(name="complex-pipeline")
    pipeline.add_node(DataSourceNode(name="input"))

    # Chain of transformations
    pipeline.add_node(CalculatorNode(name="add1", operation="add", operand=1))
    pipeline.add_node(CalculatorNode(name="multiply", operation="multiply", operand=2))
    pipeline.add_node(CalculatorNode(name="add2", operation="add", operand=3))
    pipeline.add_node(PassThroughNode(name="passthrough1"))
    pipeline.add_node(CalculatorNode(name="subtract", operation="subtract", operand=5))
    pipeline.add_node(PassThroughNode(name="passthrough2"))
    pipeline.add_node(CalculatorNode(name="divide", operation="divide", operand=2))
    pipeline.add_node(PassThroughNode(name="passthrough3"))

    pipeline.add_node(DataSinkNode(name="output"))
    return pipeline


def create_text_pipeline() -> Pipeline:
    """Create a text processing pipeline."""
    pipeline = Pipeline(name="text-pipeline")
    pipeline.add_node(DataSourceNode(name="input"))
    pipeline.add_node(TextTransformNode(name="upper", operation="uppercase"))
    pipeline.add_node(TextTransformNode(name="reverse", operation="reverse"))
    pipeline.add_node(DataSinkNode(name="output"))
    return pipeline


async def main():
    """Run all benchmarks."""
    print("="*60)
    print("REMOTEMEDIA RUNTIME PERFORMANCE BENCHMARK")
    print("="*60)
    print("\nComparing execution speeds:")
    print("  • Python native (baseline)")
    print("  • Rust runtime (when implemented)")
    print("  • RustPython VM (Phase 1)")
    print()

    bench = PipelineBenchmark(iterations=100)

    # Benchmark 1: Simple pipeline with numeric data
    print("\n[BENCHMARK 1] Simple Pipeline (3 nodes)")
    simple = create_simple_pipeline()
    input_data = list(range(100))  # 100 integers

    await bench.benchmark_pipeline(
        "Simple Pipeline (100 items)",
        simple,
        input_data,
        runtime="python"
    )

    # Benchmark 2: Complex pipeline
    print("\n[BENCHMARK 2] Complex Pipeline (10 nodes)")
    complex_pipe = create_complex_pipeline()

    await bench.benchmark_pipeline(
        "Complex Pipeline (100 items)",
        complex_pipe,
        input_data,
        runtime="python"
    )

    # Benchmark 3: Text processing
    print("\n[BENCHMARK 3] Text Processing")
    text_pipe = create_text_pipeline()
    text_data = [f"test string {i}" for i in range(100)]

    await bench.benchmark_pipeline(
        "Text Pipeline (100 items)",
        text_pipe,
        text_data,
        runtime="python"
    )

    # Benchmark 4: Large dataset
    print("\n[BENCHMARK 4] Large Dataset")
    large_data = list(range(1000))

    await bench.benchmark_pipeline(
        "Simple Pipeline (1000 items)",
        create_simple_pipeline(),
        large_data,
        runtime="python"
    )

    # Print comparison and save results
    bench.print_comparison()
    bench.save_results("benchmarks/results.json")

    print("\n" + "="*60)
    print("BASELINE ESTABLISHED")
    print("="*60)
    print("\nNext steps:")
    print("  1. Implement Rust runtime execution")
    print("  2. Re-run benchmarks with --runtime=rust")
    print("  3. Compare speedups")
    print("\nExpected improvements:")
    print("  • 2-5x speedup for compute-heavy pipelines")
    print("  • Lower latency for pipeline initialization")
    print("  • Reduced memory overhead")
    print()


if __name__ == "__main__":
    asyncio.run(main())
