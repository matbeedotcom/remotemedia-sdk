#!/usr/bin/env python3
"""
Example 1: Basic Node Instance Execution

Demonstrates passing Node instances directly to execute_pipeline().
Feature 011 - User Story 1
"""

import sys
import asyncio
from pathlib import Path

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Basic example of instance execution."""
    print("=" * 60)
    print("Example 1: Basic Node Instance Execution")
    print("=" * 60)
    print()

    from remotemedia import execute_pipeline
    from remotemedia.core.node import Node

    # Create a simple custom node with state
    class GreetingNode(Node):
        def __init__(self, greeting="Hello", **kwargs):
            super().__init__(**kwargs)
            self.greeting = greeting
            self.count = 0

        def process(self, name):
            self.count += 1
            return f"{self.greeting}, {name}! (call #{self.count})"

    # Create node instance
    node = GreetingNode(name="greeter", greeting="Welcome")
    print(f"✓ Created node: {node.name}")
    print(f"✓ Initial state: greeting='{node.greeting}', count={node.count}")
    print()

    # Execute with Node instance directly
    print("→ Calling execute_pipeline([node])...")
    result = await execute_pipeline([node])

    print(f"✓ Result: {result}")
    print(f"✓ State after: count={node.count}")
    print()
    print("✅ Instance execution complete - state was preserved!")


if __name__ == "__main__":
    asyncio.run(main())
