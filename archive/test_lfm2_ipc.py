#!/usr/bin/env python3
"""Test LFM2 node with IPC to verify spawn_blocking fix."""

import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent / 'python-grpc-client'))

import asyncio
import struct
from remotemedia_client import RemoteMediaClient, AudioBuffer, AudioFormat

async def main():
    print("🚀 Testing LFM2 with spawn_blocking fix...")
    
    client = RemoteMediaClient(address="[::1]:50051")
    
    try:
        await client.connect()
        print("✅ Connected to gRPC server")
        
        # Create pipeline with LFM2
        manifest = {
            "version": "v1",
            "metadata": {
                "name": "test_lfm2",
                "description": "Test LFM2 IPC",
                "created_at": "2025-11-05T00:00:00Z"
            },
            "nodes": [
                {
                    "id": "lfm2",
                    "node_type": "LFM2AudioNode",
                    "params": "{}",
                    "is_streaming": True
                }
            ],
            "connections": []
        }
        
        # Generate audio chunks
        CHUNK_SIZE = 8000  # 1 second at 8kHz
        NUM_CHUNKS = 2
        
        async def generate_chunks():
            for seq in range(NUM_CHUNKS):
                # Create silent audio
                samples = [struct.pack('<f', 0.01) for _ in range(CHUNK_SIZE)]
                buffer = AudioBuffer(
                    samples=b''.join(samples),
                    sample_rate=8000,
                    channels=1,
                    format=AudioFormat.F32,
                    num_samples=CHUNK_SIZE
                )
                print(f"📤 Sending chunk {seq+1}/{NUM_CHUNKS} ({CHUNK_SIZE} samples)")
                yield ("lfm2", buffer, seq)
        
        # Stream audio and collect results
        chunk_count = 0
        print("\n🎵 Starting stream...")
        
        async for chunk_result in client.stream_pipeline(
            manifest=manifest,
            audio_chunks=generate_chunks(),
            expected_chunk_size=CHUNK_SIZE
        ):
            chunk_count += 1
            print(f"📥 Received chunk {chunk_count}: {chunk_result.processing_time_ms:.2f}ms")
        
        print(f"\n✅ Test completed: processed {chunk_count} chunks")
        
    except Exception as e:
        print(f"❌ Error: {e}")
        import traceback
        traceback.print_exc()
        return 1
    finally:
        await client.close()
    
    return 0

if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
