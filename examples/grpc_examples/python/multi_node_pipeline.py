#!/usr/bin/env python3
"""
Multi-node pipeline example.

Demonstrates:
- Chaining multiple nodes together
- Using connections to pass data between nodes
- Processing audio through multiple stages
"""

import asyncio
import sys
import struct
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent / "python-grpc-client"))

from remotemedia_client import (
    RemoteMediaClient,
    RemoteMediaError,
    AudioBuffer,
    AudioFormat
)


async def main():
    client = RemoteMediaClient("localhost:50051")
    
    try:
        print("Connecting to service...")
        await client.connect()
        
        version = await client.get_version()
        print(f"Connected to service v{version.protocol_version}\n")
        
        # Create multi-node pipeline: PassThrough -> Echo
        print("=== Multi-Node Pipeline: PassThrough -> Echo ===")
        manifest = {
            "version": "v1",
            "metadata": {
                "name": "multi_node_test",
                "description": "Chain PassThrough and Echo nodes",
                "created_at": "2025-10-28T00:00:00Z"
            },
            "nodes": [
                {
                    "id": "passthrough",
                    "node_type": "PassThrough",
                    "params": "{}",
                    "is_streaming": False
                },
                {
                    "id": "echo",
                    "node_type": "Echo",
                    "params": "{}",
                    "is_streaming": False
                }
            ],
            "connections": [
                {
                    "from_node": "passthrough",
                    "from_output": "audio",
                    "to_node": "echo",
                    "to_input": "audio"
                }
            ]
        }
        
        # Generate 1 second of sine wave audio
        SAMPLE_RATE = 16000
        NUM_SAMPLES = SAMPLE_RATE  # 1 second
        FREQUENCY = 440.0  # A4 note
        
        samples = []
        for i in range(NUM_SAMPLES):
            t = i / SAMPLE_RATE
            value = 0.5 * (2.0 * 3.14159 * FREQUENCY * t)
            # Approximate sin for simplicity
            import math
            value = 0.5 * math.sin(value)
            samples.append(struct.pack('<f', value))
        
        audio_data = b''.join(samples)
        
        # Create audio buffer
        audio_buffer = AudioBuffer(
            samples=audio_data,
            sample_rate=SAMPLE_RATE,
            channels=1,
            format=AudioFormat.F32,
            num_samples=NUM_SAMPLES
        )
        
        print(f"Input audio: {NUM_SAMPLES} samples @ {SAMPLE_RATE}Hz")
        print(f"Pipeline: passthrough -> echo")
        
        # Execute pipeline
        result = await client.execute_pipeline(
            manifest=manifest,
            audio_inputs={"passthrough": audio_buffer},
            data_inputs={}
        )
        
        print(f"\n✅ Execution successful")
        print(f"   Wall time: {result.metrics.wall_time_ms:.2f}ms")
        print(f"   Nodes executed: {len(result.node_metrics)}")
        
        # Display per-node metrics
        for node_id, metrics in result.node_metrics.items():
            print(f"   - {node_id}: {metrics.execution_time_ms:.2f}ms")
        
        # Check output
        if "echo" in result.audio_outputs:
            output = result.audio_outputs["echo"]
            print(f"\n   Output audio: {output.num_samples} samples @ {output.sample_rate}Hz")
            print(f"   Format: {output.format.name}")
            print(f"   Channels: {output.channels}")
        
        print("\n✅ Multi-node pipeline completed successfully!")
        
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
