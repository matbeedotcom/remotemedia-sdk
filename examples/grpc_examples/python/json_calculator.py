#!/usr/bin/env python3
"""
JSON Calculator Example

Demonstrates JSON data streaming through CalculatorNode.
Sends JSON operations and receives JSON results.
"""

import sys
import os
import grpc
import time
import json

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "..", "python-grpc-client"))

from generated.execution_pb2 import (
    CreatePipelineRequest,
    ExecuteRequest,
)
from generated.streaming_pb2 import (
    StreamRequest,
    DataChunk,
)
from generated.common_pb2 import (
    PipelineManifest,
    NodeManifest,
    DataBuffer,
    JsonData,
)
from generated.execution_pb2_grpc import ExecutionServiceStub


def create_calculator_pipeline(stub: ExecutionServiceStub) -> str:
    """Create pipeline with CalculatorNode"""

    manifest = PipelineManifest(
        version="v1",
        metadata={
            "name": "json_calculator_pipeline",
            "description": "JSON-based calculator demo",
            "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        },
        nodes=[
            NodeManifest(
                id="calculator",
                node_type="CalculatorNode",
                params='{}',
                is_streaming=True,
            )
        ],
        connections=[],
    )

    response = stub.CreatePipeline(CreatePipelineRequest(manifest=manifest))
    print(f"✅ Created pipeline: {response.pipeline_id}")
    return response.pipeline_id


def create_calculation(operation: str, a: float, b: float) -> JsonData:
    """Create a JSON calculation request"""

    payload = {
        "operation": operation,
        "operands": [a, b],
        "timestamp": time.time(),
    }

    return JsonData(
        json_payload=json.dumps(payload),
        schema_type="calculation_request",
    )


def stream_calculations(stub: ExecutionServiceStub, pipeline_id: str):
    """Stream various calculations through the pipeline"""

    calculations = [
        ("add", 10, 5),
        ("subtract", 20, 7),
        ("multiply", 6, 7),
        ("divide", 100, 4),
        ("add", 3.14, 2.86),
        ("multiply", 1.5, 2.0),
    ]

    def generate_requests():
        for seq, (op, a, b) in enumerate(calculations):
            json_data = create_calculation(op, a, b)

            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="calculator",
                    buffer=DataBuffer(json=json_data),
                    sequence=seq,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )

            print(f"📤 Sent: {op}({a}, {b})")
            time.sleep(0.1)

    print(f"\n🧮 Streaming {len(calculations)} calculations...\n")

    result_count = 0

    for response in stub.StreamData(generate_requests()):
        # Check if we got JSON output
        if response.HasField("data_chunk"):
            chunk = response.data_chunk
            if chunk.HasField("buffer") and chunk.buffer.HasField("json"):
                json_result = chunk.buffer.json
                result = json.loads(json_result.json_payload)

                result_count += 1
                print(f"✅ Result {result_count}: {json.dumps(result, indent=2)}")

        if response.HasField("metrics"):
            print(f"   ⏱️  Processing time: {response.metrics.processing_time_ms:.2f}ms")

    print(f"\n📊 Summary:")
    print(f"   Calculations sent: {len(calculations)}")
    print(f"   Results received: {result_count}")


def main():
    """Run JSON calculator demo"""

    print("🧮 JSON Calculator Demo")
    print("=" * 60)

    channel = grpc.insecure_channel("localhost:50051")
    stub = ExecutionServiceStub(channel)

    try:
        # Create pipeline
        pipeline_id = create_calculator_pipeline(stub)

        # Stream calculations
        stream_calculations(stub, pipeline_id)

        print("\n" + "=" * 60)
        print("✅ JSON Calculator Demo Complete!\n")
        print("💡 This demo showed:")
        print("   - Creating CalculatorNode")
        print("   - Streaming JSON data")
        print("   - JSON schema validation (schema_type field)")
        print("   - Structured data processing")

    except grpc.RpcError as e:
        print(f"❌ gRPC error: {e.code()}: {e.details()}")
        sys.exit(1)
    finally:
        channel.close()


if __name__ == "__main__":
    main()
