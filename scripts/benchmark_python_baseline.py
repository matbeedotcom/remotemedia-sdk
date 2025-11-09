#!/usr/bin/env python3
"""
Python Baseline Benchmarks for Audio Processing

This script benchmarks the Python (librosa/numpy) implementations to establish
baseline performance metrics. These results should be compared with the Rust
benchmarks to validate the 50-100x speedup claim.

Run this before running `cargo bench` to get Python baseline numbers.
"""

import sys
import time
import numpy as np
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "python-client"))

# Import librosa for resampling
try:
    import librosa
    HAS_LIBROSA = True
except ImportError:
    print("Warning: librosa not installed. Install with: pip install librosa")
    HAS_LIBROSA = False


def generate_test_audio(num_channels=2, num_samples=44100, dtype=np.float32):
    """Generate test audio data matching Rust benchmark."""
    # Generate sine wave at 440 Hz (A4 note)
    frequency = 440.0
    sample_rate = 44100.0
    
    t = np.arange(num_samples) / sample_rate
    signal = 0.5 * np.sin(2 * np.pi * frequency * t)
    
    # Create multi-channel audio
    if num_channels == 1:
        audio = signal.astype(dtype)
    else:
        audio = np.tile(signal, (num_channels, 1)).astype(dtype)
    
    return audio


def benchmark_func(func, *args, runs=50, warmup=5):
    """Benchmark a function with warmup and multiple runs."""
    # Warmup runs
    for _ in range(warmup):
        func(*args)
    
    # Benchmark runs
    times = []
    for _ in range(runs):
        start = time.perf_counter()
        result = func(*args)
        elapsed = time.perf_counter() - start
        times.append(elapsed * 1000)  # Convert to milliseconds
    
    return result, times


def python_resample(audio, input_sr, output_sr):
    """Python resampling using librosa."""
    if audio.ndim == 1:
        return librosa.resample(audio, orig_sr=input_sr, target_sr=output_sr)
    else:
        # Resample each channel
        resampled = []
        for channel in audio:
            resampled.append(librosa.resample(channel, orig_sr=input_sr, target_sr=output_sr))
        return np.array(resampled)


def python_vad(audio, sample_rate, frame_duration_ms, energy_threshold):
    """Python VAD using energy-based detection."""
    if audio.ndim > 1:
        # Convert to mono
        audio = np.mean(audio, axis=0)
    
    frame_samples = int(sample_rate * frame_duration_ms / 1000)
    num_frames = len(audio) // frame_samples
    
    speech_frames = 0
    for i in range(num_frames):
        start = i * frame_samples
        end = start + frame_samples
        frame = audio[start:end]
        
        # Calculate RMS energy
        energy = np.sqrt(np.mean(frame**2))
        
        if energy > energy_threshold:
            speech_frames += 1
    
    return {
        "is_speech": speech_frames > 0,
        "speech_frames": speech_frames,
        "total_frames": num_frames
    }


def python_format_convert(audio, target_format):
    """Python format conversion using numpy."""
    current_dtype = audio.dtype
    
    if target_format == "i16":
        if current_dtype == np.float32:
            return (audio * 32767.0).astype(np.int16)
        elif current_dtype == np.int32:
            return (audio / 65536).astype(np.int16)
        else:
            return audio.astype(np.int16)
    elif target_format == "f32":
        if current_dtype == np.int16:
            return audio.astype(np.float32) / 32768.0
        elif current_dtype == np.int32:
            return audio.astype(np.float32) / 2147483648.0
        else:
            return audio.astype(np.float32)
    elif target_format == "i32":
        if current_dtype == np.float32:
            return (audio * 2147483647.0).astype(np.int32)
        elif current_dtype == np.int16:
            return (audio.astype(np.int32) * 65536)
        else:
            return audio.astype(np.int32)
    else:
        raise ValueError(f"Unknown format: {target_format}")


def benchmark_resample():
    """Benchmark resampling (Python baseline)."""
    print("\n" + "=" * 70)
    print("RESAMPLE BENCHMARK - Python Baseline (librosa)")
    print("=" * 70)
    print("Target: <2ms per second of audio (Rust)")
    print()
    
    if not HAS_LIBROSA:
        print("Skipping: librosa not available")
        return
    
    input_sr = 44100
    output_sr = 16000
    
    for duration_secs in [1.0, 5.0, 10.0]:
        num_samples = int(input_sr * duration_secs)
        audio = generate_test_audio(num_channels=2, num_samples=num_samples, dtype=np.float32)
        
        print(f"Duration: {duration_secs}s ({num_samples} samples)")
        result, times = benchmark_func(python_resample, audio, input_sr, output_sr, runs=20)
        
        avg_time = np.mean(times)
        min_time = np.min(times)
        max_time = np.max(times)
        
        # Calculate time per second of audio
        time_per_sec = avg_time / duration_secs
        
        print(f"  Average: {avg_time:.2f} ms ({time_per_sec:.2f} ms/sec of audio)")
        print(f"  Min:     {min_time:.2f} ms")
        print(f"  Max:     {max_time:.2f} ms")
        print(f"  Output shape: {result.shape}")
        print()


