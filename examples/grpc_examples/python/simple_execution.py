#!/usr/bin/env python3
"""
Simple ExecutePipeline example.

Demonstrates:
- Connecting to the gRPC service
- Checking version compatibility
- Executing a simple calculator pipeline
- Handling results and errors
"""

import asyncio
import sys
from pathlib import Path

# Add python-grpc-client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent / "python-grpc-client"))

from remotemedia_client import RemoteMediaClient, RemoteMediaError


async def main():
    # Create client (no auth for local development)
    client = RemoteMediaClient("localhost:50051")
    
    try:
        # Connect
        print("Connecting to gRPC service...")
        await client.connect()
        
        # Check version
        version = await client.get_version()
        print(f"✅ Connected to service v{version.protocol_version}")
        print(f"   Runtime version: {version.runtime_version}")
        print(f"   Supported nodes: {', '.join(version.supported_node_types[:5])}...")
        
        # Create simple calculator pipeline
        print("\n=== Executing Calculator Pipeline ===")
        manifest = {
            "version": "v1",
            "metadata": {
                "name": "simple_calculator",
                "description": "Add 5 to input value",
                "created_at": "2025-10-28T00:00:00Z"
            },
            "nodes": [
                {
                    "id": "calc",
                    "node_type": "CalculatorNode",
                    "params": '{"operation": "add", "value": 5.0}',
                    "is_streaming": False
                }
            ],
            "connections": []
        }
        
        # Execute pipeline
        result = await client.execute_pipeline(
            manifest=manifest,
            audio_inputs={},
            data_inputs={"calc": '{"value": 10.0}'}
        )
        
        # Display result
        print(f"✅ Execution successful")
        print(f"   Input: 10.0")
        print(f"   Operation: add 5.0")
        print(f"   Result: {result.data_outputs['calc']}")
        print(f"   Wall time: {result.metrics.wall_time_ms:.2f}ms")
        
        # Try multiplication
        print("\n=== Executing Multiply Pipeline ===")
        manifest["nodes"][0]["params"] = '{"operation": "multiply", "value": 3.0}'
        manifest["metadata"]["description"] = "Multiply by 3"
        
        result = await client.execute_pipeline(
            manifest=manifest,
            audio_inputs={},
            data_inputs={"calc": '{"value": 7.0}'}
        )
        
        print(f"✅ Execution successful")
        print(f"   Input: 7.0")
        print(f"   Operation: multiply 3.0")
        print(f"   Result: {result.data_outputs['calc']}")
        print(f"   Wall time: {result.metrics.wall_time_ms:.2f}ms")
        
        print("\n✅ All tests passed!")
        
    except RemoteMediaError as e:
        print(f"\n❌ Error: {e.message}")
        if e.error_type:
            print(f"   Error type: {e.error_type.name}")
        if e.failing_node_id:
            print(f"   Failing node: {e.failing_node_id}")
        sys.exit(1)
        
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
