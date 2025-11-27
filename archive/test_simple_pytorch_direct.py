"""Direct gRPC test for SimplePyTorchNode"""
import grpc
import sys
import json

# Add the runtime proto path
sys.path.insert(0, 'runtime/src/grpc_service')

try:
    from generated import remotemedia_pb2, remotemedia_pb2_grpc
except ImportError:
    print("ERROR: Could not import generated protobuf files")
    print("You may need to run: python -m grpc_tools.protoc ...")
    sys.exit(1)


def test_simple_pytorch():
    """Test SimplePyTorchNode via direct gRPC call"""
    print("=" * 60)
    print("Testing SimplePyTorchNode (minimal PyTorch test)")
    print("=" * 60)

    # Connect to gRPC server
    channel = grpc.insecure_channel('localhost:50051')
    stub = remotemedia_pb2_grpc.RemoteMediaServiceStub(channel)

    # Create pipeline definition
    pipeline_def = {
        "nodes": [
            {
                "id": "test",
                "type": "SimplePyTorchNode",
                "params": {}
            }
        ],
        "connections": []
    }

    # Create input data (text)
    input_chunk = remotemedia_pb2.DataChunk(
        text_buffer=remotemedia_pb2.TextBuffer(
            text="dummy test input"
        )
    )

    # Create stream request
    request = remotemedia_pb2.StreamPipelineRequest(
        pipeline_json=json.dumps(pipeline_def),
        input_data=input_chunk
    )

    print("\nSending gRPC request...")
    print(f"Pipeline: {pipeline_def}")

    try:
        # Call the streaming RPC
        response_stream = stub.StreamPipeline(request)

        chunk_count = 0
        for response in response_stream:
            chunk_count += 1
            print(f"\nReceived chunk {chunk_count}:")
            print(f"  Session ID: {response.session_id}")

            if response.HasField('data'):
                print(f"  Has data field")
                # Try to decode the data
                if response.data.HasField('text_buffer'):
                    print(f"  Text: {response.data.text_buffer.text}")
                elif response.data.HasField('json_data'):
                    print(f"  JSON: {response.data.json_data.json}")

            if response.error:
                print(f"  ERROR: {response.error}")
                return False

        print("\n" + "=" * 60)
        print(f"Test COMPLETED - Received {chunk_count} chunks")
        if chunk_count > 0:
            print("SUCCESS - No heap corruption!")
        else:
            print("WARNING - No chunks received")
        print("=" * 60)
        return chunk_count > 0

    except grpc.RpcError as e:
        print(f"\ngRPC Error: {e.code()}")
        print(f"Details: {e.details()}")
        return False
    except Exception as e:
        print(f"\nTest FAILED with error: {e}")
        import traceback
        traceback.print_exc()
        return False


if __name__ == "__main__":
    success = test_simple_pytorch()
    sys.exit(0 if success else 1)