def benchmark_vad():
    """Benchmark VAD (Python baseline)."""
    print("\n" + "=" * 70)
    print("VAD BENCHMARK - Python Baseline (numpy)")
    print("=" * 70)
    print("Target: <50μs per 30ms frame (Rust)")
    print()
    
    sample_rate = 16000
    frame_duration_ms = 30
    energy_threshold = 0.02
    
    for num_frames in [1, 10, 33]:  # 33 frames = 1 second
        frame_samples = (sample_rate * frame_duration_ms) // 1000
        total_samples = frame_samples * num_frames
        audio = generate_test_audio(num_channels=1, num_samples=total_samples, dtype=np.float32)
        
        print(f"Frames: {num_frames} ({total_samples} samples)")
        result, times = benchmark_func(python_vad, audio, sample_rate, frame_duration_ms, energy_threshold, runs=100)
        
        avg_time = np.mean(times)
        min_time = np.min(times)
        time_per_frame = avg_time / num_frames
        
        print(f"  Average: {avg_time:.2f} ms ({time_per_frame:.2f} ms/frame or {time_per_frame*1000:.0f} μs/frame)")
        print(f"  Min:     {min_time:.2f} ms")
        print(f"  Result:  {result}")
        print()


def benchmark_format_conversion():
    """Benchmark format conversion (Python baseline)."""
    print("\n" + "=" * 70)
    print("FORMAT CONVERSION BENCHMARK - Python Baseline (numpy)")
    print("=" * 70)
    print("Target: <100μs for 1M samples (Rust)")
    print()
    
    num_samples = 500_000  # 500k per channel = 1M total
    num_channels = 2
    total_samples = num_samples * num_channels
    
    conversions = [
        ("f32", "i16", np.float32),
        ("i16", "f32", np.int16),
        ("f32", "i32", np.float32),
        ("i16", "i32", np.int16),
    ]
    
    for source_format, target_format, dtype in conversions:
        audio = generate_test_audio(num_channels=num_channels, num_samples=num_samples, dtype=dtype)
        
        print(f"Conversion: {source_format} -> {target_format} ({total_samples} samples)")
        result, times = benchmark_func(python_format_convert, audio, target_format, runs=100)
        
        avg_time = np.mean(times)
        min_time = np.min(times)
        
        # Calculate in microseconds
        avg_time_us = avg_time * 1000
        min_time_us = min_time * 1000
        
        print(f"  Average: {avg_time:.3f} ms ({avg_time_us:.0f} μs)")
        print(f"  Min:     {min_time:.3f} ms ({min_time_us:.0f} μs)")
        print(f"  Throughput: {total_samples / (avg_time / 1000) / 1e6:.1f}M samples/sec")
        print()


def benchmark_full_pipeline():
    """Benchmark full pipeline (Python baseline)."""
    print("\n" + "=" * 70)
    print("FULL PIPELINE BENCHMARK - Python Baseline")
    print("=" * 70)
    print("Pipeline: VAD + Resample + Format Conversion")
    print()
    
    if not HAS_LIBROSA:
        print("Skipping: librosa not available")
        return
    
    input_sr = 44100
    output_sr = 16000
    duration_secs = 1.0
    num_samples = int(input_sr * duration_secs)
    
    audio = generate_test_audio(num_channels=2, num_samples=num_samples, dtype=np.float32)
    
    def pipeline(audio):
        # Stage 1: VAD
        vad_result = python_vad(audio, input_sr, 30, 0.02)
        
        # Stage 2: Resample
        resampled = python_resample(audio, input_sr, output_sr)
        
        # Stage 3: Format conversion
        converted = python_format_convert(resampled, "i16")
        
        return converted, vad_result
    
    print(f"Audio: {duration_secs}s, {num_samples} samples @ {input_sr} Hz")
    result, times = benchmark_func(pipeline, audio, runs=50)
    
    avg_time = np.mean(times)
    min_time = np.min(times)
    max_time = np.max(times)
    
    converted_audio, vad_result = result
    
    print(f"  Average: {avg_time:.2f} ms")
    print(f"  Min:     {min_time:.2f} ms")
    print(f"  Max:     {max_time:.2f} ms")
    print(f"  Output shape: {converted_audio.shape}, dtype: {converted_audio.dtype}")
    print(f"  VAD result: {vad_result}")
    print()


def main():
    """Run all Python baseline benchmarks."""
    print("=" * 70)
    print("Python Baseline Benchmarks")
    print("=" * 70)
    print()
    print("These benchmarks establish Python performance baselines using:")
    print("  - librosa for resampling")
    print("  - numpy for format conversion")
    print("  - numpy for energy-based VAD")
    print()
    print("Compare these results with `cargo bench` to verify speedup.")
    print()
    print("Expected speedup from Rust implementation: 50-100x")
    print("=" * 70)
    
    benchmark_resample()
    benchmark_vad()
    benchmark_format_conversion()
    benchmark_full_pipeline()
    
    print("\n" + "=" * 70)
    print("Python Baseline Benchmarks Complete")
    print("=" * 70)
    print()
    print("Next steps:")
    print("  1. Run: cd runtime && cargo bench --bench audio_nodes")
    print("  2. Compare Rust results with Python baselines above")
    print("  3. Calculate speedup: Python_time / Rust_time")
    print("  4. Verify 50-100x speedup target is met")
    print("=" * 70)
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
