#!/usr/bin/env python3
"""
Audio-Video Synchronization Example using DataChunk with named_buffers.

Demonstrates:
- Multi-input streaming (audio + video simultaneously via named_buffers)
- SynchronizedAudioVideoNode analyzing sync timing
- JSON output showing sync quality and recommendations
- Phase 4: Mixed-type pipeline chains
"""

import asyncio
import sys
import struct
import math
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent / "python-grpc-client"))

# Import generated proto types directly
from generated import (
    AudioBuffer,
    VideoFrame,
    DataBuffer,
    DataChunk,
    PipelineManifest,
    ManifestMetadata,
    NodeManifest,
    StreamRequest,
    StreamInit,
    StreamControl,
    StreamingPipelineServiceStub,
    # Enum values
    AUDIO_FORMAT_F32,
    PIXEL_FORMAT_RGB24,
)
from generated import streaming_pb2

import grpc


async def main():
    # Connect to gRPC server
    channel = grpc.aio.insecure_channel(
        "localhost:50051",
        options=[
            ("grpc.max_receive_message_length", 10 * 1024 * 1024),
            ("grpc.max_send_message_length", 10 * 1024 * 1024),
        ],
    )
    stub = StreamingPipelineServiceStub(channel)

    print("🎬 Audio-Video Synchronization Demo")
    print("=" * 60)

    # Create manifest for SynchronizedAudioVideoNode
    manifest = PipelineManifest(
        version="v1",
        metadata=ManifestMetadata(
            name="audio_video_sync_test",
            description="Test audio-video synchronization with multi-input node",
            created_at="2025-10-29T00:00:00Z",
        ),
        nodes=[
            NodeManifest(
                id="sync_node",
                node_type="SynchronizedAudioVideoNode",
                params='{"sync_tolerance_ms": 20.0}',
                is_streaming=True,
                capabilities=None,
                host="",
                runtime_hint=0,
                input_types=[1, 2],  # AUDIO=1, VIDEO=2
                output_types=[3],  # JSON=3
            )
        ],
        connections=[],
    )

    # Test scenarios with different sync offsets
    test_cases = [
        (0, 0, "Perfect sync"),
        (0, 15_000, "Video 15ms ahead (good)"),
        (0, -10_000, "Audio 10ms ahead (good)"),
        (0, 50_000, "Video 50ms ahead (poor)"),
    ]

    # Create request stream
    async def request_generator():
        # Send init
        yield StreamRequest(
            init=StreamInit(
                manifest=manifest,
                client_version="v1",
                data_inputs={},
                resource_limits=None,
                expected_chunk_size=0,
            )
        )

        # Wait a bit for server to initialize
        await asyncio.sleep(0.1)

        # Send test cases
        for seq, (audio_ts, video_ts, description) in enumerate(test_cases):
            print(f"\n📊 Test case {seq + 1}: {description}")

            # Create audio buffer (100ms @ 16kHz)
            samples_f32 = [0.0] * 1600  # 100ms @ 16kHz
            samples_bytes = b"".join(struct.pack("<f", s) for s in samples_f32)

            audio_buffer = AudioBuffer(
                samples=samples_bytes,
                sample_rate=16000,
                channels=1,
                format=AUDIO_FORMAT_F32,
                num_samples=1600,
            )

            # Create video frame with specific timestamp
            video_frame = VideoFrame(
                pixel_data=bytes([128] * (320 * 240 * 3)),  # Gray frame
                width=320,
                height=240,
                format=PIXEL_FORMAT_RGB24,
                frame_number=seq,
                timestamp_us=video_ts if video_ts >= 0 else 0,  # Can't be negative
            )

            # Create named_buffers map
            named_buffers = {
                "audio": DataBuffer(audio=audio_buffer),
                "video": DataBuffer(video=video_frame),
            }

            # Send DataChunk with both inputs
            yield StreamRequest(
                data_chunk=DataChunk(
                    node_id="sync_node",
                    buffer=None,  # Using named_buffers instead
                    named_buffers=named_buffers,
                    sequence=seq,
                    timestamp_ms=0,
                )
            )

            await asyncio.sleep(0.05)  # Small delay between chunks

        # Send close command
        yield StreamRequest(
            control=StreamControl(
                command=streaming_pb2.COMMAND_CLOSE,
            )
        )

    # Stream pipeline
    try:
        response_stream = stub.StreamPipeline(request_generator())

        async for response in response_stream:
            if response.HasField("ready"):
                print(f"Session started: {response.ready.session_id}\n")

            elif response.HasField("result"):
                result = response.result
                # Extract JSON output from sync_node
                if "sync_node" in result.data_outputs:
                    data_buffer = result.data_outputs["sync_node"]
                    if data_buffer.HasField("json"):
                        import json

                        sync_report = json.loads(data_buffer.json.json_payload)

                        sync_status = sync_report["sync_status"]
                        recommendation = sync_report["recommendation"]

                        print(f"  Sync Status: is_synced={sync_status['is_synced']}, "
                              f"quality=\"{sync_status['quality']}\", "
                              f"offset={sync_status['offset_ms']}ms")
                        print(f"  Recommendation: {recommendation}")

            elif response.HasField("error"):
                print(f"\nError: {response.error.message}")
                break

            elif response.HasField("closed"):
                print(f"\nStream closed: {response.closed.reason}")
                break

        print("\n" + "=" * 60)
        print("Audio-Video Synchronization Demo Complete!")

    except (grpc.RpcError, asyncio.CancelledError) as e:
        if isinstance(e, grpc.RpcError):
            print(f"\ngRPC Error: {e.code()}: {e.details()}")
            sys.exit(1)
        # CancelledError is expected on clean shutdown, ignore it
    finally:
        await channel.close()


if __name__ == "__main__":
    asyncio.run(main())
