#!/usr/bin/env python3
"""
Example 6: Streaming Input and Output with Node Instances

Demonstrates streaming execution with Node instances that yield multiple outputs.
Feature 011 - Streaming Support
"""

import sys
import asyncio
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Streaming input/output example."""
    print("=" * 70)
    print("Example 6: Streaming Input and Output with Node Instances")
    print("=" * 70)
    print()

    from remotemedia.core.node import Node
    from remotemedia.core.pipeline import Pipeline

    # Streaming Source Node - generates multiple outputs
    class StreamingSourceNode(Node):
        """Node that yields multiple outputs from a single input."""

        def __init__(self, count=3, **kwargs):
            super().__init__(**kwargs)
            self.count = count
            self.is_streaming = True  # Mark as streaming

        async def process(self, input_stream):
            """Process input stream and yield outputs."""
            print(f"  [Source] Starting stream generation (count={self.count})")

            async for item in input_stream:
                print(f"  [Source] Received: {item}")

                # Yield multiple outputs per input
                for i in range(self.count):
                    output = f"{item}_chunk_{i+1}"
                    print(f"  [Source] → Yielding: {output}")
                    yield output

            print(f"  [Source] Stream complete")

    # Streaming Transform Node - processes each chunk
    class StreamingTransformNode(Node):
        """Node that transforms streaming data."""

        def __init__(self, prefix="[T]", **kwargs):
            super().__init__(**kwargs)
            self.prefix = prefix
            self.is_streaming = True
            self.processed_count = 0

        async def process(self, input_stream):
            """Transform each item in stream."""
            print(f"  [Transform] Starting transform (prefix='{self.prefix}')")

            async for item in input_stream:
                self.processed_count += 1
                output = f"{self.prefix} {item}"
                print(f"  [Transform] {item} → {output}")
                yield output

            print(f"  [Transform] Processed {self.processed_count} items")

    # Create streaming pipeline with instances
    print("✓ Creating streaming pipeline with Node instances:")
    source = StreamingSourceNode(name="source", count=2)
    transform = StreamingTransformNode(name="transform", prefix="[STREAM]")

    print(f"  - {source.name}: generates {source.count} outputs per input")
    print(f"  - {transform.name}: transforms with prefix '{transform.prefix}'")
    print()

    # Create pipeline
    pipeline = Pipeline(name="streaming-demo")
    pipeline.add_node(source)
    pipeline.add_node(transform)

    print("→ Executing streaming pipeline...")
    print()

    # Execute with streaming input
    async def input_generator():
        """Generate streaming inputs."""
        inputs = ["input1", "input2"]
        for inp in inputs:
            print(f"  [Input] Feeding: {inp}")
            yield inp

    # Process streaming pipeline
    results = []
    async with pipeline.managed_execution():
        async for output in pipeline.process(input_generator()):
            results.append(output)
            print(f"  [Output] Received: {output}")

    print()
    print(f"✓ Streaming complete!")
    print(f"✓ Total outputs received: {len(results)}")
    print(f"✓ Results: {results}")
    print()
    print("✅ Streaming input/output with instances works!")


if __name__ == "__main__":
    asyncio.run(main())
