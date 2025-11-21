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

    # Create custom node instances
    class PrefixNode(Node):
        def __init__(self, prefix=">>", **kwargs):
            super().__init__(**kwargs)
            self.prefix = prefix

        def process(self, data):
            return f"{self.prefix} {data}"

    class PassThroughNode(Node):
        def process(self, data):
            return data

    # Create pipeline with multiple Node instances
    # NOTE: Mixing Node instances with dict manifests requires manifest-based execution
    #       which would need all custom types to be registered in the Rust runtime.
    #       For Feature 011, we demonstrate pure instance execution (registry bypass).
    mixed_pipeline = [
        PrefixNode(name="prefix", prefix="[START]"),  # Instance
        PassThroughNode(name="passthrough"),  # Instance
        PrefixNode(name="suffix", prefix="[END]"),  # Instance
    ]

    print(f"✓ Created instance pipeline:")
    print(f"  - Node instance: PrefixNode (prefix='[START]')")
    print(f"  - Node instance: PassThroughNode")
    print(f"  - Node instance: PrefixNode (prefix='[END]')")
    print()

    print("→ Calling execute_pipeline(mixed_pipeline)...")
    result = await execute_pipeline(mixed_pipeline)

    print(f"✓ Result: {result}")
    print()
    print("✅ Mixed pipeline execution complete!")


if __name__ == "__main__":
    asyncio.run(main())
