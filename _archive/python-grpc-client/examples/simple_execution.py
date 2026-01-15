#!/usr/bin/env python3
"""
Simple ExecutePipeline example.

Demonstrates:
- Connecting to Rust gRPC service
- Checking service version
- Executing a simple CalculatorNode pipeline
- Handling results
"""

import asyncio
import sys
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia_client import RemoteMediaClient, RemoteMediaError


async def main():
    # Create client
    client = RemoteMediaClient("localhost:50051")
    
    try:
        # Connect to service
        print("Connecting to service...")
        await client.connect()
        
        # Get version info
        print("\n=== Service Version ===")
        version = await client.get_version()
        print(f"Protocol: {version.protocol_version}")
        print(f"Runtime: {version.runtime_version}")
        print(f"Build: {version.build_timestamp}")
        print(f"Supported nodes: {len(version.supported_node_types)}")
        for node_type in version.supported_node_types:
            print(f"  - {node_type}")
        
        # Create simple calculator pipeline
        print("\n=== Executing Pipeline ===")
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
        
        # Execute with data input
        print("Input: 10.0")
        print("Operation: add 5.0")
        
        result = await client.execute_pipeline(
            manifest=manifest,
            audio_inputs={},
            data_inputs={"calc": '{"value": 10.0}'}
        )
        
        # Display results
        print(f"\n=== Results ===")
        print(f"Status: {result.status}")
        print(f"Output: {result.data_outputs.get('calc', 'N/A')}")
        
        # Display metrics
        print(f"\n=== Metrics ===")
        print(f"Wall time: {result.metrics.wall_time_ms:.2f}ms")
        print(f"CPU time: {result.metrics.cpu_time_ms:.2f}ms")
        print(f"Memory: {result.metrics.memory_used_bytes / 1024:.1f} KB")
        print(f"Serialization: {result.metrics.serialization_time_ms:.2f}ms")
        
        # Test multiple operations
        print("\n=== Testing Multiple Operations ===")
        operations = [
            ("add", 10.0, 3.0),
            ("subtract", 10.0, 3.0),
            ("multiply", 10.0, 3.0),
            ("divide", 10.0, 2.0),
        ]
        
        for op, val1, val2 in operations:
            manifest["nodes"][0]["params"] = f'{{"operation": "{op}", "value": {val2}}}'
            result = await client.execute_pipeline(
                manifest=manifest,
                audio_inputs={},
                data_inputs={"calc": f'{{"value": {val1}}}'}
            )
            output = result.data_outputs.get("calc", "N/A")
            print(f"{val1} {op} {val2} = {output}")
        
        print("\n✅ All tests passed!")
        
    except RemoteMediaError as e:
        print(f"\n❌ Error: {e}")
        if e.error_type:
            print(f"Error type: {e.error_type.name}")
        if e.failing_node_id:
            print(f"Failing node: {e.failing_node_id}")
        if e.context:
            print(f"Context: {e.context}")
        sys.exit(1)
    
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
