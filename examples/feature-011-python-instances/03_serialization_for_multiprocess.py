#!/usr/bin/env python3
"""
Example 3: Node Instance Serialization for Multiprocess

Demonstrates cloudpickle serialization with cleanup()/initialize() lifecycle.
Feature 011 - User Story 3
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


def main():
    """Serialization example."""
    print("=" * 60)
    print("Example 3: Node Instance Serialization for Multiprocess")
    print("=" * 60)
    print()

    from remotemedia.core.node import Node
    from remotemedia.core.node_serialization import (
        serialize_node_for_ipc,
        deserialize_node_from_ipc
    )

    # Create a node with state
    class StatefulWorkerNode(Node):
        def __init__(self, config_value="default", **kwargs):
            super().__init__(**kwargs)
            self.config_value = config_value
            self.processed_items = []
            self.resource = None

        def initialize(self):
            """Load resources."""
            super().initialize()
            self.resource = f"Resource for {self.config_value}"
            print(f"  [initialize] Loaded resource: {self.resource}")

        def process(self, data):
            """Process with state."""
            self.processed_items.append(data)
            return f"{data}_{self.config_value}_{len(self.processed_items)}"

        def cleanup(self):
            """Release resources."""
            print(f"  [cleanup] Releasing resource: {self.resource}")
            self.resource = None
            super().cleanup()

    # Parent process: create node
    node = StatefulWorkerNode(name="worker", config_value="production")
    node.processed_items = ["item1", "item2"]  # Some existing state

    print(f"✓ Created node in parent process:")
    print(f"  - config_value: {node.config_value}")
    print(f"  - processed_items: {node.processed_items}")
    print()

    # Serialize for IPC transfer
    print("→ Serializing node for IPC...")
    serialized_bytes = serialize_node_for_ipc(node)

    size_kb = len(serialized_bytes) / 1024
    print(f"✓ Serialized to {size_kb:.2f} KB")
    print(f"✓ cleanup() was called automatically")
    print()

    # Simulate IPC transfer to subprocess
    print("→ Deserializing in subprocess...")
    restored_node = deserialize_node_from_ipc(serialized_bytes)

    print(f"✓ Deserialized node: {restored_node.name}")
    print(f"✓ State preserved:")
    print(f"  - config_value: {restored_node.config_value}")
    print(f"  - processed_items: {restored_node.processed_items}")
    print(f"✓ initialize() was called automatically")
    print()

    # Verify functionality
    result = restored_node.process("new_data")
    print(f"✓ Node is functional: {result}")
    print()
    print("✅ Serialization complete - ready for multiprocess execution!")


if __name__ == "__main__":
    main()
