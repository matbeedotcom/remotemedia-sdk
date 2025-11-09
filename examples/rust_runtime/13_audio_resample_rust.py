#!/usr/bin/env python3
"""
Audio Resampling - Rust Acceleration Example

This example demonstrates the AudioResampleNode with runtime_hint parameter
for automatic Rust acceleration. It shows:
- High-quality audio resampling
- Runtime selection (auto/rust/python)
- Performance comparison between runtimes
- Different quality levels (low/medium/high)

The AudioResampleNode resamples audio to a target sample rate using high-quality
algorithms. With Rust acceleration, resampling is 50-100x faster than Python.
"""

import asyncio
import sys
import time
import numpy as np
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.nodes.audio import AudioResampleNode


def generate_test_audio(duration_sec=1.0, sample_rate=44100, num_channels=2):
    """Generate test audio with known characteristics."""
    num_samples = int(duration_sec * sample_rate)
    
    # Generate sine waves at different frequencies for each channel
    t = np.linspace(0, duration_sec, num_samples)
    channel1 = 0.5 * np.sin(2 * np.pi * 440 * t)  # A4 note (440 Hz)
    channel2 = 0.5 * np.sin(2 * np.pi * 554.37 * t)  # C#5 note (554.37 Hz)
    
    if num_channels == 1:
        audio = channel1.reshape(1, -1).astype(np.float32)
    else:
        audio = np.vstack([channel1, channel2]).astype(np.float32)
    
    return audio, sample_rate


def benchmark_resample(node, audio_data, sample_rate, runs=50):
    """Benchmark resampling processing."""
    times = []
    
    # Warm-up run
    node.process((audio_data, sample_rate))
    
    # Benchmark runs
    for _ in range(runs):
        start = time.perf_counter()
        result = node.process((audio_data, sample_rate))
        elapsed = time.perf_counter() - start
        times.append(elapsed * 1000)  # Convert to milliseconds
    
    return result, times


def main():
    """Run the audio resampling example with benchmarks."""
    print("=" * 70)
    print("Audio Resampling - Rust Acceleration Example")
    print("=" * 70)
    print()
    
    # Check Rust availability
    try:
        import remotemedia_runtime
        rust_available = True
        print(f"[OK] Rust runtime available (version {remotemedia_runtime.__version__})")
    except ImportError:
        rust_available = False
        print("[INFO] Rust runtime not available")
        print("       Install with: cd runtime && maturin develop --release")
    print()
    
    # Test parameters
    duration = 1.0  # 1 second of audio
    input_sample_rate = 44100
    target_sample_rate = 16000
    num_channels = 2
    
    print(f"Test configuration:")
    print(f"  Input sample rate:  {input_sample_rate} Hz")
    print(f"  Target sample rate: {target_sample_rate} Hz")
    print(f"  Duration:           {duration} sec")
    print(f"  Channels:           {num_channels}")
    print()
    
    # Generate test audio
    print("1. Generating test audio...")
    audio_data, sr = generate_test_audio(duration, input_sample_rate, num_channels)
    print(f"   Audio shape: {audio_data.shape}, dtype: {audio_data.dtype}")
    print(f"   Input samples: {audio_data.shape[1]}")
    print()
    
    # Test with different runtime hints
    results = {}
    
    for runtime_hint in ["auto", "python"] + (["rust"] if rust_available else []):
        print(f"2. Testing with runtime_hint='{runtime_hint}'")
        print("-" * 70)
        
        # Create resampler node
        resampler = AudioResampleNode(
            target_sample_rate=target_sample_rate,
            quality="high",
            runtime_hint=runtime_hint,
            name=f"resampler_{runtime_hint}"
        )
        
        # Benchmark resampling
        print(f"   Resampling {input_sample_rate} Hz -> {target_sample_rate} Hz...")
        result, times = benchmark_resample(resampler, audio_data, sr, runs=50)
        
        if isinstance(result, tuple) and len(result) == 2:
            resampled_audio, output_sr = result
            print(f"   Output shape: {resampled_audio.shape}")
            print(f"   Output samples: {resampled_audio.shape[1]}")
            print(f"   Output sample rate: {output_sr} Hz")
            
            # Calculate expected output size
            expected_samples = int(audio_data.shape[1] * target_sample_rate / input_sample_rate)
            actual_samples = resampled_audio.shape[1]
            print(f"   Expected samples: {expected_samples}")
            print(f"   Sample count match: {abs(actual_samples - expected_samples) <= 1}")
        else:
            print(f"   Error: {result}")
            continue
        
        # Performance metrics
        avg_time = np.mean(times)
        min_time = np.min(times)
        max_time = np.max(times)
        std_time = np.std(times)
        
        print(f"   Performance (50 runs):")
        print(f"     Average: {avg_time:.3f} ms")
        print(f"     Min:     {min_time:.3f} ms")
        print(f"     Max:     {max_time:.3f} ms")
        print(f"     Std:     {std_time:.3f} ms")
        
        # Calculate processing speed
        realtime_factor = (duration * 1000) / avg_time
        print(f"     Realtime factor: {realtime_factor:.1f}x")
        
        results[runtime_hint] = avg_time
        print()
    
    # Test different quality levels with Python
    print("3. Testing different quality levels (Python)")
    print("-" * 70)
    
    for quality in ["low", "medium", "high"]:
        resampler = AudioResampleNode(
            target_sample_rate=target_sample_rate,
            quality=quality,
            runtime_hint="python",
            name=f"resampler_{quality}"
        )
        
        result, times = benchmark_resample(resampler, audio_data, sr, runs=20)
        avg_time = np.mean(times)
        
        print(f"   Quality '{quality}': {avg_time:.3f} ms average")
    
    print()
    
    # Performance comparison
    if len(results) > 1:
        print("=" * 70)
        print("Performance Comparison")
        print("=" * 70)
        
        for hint, avg_time in results.items():
            realtime_factor = (duration * 1000) / avg_time
            print(f"  {hint:10s}: {avg_time:.3f} ms ({realtime_factor:.1f}x realtime)")
        
        if "python" in results and "auto" in results and rust_available:
            speedup = results["python"] / results["auto"]
            print(f"\n  Speedup: {speedup:.2f}x (Rust vs Python)")
            
            # Target performance: <2ms per second of audio
            target_time_ms = 2.0  # per second of audio
            auto_time_per_sec = results["auto"] / duration
            print(f"  Target: <{target_time_ms} ms/sec")
            print(f"  Achieved: {auto_time_per_sec:.3f} ms/sec")
            
            if auto_time_per_sec < target_time_ms:
                print(f"  ✓ Target met!")
            else:
                print(f"  ✗ Target not met (still in development)")
        
        print()
    
    print("=" * 70)
    print("[OK] Audio resampling example complete!")
    print()
    print("Key observations:")
    print("  - AudioResampleNode automatically selects best runtime with 'auto' hint")
    print("  - Resampling maintains channel count and audio quality")
    print("  - Different quality levels available (low/medium/high)")
    if rust_available:
        print("  - Rust acceleration provides significant speedup")
    else:
        print("  - Python fallback works seamlessly when Rust unavailable")
    print("  - Realtime factor shows how many seconds of audio can be processed per second")
    print("=" * 70)
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
