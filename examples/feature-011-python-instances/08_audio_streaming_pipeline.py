#!/usr/bin/env python3
"""
Example 8: Audio Streaming Pipeline with Instance Serialization

Demonstrates realistic audio streaming use case with:
- Streaming audio chunks
- Stateful processing (VAD with memory)
- Instance serialization for multiprocess
Feature 011 - Real-World Audio Use Case
"""

import sys
import asyncio
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))


async def main():
    """Audio streaming example."""
    print("=" * 70)
    print("Example 8: Audio Streaming Pipeline with Instance Serialization")
    print("=" * 70)
    print()

    from remotemedia.core.node import Node
    from remotemedia.core.pipeline import Pipeline
    from remotemedia.core.node_serialization import (
        serialize_node_for_ipc,
        deserialize_node_from_ipc
    )

    # Simulated Audio VAD Node with state
    class VADNode(Node):
        """Voice Activity Detection with state memory."""

        def __init__(self, threshold=0.5, history_size=5, **kwargs):
            super().__init__(**kwargs)
            self.threshold = threshold
            self.history_size = history_size
            self.history = []  # Keep history of detections
            self.is_streaming = True
            self.total_chunks = 0
            self.voice_detected_count = 0

        def initialize(self):
            """Initialize VAD model."""
            super().initialize()
            print(f"  [VAD.initialize] Loaded VAD model (threshold={self.threshold})")
            self.history = []

        async def process(self, audio_stream):
            """Process audio chunks with voice detection."""
            print(f"  [VAD] Starting stream processing")

            async for chunk in audio_stream:
                self.total_chunks += 1

                # Simulate VAD processing
                energy = len(str(chunk)) % 10 / 10.0  # Fake energy calculation
                has_voice = energy > self.threshold

                if has_voice:
                    self.voice_detected_count += 1

                # Update history
                self.history.append(has_voice)
                if len(self.history) > self.history_size:
                    self.history.pop(0)

                result = {
                    "chunk_id": self.total_chunks,
                    "has_voice": has_voice,
                    "energy": energy,
                    "history": self.history.copy()
                }

                print(f"  [VAD] Chunk #{self.total_chunks}: voice={has_voice}, energy={energy:.2f}")
                yield result

            print(f"  [VAD] Stream complete: {self.total_chunks} chunks, {self.voice_detected_count} with voice")

        def cleanup(self):
            """Unload VAD model."""
            print(f"  [VAD.cleanup] Unloading VAD model")
            super().cleanup()

    # Create VAD node
    vad_node = VADNode(name="vad", threshold=0.4, history_size=3)

    print(f"✓ Created VAD node:")
    print(f"  - threshold: {vad_node.threshold}")
    print(f"  - history_size: {vad_node.history_size}")
    print()

    # Part 1: Demonstrate serialization for multiprocess
    print("Part 1: Serialize VAD node for multiprocess execution")
    print("-" * 70)

    # Simulate state before serialization
    vad_node.history = [True, False, True]
    vad_node.total_chunks = 100

    print(f"  State before: history={vad_node.history}, chunks={vad_node.total_chunks}")

    # Serialize
    serialized = serialize_node_for_ipc(vad_node)
    print(f"  ✓ Serialized: {len(serialized) / 1024:.2f} KB")
    print(f"  ✓ cleanup() called automatically")
    print()

    # Deserialize in "subprocess"
    restored_vad = deserialize_node_from_ipc(serialized)
    print(f"  ✓ Deserialized in subprocess")
    print(f"  State restored: history={restored_vad.history}, chunks={restored_vad.total_chunks}")
    print(f"  ✓ initialize() called automatically")
    print()

    # Part 2: Demonstrate streaming execution
    print("Part 2: Execute streaming VAD pipeline")
    print("-" * 70)

    # Create fresh VAD node for streaming
    vad_streaming = VADNode(name="vad_stream", threshold=0.4, history_size=3)

    # Create pipeline
    pipeline = Pipeline(name="audio-vad-pipeline")
    pipeline.add_node(vad_streaming)

    # Simulate streaming audio chunks
    async def audio_chunk_stream():
        """Simulate audio chunks arriving over time."""
        chunks = [
            "audio_chunk_001",
            "audio_chunk_002",
            "audio_chunk_003",
            "audio_chunk_004",
            "audio_chunk_005",
        ]

        for chunk in chunks:
            print(f"  [AudioSource] → {chunk}")
            yield chunk
            await asyncio.sleep(0.1)  # Simulate real-time streaming

    print("  Starting audio stream...")
    print()

    # Execute streaming pipeline
    vad_results = []
    async with pipeline.managed_execution():
        async for vad_output in pipeline.process(audio_chunk_stream()):
            vad_results.append(vad_output)
            print(f"  [Pipeline Output] Voice detected: {vad_output['has_voice']}, "
                  f"energy: {vad_output['energy']:.2f}")

    print()
    print(f"✓ Processed {len(vad_results)} chunks")
    print(f"✓ Voice detected in: {sum(1 for r in vad_results if r['has_voice'])} chunks")
    print(f"✓ Final VAD state:")
    print(f"  - total_chunks: {vad_streaming.total_chunks}")
    print(f"  - voice_detected_count: {vad_streaming.voice_detected_count}")
    print(f"  - history: {vad_streaming.history}")
    print()

    print("=" * 70)
    print("✅ Streaming execution complete!")
    print()
    print("Key Features Demonstrated:")
    print("  ✓ Streaming input (async generator)")
    print("  ✓ Streaming output (yield multiple per input)")
    print("  ✓ Stateful processing (history buffer)")
    print("  ✓ Instance serialization (for multiprocess)")
    print("  ✓ Lifecycle management (initialize/cleanup)")
    print("=" * 70)


if __name__ == "__main__":
    asyncio.run(main())
