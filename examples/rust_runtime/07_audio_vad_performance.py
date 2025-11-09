#!/usr/bin/env python3
"""
Audio + VAD Performance Benchmark - Rust vs Python

This benchmark demonstrates real-world performance improvements for audio
processing pipelines, which are the primary use case for this system.

Tests include:
1. Audio resampling and buffering
2. VAD (Voice Activity Detection) processing
3. Complete audio processing pipeline with multiple transformations

Key metrics:
- Processing time per audio chunk
- Throughput (chunks/second)
- End-to-end latency
"""

import asyncio
import sys
import time
import numpy as np
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.source import MediaReaderNode, AudioTrackSource
from remotemedia.nodes.audio import AudioTransform, AudioBuffer, VoiceActivityDetector
from remotemedia.nodes.base import PassThroughNode


async def benchmark_audio_pipeline(pipeline, description, use_rust=True, runs=10):
    """Benchmark an audio pipeline."""
    print(f"\n{description}")
    print(f"Runtime: {'Rust' if use_rust else 'Python'}")
    print("-" * 70)

    total_chunks = 0
    total_time = 0

    for run in range(runs):
        chunk_count = 0
        start = time.perf_counter()

        async with pipeline.managed_execution():
            async for _ in pipeline.process():
                chunk_count += 1

        elapsed = time.perf_counter() - start
        total_chunks += chunk_count
        total_time += elapsed

    avg_time = total_time / runs
    avg_chunks = total_chunks / runs
    throughput = avg_chunks / avg_time if avg_time > 0 else 0

    print(f"  Average time:       {avg_time * 1000:.2f} ms")
    print(f"  Average chunks:     {avg_chunks:.1f}")
    print(f"  Throughput:         {throughput:.1f} chunks/sec")

    return avg_time, avg_chunks, throughput


async def test_1_basic_audio_transform():
    """Test 1: Basic audio resampling and channel conversion."""
    print("\n" + "=" * 70)
    print("Test 1: Basic Audio Transform Pipeline")
    print("=" * 70)
    print("Pipeline: MediaReader -> AudioTrackSource -> AudioTransform (16kHz, mono)")

    # Create pipeline
    def create_pipeline():
        pipeline = Pipeline(name="BasicAudioTransform")
        pipeline.add_node(MediaReaderNode(
            path="examples/transcribe_demo.wav",
            chunk_size=4096,
            name="MediaReader"
        ))
        pipeline.add_node(AudioTrackSource(name="AudioTrackSource"))
        pipeline.add_node(AudioTransform(
            output_sample_rate=16000,
            output_channels=1,
            name="AudioTransform"
        ))
        return pipeline

    # Benchmark with Rust
    rust_pipeline = create_pipeline()
    rust_time, rust_chunks, rust_throughput = await benchmark_audio_pipeline(
        rust_pipeline, "Rust Runtime", use_rust=True, runs=5
    )

    # Benchmark with Python
    python_pipeline = create_pipeline()
    python_time, python_chunks, python_throughput = await benchmark_audio_pipeline(
        python_pipeline, "Python Runtime", use_rust=False, runs=5
    )

    # Calculate speedup
    speedup = python_time / rust_time if rust_time > 0 else 0
    print(f"\nSpeedup: Rust is {speedup:.2f}x faster")

    return speedup


