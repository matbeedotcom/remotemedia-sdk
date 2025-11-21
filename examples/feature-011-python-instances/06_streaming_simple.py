#!/usr/bin/env python3
"""
Example 6: Simple Streaming with execute_pipeline_with_input

Demonstrates streaming multiple inputs through Node instances.
Feature 011 - Batch Processing
"""

import sys
import asyncio
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Simple streaming example using execute_pipeline_with_input."""
    print("=" * 70)
    print("Example 6: Streaming Multiple Inputs with Node Instances")
    print("=" * 70)
    print()

    from remotemedia import execute_pipeline_with_input
    from remotemedia.core.node import Node

    # Stateful streaming processor
    class StreamingProcessorNode(Node):
        """Processes stream of inputs with state."""

        def __init__(self, multiplier=1, **kwargs):
            super().__init__(**kwargs)
            self.multiplier = multiplier
            self.processed_count = 0
            self.running_total = 0

        def process(self, data):
            """Process each input, updating state."""
            self.processed_count += 1
            value = int(data) if isinstance(data, (int, str)) else data
            result = value * self.multiplier
            self.running_total += result

            print(f"  [Processor] Input #{self.processed_count}: {data} × {self.multiplier} = {result}")
            print(f"  [Processor] Running total: {self.running_total}")

            return result

    # Create node instance
    processor = StreamingProcessorNode(name="processor", multiplier=3)

    print(f"✓ Created streaming processor:")
    print(f"  - multiplier: {processor.multiplier}")
    print(f"  - Initial state: processed_count={processor.processed_count}, total={processor.running_total}")
    print()

    # Stream of inputs
    input_stream = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]

    print(f"→ Processing stream of {len(input_stream)} inputs...")
    print()

    # Execute with streaming inputs
    results = await execute_pipeline_with_input([processor], input_stream)

    print()
    print(f"✓ Stream processing complete!")
    print(f"✓ Results: {results}")
    print()
    print(f"Final Node State (preserved across all inputs):")
    print(f"  - processed_count: {processor.processed_count}")
    print(f"  - running_total: {processor.running_total}")
    print(f"  - multiplier: {processor.multiplier}")
    print()
    print("=" * 70)
    print("✅ Streaming with state preservation works!")
    print()
    print("Key Features:")
    print("  ✓ State persists across ALL inputs in stream")
    print("  ✓ Node instance accessible after execution")
    print("  ✓ Statistics can be extracted from node state")
    print("=" * 70)


if __name__ == "__main__":
    asyncio.run(main())
