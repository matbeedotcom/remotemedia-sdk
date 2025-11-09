#!/usr/bin/env python3
"""
Video Streaming Example

Demonstrates video frame streaming through VideoProcessorNode.
The node processes video frames and can perform transformations.
"""

import sys
import os
import grpc
import time

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
    VideoFrame,
    PixelFormat,
)
from generated.execution_pb2_grpc import ExecutionServiceStub


def create_video_pipeline(stub: ExecutionServiceStub) -> str:
    """Create pipeline with VideoProcessorNode"""

    manifest = PipelineManifest(
        version="v1",
        metadata={
            "name": "video_streaming_pipeline",
            "description": "Video frame streaming demo",
            "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        },
        nodes=[
            NodeManifest(
                id="video_processor",
                node_type="VideoProcessorNode",
                params='{"processing_mode": "passthrough"}',
                is_streaming=True,
            )
        ],
        connections=[],
    )

    response = stub.CreatePipeline(CreatePipelineRequest(manifest=manifest))
    print(f"✅ Created pipeline: {response.pipeline_id}")
    return response.pipeline_id


def create_rgb_frame(frame_number: int, width: int = 320, height: int = 240) -> VideoFrame:
    """Create a simple RGB24 video frame with a pattern"""

    # Create a simple pattern: gradient based on frame number
    pixel_data = bytearray(width * height * 3)

    for y in range(height):
        for x in range(width):
            offset = (y * width + x) * 3
            # Create animated gradient
            pixel_data[offset] = (x + frame_number) % 256  # R
            pixel_data[offset + 1] = (y + frame_number) % 256  # G
            pixel_data[offset + 2] = ((x + y + frame_number) // 2) % 256  # B

    return VideoFrame(
        pixel_data=bytes(pixel_data),
        width=width,
        height=height,
        format=PixelFormat.PIXEL_FORMAT_RGB24,
        frame_number=frame_number,
        timestamp_us=frame_number * 33333,  # 30fps timing
    )


def stream_video_frames(stub: ExecutionServiceStub, pipeline_id: str, num_frames: int = 10):
    """Stream video frames through the pipeline"""

    def generate_requests():
        for frame_num in range(num_frames):
            video_frame = create_rgb_frame(frame_num)

            yield StreamRequest(
                pipeline_id=pipeline_id,
                data_chunk=DataChunk(
                    node_id="video_processor",
                    buffer=DataBuffer(video=video_frame),
                    sequence=frame_num,
                    timestamp_ms=int(time.time() * 1000),
                ),
            )

            print(f"📹 Sent frame {frame_num}: {video_frame.width}x{video_frame.height} RGB24")
            time.sleep(0.033)  # Simulate 30fps

    print(f"\n🎬 Streaming {num_frames} video frames...\n")

    frame_count = 0
    total_bytes = 0

    for response in stub.StreamData(generate_requests()):
        frame_count += 1

        # Check if we got video output
        if response.HasField("data_chunk"):
            chunk = response.data_chunk
            if chunk.HasField("buffer") and chunk.buffer.HasField("video"):
                video = chunk.buffer.video
                total_bytes += len(video.pixel_data)
                print(f"✅ Received frame {video.frame_number}: "
                      f"{video.width}x{video.height} {PixelFormat.Name(video.format)} "
                      f"({len(video.pixel_data)} bytes)")

        if response.HasField("metrics"):
            print(f"   ⏱️  Processing time: {response.metrics.processing_time_ms:.2f}ms")

    print(f"\n📊 Summary:")
    print(f"   Frames processed: {frame_count}")
    print(f"   Total pixel data: {total_bytes:,} bytes ({total_bytes / 1024:.2f} KB)")
    print(f"   Average frame size: {total_bytes / max(frame_count, 1):.0f} bytes")


def main():
    """Run video streaming demo"""

    print("🎨 Video Streaming Demo")
    print("=" * 60)

    channel = grpc.insecure_channel("localhost:50051")
    stub = ExecutionServiceStub(channel)

    try:
        # Create pipeline
        pipeline_id = create_video_pipeline(stub)

        # Stream video frames
        stream_video_frames(stub, pipeline_id, num_frames=10)

        print("\n" + "=" * 60)
        print("✅ Video Streaming Demo Complete!\n")
        print("💡 This demo showed:")
        print("   - Creating VideoProcessorNode")
        print("   - Streaming RGB24 video frames")
        print("   - 30fps frame timing")
        print("   - Video frame structure (width, height, format, pixel_data)")

    except grpc.RpcError as e:
        print(f"❌ gRPC error: {e.code()}: {e.details()}")
        sys.exit(1)
    finally:
        channel.close()


if __name__ == "__main__":
    main()
