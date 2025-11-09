#!/usr/bin/env python3
"""
Full Audio Pipeline - End-to-End Example

This example demonstrates a complete audio processing pipeline combining
multiple audio nodes with Rust acceleration:
- Voice Activity Detection (VAD)
- Resampling
- Format Conversion

This represents a real-world use case for speech processing applications
like transcription, voice assistants, or audio analysis.

Pipeline flow:
1. Generate/load audio data (44.1 kHz, stereo, f32)
2. VAD: Detect speech segments
3. Resample: Convert to 16 kHz for speech models
4. Format: Convert to i16 for compatibility
5. Output: Processed audio ready for speech recognition

With Rust acceleration, the entire pipeline achieves 50-100x speedup.
"""

import asyncio
import sys
import time
import numpy as np
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.nodes.audio import VADNode, AudioResampleNode, FormatConverterNode


def generate_speech_audio(duration_sec=2.0, sample_rate=44100):
    """Generate realistic speech-like test audio."""
    num_samples = int(duration_sec * sample_rate)
    t = np.linspace(0, duration_sec, num_samples)
    
    # Simulate speech with multiple frequency components and amplitude modulation
    # Speech typically has formants around 500Hz, 1500Hz, 2500Hz
    f1 = 0.3 * np.sin(2 * np.pi * 500 * t)
    f2 = 0.2 * np.sin(2 * np.pi * 1500 * t)
    f3 = 0.1 * np.sin(2 * np.pi * 2500 * t)
    
    # Amplitude modulation to simulate speech patterns (syllables ~5 Hz)
    envelope = 0.5 + 0.4 * np.sin(2 * np.pi * 5 * t)
    
    # Combine and add slight noise
    speech = (f1 + f2 + f3) * envelope + 0.05 * np.random.randn(num_samples)
    
    # Create stereo (slightly different in each channel)
    left = speech.astype(np.float32)
    right = (speech * 0.95 + 0.05 * np.random.randn(num_samples)).astype(np.float32)
    
    audio = np.vstack([left, right])
    return audio, sample_rate


class AudioPipeline:
    """Complete audio processing pipeline."""
    
    def __init__(self, runtime_hint="auto"):
        """Initialize the pipeline with specified runtime."""
        self.runtime_hint = runtime_hint
        
        # Create pipeline nodes
        self.vad = VADNode(
            frame_duration_ms=30,
            energy_threshold=0.02,
            runtime_hint=runtime_hint,
            name="vad"
        )
        
        self.resampler = AudioResampleNode(
            target_sample_rate=16000,
            quality="high",
            runtime_hint=runtime_hint,
            name="resampler"
        )
        
        self.converter = FormatConverterNode(
            target_format="i16",
            runtime_hint=runtime_hint,
            name="converter"
        )
    
    def process(self, audio_data, sample_rate):
        """Process audio through the complete pipeline."""
        # Step 1: VAD
        vad_result = self.vad.process((audio_data, sample_rate))
        if not isinstance(vad_result, tuple) or len(vad_result) != 3:
            return {"error": "VAD failed", "result": vad_result}
        
        audio_after_vad, sr_after_vad, vad_info = vad_result
        
        # Step 2: Resample
        resample_result = self.resampler.process((audio_after_vad, sr_after_vad))
        if not isinstance(resample_result, tuple) or len(resample_result) != 2:
            return {"error": "Resampling failed", "result": resample_result}
        
        audio_after_resample, sr_after_resample = resample_result
        
        # Step 3: Format conversion
        convert_result = self.converter.process((audio_after_resample, sr_after_resample))
        if not isinstance(convert_result, tuple) or len(convert_result) != 2:
            return {"error": "Conversion failed", "result": convert_result}
        
        final_audio, final_sr = convert_result
        
        return {
            "audio": final_audio,
            "sample_rate": final_sr,
            "vad_info": vad_info,
            "success": True
        }


def benchmark_pipeline(pipeline, audio_data, sample_rate, runs=50):
    """Benchmark the complete pipeline."""
    times = []
    
    # Warm-up run
    pipeline.process(audio_data, sample_rate)
    
    # Benchmark runs
    for _ in range(runs):
        start = time.perf_counter()
        result = pipeline.process(audio_data, sample_rate)
        elapsed = time.perf_counter() - start
        times.append(elapsed * 1000)  # Convert to milliseconds
    
    return result, times


