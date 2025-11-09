#!/usr/bin/env python3
"""
Test WhisperX transcription with RemoteMedia SDK.

This script tests the Python WhisperX implementation before
comparing it with the Rust rwhisper implementation.
"""

import asyncio
import sys
import time
from pathlib import Path
import logging

# Enable logging to see what's happening
logging.basicConfig(level=logging.INFO, format='%(name)s - %(levelname)s - %(message)s')

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.source import MediaReaderNode, AudioTrackSource
from remotemedia.nodes.audio import AudioTransform, AudioBuffer
from remotemedia.nodes.transcription import WhisperXTranscriber


async def test_whisperx_transcription():
    """Test WhisperX transcription pipeline."""
    print("=" * 70)
    print("WhisperX Transcription Test")
    print("=" * 70)

    # Check audio file
    audio_file = Path("examples/transcribe_demo.wav")
    if not audio_file.exists():
        print(f"\n[ERROR] Audio file not found: {audio_file}")
        return 1

    print(f"\nAudio file: {audio_file}")
    print("\nBuilding pipeline:")
    print("  Audio Input -> Resample (16kHz) -> Buffer (2s) -> WhisperX Transcribe -> Output")
    print()

    # Build pipeline
    pipeline = Pipeline(name="WhisperXTest")

    # 1. Read audio file
    pipeline.add_node(MediaReaderNode(
        path=str(audio_file),
        chunk_size=4096,
        name="MediaReader"
    ))

    # 2. Extract audio track
    pipeline.add_node(AudioTrackSource(name="AudioSource"))

    # 3. Resample to 16kHz mono (Whisper requirement)
    pipeline.add_node(AudioTransform(
        output_sample_rate=16000,
        output_channels=1,
        name="Resample"
    ))

    # 4. Buffer audio to accumulate larger chunks for transcription
    # 2 seconds = 32000 samples at 16kHz
    pipeline.add_node(AudioBuffer(
        buffer_size_samples=32000,
        name="AudioBuffer"
    ))

    # 5. Transcribe with WhisperX
    # Using "tiny" model for fast testing - change to "base", "small", "medium", or "large-v3" for better accuracy
    pipeline.add_node(WhisperXTranscriber(
        model_size="tiny",  # Options: tiny, base, small, medium, large-v2, large-v3
        device="cpu",       # Change to "cuda" if you have GPU
        compute_type="float32",  # Options: float32, float16, int8
        batch_size=16,
        language="en",      # Set language or None for auto-detect
        align_model=False,  # Set True for word-level timestamps
        vad_onset=0.200,    # Lower threshold for speech detection (default: 0.500)
        vad_offset=0.200,   # Lower threshold for silence detection (default: 0.363)
        name="WhisperX"
    ))

    print("Starting transcription...")
    start = time.perf_counter()

    try:
        async with pipeline.managed_execution():
            results = []
            async for result in pipeline.process():
                if isinstance(result, dict) and "text" in result:
                    results.append(result)
                    print(f"\n[Transcription Result]")
                    print(f"  Text: {result['text']}")
                    print(f"  Language: {result.get('language', 'unknown')}")
                    print(f"  Audio duration: {result['audio_duration']:.2f}s")
                    if result.get('segments'):
                        print(f"  Segments: {len(result['segments'])}")

        elapsed = time.perf_counter() - start

        print("\n" + "=" * 70)
        print("Transcription Summary")
        print("=" * 70)
        print(f"  Total time: {elapsed:.2f}s")
        print(f"  Chunks processed: {len(results)}")
        if results:
            total_audio = sum(r['audio_duration'] for r in results)
            print(f"  Total audio: {total_audio:.2f}s")
            print(f"  Real-time factor: {elapsed / total_audio:.2f}x")
            print()
            print("Full transcript:")
            print("-" * 70)
            for r in results:
                if r['text'].strip():
                    print(r['text'])
        print("=" * 70)

        return 0

    except ImportError as e:
        print(f"\n[ERROR] {e}")
        print("\nTo install WhisperX:")
        print("  pip install git+https://github.com/m-bain/whisperx.git")
        print("\nNote: WhisperX requires:")
        print("  - ffmpeg")
        print("  - For GPU: CUDA toolkit")
        return 1
    except Exception as e:
        print(f"\n[ERROR] Transcription failed: {e}")
        import traceback
        traceback.print_exc()
        return 1


async def main():
    """Run the WhisperX test."""
    return await test_whisperx_transcription()


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
