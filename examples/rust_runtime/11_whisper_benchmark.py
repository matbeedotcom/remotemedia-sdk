#!/usr/bin/env python3
"""
Whisper Transcription Benchmark: Python (WhisperX) vs Rust (rwhisper)

This benchmark compares real Whisper transcription performance between:
- Python: WhisperX (CTranslate2 optimized)
- Rust: rwhisper (Rust bindings to whisper.cpp)

Pipeline: Audio Input -> Resample (16kHz) -> Whisper Transcribe -> Output

Key Metrics:
- Transcription time
- Real-time factor (processing_time / audio_duration)
- Accuracy (word error rate if reference available)
- Memory usage
- CPU utilization
"""

import asyncio
import sys
import time
from pathlib import Path
import psutil
import os
import logging

# Configure logging to show INFO level from remotemedia
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.source import MediaReaderNode, AudioTrackSource
from remotemedia.nodes.audio import AudioTransform, AudioBuffer
from remotemedia.nodes.transcription import WhisperXTranscriber, RustWhisperTranscriber


async def benchmark_python_whisperx(audio_file: Path, model_size: str = "tiny"):
    """Benchmark Python WhisperX transcription."""
    print(f"\n{'='*70}")
    print(f"Python WhisperX Benchmark (model: {model_size})")
    print(f"{'='*70}\n")

    pipeline = Pipeline(name="WhisperX_Python")

    # Build pipeline
    pipeline.add_node(MediaReaderNode(
        path=str(audio_file),
        chunk_size=4096,
        name="MediaReader"
    ))

    pipeline.add_node(AudioTrackSource(name="AudioSource"))

    pipeline.add_node(AudioTransform(
        output_sample_rate=16000,
        output_channels=1,
        name="Resample"
    ))

    # Buffer audio to accumulate larger chunks for transcription
    # 2 seconds = 32000 samples at 16kHz
    pipeline.add_node(AudioBuffer(
        buffer_size_samples=32000,
        name="AudioBuffer"
    ))

    pipeline.add_node(WhisperXTranscriber(
        model_size=model_size,
        device="cpu",  # Change to "cuda" for GPU
        compute_type="float32",
        batch_size=16,
        language="en",
        name="WhisperX"
    ))

    # Track metrics
    process = psutil.Process(os.getpid())
    mem_before = process.memory_info().rss / 1024 / 1024  # MB

    start = time.perf_counter()

    try:
        # run() returns the collected results directly
        result = await pipeline.run(use_rust=False)

        # Result can be a single dict or a list of dicts
        if isinstance(result, list):
            results = [r for r in result if isinstance(r, dict) and "text" in r]
        elif isinstance(result, dict) and "text" in result:
            results = [result]
        else:
            results = []

        elapsed = time.perf_counter() - start
        mem_after = process.memory_info().rss / 1024 / 1024  # MB
        mem_used = mem_after - mem_before

        # Calculate metrics
        total_audio = sum(r['audio_duration'] for r in results) if results else 0
        rtf = elapsed / total_audio if total_audio > 0 else 0
        full_transcript = " ".join(r['text'] for r in results if r['text'].strip())

        print(f"[OK] Transcription completed")
        print(f"\n  Time: {elapsed:.2f}s")
        print(f"  Audio duration: {total_audio:.2f}s")
        print(f"  Real-time factor: {rtf:.3f}x")
        print(f"  Memory used: {mem_used:.1f} MB")
        print(f"  Chunks processed: {len(results)}")
        print(f"\nTranscript:\n{'-'*70}")
        print(full_transcript)
        print(f"{'-'*70}")

        return {
            "runtime": "Python WhisperX",
            "model": model_size,
            "time": elapsed,
            "audio_duration": total_audio,
            "rtf": rtf,
            "memory_mb": mem_used,
            "transcript": full_transcript,
            "success": True
        }

    except ImportError as e:
        print(f"[ERROR] {e}")
        print("\nTo install WhisperX:")
        print("  pip install git+https://github.com/m-bain/whisperx.git")
        return {"runtime": "Python WhisperX", "success": False, "error": str(e)}
    except Exception as e:
        print(f"[ERROR] {e}")
        import traceback
        traceback.print_exc()
        return {"runtime": "Python WhisperX", "success": False, "error": str(e)}