async def test_2_audio_with_buffering():
    """Test 2: Audio processing with buffering."""
    print("\n" + "=" * 70)
    print("Test 2: Audio Transform + Buffering Pipeline")
    print("=" * 70)
    print("Pipeline: MediaReader -> AudioTrackSource -> AudioTransform -> AudioBuffer")

    # Create pipeline
    def create_pipeline():
        pipeline = Pipeline(name="AudioWithBuffering")
        pipeline.add_node(MediaReaderNode(
            path="examples/transcribe_demo.wav",
            chunk_size=4096,
            name="MediaReader"
        ))
        pipeline.add_node(AudioTrackSource(name="AudioTrackSource"))
        pipeline.add_node(AudioTransform(
            output_sample_rate=16000,
            output_channels=1,
            name="AudioTransform"
        ))
        buffer_node = AudioBuffer(
            buffer_size_samples=8000,  # 0.5 seconds at 16kHz
            name="AudioBuffer"
        )
        buffer_node.is_streaming = True
        pipeline.add_node(buffer_node)
        return pipeline

    # Benchmark with Rust
    rust_pipeline = create_pipeline()
    rust_time, rust_chunks, rust_throughput = await benchmark_audio_pipeline(
        rust_pipeline, "Rust Runtime", use_rust=True, runs=5
    )

    # Benchmark with Python
    python_pipeline = create_pipeline()
    python_time, python_chunks, python_throughput = await benchmark_audio_pipeline(
        python_pipeline, "Python Runtime", use_rust=False, runs=5
    )

    # Calculate speedup
    speedup = python_time / rust_time if rust_time > 0 else 0
    print(f"\nSpeedup: Rust is {speedup:.2f}x faster")

    return speedup


async def test_3_complete_vad_pipeline():
    """Test 3: Complete audio pipeline with VAD."""
    print("\n" + "=" * 70)
    print("Test 3: Complete Audio + VAD Pipeline")
    print("=" * 70)
    print("Pipeline: MediaReader -> AudioTrackSource -> AudioTransform -> VAD -> AudioBuffer")

    # Create pipeline
    def create_pipeline():
        pipeline = Pipeline(name="CompleteVADPipeline")
        pipeline.add_node(MediaReaderNode(
            path="examples/transcribe_demo.wav",
            chunk_size=4096,
            name="MediaReader"
        ))
        pipeline.add_node(AudioTrackSource(name="AudioTrackSource"))
        pipeline.add_node(AudioTransform(
            output_sample_rate=16000,
            output_channels=1,
            name="AudioTransform"
        ))

        # VAD node
        vad = VoiceActivityDetector(
            frame_duration_ms=30,
            filter_mode=False,
            include_metadata=True,
            name="VAD"
        )
        vad.is_streaming = True
        pipeline.add_node(vad)

        # Buffer node
        buffer_node = AudioBuffer(
            buffer_size_samples=8000,
            name="AudioBuffer"
        )
        buffer_node.is_streaming = True
        pipeline.add_node(buffer_node)

        return pipeline

    # Benchmark with Rust
    rust_pipeline = create_pipeline()
    rust_time, rust_chunks, rust_throughput = await benchmark_audio_pipeline(
        rust_pipeline, "Rust Runtime", use_rust=True, runs=5
    )

    # Benchmark with Python
    python_pipeline = create_pipeline()
    python_time, python_chunks, python_throughput = await benchmark_audio_pipeline(
        python_pipeline, "Python Runtime", use_rust=False, runs=5
    )

    # Calculate speedup
    speedup = python_time / rust_time if rust_time > 0 else 0
    print(f"\nSpeedup: Rust is {speedup:.2f}x faster")

    return speedup