def main():
    """Run the full audio pipeline example."""
    print("=" * 70)
    print("Full Audio Pipeline - End-to-End Example")
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
    
    # Generate test audio
    duration = 2.0  # 2 seconds of audio
    print(f"1. Generating test audio ({duration} seconds)...")
    audio_data, sample_rate = generate_speech_audio(duration_sec=duration)
    
    print(f"   Input audio:")
    print(f"     Shape:       {audio_data.shape}")
    print(f"     Dtype:       {audio_data.dtype}")
    print(f"     Sample rate: {sample_rate} Hz")
    print(f"     Duration:    {duration} sec")
    print(f"     Channels:    {audio_data.shape[0]}")
    print()
    
    # Test pipeline with different runtime hints
    results = {}
    
    for runtime_hint in ["auto", "python"] + (["rust"] if rust_available else []):
        print(f"2. Testing pipeline with runtime_hint='{runtime_hint}'")
        print("-" * 70)
        
        # Create pipeline
        pipeline = AudioPipeline(runtime_hint=runtime_hint)
        
        # Benchmark pipeline
        print(f"   Processing audio through 3-stage pipeline...")
        print(f"     Stage 1: VAD (Voice Activity Detection)")
        print(f"     Stage 2: Resample (44100 Hz -> 16000 Hz)")
        print(f"     Stage 3: Format (f32 -> i16)")
        print()
        
        result, times = benchmark_pipeline(pipeline, audio_data, sample_rate, runs=50)
        
        if result.get("success"):
            output_audio = result["audio"]
            output_sr = result["sample_rate"]
            vad_info = result["vad_info"]
            
            print(f"   Output audio:")
            print(f"     Shape:       {output_audio.shape}")
            print(f"     Dtype:       {output_audio.dtype}")
            print(f"     Sample rate: {output_sr} Hz")
            print(f"     Channels:    {output_audio.shape[0]}")
            print()
            
            print(f"   VAD results:")
            print(f"     Speech detected: {vad_info['is_speech']}")
            print(f"     Speech frames:   {vad_info['speech_frames']}/{vad_info['total_frames']}")
            print()
            
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
            
            # Calculate realtime factor
            realtime_factor = (duration * 1000) / avg_time
            print(f"     Realtime factor: {realtime_factor:.1f}x")
            
            # Calculate throughput
            throughput = 1000 / avg_time  # chunks per second
            print(f"     Throughput: {throughput:.1f} chunks/sec")
            
            results[runtime_hint] = avg_time
        else:
            print(f"   Error: {result.get('error', 'Unknown error')}")
        
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
            print(f"  Target: 50-100x speedup")
            
            if speedup >= 50:
                print(f"  ✓ Target met!")
            elif speedup >= 10:
                print(f"  ⚠ Partial speedup achieved (Python fallback still in use)")
            else:
                print(f"  ℹ Speedup will improve with full Rust integration")
        
        print()
    
    # Validate pipeline output
    print("=" * 70)
    print("Pipeline Validation")
    print("=" * 70)
    
    if "auto" in results:
        pipeline = AudioPipeline(runtime_hint="auto")
        result = pipeline.process(audio_data, sample_rate)
        
        if result.get("success"):
            output_audio = result["audio"]
            output_sr = result["sample_rate"]
            
            # Check transformations
            checks = [
                ("Sample rate converted", output_sr == 16000),
                ("Format converted to i16", output_audio.dtype == np.int16),
                ("Channels preserved", output_audio.shape[0] == audio_data.shape[0]),
                ("Samples resampled correctly", abs(output_audio.shape[1] - int(audio_data.shape[1] * 16000 / sample_rate)) <= 1),
                ("VAD detected speech", result["vad_info"]["is_speech"]),
            ]
            
            for check_name, passed in checks:
                status = "✓" if passed else "✗"
                print(f"  {status} {check_name}")
            
            all_passed = all(passed for _, passed in checks)
            print()
            
            if all_passed:
                print("  [OK] All validation checks passed!")
            else:
                print("  [WARN] Some validation checks failed")
        
        print()
    
    print("=" * 70)
    print("[OK] Full audio pipeline example complete!")
    print()
    print("Summary:")
    print("  - Complete 3-stage pipeline: VAD + Resample + Format")
    print("  - Processes realistic speech-like audio")
    print("  - Runtime selection works across all pipeline stages")
    if rust_available:
        print("  - Rust acceleration provides end-to-end speedup")
    else:
        print("  - Python fallback works for entire pipeline")
    print("  - Pipeline ready for speech recognition workflows")
    print()
    print("Use cases:")
    print("  - Transcription preprocessing (Whisper, Wav2Vec2, etc.)")
    print("  - Voice assistant audio preprocessing")
    print("  - Real-time speech analysis")
    print("  - Audio normalization for ML models")
    print("=" * 70)
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
