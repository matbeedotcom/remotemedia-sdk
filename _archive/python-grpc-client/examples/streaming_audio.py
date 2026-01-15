#!/usr/bin/env python3
"""
Bidirectional streaming example.

Demonstrates:
- Streaming audio chunks to server
- Receiving processed results in real-time
- Measuring per-chunk latency
"""

import asyncio
import sys
import struct
import math
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia_client import (
    RemoteMediaClient,
    RemoteMediaError,
    AudioBuffer,
    AudioFormat
)


async def main():
    client = RemoteMediaClient("localhost:50051")
    
    try:
        # Connect
        print("Connecting to service...")
        await client.connect()
        
        # Get version
        version = await client.get_version()
        print(f"Connected to service v{version.protocol_version}")
        
        # Create streaming pipeline
        print("\n=== Streaming Pipeline ===")
        manifest = {
            "version": "v1",
            "metadata": {
                "name": "streaming_test",
                "description": "PassThrough streaming test",
                "created_at": "2025-10-28T00:00:00Z"
            },
            "nodes": [
                {
                    "id": "source",
                    "node_type": "PassThrough",
                    "params": "{}",
                    "is_streaming": False
                }
            ],
            "connections": []
        }
        
        # Audio generation parameters
        SAMPLE_RATE = 16000
        CHUNK_SIZE = 1600  # 100ms at 16kHz
        NUM_CHUNKS = 10
        FREQUENCY = 440.0  # A4 note
        
        print(f"Sample rate: {SAMPLE_RATE} Hz")
        print(f"Chunk size: {CHUNK_SIZE} samples ({CHUNK_SIZE/SAMPLE_RATE*1000:.1f}ms)")
        print(f"Number of chunks: {NUM_CHUNKS}")
        print(f"Total duration: {NUM_CHUNKS*CHUNK_SIZE/SAMPLE_RATE:.1f}s")
        
        # Audio chunk generator
        async def generate_audio():
            """Generate sine wave chunks."""
            for seq in range(NUM_CHUNKS):
                # Generate sine wave samples
                samples = []
                for i in range(CHUNK_SIZE):
                    sample_index = seq * CHUNK_SIZE + i
                    t = sample_index / SAMPLE_RATE
                    value = 0.5 * math.sin(2 * math.pi * FREQUENCY * t)
                    samples.append(struct.pack('<f', value))
                
                # Create audio buffer
                buffer = AudioBuffer(
                    samples=b''.join(samples),
                    sample_rate=SAMPLE_RATE,
                    channels=1,
                    format=AudioFormat.F32,
                    num_samples=CHUNK_SIZE
                )
                
                yield ("source", buffer, seq)
                
                # Small delay to simulate real-time streaming
                await asyncio.sleep(0.01)
        
        # Stream pipeline
        print("\n=== Processing Chunks ===")
        latencies = []
        
        async for result in client.stream_pipeline(
            manifest=manifest,
            audio_chunks=generate_audio(),
            expected_chunk_size=CHUNK_SIZE
        ):
            latencies.append(result.processing_time_ms)
            print(
                f"Chunk {result.sequence:2d}: "
                f"{result.processing_time_ms:6.2f}ms "
                f"(total: {result.total_samples_processed} samples)"
            )
        
        # Display statistics
        print("\n=== Statistics ===")
        print(f"Chunks processed: {len(latencies)}")
        print(f"Average latency: {sum(latencies)/len(latencies):.2f}ms")
        print(f"Min latency: {min(latencies):.2f}ms")
        print(f"Max latency: {max(latencies):.2f}ms")
        
        # Check if target met (<50ms)
        avg_latency = sum(latencies) / len(latencies)
        target_latency = 50.0
        
        if avg_latency < target_latency:
            print(f"\n✅ Target met: {avg_latency:.2f}ms < {target_latency}ms")
        else:
            print(f"\n⚠️ Target missed: {avg_latency:.2f}ms >= {target_latency}ms")
        
    except RemoteMediaError as e:
        print(f"\n❌ Error: {e}")
        if e.error_type:
            print(f"Error type: {e.error_type.name}")
        sys.exit(1)
    
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
