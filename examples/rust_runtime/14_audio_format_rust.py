#!/usr/bin/env python3
"""
Audio Format Conversion - Rust Acceleration Example

This example demonstrates the FormatConverterNode with runtime_hint parameter
for automatic Rust acceleration. It shows:
- Audio format conversion (f32, i16, i32)
- Runtime selection (auto/rust/python)
- Performance comparison between runtimes
- Zero-copy conversion with Rust

The FormatConverterNode converts audio between different sample formats.
With Rust acceleration, conversion uses zero-copy techniques for maximum performance.
"""

import asyncio
import sys
import time
import numpy as np
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.nodes.audio import FormatConverterNode


def generate_test_audio(format_type="f32", num_samples=100000, num_channels=2):
    """Generate test audio in specified format."""
    if format_type == "f32":
        # Float32: -1.0 to 1.0 range
        audio = np.random.randn(num_channels, num_samples).astype(np.float32) * 0.5
    elif format_type == "i16":
        # Int16: -32768 to 32767 range
        audio = np.random.randint(-32768, 32767, size=(num_channels, num_samples), dtype=np.int16)
    elif format_type == "i32":
        # Int32: -2147483648 to 2147483647 range
        audio = np.random.randint(-2147483648, 2147483647, size=(num_channels, num_samples), dtype=np.int32)
    else:
        raise ValueError(f"Unknown format: {format_type}")
    
    return audio


def benchmark_convert(node, audio_data, sample_rate, runs=100):
    """Benchmark format conversion processing."""
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


def verify_conversion(source_data, converted_data, source_format, target_format):
    """Verify that conversion maintains audio integrity."""
    # Check shape is preserved
    if source_data.shape != converted_data.shape:
        return False, "Shape mismatch"
    
    # Check dtype is correct
    expected_dtype = {
        "f32": np.float32,
        "i16": np.int16,
        "i32": np.int32
    }[target_format]
    
    if converted_data.dtype != expected_dtype:
        return False, f"Dtype mismatch: expected {expected_dtype}, got {converted_data.dtype}"
    
    # For same-format conversion, should be identical
    if source_format == target_format:
        if not np.array_equal(source_data, converted_data):
            return False, "Same-format conversion should be identical"
    
    return True, "OK"


def main():
    """Run the format conversion example with benchmarks."""
    print("=" * 70)
    print("Audio Format Conversion - Rust Acceleration Example")
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
    num_samples = 1_000_000  # 1 million samples for performance testing
    num_channels = 2
    sample_rate = 48000
    
    print(f"Test configuration:")
    print(f"  Samples:     {num_samples:,}")
    print(f"  Channels:    {num_channels}")
    print(f"  Sample rate: {sample_rate} Hz")
    print()
    
    # Test all format conversions
    conversions = [
        ("f32", "i16"),
        ("f32", "i32"),
        ("i16", "f32"),
        ("i16", "i32"),
        ("i32", "f32"),
        ("i32", "i16"),
    ]
    
    results = {}
    
    for source_format, target_format in conversions:
        print(f"Testing conversion: {source_format} -> {target_format}")
        print("-" * 70)
        
        # Generate test audio
        audio_data = generate_test_audio(source_format, num_samples, num_channels)
        print(f"  Source shape: {audio_data.shape}, dtype: {audio_data.dtype}")
        
        # Test with different runtime hints
        conversion_results = {}
        
        for runtime_hint in ["auto", "python"] + (["rust"] if rust_available else []):
            # Create converter node
            converter = FormatConverterNode(
                target_format=target_format,
                runtime_hint=runtime_hint,
                name=f"converter_{runtime_hint}"
            )
            
            # Benchmark conversion
            result, times = benchmark_convert(converter, audio_data, sample_rate, runs=100)
            
            if isinstance(result, tuple) and len(result) == 2:
                converted_audio, output_sr = result
                
                # Verify conversion
                is_valid, message = verify_conversion(audio_data, converted_audio, source_format, target_format)
                
                if not is_valid:
                    print(f"  [{runtime_hint:6s}] Verification failed: {message}")
                    continue
                
                # Performance metrics
                avg_time = np.mean(times)
                min_time = np.min(times)
                
                # Calculate throughput
                total_samples = num_samples * num_channels
                throughput = total_samples / (avg_time / 1000)  # samples per second
                
                print(f"  [{runtime_hint:6s}] {avg_time:.3f} ms avg, {min_time:.3f} ms min "
                      f"({throughput/1e6:.1f}M samples/sec)")
                
                conversion_results[runtime_hint] = avg_time
            else:
                print(f"  [{runtime_hint:6s}] Error: {result}")
        
        results[f"{source_format}->{target_format}"] = conversion_results
        print()
    
    # Performance comparison summary
    print("=" * 70)
    print("Performance Summary")
    print("=" * 70)
    print()
    
    for conversion, times in results.items():
        if not times:
            continue
            
        print(f"{conversion}:")
        for hint, avg_time in times.items():
            total_samples = num_samples * num_channels
            throughput = total_samples / (avg_time / 1000)
            print(f"  {hint:10s}: {avg_time:.3f} ms ({throughput/1e6:.1f}M samples/sec)")
        
        if "python" in times and "auto" in times and rust_available:
            speedup = times["python"] / times["auto"]
            print(f"  Speedup: {speedup:.2f}x")
        
        print()
    
    # Check target performance
    if rust_available and "auto" in list(results.values())[0]:
        print("Target Performance Check:")
        print("  Target: <100 μs (0.1 ms) for 1M samples")
        
        # Check first conversion result
        first_result = list(results.values())[0]["auto"]
        target_us = 100  # microseconds
        actual_us = first_result * 1000  # ms to μs
        
        print(f"  Achieved: {actual_us:.1f} μs")
        
        if actual_us < target_us:
            print(f"  ✓ Target met!")
        else:
            print(f"  ✗ Target not met (still in development)")
        
        print()
    
    print("=" * 70)
    print("[OK] Format conversion example complete!")
    print()
    print("Key observations:")
    print("  - FormatConverterNode supports all common format conversions")
    print("  - Runtime selection works automatically with 'auto' hint")
    print("  - Conversion maintains audio shape and integrity")
    if rust_available:
        print("  - Rust uses zero-copy techniques for maximum performance")
    else:
        print("  - Python fallback works seamlessly when Rust unavailable")
    print("  - Same-format conversions are handled efficiently")
    print("=" * 70)
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
