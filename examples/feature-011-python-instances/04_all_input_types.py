#!/usr/bin/env python3
"""
Example 4: All Supported Input Types

Demonstrates all input types supported by execute_pipeline().
Feature 011 - Complete API Surface
"""

import sys
import asyncio
import json
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Demonstrate all input types."""
    print("=" * 60)
    print("Example 4: All Supported Input Types")
    print("=" * 60)
    print()

    from remotemedia import execute_pipeline
    from remotemedia.core.pipeline import Pipeline
    from remotemedia.core.node import Node
    from remotemedia.nodes import PassThroughNode

    # Custom node for demo
    class EchoNode(Node):
        def process(self, data):
            return f"Echo: {data}"

    print("Testing all input types to execute_pipeline():")
    print()

    # Type 1: Pipeline instance
    print("1. Pipeline Instance")
    pipeline = Pipeline(name="demo")
    pipeline.add_node(EchoNode(name="echo"))
    result1 = await execute_pipeline(pipeline)
    print(f"   ✓ Pipeline instance: {result1}")
    print()

    # Type 2: List of Node instances
    print("2. List of Node Instances")
    nodes = [EchoNode(name="echo1"), EchoNode(name="echo2")]
    result2 = await execute_pipeline(nodes)
    print(f"   ✓ List[Node]: {result2}")
    print()

    # Type 3: Mixed list (Node + dict)
    print("3. Mixed List (Node + dict)")
    mixed = [
        EchoNode(name="echo"),
        {"node_type": "PassThrough", "params": {}}
    ]
    result3 = await execute_pipeline(mixed)
    print(f"   ✓ Mixed list: {result3}")
    print()

    # Type 4: Dict manifest
    print("4. Dict Manifest")
    manifest_dict = {
        "version": "v1",
        "metadata": {"name": "dict-demo"},
        "nodes": [
            {"id": "echo_0", "node_type": "PassThrough", "params": {}}
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
    print("✅ All 5 input types work correctly!")
    print("Feature 011 provides complete flexibility in API usage")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(main())