async def benchmark_rust_rwhisper(audio_file: Path, model_source: str = "tiny"):
    """Benchmark Rust rwhisper transcription."""
    print(f"\n{'='*70}")
    print(f"Rust rwhisper Benchmark (model: {model_source})")
    print(f"{'='*70}\n")

    pipeline = Pipeline(name="Whisper_Rust")

    # Build pipeline
    pipeline.add_node(MediaReaderNode(
        path=str(audio_file),
        chunk_size=4096,
        name="MediaReader"
    ))

    pipeline.add_node(AudioTrackSource(name="AudioSource"))

    pipeline.add_node(AudioTransform(
        output_sample_rate=16000,
        output_channels=1,
        name="Resample"
    ))

    # Buffer audio to accumulate larger chunks for transcription
    # 2 seconds = 32000 samples at 16kHz
    pipeline.add_node(AudioBuffer(
        buffer_size_samples=32000,
        name="AudioBuffer"
    ))

    pipeline.add_node(RustWhisperTranscriber(
        model_source=model_source,
        language="en",
        n_threads=4,
        name="RustWhisper"
    ))

    # Track metrics
    process = psutil.Process(os.getpid())
    mem_before = process.memory_info().rss / 1024 / 1024  # MB

    start = time.perf_counter()

    try:
        # run() returns the collected results directly
        result = await pipeline.run(use_rust=True)

        # Result can be a single dict or a list of dicts
        if isinstance(result, list):
            results = [r for r in result if isinstance(r, dict) and "text" in r]
        elif isinstance(result, dict) and "text" in result:
            results = [result]
        else:
            results = []

        elapsed = time.perf_counter() - start
        mem_after = process.memory_info().rss / 1024 / 1024  # MB
        mem_used = mem_after - mem_before

        # Calculate metrics
        total_audio = sum(r['audio_duration'] for r in results) if results else 0
        rtf = elapsed / total_audio if total_audio > 0 else 0
        full_transcript = " ".join(r['text'] for r in results if r['text'].strip())

        print(f"[OK] Transcription completed")
        print(f"\n  Time: {elapsed:.2f}s")
        print(f"  Audio duration: {total_audio:.2f}s")
        print(f"  Real-time factor: {rtf:.3f}x")
        print(f"  Memory used: {mem_used:.1f} MB")
        print(f"  Chunks processed: {len(results)}")
        print(f"\nTranscript:\n{'-'*70}")
        print(full_transcript)
        print(f"{'-'*70}")

        return {
            "runtime": "Rust rwhisper",
            "model": model_source,
            "time": elapsed,
            "audio_duration": total_audio,
            "rtf": rtf,
            "memory_mb": mem_used,
            "transcript": full_transcript,
            "success": True
        }

    except Exception as e:
        print(f"[ERROR] {e}")
        print("\nNotes:")
        print("  - Ensure Rust runtime is built with whisper feature:")
        print("    cd runtime && maturin develop --release --features whisper")
        print("  - Download GGML model files from HuggingFace")
        import traceback
        traceback.print_exc()
        return {"runtime": "Rust rwhisper", "success": False, "error": str(e)}


async def main():
    """Run the Whisper benchmark comparison."""
    print("=" * 70)
    print("Whisper Transcription Benchmark")
    print("Python (WhisperX) vs Rust (rwhisper)")
    print("=" * 70)

    # Check audio file (relative to project root)
    script_dir = Path(__file__).parent
    audio_file = script_dir.parent / "transcribe_demo.wav"

    if not audio_file.exists():
        print(f"\n[ERROR] Audio file not found: {audio_file}")
        print("Please provide an audio file for transcription testing.")
        return 1

    # Configuration
    python_model_size = "tiny"  # Options: tiny, base, small, medium, large-v3
    rust_model_source = "tiny"  # Options: tiny, tiny.en, base, base.en, small, small.en

    print(f"\nConfiguration:")
    print(f"  Audio file: {audio_file}")
    print(f"  Python model: {python_model_size}")
    print(f"  Rust model: {rust_model_source}")
    print()

    # Run benchmarks
    python_result = await benchmark_python_whisperx(audio_file, python_model_size)
    rust_result = await benchmark_rust_rwhisper(audio_file, rust_model_source)
    print("Python Results:", python_result)
    print("Rust Results:", rust_result)
    # Compare results
    print(f"\n{'='*70}")
    print("Benchmark Comparison")
    print(f"{'='*70}\n")

    # if python_result["success"] and rust_result["success"]:
    #     print(f"{'Metric':<25} {'Python WhisperX':<20} {'Rust rwhisper':<20}")
    #     print(f"{'-'*70}")
    #     print(f"{'Time':<25} {python_result['time']:.2f}s{'':<14} {rust_result['time']:.2f}s")
    #     print(f"{'Audio Duration':<25} {python_result['audio_duration']:.2f}s{'':<14} {rust_result['audio_duration']:.2f}s")
    #     print(f"{'Real-Time Factor':<25} {python_result['rtf']:.3f}x{'':<15} {rust_result['rtf']:.3f}x")
    #     print(f"{'Memory Used':<25} {python_result['memory_mb']:.1f} MB{'':<13} {rust_result['memory_mb']:.1f} MB")

    #     speedup = python_result['time'] / rust_result['time']
    #     print(f"\n{'Speedup:':<25} Rust is {speedup:.2f}x faster")

    #     # Compare transcripts (basic comparison)
    #     py_words = set(python_result['transcript'].lower().split())
    #     rust_words = set(rust_result['transcript'].lower().split())
    #     overlap = len(py_words & rust_words)
    #     total = len(py_words | rust_words)
    #     similarity = overlap / total if total > 0 else 0

    #     print(f"{'Transcript Similarity:':<25} {similarity*100:.1f}%")
    #     print(f"\n{'='*70}")
    #     print("Notes:")
    #     print(f"{'='*70}")
    #     print("- Real-time factor < 1.0 means faster than real-time")
    #     print("- Lower RTF is better for real-time applications")
    #     print("- Memory usage varies by model size and implementation")
    #     print("- Transcript similarity is a rough comparison (word overlap)")
    #     print("- For production, consider:")
    #     print("  * GPU acceleration (CUDA)")
    #     print("  * Larger models for better accuracy")
    #     print("  * Batching strategies")
    #     print("  * Concurrent stream processing")

    # else:
    #     print("\n[WARN] Benchmark incomplete - check errors above")
    #     if not python_result["success"]:
    #         print(f"  Python WhisperX: {python_result.get('error', 'Unknown error')}")
    #     if not rust_result["success"]:
    #         print(f"  Rust rwhisper: {rust_result.get('error', 'Unknown error')}")

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
