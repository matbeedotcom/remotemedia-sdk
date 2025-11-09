"""
Test SimplePyTorchNode via Rust runtime (no gRPC, direct call)
This tests if PyTorch works when called through PyO3/Rust.
"""
import asyncio
import sys

# Test the Rust runtime binding directly
sys.path.insert(0, 'python-client')

try:
    from remotemedia_runtime.runtime_data import RuntimeData
    print("[OK] RuntimeData imported successfully")
except ImportError as e:
    print(f"[ERROR] Failed to import RuntimeData: {e}")
    sys.exit(1)

from remotemedia.nodes.simple_pytorch_test import SimplePyTorchNode


async def test_pytorch_via_rust_runtime():
    """
    Test PyTorch node when its process() method is called through
    the Rust CPythonNodeExecutor (simulating what happens in gRPC server)
    """
    print("=" * 60)
    print("Test: SimplePyTorchNode via Rust Runtime")
    print("=" * 60)

    # Create the node
    node = SimplePyTorchNode(node_id="test")

    # Initialize
    print("\n1. Initializing node...")
    await node.initialize()
    print("   [OK] Node initialized")

    # Create RuntimeData input (using real Rust bindings)
    print("\n2. Creating RuntimeData.Text input...")
    input_data = RuntimeData.text("test input")
    print(f"   [OK] Created RuntimeData: {type(input_data)}")

    # Call process() - this should work if our dict approach is correct
    print("\n3. Calling process() (returns dict, not RuntimeData)...")
    chunk_count = 0

    try:
        async for result in node.process(input_data):
            chunk_count += 1
            print(f"\n   Received chunk {chunk_count}:")
            print(f"     Type: {type(result)}")
            print(f"     Keys: {list(result.keys())}")

            if '_message' in result:
                print(f"     Message: {result['_message']}")
            if '_test_result' in result:
                import numpy as np
                if isinstance(result['_test_result'], np.ndarray):
                    print(f"     Result: numpy array, shape {result['_test_result'].shape}")
                else:
                    print(f"     Result: {type(result['_test_result'])}")

        print(f"\n   [OK] Processing complete - received {chunk_count} chunks")

    except Exception as e:
        print(f"\n   [ERROR] Error during processing: {e}")
        import traceback
        traceback.print_exc()
        return False

    # Cleanup
    print("\n4. Cleaning up...")
    await node.cleanup()
    print("   [OK] Cleanup complete")

    print("\n" + "=" * 60)
    if chunk_count > 0:
        print("SUCCESS - PyTorch works when called with RuntimeData!")
        print("The node returns dicts (not RuntimeData) from async generator")
    else:
        print("FAILURE - No chunks received")
    print("=" * 60)

    return chunk_count > 0


if __name__ == "__main__":
    success = asyncio.run(test_pytorch_via_rust_runtime())
    sys.exit(0 if success else 1)
