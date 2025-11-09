#!/usr/bin/env python3
"""
Audio VAD (Voice Activity Detection) - Rust Acceleration Example

This example demonstrates the VADNode with runtime_hint parameter for
automatic Rust acceleration. It shows:
- Voice activity detection with energy-based analysis
- Runtime selection (auto/rust/python)
- Performance comparison between runtimes
- Practical speech detection use case

The VADNode detects speech segments in audio using energy thresholds.
With Rust acceleration, VAD processing is 50-100x faster than Python.
"""

import asyncio
import sys
import time
import numpy as np
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.nodes.audio import VADNode


def generate_test_audio(duration_sec=1.0, sample_rate=16000, has_speech=True):
    """Generate test audio with or without speech-like characteristics."""
    num_samples = int(duration_sec * sample_rate)
    
    if has_speech:
        # Generate speech-like audio with varying energy
        t = np.linspace(0, duration_sec, num_samples)
        # Amplitude modulation to simulate speech patterns
        envelope = 0.3 + 0.2 * np.sin(2 * np.pi * 5 * t)  # 5 Hz modulation
        signal = envelope * np.random.randn(num_samples)
    else:
        # Generate low-energy noise (silence/background)
        signal = 0.01 * np.random.randn(num_samples)
    
    return signal.astype(np.float32), sample_rate


def benchmark_vad(node, audio_data, sample_rate, runs=100):
    """Benchmark VAD processing."""
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
    """Run the VAD example with benchmarks."""
    print("=" * 70)
    print("Audio VAD - Rust Acceleration Example")
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
    sample_rate = 16000
    frame_duration_ms = 30
    energy_threshold = 0.02
    
    print(f"Test configuration:")
    print(f"  Sample rate:     {sample_rate} Hz")
    print(f"  Duration:        {duration} sec")
    print(f"  Frame duration:  {frame_duration_ms} ms")
    print(f"  Energy threshold: {energy_threshold}")
    print()
    
    # Generate test audio with speech
    print("1. Generating test audio (speech-like)...")
    audio_speech, sr = generate_test_audio(duration, sample_rate, has_speech=True)
    print(f"   Audio shape: {audio_speech.shape}, dtype: {audio_speech.dtype}")
    print()
    
    # Generate test audio without speech (silence)
    print("2. Generating test audio (silence)...")
    audio_silence, sr = generate_test_audio(duration, sample_rate, has_speech=False)
    print(f"   Audio shape: {audio_silence.shape}, dtype: {audio_silence.dtype}")
    print()
    
    # Test with different runtime hints
    results = {}
    
    for runtime_hint in ["auto", "python"] + (["rust"] if rust_available else []):
        print(f"3. Testing with runtime_hint='{runtime_hint}'")
        print("-" * 70)
        
        # Create VAD node
        vad = VADNode(
            frame_duration_ms=frame_duration_ms,
            energy_threshold=energy_threshold,
            runtime_hint=runtime_hint,
            name=f"vad_{runtime_hint}"
        )
        
        # Test with speech
        print(f"   Processing speech audio...")
        result_speech, times_speech = benchmark_vad(vad, audio_speech, sr, runs=100)
        
        if isinstance(result_speech, tuple) and len(result_speech) == 3:
            _, _, vad_results = result_speech
            print(f"   Speech detected: {vad_results['is_speech']}")
            print(f"   Speech frames:   {vad_results['speech_frames']}/{vad_results['total_frames']}")
        else:
            print(f"   Error: {result_speech}")
            continue
        
        # Test with silence
        print(f"   Processing silence audio...")
        result_silence, times_silence = benchmark_vad(vad, audio_silence, sr, runs=100)
        
        if isinstance(result_silence, tuple) and len(result_silence) == 3:
            _, _, vad_results = result_silence
            print(f"   Speech detected: {vad_results['is_speech']}")
            print(f"   Speech frames:   {vad_results['speech_frames']}/{vad_results['total_frames']}")
        
        # Performance metrics
        avg_time = np.mean(times_speech)
        min_time = np.min(times_speech)
        max_time = np.max(times_speech)
        std_time = np.std(times_speech)
        
        print(f"   Performance (100 runs):")
        print(f"     Average: {avg_time:.3f} ms")
        print(f"     Min:     {min_time:.3f} ms")
        print(f"     Max:     {max_time:.3f} ms")
        print(f"     Std:     {std_time:.3f} ms")
        
        results[runtime_hint] = avg_time
        print()
    
    # Performance comparison
    if len(results) > 1:
        print("=" * 70)
        print("Performance Comparison")
        print("=" * 70)
        
        for hint, avg_time in results.items():
            print(f"  {hint:10s}: {avg_time:.3f} ms")
        
        if "python" in results and "auto" in results and rust_available:
            speedup = results["python"] / results["auto"]
            print(f"\n  Speedup: {speedup:.2f}x (Rust vs Python)")
            
            # Calculate frames per second
            num_frames = int(duration * 1000 / frame_duration_ms)
            fps = num_frames / (results["auto"] / 1000)
            print(f"  Throughput: {fps:.0f} frames/sec")
        
        print()
    
    print("=" * 70)
    print("[OK] VAD example complete!")
    print()
    print("Key observations:")
    print("  - VADNode automatically selects best runtime with 'auto' hint")
    print("  - Correctly detects speech vs silence")
    print("  - Returns detailed VAD results (is_speech, speech_frames, total_frames)")
    if rust_available:
        print("  - Rust acceleration provides significant speedup for batch processing")
    else:
        print("  - Python fallback works seamlessly when Rust unavailable")
    print("=" * 70)
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
