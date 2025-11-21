#!/usr/bin/env python3
"""
Example 4: All Supported Input Types with Node Instances

Demonstrates different ways to execute pipelines with Node instances.
Feature 011 - Direct Instance Execution
"""

import sys
import asyncio
import json
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Demonstrate all input types for instance execution."""
    print("=" * 60)
    print("Example 4: All Supported Input Types")
    print("=" * 60)
    print()

    from remotemedia import execute_pipeline, execute_pipeline_with_input
    from remotemedia.core.node import Node

    # Custom node for demo
    class EchoNode(Node):
        def __init__(self, count=1, **kwargs):
            super().__init__(**kwargs)
            self.count = count
            self.executions = 0

        def process(self, data):
            self.executions += 1
            return f"Echo #{self.executions}: {data}"

    print("Testing input types for instance execution:")
    print()

    # Type 1: Single Node instance (as list)
    print("1. Single Node Instance (List)")
    single_node = [EchoNode(name="echo")]
    result1 = await execute_pipeline(single_node)
    print(f"   ✓ Single instance: {result1}")
    print()

    # Type 2: Multiple Node instances in sequence
    print("2. Multiple Node Instances (Chained)")
    nodes = [
        EchoNode(name="echo1", count=1),
        EchoNode(name="echo2", count=2)
    ]
    result2 = await execute_pipeline(nodes)
    print(f"   ✓ Chained instances: {result2}")
    print()

    # Type 3: Instance execution with streaming input
    print("3. Instance with Streaming Input")
    processor = EchoNode(name="processor")
    results3 = await execute_pipeline_with_input(
        [processor],
        ["input1", "input2", "input3"]
    )
    print(f"   ✓ Streaming results: {results3}")
    print(f"   ✓ State preserved: processed {processor.executions} items")
    print()

    # Type 4: For registered nodes, manifest still works
    print("4. Dict Manifest (Registered Nodes Only)")
    manifest_dict = {
        "version": "v1",
        "metadata": {"name": "dict-demo"},
        "nodes": [
            {"id": "pass_0", "node_type": "PassThrough", "params": {}}
        ],
        "connections": []
    }
    result4 = await execute_pipeline(manifest_dict)
    print(f"   ✓ Dict manifest: {result4}")
    print()

    # Type 5: JSON string (backward compatible)
    print("5. JSON String (Backward Compatible)")
    manifest_json = json.dumps(manifest_dict)
    result5 = await execute_pipeline(manifest_json)
    print(f"   ✓ JSON string: {result5}")
    print()

    print("=" * 60)
    print("✅ All input types work correctly!")
    print()
    print("Key Features:")
    print("  ✓ Custom Node instances execute without registration")
    print("  ✓ State preserved across streaming inputs")
    print("  ✓ Backward compatible with manifest-based execution")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(main())
