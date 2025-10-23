#!/usr/bin/env python3
"""
Calculator Pipeline Example with Rust Runtime

This example demonstrates a pipeline with actual data transformation
using calculator nodes. Shows that the Rust runtime can execute
stateful Python nodes with parameters.

Key Features Demonstrated:
- Nodes with parameters
- Data transformation (multiply, add operations)
- Chaining multiple transformation nodes
- Verifying correct computation
"""

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.simple_math import MultiplyNode, AddNode


async def main():
    """Run the calculator pipeline example."""
    print("=" * 70)
    print("RemoteMedia SDK - Calculator Pipeline with Rust Runtime")
    print("=" * 70)
    print()

    # Create pipeline with math nodes
    print("1. Creating pipeline with math nodes...")
    pipeline = Pipeline(name="MathExample")

    # Chain of transformations: multiply by 2, then add 10
    pipeline.add_node(MultiplyNode(factor=2, name="multiply"))
    pipeline.add_node(AddNode(addend=10, name="add"))

    print(f"   [OK] Pipeline: input -> multiply(x2) -> add(+10) -> output")
    print()

    # Test with various inputs
    test_cases = [
        [1, 2, 3],
        [10, 20, 30],
        [0, -5, 100]
    ]

    for test_data in test_cases:
        print(f"2. Input: {test_data}")

        # Execute with Rust runtime
        result = await pipeline.run(test_data)

        # Calculate expected result: (x * 2) + 10
        expected = [(x * 2) + 10 for x in test_data]

        print(f"   Result:   {result}")
        print(f"   Expected: {expected}")

        # Verify correctness
        if result == expected:
            print(f"   [OK] Computation correct!")
        else:
            print(f"   [ERROR] Result doesn't match expected!")
            return 1

        print()

    print("=" * 70)
    print("[OK] All computations successful!")
    print("[OK] Rust runtime correctly executed Python calculator nodes")
    print("=" * 70)

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