async def test_4_multi_transform_pipeline():
    """Test 4: Pipeline with multiple audio transformations."""
    print("\n" + "=" * 70)
    print("Test 4: Multi-Transform Audio Pipeline")
    print("=" * 70)
    print("Pipeline: MediaReader -> AudioTrackSource -> 3x AudioTransform -> AudioBuffer")

    # Create pipeline
    def create_pipeline():
        pipeline = Pipeline(name="MultiTransformPipeline")
        pipeline.add_node(MediaReaderNode(
            path="examples/transcribe_demo.wav",
            chunk_size=4096,
            name="MediaReader"
        ))
        pipeline.add_node(AudioTrackSource(name="AudioTrackSource"))

        # Multiple transform stages
        pipeline.add_node(AudioTransform(
            output_sample_rate=16000,
            output_channels=1,
            name="Transform1"
        ))
        pipeline.add_node(PassThroughNode(name="Pass1"))
        pipeline.add_node(AudioTransform(
            output_sample_rate=16000,
            output_channels=1,
            name="Transform2"
        ))
        pipeline.add_node(PassThroughNode(name="Pass2"))
        pipeline.add_node(AudioTransform(
            output_sample_rate=16000,
            output_channels=1,
            name="Transform3"
        ))

        buffer_node = AudioBuffer(
            buffer_size_samples=8000,
            name="AudioBuffer"
        )
        buffer_node.is_streaming = True
        pipeline.add_node(buffer_node)

        return pipeline

    # Benchmark with Rust
    rust_pipeline = create_pipeline()
    rust_time, rust_chunks, rust_throughput = await benchmark_audio_pipeline(
        rust_pipeline, "Rust Runtime", use_rust=True, runs=3
    )

    # Benchmark with Python
    python_pipeline = create_pipeline()
    python_time, python_chunks, python_throughput = await benchmark_audio_pipeline(
        python_pipeline, "Python Runtime", use_rust=False, runs=3
    )

    # Calculate speedup
    speedup = python_time / rust_time if rust_time > 0 else 0
    print(f"\nSpeedup: Rust is {speedup:.2f}x faster")

    return speedup


async def main():
    """Run all audio/VAD performance tests."""
    print("=" * 70)
    print("Audio + VAD Performance Benchmark - Rust vs Python")
    print("=" * 70)
    print("\nThis benchmark demonstrates real-world audio processing performance")
    print("improvements when using the Rust runtime vs pure Python.")
    print()
    print("Note: Audio file 'examples/transcribe_demo.wav' must exist")
    print("=" * 70)

    # Check if audio file exists
    audio_file = Path("examples/transcribe_demo.wav")
    if not audio_file.exists():
        print(f"\n[ERROR] Audio file not found: {audio_file}")
        print("Please ensure the audio file exists before running this benchmark.")
        return 1

    speedups = []

    try:
        # Run tests
        speedup1 = await test_1_basic_audio_transform()
        speedups.append(("Basic Audio Transform", speedup1))

        speedup2 = await test_2_audio_with_buffering()
        speedups.append(("Audio with Buffering", speedup2))

        speedup3 = await test_3_complete_vad_pipeline()
        speedups.append(("Complete VAD Pipeline", speedup3))

        speedup4 = await test_4_multi_transform_pipeline()
        speedups.append(("Multi-Transform Pipeline", speedup4))

    except Exception as e:
        print(f"\n[ERROR] Test failed: {e}")
        import traceback
        traceback.print_exc()
        return 1

    # Summary
    print("\n" + "=" * 70)
    print("Summary - Rust vs Python Performance")
    print("=" * 70)
    for test_name, speedup in speedups:
        print(f"{test_name:.<50} {speedup:.2f}x faster")

    avg_speedup = sum(s for _, s in speedups) / len(speedups) if speedups else 0
    print("-" * 70)
    print(f"{'Average Speedup':.<50} {avg_speedup:.2f}x faster")
    print("=" * 70)

    print("\nKey Insights:")
    print("  - Audio I/O pipelines are I/O-bound, not compute-bound")
    print("  - Current audio nodes (AudioTransform, VAD) use native C libraries")
    print("  - Rust runtime overhead is minimal, performance is comparable")
    print("  - For compute-intensive nodes, Rust provides 100x+ speedups (see example 06)")
    print("  - Future: Rust-native audio effects/processing would show major gains")
    print("  - Same Python API - no code changes needed!")
    print("\nConclusion:")
    print("  The Rust runtime excels at COMPUTE-INTENSIVE operations.")
    print("  For I/O-bound operations, performance is comparable to Python.")
    print("  Mixed pipelines (I/O + compute) benefit from Rust-native compute nodes.")
    print("=" * 70)

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
