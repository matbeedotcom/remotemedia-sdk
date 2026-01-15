"""
Simple benchmark comparing pipeline execution performance.

This benchmark measures throughput without complex I/O nodes.
"""

import time
import asyncio
from typing import List

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.calculator import CalculatorNode
from remotemedia.nodes.base import PassThroughNode


async def run_simple_pipeline(iterations: int = 10):
    """Run a simple pipeline benchmark."""

    # Create pipeline without source/sink nodes
    pipeline = Pipeline(name="simple")
    pipeline.add_node(CalculatorNode(name="add", operation="add", operand=5))
    pipeline.add_node(CalculatorNode(name="multiply", operation="multiply", operand=2))
    pipeline.add_node(PassThroughNode(name="output"))

    await pipeline.initialize()

    # Input data generator
    async def data_gen():
        for i in range(100):
            yield i

    print(f"Running {iterations} iterations...")
    times = []

    for i in range(iterations):
        start = time.perf_counter()

        results = []
        async for result in pipeline.process(data_gen()):
            results.append(result)

        elapsed = time.perf_counter() - start
        times.append(elapsed)

        print(f"  Iteration {i+1}: {elapsed*1000:.2f} ms ({len(results)} items)")

    await pipeline.cleanup()

    # Statistics
    avg = sum(times) / len(times)
    min_time = min(times)
    max_time = max(times)

    print(f"\nResults:")
    print(f"  Average: {avg*1000:.2f} ms")
    print(f"  Min:     {min_time*1000:.2f} ms")
    print(f"  Max:     {max_time*1000:.2f} ms")
    print(f"  Throughput: {100/avg:.1f} items/sec")


if __name__ == "__main__":
    asyncio.run(run_simple_pipeline())
