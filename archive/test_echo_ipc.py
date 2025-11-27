#!/usr/bin/env python3
"""
Standalone test for IPC echo communication.

This script tests the Rust-to-Python IPC roundtrip without needing the full gRPC server.
"""

import asyncio
import sys
import logging

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)

async def test_echo_node():
    """Test the EchoNode receives and echoes data."""
    from remotemedia.core.multiprocessing import get_node_class

    # Get the EchoNode class
    try:
        EchoNode = get_node_class("EchoNode")
        print(f"[OK] EchoNode class found: {EchoNode}")
    except KeyError as e:
        print(f"[FAIL] Failed to find EchoNode: {e}")
        return False

    # Create node instance
    node = EchoNode("test_echo")
    print(f"[OK] Created EchoNode instance")

    # Initialize
    await node.initialize()
    print(f"[OK] Initialized node")

    # Test process method directly (not via IPC yet)
    try:
        from remotemedia.core.multiprocessing.data import RuntimeData
        test_input = RuntimeData.text("Hello from test!")
        output = await node.process(test_input)
        print(f"[OK] Direct process call returned output")
        if output and output.is_text():
            echo_text = output.as_text()
            print(f"[OK] Echo text: {echo_text}")
            assert "Echo" in echo_text, "Output should contain 'Echo'"
    except Exception as e:
        print(f"[FAIL] Process failed: {e}")
        import traceback
        traceback.print_exc()
        return False

    # Cleanup
    await node.cleanup()
    print(f"[OK] Node cleaned up")

    return True

if __name__ == "__main__":
    success = asyncio.run(test_echo_node())
    sys.exit(0 if success else 1)
