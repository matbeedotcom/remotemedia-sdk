#!/usr/bin/env python3
"""
Async Streaming Pipeline Example

This example demonstrates the Rust runtime's support for async streaming nodes,
including async generators and streaming data flow.

Key Features Demonstrated:
- Async generator nodes (streaming data producers)
- Handling streaming outputs in the Rust runtime
- Processing data items one-by-one
- Async/await support in Python nodes executed by Rust
"""

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.core.node import Node


class StreamingSourceNode(Node):
    """A node that generates a stream of numbers asynchronously."""

    def __init__(self, count=10, name="source"):
        super().__init__(name=name)
        self.count = count

    async def process(self, data=None):
        """Generate numbers asynchronously."""
        for i in range(self.count):
            # Simulate async work
            await asyncio.sleep(0.01)
            yield i


class StreamingTransformNode(Node):
    """A node that transforms each item in the stream."""

    def __init__(self, multiplier=2, name="transform"):
        super().__init__(name=name)
        self.multiplier = multiplier

    async def process(self, data):
        """Transform data asynchronously."""
        # Simulate async work
        await asyncio.sleep(0.01)
        yield data * self.multiplier


async def main():
    """Run the async streaming pipeline example."""
    print("=" * 70)
    print("RemoteMedia SDK - Async Streaming with Rust Runtime")
    print("=" * 70)
    print()

    # Check if Rust runtime is available
    try:
        import remotemedia_runtime
        print(f"[OK] Using Rust runtime (version {remotemedia_runtime.__version__})")
    except ImportError:
        print("[WARN] Rust runtime not available, will use Python executor")
    print()

    # Create streaming pipeline
    print("1. Creating streaming pipeline...")
    pipeline = Pipeline(name="StreamingExample")

    # Add streaming nodes
    pipeline.add_node(StreamingSourceNode(count=5, name="source"))
    pipeline.add_node(StreamingTransformNode(multiplier=10, name="transform"))

    print("   [OK] Pipeline: source (generates 5 items) -> transform (x10)")
    print()

    # Execute with Rust runtime
    print("2. Executing streaming pipeline...")
    print("   Items as they stream:")

    try:
        # Note: For streaming pipelines without input, we don't pass data
        result = await pipeline.run(None)

        # Display results
        if isinstance(result, list):
            for i, item in enumerate(result):
                print(f"      Item {i}: {item}")
        else:
            print(f"      Result: {result}")

        print()
        print("   [OK] Streaming execution successful!")

    except Exception as e:
        print(f"   [ERROR] Error: {e}")
        import traceback
        traceback.print_exc()
        return 1

    print()
    print("=" * 70)
    print("[OK] Example completed successfully!")
    print("[OK] Rust runtime handled async streaming nodes correctly")
    print("=" * 70)

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
