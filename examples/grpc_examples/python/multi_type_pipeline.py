#!/usr/bin/env python3
"""
Multi-Type Pipeline Example

Demonstrates a complex pipeline with multiple data types:
- Audio streaming through VAD
- Video frame processing
- JSON control messages
- Multi-input synchronization

This shows the full power of the generic streaming protocol.
"""

import sys
import os
import grpc
import time
import json
import struct

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
    AudioBuffer,
    VideoFrame,
    JsonData,
    AudioFormat,
    PixelFormat,
)
from generated.execution_pb2_grpc import ExecutionServiceStub


def create_multi_type_pipeline(stub: ExecutionServiceStub) -> str:
    """Create pipeline with audio, video, and JSON nodes"""

    manifest = PipelineManifest(
        version="v1",
        metadata={
            "name": "multi_type_pipeline",
            "description": "Audio + Video + JSON streaming demo",
            "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        },
        nodes=[
            # Audio VAD node
            NodeManifest(
                id="vad_node",
                node_type="RustVADNode",
                params='{"threshold": 0.5, "min_speech_duration_ms": 100}',
                is_streaming=True,
            ),
            # Video processor
            NodeManifest(
                id="video_node",
                node_type="VideoProcessorNode",
                params='{"processing_mode": "passthrough"}',
                is_streaming=True,
            ),
            # Calculator for JSON
            NodeManifest(
                id="calc_node",
                node_type="CalculatorNode",
                params='{}',
                is_streaming=True,
            ),
            # Synchronized multi-input node (audio + video)
            NodeManifest(
                id="sync_node",
                node_type="SynchronizedAudioVideoNode",
                params='{"sync_tolerance_ms": 20.0}',
                is_streaming=True,
            ),
        ],
        connections=[],
    )

    response = stub.CreatePipeline(CreatePipelineRequest(manifest=manifest))
    print(f"✅ Created pipeline: {response.pipeline_id}")
    return response.pipeline_id


def create_audio_chunk(duration_ms: int = 100) -> AudioBuffer:
    """Create a 100ms audio chunk at 16kHz, mono, F32"""
    sample_rate = 16000
    num_samples = (sample_rate * duration_ms) // 1000

    # Generate simple sine wave
    samples = bytearray()
    for i in range(num_samples):
        # 440Hz tone (A note)
        amplitude = 0.3
        value = amplitude * (1.0 if (i % 40) < 20 else -1.0)  # Square wave
        samples.extend(struct.pack("f", value))

    return AudioBuffer(
        samples=bytes(samples),
        sample_rate=sample_rate,
        channels=1,
        format=AudioFormat.AUDIO_FORMAT_F32,
        num_samples=num_samples,
    )


def create_video_frame(frame_number: int) -> VideoFrame:
    """Create a simple 320x240 RGB24 video frame"""
    width, height = 320, 240
    pixel_data = bytearray(width * height * 3)

    # Create animated gradient
    for y in range(height):
        for x in range(width):
            offset = (y * width + x) * 3
            pixel_data[offset] = (x + frame_number * 2) % 256  # R
            pixel_data[offset + 1] = (y + frame_number * 2) % 256  # G
            pixel_data[offset + 2] = ((x + y) // 2) % 256  # B

    return VideoFrame(
        pixel_data=bytes(pixel_data),
        width=width,
        height=height,
        format=PixelFormat.PIXEL_FORMAT_RGB24,
        frame_number=frame_number,
        timestamp_us=frame_number * 33333,  # 30fps
    )


def create_json_operation(operation: str, a: float, b: float) -> JsonData:
    """Create a JSON calculation"""
    payload = {"operation": operation, "operands": [a, b]}
    return JsonData(
        json_payload=json.dumps(payload),
        schema_type="calculation",
    )


def demo_individual_streams(stub: ExecutionServiceStub, pipeline_id: str):
    """Demo 1: Stream each data type individually"""

    print("\n" + "=" * 60)
    print("DEMO 1: Individual Data Type Streams")
    print("=" * 60)

    # 1. Audio through VAD
    print("\n🎤 Streaming audio through VAD...")

    def audio_stream():
        for i in range(3):
            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="vad_node",
                    buffer=DataBuffer(audio=create_audio_chunk()),
                    sequence=i,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )
            time.sleep(0.1)

    for response in stub.StreamData(audio_stream()):
        if response.HasField("data_chunk"):
            print(f"  ✅ VAD processed audio chunk {response.data_chunk.sequence}")

    # 2. Video through processor
    print("\n📹 Streaming video frames...")

    def video_stream():
        for i in range(3):
            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="video_node",
                    buffer=DataBuffer(video=create_video_frame(i)),
                    sequence=i,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )
            time.sleep(0.033)

    for response in stub.StreamData(video_stream()):
        if response.HasField("data_chunk"):
            print(f"  ✅ Processed video frame {response.data_chunk.sequence}")

    # 3. JSON through calculator
    print("\n🧮 Streaming JSON calculations...")

    def json_stream():
        ops = [("add", 10, 5), ("multiply", 6, 7), ("divide", 100, 4)]
        for i, (op, a, b) in enumerate(ops):
            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="calc_node",
                    buffer=DataBuffer(json=create_json_operation(op, a, b)),
                    sequence=i,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )
            time.sleep(0.1)

    for response in stub.StreamData(json_stream()):
        if response.HasField("data_chunk") and response.data_chunk.buffer.HasField("json"):
            result = json.loads(response.data_chunk.buffer.json.json_payload)
            print(f"  ✅ Calculation result: {result}")


