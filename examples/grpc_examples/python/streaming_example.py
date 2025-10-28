#!/usr/bin/env python3
"""
Bidirectional streaming example.

Demonstrates:
- Streaming audio chunks to server
- Receiving processed results in real-time
- Measuring per-chunk latency
- Proper session management
"""

import asyncio
import sys
import struct
import math
import time
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
        
        # Create streaming pipeline
        print("=== Streaming Audio Pipeline ===")
        manifest = {
            "version": "v1",
            "metadata": {
                "name": "streaming_test",
                "description": "PassThrough streaming",
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
        
        # Audio parameters
        SAMPLE_RATE = 16000
        CHUNK_SIZE = 1600  # 100ms chunks
        NUM_CHUNKS = 20
        FREQUENCY = 440.0  # A4 note
        
        print(f"Sample rate: {SAMPLE_RATE} Hz")
        print(f"Chunk size: {CHUNK_SIZE} samples ({CHUNK_SIZE/SAMPLE_RATE*1000:.0f}ms)")
        print(f"Total chunks: {NUM_CHUNKS}")
        print(f"Total duration: {NUM_CHUNKS*CHUNK_SIZE/SAMPLE_RATE:.1f}s\n")
        
        # Audio chunk generator
        async def generate_chunks():
            """Generate sine wave chunks with timestamps."""
            for seq in range(NUM_CHUNKS):
                # Generate samples
                samples = []
                for i in range(CHUNK_SIZE):
                    sample_idx = seq * CHUNK_SIZE + i
                    t = sample_idx / SAMPLE_RATE
                    value = 0.5 * math.sin(2 * math.pi * FREQUENCY * t)
                    samples.append(struct.pack('<f', value))
                
                # Create buffer
                buffer = AudioBuffer(
                    samples=b''.join(samples),
                    sample_rate=SAMPLE_RATE,
                    channels=1,
                    format=AudioFormat.F32,
                    num_samples=CHUNK_SIZE
                )
                
                yield ("source", buffer, seq)
                
                # Simulate real-time streaming
                await asyncio.sleep(0.05)  # 50ms delay between chunks
        
        # Stream pipeline and collect results
        print("=== Processing Chunks ===")
        latencies = []
        start_time = time.time()
        
        async for result in client.stream_pipeline(
            manifest=manifest,
            audio_chunks=generate_chunks(),
            expected_chunk_size=CHUNK_SIZE
        ):
            latencies.append(result.processing_time_ms)
            print(
                f"Chunk {result.sequence:2d}: "
                f"{result.processing_time_ms:6.2f}ms "
                f"({result.total_samples_processed:6d} samples total)"
            )
        
        total_time = time.time() - start_time
        
        # Display statistics
        print("\n=== Statistics ===")
        print(f"Total chunks: {len(latencies)}")
        print(f"Total time: {total_time:.2f}s")
        print(f"Average latency: {sum(latencies)/len(latencies):.2f}ms")
        print(f"Min latency: {min(latencies):.2f}ms")
        print(f"Max latency: {max(latencies):.2f}ms")
        
        # Check target
        avg_latency = sum(latencies) / len(latencies)
        target_latency = 50.0
        
        if avg_latency < target_latency:
            print(f"\n✅ Target met: {avg_latency:.2f}ms < {target_latency}ms")
        else:
            print(f"\n⚠️  Target missed: {avg_latency:.2f}ms >= {target_latency}ms")
        
    except RemoteMediaError as e:
        print(f"\n❌ Error: {e.message}")
        if e.error_type:
            print(f"   Error type: {e.error_type.name}")
        sys.exit(1)
        
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
