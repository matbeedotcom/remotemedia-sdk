#!/usr/bin/env python3
"""
Example 9: Real-Time Audio Processing with Multiple Streaming Nodes

Demonstrates complex streaming pipeline with:
- Multiple streaming nodes in sequence
- Real-time audio chunk processing
- State persistence across stream
- Instance-based configuration
Feature 011 - Production Audio Pipeline
"""

import sys
import asyncio
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Real-time audio processing example."""
    print("=" * 70)
    print("Example 9: Real-Time Audio Processing Pipeline")
    print("=" * 70)
    print()

    from remotemedia.core.node import Node
    from remotemedia.core.pipeline import Pipeline

    # Audio Chunker - splits audio into processable chunks
    class AudioChunkerNode(Node):
        """Splits audio stream into fixed-size chunks."""

        def __init__(self, chunk_size_ms=100, **kwargs):
            super().__init__(**kwargs)
            self.chunk_size_ms = chunk_size_ms
            self.is_streaming = True
            self.chunks_created = 0

        async def process(self, audio_stream):
            """Chunk audio stream."""
            print(f"  [Chunker] Starting (chunk_size={self.chunk_size_ms}ms)")

            async for audio_data in audio_stream:
                # Simulate chunking
                chunks = [
                    f"{audio_data}_chunk_{i}"
                    for i in range(3)  # Simulate 3 chunks per input
                ]

                for chunk in chunks:
                    self.chunks_created += 1
                    print(f"  [Chunker] → Chunk #{self.chunks_created}: {chunk}")
                    yield chunk

            print(f"  [Chunker] Created {self.chunks_created} total chunks")

    # VAD Filter - only passes chunks with voice
    class VADFilterNode(Node):
        """Filters chunks based on voice activity detection."""

        def __init__(self, sensitivity=0.5, **kwargs):
            super().__init__(**kwargs)
            self.sensitivity = sensitivity
            self.is_streaming = True
            self.chunks_passed = 0
            self.chunks_filtered = 0

        async def process(self, chunk_stream):
            """Filter chunks with VAD."""
            print(f"  [VAD] Starting filter (sensitivity={self.sensitivity})")

            async for chunk in chunk_stream:
                # Simulate VAD detection
                has_voice = hash(chunk) % 2 == 0  # Fake VAD

                if has_voice:
                    self.chunks_passed += 1
                    print(f"  [VAD] ✓ Voice: {chunk}")
                    yield chunk
                else:
                    self.chunks_filtered += 1
                    print(f"  [VAD] ✗ Silence: {chunk} (filtered)")

            print(f"  [VAD] Passed: {self.chunks_passed}, Filtered: {self.chunks_filtered}")

    # Transcription Node - processes voice chunks
    class TranscriptionNode(Node):
        """Transcribes voice chunks."""

        def __init__(self, model="whisper-base", **kwargs):
            super().__init__(**kwargs)
            self.model = model
            self.is_streaming = True
            self.transcribed_count = 0
            self.transcription_buffer = []

        def initialize(self):
            """Load transcription model."""
            super().initialize()
            print(f"  [Transcribe.init] Loaded model: {self.model}")

        async def process(self, voice_stream):
            """Transcribe voice chunks."""
            print(f"  [Transcribe] Starting")

            async for chunk in voice_stream:
                self.transcribed_count += 1

                # Simulate transcription
                text = f"text_from_{chunk}"
                self.transcription_buffer.append(text)

                result = {
                    "chunk": chunk,
                    "text": text,
                    "model": self.model
                }

                print(f"  [Transcribe] → Chunk #{self.transcribed_count}: {text}")
                yield result

            print(f"  [Transcribe] Transcribed {self.transcribed_count} chunks")

        def cleanup(self):
            """Unload model."""
            print(f"  [Transcribe.cleanup] Unloading model")
            super().cleanup()

    # Build real-time pipeline with instances
    print("✓ Creating real-time audio pipeline:")
    chunker = AudioChunkerNode(name="chunker", chunk_size_ms=100)
    vad = VADFilterNode(name="vad", sensitivity=0.5)
    transcribe = TranscriptionNode(name="transcribe", model="whisper-base")

    pipeline = Pipeline(name="realtime-audio")
    pipeline.add_node(chunker)
    pipeline.add_node(vad)
    pipeline.add_node(transcribe)

    print(f"  - Stage 1: {chunker.name} (chunk_size={chunker.chunk_size_ms}ms)")
    print(f"  - Stage 2: {vad.name} (sensitivity={vad.sensitivity})")
    print(f"  - Stage 3: {transcribe.name} (model={transcribe.model})")
    print()

    # Simulate real-time audio stream
    async def realtime_audio_stream():
        """Simulate real-time audio packets."""
        print("  [AudioSource] Starting real-time stream...")
        audio_packets = [
            "audio_packet_001",
            "audio_packet_002",
            "audio_packet_003",
        ]

        for packet in audio_packets:
            print(f"  [AudioSource] → {packet}")
            yield packet
            await asyncio.sleep(0.2)  # Simulate real-time delay

    print("→ Starting real-time processing...")
    print()

    # Execute pipeline
    transcriptions = []
    async with pipeline.managed_execution():
        async for result in pipeline.process(realtime_audio_stream()):
            transcriptions.append(result)

    print()
    print("=" * 70)
    print("✅ Real-time audio pipeline complete!")
    print()
    print(f"Final Statistics:")
    print(f"  - Audio packets: 3")
    print(f"  - Chunks created: {chunker.chunks_created}")
    print(f"  - Chunks passed VAD: {vad.chunks_passed}")
    print(f"  - Chunks filtered: {vad.chunks_filtered}")
    print(f"  - Transcriptions: {len(transcriptions)}")
    print()
    print(f"Pipeline State (preserved across stream):")
    print(f"  - Chunker: {chunker.chunks_created} chunks created")
    print(f"  - VAD: {vad.chunks_passed}/{vad.chunks_passed + vad.chunks_filtered} pass rate")
    print(f"  - Transcribe: {transcribe.transcribed_count} transcriptions, "
          f"{len(transcribe.transcription_buffer)} buffered")
    print()
    print("Instance Features Demonstrated:")
    print("  ✓ Complex stateful nodes (history, counters, buffers)")
    print("  ✓ Multi-stage streaming pipeline")
    print("  ✓ Lifecycle methods (initialize/cleanup)")
    print("  ✓ State accessible after execution")
    print("  ✓ Ready for multiprocess via serialize_node_for_ipc()")
    print("=" * 70)


if __name__ == "__main__":
    asyncio.run(main())
