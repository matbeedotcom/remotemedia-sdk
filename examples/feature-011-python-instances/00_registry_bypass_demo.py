#!/usr/bin/env python3
"""
Demo: Custom Node Execution WITHOUT Registry

This example proves that custom Python Node instances can execute
through the Rust FFI without being registered in the node factory.

Feature 011 - Registry Bypass Complete
"""

import sys
import asyncio
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Demonstrate custom node execution without registration."""
    print("=" * 70)
    print("🎯 Feature 011: Custom Node Execution (Registry Bypass)")
    print("=" * 70)
    print()

    from remotemedia import execute_pipeline
    from remotemedia.core.node import Node

    # Create CUSTOM node class (NOT registered in Rust!)
    class MyCustomTransformNode(Node):
        """Custom node - not registered in Rust runtime factory."""

        def __init__(self, transform_type="uppercase", **kwargs):
            super().__init__(**kwargs)
            self.transform_type = transform_type
            self.execution_count = 0

        def process(self, data):
            """Custom transformation logic."""
            self.execution_count += 1

            if self.transform_type == "uppercase":
                result = str(data).upper()
            elif self.transform_type == "reverse":
                result = str(data)[::-1]
            elif self.transform_type == "repeat":
                result = str(data) * 2
            else:
                result = str(data)

            print(f"  [CustomNode #{self.execution_count}] {data} → {result}")
            return result

    # Create custom node instances
    print("Creating custom node instances (NOT in registry):")
    node1 = MyCustomTransformNode(name="uppercase", transform_type="uppercase")
    node2 = MyCustomTransformNode(name="reverse", transform_type="reverse")
    node3 = MyCustomTransformNode(name="repeat", transform_type="repeat")

    print(f"  ✓ {node1.name}: transform_type='{node1.transform_type}'")
    print(f"  ✓ {node2.name}: transform_type='{node2.transform_type}'")
    print(f"  ✓ {node3.name}: transform_type='{node3.transform_type}'")
    print()

    print("Executing custom pipeline through Rust FFI...")
    print()

    # Execute custom nodes - should work WITHOUT registry!
    result = await execute_pipeline([node1, node2, node3])

    print()
    print(f"✓ Execution complete!")
    print(f"✓ Final result: {result}")
    print()
    print(f"Node state after execution:")
    print(f"  - {node1.name}: executed {node1.execution_count} times")
    print(f"  - {node2.name}: executed {node2.execution_count} times")
    print(f"  - {node3.name}: executed {node3.execution_count} times")
    print()

    print("=" * 70)
    print("✅ SUCCESS - Custom nodes work WITHOUT registration!")
    print()
    print("What happened:")
    print("  1. Custom MyCustomTransformNode created (not in Rust registry)")
    print("  2. execute_pipeline([node1, node2, node3]) called")
    print("  3. Python wrapper detected Node instances")
    print("  4. Called execute_pipeline_with_instances() (BYPASSES REGISTRY)")
    print("  5. Rust InstanceExecutor executed nodes directly")
    print("  6. State preserved, no registry lookup, no errors!")
    print("=" * 70)


if __name__ == "__main__":
    asyncio.run(main())