def demo_synchronized_stream(stub: ExecutionServiceStub, pipeline_id: str):
    """Demo 2: Stream audio + video through synchronized node"""

    print("\n" + "=" * 60)
    print("DEMO 2: Synchronized Audio + Video Stream")
    print("=" * 60)

    print("\n🎭 Streaming synchronized audio and video...")

    def sync_stream():
        for i in range(5):
            named_buffers = {
                "audio": DataBuffer(audio=create_audio_chunk()),
                "video": DataBuffer(video=create_video_frame(i)),
            }

            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="sync_node",
                    buffer=None,
                    named_buffers=named_buffers,
                    sequence=i,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )

            print(f"  📤 Sent synchronized chunk {i} (audio + video)")
            time.sleep(0.1)

    for response in stub.StreamData(sync_stream()):
        if response.HasField("data_chunk"):
            chunk = response.data_chunk
            if chunk.HasField("buffer") and chunk.buffer.HasField("json"):
                result = json.loads(chunk.buffer.json.json_payload)
                print(f"  ✅ Sync result: {json.dumps(result)}")


def demo_interleaved_streams(stub: ExecutionServiceStub, pipeline_id: str):
    """Demo 3: Interleave different data types in single stream"""

    print("\n" + "=" * 60)
    print("DEMO 3: Interleaved Multi-Type Stream")
    print("=" * 60)

    print("\n🌀 Streaming interleaved audio, video, and JSON...")

    def interleaved_stream():
        seq = 0
        for i in range(3):
            # Audio chunk
            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="vad_node",
                    buffer=DataBuffer(audio=create_audio_chunk()),
                    sequence=seq,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )
            print(f"  📤 [{seq}] Audio")
            seq += 1
            time.sleep(0.05)

            # Video frame
            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="video_node",
                    buffer=DataBuffer(video=create_video_frame(i)),
                    sequence=seq,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )
            print(f"  📤 [{seq}] Video")
            seq += 1
            time.sleep(0.05)

            # JSON calculation
            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="calc_node",
                    buffer=DataBuffer(json=create_json_operation("add", i, i + 1)),
                    sequence=seq,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )
            print(f"  📤 [{seq}] JSON")
            seq += 1
            time.sleep(0.05)

    response_count = 0
    for response in stub.StreamData(interleaved_stream()):
        if response.HasField("data_chunk"):
            response_count += 1
            print(f"  ✅ Received response {response_count}")

    print(f"\n  Total responses: {response_count}")


def main():
    """Run multi-type pipeline demo"""

    print("🎨 Multi-Type Pipeline Demo")
    print("=" * 60)
    print("\nThis demo showcases:")
    print("  - Audio streaming (VAD)")
    print("  - Video streaming (frame processing)")
    print("  - JSON streaming (calculator)")
    print("  - Multi-input synchronization (audio + video)")
    print("  - Interleaved multi-type streaming")

    channel = grpc.insecure_channel("localhost:50051")
    stub = ExecutionServiceStub(channel)

    try:
        # Create pipeline
        pipeline_id = create_multi_type_pipeline(stub)

        # Run demos
        demo_individual_streams(stub, pipeline_id)
        demo_synchronized_stream(stub, pipeline_id)
        demo_interleaved_streams(stub, pipeline_id)

        print("\n" + "=" * 60)
        print("✅ Multi-Type Pipeline Demo Complete!\n")
        print("💡 Key Achievements:")
        print("   - Streamed 3 different data types (audio, video, JSON)")
        print("   - Used multi-input node (audio + video sync)")
        print("   - Demonstrated interleaved streaming")
        print("   - All types work seamlessly in one pipeline")

    except grpc.RpcError as e:
        print(f"❌ gRPC error: {e.code()}: {e.details()}")
        sys.exit(1)
    finally:
        channel.close()


if __name__ == "__main__":
    main()
