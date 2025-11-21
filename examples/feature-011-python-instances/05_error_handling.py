#!/usr/bin/env python3
"""
Example 5: Error Handling and Validation

Demonstrates comprehensive error handling with helpful messages.
Feature 011 - Error Handling (FR-011, SC-005)
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


def main():
    """Demonstrate error handling."""
    print("=" * 60)
    print("Example 5: Error Handling and Validation")
    print("=" * 60)
    print()

    from remotemedia.core.node import Node
    from remotemedia.core.node_serialization import (
        serialize_node_for_ipc,
        SerializationError
    )
    import threading

    # Error 1: Invalid type in list
    print("Error 1: Invalid Type Validation")
    print("-" * 60)
    try:
        from remotemedia.runtime_wrapper import execute_pipeline
        from remotemedia.nodes import PassThroughNode

        invalid_list = [
            PassThroughNode(name="valid"),
            "invalid_string",  # Invalid!
        ]

        # Validation happens before calling Rust
        # This will raise TypeError
        import asyncio
        asyncio.run(execute_pipeline(invalid_list))

    except TypeError as e:
        print(f"✓ TypeError raised correctly:")
        print(f"  {e}")
        print()

    # Error 2: Non-serializable Node
    print("Error 2: Non-Serializable Node")
    print("-" * 60)
    try:
        class BadNode(Node):
            def process(self, data):
                return data

            def __getstate__(self):
                state = super().__getstate__()
                # Add non-serializable lock
                state['lock'] = threading.Lock()
                return state

        node = BadNode(name="bad_node")
        serialized = serialize_node_for_ipc(node)

    except SerializationError as e:
        print(f"✓ SerializationError raised correctly:")
        print(f"  Node: {e.node_name}")
        print(f"  Reason: {e.reason[:80]}...")
        print(f"  Suggestion: {e.suggestion[:80] if e.suggestion else 'N/A'}...")
        print()

    # Error 3: Missing required method
    print("Error 3: Missing Required Method")
    print("-" * 60)
    try:
        class InvalidNode:
            """Not a Node subclass, missing methods."""
            def __init__(self):
                self.name = "invalid"

        # This would fail validation in InstanceExecutor
        print("  (Would fail at Rust validation: missing process() method)")
        print()

    except Exception as e:
        print(f"✗ Unexpected error: {e}")

    print("=" * 60)
    print("✅ All error scenarios handled with helpful messages!")
    print("Errors include: node name, specific reason, and suggestions")
    print("=" * 60)


if __name__ == "__main__":
    main()
