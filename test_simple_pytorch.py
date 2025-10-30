"""Test script for SimplePyTorchNode via gRPC"""
import sys
sys.path.insert(0, 'nodejs-client/src')

from remotemedia_client import RemoteMediaClient

def test_simple_pytorch():
    """Test the SimplePyTorchNode"""
    print("=" * 60)
    print("Testing SimplePyTorchNode (minimal PyTorch test)")
    print("=" * 60)

    client = RemoteMediaClient("localhost:50051")

    # Define a simple pipeline with just the SimplePyTorchNode
    pipeline = {
        "nodes": {
            "test": {
                "type": "SimplePyTorchNode",
                "params": {}
            }
        },
        "connections": []
    }

    # Send some dummy text input
    input_data = {
        "type": "text",
        "content": "test"
    }

    print("\nSending request...")
    try:
        for i, chunk in enumerate(client.stream_pipeline(pipeline, input_data)):
            print(f"\nReceived chunk {i+1}:")
            print(f"  Type: {chunk.get('type', 'unknown')}")
            if 'data' in chunk:
                print(f"  Data keys: {list(chunk['data'].keys())}")
                if '_message' in chunk['data']:
                    print(f"  Message: {chunk['data']['_message']}")

        print("\n" + "=" * 60)
        print("Test PASSED - No heap corruption!")
        print("=" * 60)

    except Exception as e:
        print(f"\nTest FAILED with error: {e}")
        print("=" * 60)
        import traceback
        traceback.print_exc()
        return False

    return True

if __name__ == "__main__":
    success = test_simple_pytorch()
    sys.exit(0 if success else 1)
