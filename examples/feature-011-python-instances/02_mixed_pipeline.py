#!/usr/bin/env python3
"""
Example 2: Mixed Manifest and Instance Pipelines

Demonstrates mixing Node instances with dict manifests.
Feature 011 - User Story 2
"""

import sys
import asyncio
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Mixed pipeline example."""
    print("=" * 60)
    print("Example 2: Mixed Manifest and Instance Pipelines")
    print("=" * 60)
    print()

    from remotemedia import execute_pipeline
    from remotemedia.core.node import Node

    # Create custom node instance
    class PrefixNode(Node):
        def __init__(self, prefix=">>", **kwargs):
            super().__init__(**kwargs)
            self.prefix = prefix

        def process(self, data):
            return f"{self.prefix} {data}"

    # Mix Node instances with dict manifests
    mixed_pipeline = [
        PrefixNode(name="prefix", prefix="[START]"),  # Instance
        {
            "node_type": "PassThrough",  # Dict manifest
            "params": {}
        },
        PrefixNode(name="suffix", prefix="[END]"),  # Instance
    ]

    print(f"✓ Created mixed pipeline:")
    print(f"  - Node instance: PrefixNode (prefix='[START]')")
    print(f"  - Dict manifest: PassThrough")
    print(f"  - Node instance: PrefixNode (prefix='[END]')")
    print()

    print("→ Calling execute_pipeline(mixed_pipeline)...")
    result = await execute_pipeline(mixed_pipeline)

    print(f"✓ Result: {result}")
    print()
    print("✅ Mixed pipeline execution complete!")


if __name__ == "__main__":
    asyncio.run(main())
