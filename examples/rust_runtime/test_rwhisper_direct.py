#!/usr/bin/env python3
"""Test rwhisper transcription without AudioBuffer - send frames directly"""

import asyncio
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.source import MediaReaderNode, AudioTrackSource
from remotemedia.nodes.audio import AudioTransform
from remotemedia.nodes.transcription import RustWhisperTranscriber

async def test_single_buffer():
    """Test transcribing entire audio as one buffer"""
    audio_file = Path("C:/Users/mail/dev/personal/remotemedia-sdk/examples/transcribe_demo.wav")

    print("Testing Rust Whisper WITHOUT AudioBuffer")
    print("=" * 70)

    pipeline = Pipeline(name="SingleBuffer_Test")
    pipeline.add_node(MediaReaderNode(path=str(audio_file), chunk_size=4096, name="MediaReader"))
    pipeline.add_node(AudioTrackSource(name="AudioSource"))
    pipeline.add_node(AudioTransform(output_sample_rate=16000, output_channels=1, name="Resample"))
    # NO AudioBuffer - send frames directly to Whisper
    pipeline.add_node(RustWhisperTranscriber(model_source="tiny", language="en", name="RustWhisper"))

    result = await pipeline.run(use_rust=True)

    print("\nResults:")
    print("=" * 70)
    if isinstance(result, list):
        print(f"Received {len(result)} results")
        for i, r in enumerate(result):
            print(f"\nResult {i+1}: {r}")
    else:
        print(f"Result: {result}")

if __name__ == "__main__":
    asyncio.run(test_single_buffer())
