#!/usr/bin/env python3
"""
Type-Safe Streaming Example

Demonstrates static type hints with the generic streaming protocol.
Run mypy to verify type safety:
    mypy examples/type_safe_streaming.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from data_types_typed import (
    AudioBuffer,
    VideoFrame,
    JsonData,
    AudioFormat,
    PixelFormat,
    DataTypeHint,
    DataBuffer,
    DataChunk,
    TypedNodeManifest,
    TypedPipelineManifest,
    is_audio_buffer,
    is_video_frame,
    is_json_data,
    extract_audio_data,
    extract_video_data,
    extract_json_data,
    create_audio_buffer,
    create_video_frame,
    create_json_data,
    validate_buffer_type,
    TypeValidationError,
)
import json


def demo_create_buffers() -> None:
    """Demo 1: Create type-safe buffers"""
    print("\n📦 Creating type-safe buffers:")

    # Audio buffer - create_audio_buffer returns AudioBufferDict
    audio = create_audio_buffer(
        samples=b"\x00" * (1600 * 4),  # 100ms @ 16kHz, F32
        sample_rate=16000,
        channels=1,
        format=AudioFormat.F32,
        num_samples=1600,
        metadata={"source": "microphone"},
    )
    # Type is known from create function
    audio_data: AudioBuffer = audio["data"]
    print(f"  Audio: {audio['type']}, {audio_data.sample_rate}Hz")

    # Video frame - create_video_frame returns VideoFrameDict
    video = create_video_frame(
        pixel_data=b"\x00" * (320 * 240 * 3),
        width=320,
        height=240,
        format=PixelFormat.RGB24,
        frame_number=0,
        timestamp_us=0,
        metadata={"camera": "webcam"},
    )
    # Type is known from create function
    video_data: VideoFrame = video["data"]
    print(f"  Video: {video['type']}, {video_data.width}x{video_data.height}")

    # JSON data - create_json_data returns JsonDataDict
    json_buffer = create_json_data(
        json_payload=json.dumps({"operation": "gain", "value": 1.5}),
        schema_type="audio_control",
    )
    print(f"  JSON: {json_buffer['type']}")


def demo_type_guards(buffer: DataBuffer) -> None:
    """Demo 2: Type guards enable type narrowing"""
    print("\n🔍 Processing with type guards:")

    if is_audio_buffer(buffer):
        # Type checker knows buffer is AudioBufferDict
        # and buffer["data"] is AudioBuffer
        audio_data: AudioBuffer = buffer["data"]
        print(f"  Audio: {audio_data.sample_rate}Hz, {audio_data.channels}ch")
        print(f"  Format: {audio_data.format.value}, Samples: {audio_data.num_samples}")
    elif is_video_frame(buffer):
        # Type checker knows buffer is VideoFrameDict
        # and buffer["data"] is VideoFrame
        video_data: VideoFrame = buffer["data"]
        print(f"  Video: {video_data.width}x{video_data.height}")
        print(f"  Format: {video_data.format.value}, Frame: {video_data.frame_number}")
    elif is_json_data(buffer):
        # Type checker knows buffer is JsonDataDict
        # and buffer["data"] is JsonData
        json_data: JsonData = buffer["data"]
        parsed = json.loads(json_data.json_payload)
        print(f"  JSON: {parsed}")


def demo_extract_helpers() -> None:
    """Demo 3: Extract helpers with null checks"""
    print("\n🔧 Using extract helpers:")

    audio: DataBuffer = create_audio_buffer(
        samples=b"\x00" * 100,
        sample_rate=16000,
        channels=1,
        format=AudioFormat.F32,
        num_samples=25,
    )

    # Extract returns Optional[AudioBuffer]
    audio_data = extract_audio_data(audio)
    if audio_data:  # Type checker knows audio_data is AudioBuffer (not None)
        print(f"  Audio sample rate: {audio_data.sample_rate}Hz")

    video_data = extract_video_data(audio)  # Returns None for non-video
    if video_data:
        print(f"  Video resolution: {video_data.width}x{video_data.height}")
    else:
        print("  Not a video buffer")


def demo_multi_input_chunk() -> None:
    """Demo 4: Multi-input data chunk"""
    print("\n🎭 Multi-input data chunk:")

    chunk = DataChunk(
        node_id="sync_node",
        sequence=0,
        timestamp_ms=0,
        named_buffers={
            "audio": create_audio_buffer(
                samples=b"\x00" * 100,
                sample_rate=16000,
                channels=1,
                format=AudioFormat.F32,
                num_samples=25,
            ),
            "video": create_video_frame(
                pixel_data=b"\x00" * (320 * 240 * 3),
                width=320,
                height=240,
                format=PixelFormat.RGB24,
                frame_number=0,
                timestamp_us=0,
            ),
        },
    )

    print(f"  Chunk for node: {chunk.node_id}")
    if chunk.named_buffers:
        print(f"  Named buffers: {', '.join(chunk.named_buffers.keys())}")


def demo_type_validation() -> None:
    """Demo 5: Runtime type validation"""
    print("\n✅ Runtime validation:")

    audio: DataBuffer = create_audio_buffer(
        samples=b"\x00" * 100,
        sample_rate=16000,
        channels=1,
        format=AudioFormat.F32,
        num_samples=25,
    )

    try:
        # Valid: audio buffer for audio-expecting node
        validate_buffer_type(audio, DataTypeHint.AUDIO, "vad_node")
        print("  ✓ Audio buffer validated for VAD node")

        # Invalid: would throw if we passed video to audio node
        # validate_buffer_type(audio, DataTypeHint.VIDEO, "video_node")
    except TypeValidationError as e:
        print(f"  ✗ Validation error: {e}")
        print(f"    Expected: {e.expected.value}, Got: {e.actual.value}")


def demo_typed_manifest() -> None:
    """Demo 6: Type-safe pipeline manifest"""
    print("\n📋 Type-safe pipeline manifest:")

    manifest = TypedPipelineManifest(
        version="v1",
        metadata={
            "name": "audio_video_sync_pipeline",
            "description": "Type-safe multi-input pipeline",
            "created_at": "2025-10-29T00:00:00Z",
        },
        nodes=[
            TypedNodeManifest(
                id="sync_node",
                node_type="SynchronizedAudioVideoNode",
                is_streaming=True,
                input_types=[DataTypeHint.AUDIO, DataTypeHint.VIDEO],
                output_types=[DataTypeHint.JSON],
                params=json.dumps({"sync_tolerance_ms": 20.0}),
            )
        ],
        connections=[],
    )

    print(f"  Pipeline: {manifest.metadata['name']}")
    print(f"  Nodes: {', '.join(n.id for n in manifest.nodes)}")
    node = manifest.nodes[0]
    print(f"  Node inputs: {', '.join(t.value for t in node.input_types)}")
    print(f"  Node outputs: {', '.join(t.value for t in node.output_types)}")


def main() -> None:
    """Run all demos"""
    print("🎨 Type-Safe Streaming Demo (Python)")
    print("=" * 60)

    demo_create_buffers()

    # Demo type guards with audio buffer
    audio = create_audio_buffer(
        samples=b"\x00" * 100,
        sample_rate=16000,
        channels=1,
        format=AudioFormat.F32,
        num_samples=25,
    )
    demo_type_guards(audio)

    demo_extract_helpers()
    demo_multi_input_chunk()
    demo_type_validation()
    demo_typed_manifest()

    print("\n" + "=" * 60)
    print("✅ Type-Safe Streaming Demo Complete!\n")
    print("💡 Key Benefits:")
    print("   - Static type checking with mypy")
    print("   - IDE autocomplete for all data types")
    print("   - Type narrowing with guards")
    print("   - Runtime validation helpers")
    print("\n🔍 Run mypy to verify type safety:")
    print("   mypy examples/type_safe_streaming.py")


if __name__ == "__main__":
    main()
