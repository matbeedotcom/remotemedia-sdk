# Rust vs Python Audio Performance Benchmarks

## Overview

This document compares the performance of Rust audio processing implementations against Python (librosa, numpy) baselines to validate the Phase 5 claim of 50-100x speedup.

## Test Environment

- **Hardware**: Windows system with standard CPU
- **Rust**: Release build with optimizations (`opt-level = 3`)
- **Python**: CPython with librosa, numpy
- **Audio Format**: Stereo (2 channels), 44.1kHz sample rate
- **Benchmark Framework**: Criterion.rs (Rust), custom Python benchmark

---

## Results Summary

### Performance Targets (from spec):
- ✅ Resample: <2ms per second of audio (target: 50-100x speedup)
- ✅ VAD: <50μs per 30ms frame (target: 50-100x speedup)
- ❌ Format conversion: <100μs for 1M samples (target: 50-100x speedup)

---

## 1. Audio Resampling (44.1kHz → 16kHz)

### Python Baseline (librosa)
```
Duration | Avg Time | Samples/sec | Quality
---------|----------|-------------|--------
1 second | 0.27 ms  | 163M/s      | High
5 second | 1.56 ms  | 141M/s      | High
10 second| 3.12 ms  | 141M/s      | High
```

### Rust Performance (rubato)
```
Duration | Quality | Avg Time | Samples/sec | Speedup
---------|---------|----------|-------------|--------
1 second | Low     | 1.29 ms  | 34.2M/s     | 0.21x ❌
1 second | Medium  | 1.32 ms  | 33.3M/s     | 0.20x ❌
1 second | High    | 1.30 ms  | 33.9M/s     | 0.21x ❌
5 second | Low     | 7.20 ms  | 30.6M/s     | 0.22x ❌
5 second | Medium  | 7.40 ms  | 29.8M/s     | 0.21x ❌
5 second | High    | 7.42 ms  | 29.7M/s     | 0.21x ❌
10 second| Low     | 15.26 ms | 28.9M/s     | 0.20x ❌
10 second| Medium  | 15.44 ms | 28.6M/s     | 0.18x ❌
10 second| High    | 15.09 ms | 29.2M/s     | 0.21x ❌
```

**Analysis**: 
- **Rust is ~5x SLOWER than Python** for resampling operations
- Target of <2ms/sec: ❌ Failed (Rust: 1.3ms for 1 sec, Python: 0.27ms)
- Likely cause: Node initialization overhead, async/await overhead, JSON serialization
- librosa uses highly optimized NumPy/SciPy routines under the hood
- **Conclusion**: For resampling, Python+librosa is significantly faster

---

## 2. Voice Activity Detection (VAD)

### Python Baseline (numpy energy-based)
```
Audio Duration | Frame Duration | Avg Time/Frame | Frames/sec
---------------|----------------|----------------|------------
1 second       | 30ms           | 4.97 μs       | 201,206/s
10 seconds     | 30ms           | 5.06 μs       | 197,628/s
33 seconds     | 30ms           | 5.08 μs       | 196,850/s
```

### Rust Performance (FFT-based energy VAD)
```
Audio Duration | Frame Duration | Avg Time/Frame | Frames/sec | Speedup
---------------|----------------|----------------|------------|---------
1 second       | 30ms           | 15.86 μs      | 63,057/s   | 0.31x ❌
10 seconds     | 30ms           | 109.86 μs     | 91,028/s   | 0.46x ❌
33 seconds     | 30ms           | 337.20 μs     | 97,864/s   | 0.50x ❌
```

**Analysis**:
- **Rust is ~2-3x SLOWER than Python** for VAD
- Target of <50μs/frame: ❌ Failed for 1-second audio (15.86 μs but includes overhead)
- Note: Rust VAD implementation includes FFT processing, Python uses simple energy calculation
- Large discrepancy in multi-second audio suggests batch processing overhead
- **Conclusion**: For VAD, Python+numpy is faster due to simpler algorithm

---

## 3. Audio Format Conversion

### Python Baseline (numpy)
```
Conversion    | Time (1M samples) | Samples/sec
--------------|-------------------|--------------
F32 → I16     | 1935 μs          | 517M/s
I16 → F32     | 1392 μs          | 718M/s
F32 → I32     | 1939 μs          | 516M/s
I16 → I32     | 1370 μs          | 730M/s
```

### Rust Performance (bytemuck + manual conversion)
```
Conversion    | Time (1M samples) | Samples/sec | Speedup
--------------|-------------------|-------------|----------
F32 → I16     | 22,177 μs        | 45.1M/s     | 0.087x ❌
I16 → F32     | 25,439 μs        | 39.3M/s     | 0.055x ❌
F32 → I32     | 29,104 μs        | 34.4M/s     | 0.067x ❌
I16 → I32     | 27,430 μs        | 36.5M/s     | 0.050x ❌
```

**Analysis**:
- **Rust is ~11-18x SLOWER than Python** for format conversion
- Target of <100μs for 1M samples: ❌ Failed (22-29ms vs target 100μs)
- Python numpy uses highly optimized SIMD vectorized operations
- Rust implementation likely has JSON parsing overhead, memory allocation overhead
- **Conclusion**: For format conversion, Python+numpy is dramatically faster

---

## 4. Full Pipeline (VAD → Resample → Format)

### Python Baseline
```
Average pipeline time: 0.80 ms (for 2 seconds of audio)
```

### Rust Performance
```
Pipeline time: 4.17 ms (for 2 seconds of audio, ~44,100 samples)
Throughput: 10.6M samples/sec
```

**Analysis**:
- **Rust is ~5.2x SLOWER than Python** for full pipeline
- Combined overhead of all three operations
- **Conclusion**: Full pipeline shows cumulative overhead issues

---

## Overall Conclusions

### Performance Target Assessment

| Operation    | Target        | Python   | Rust     | Status | Speedup Ratio |
|--------------|---------------|----------|----------|--------|---------------|
| Resample     | <2ms/sec      | 0.27 ms  | 1.30 ms  | ⚠️     | 0.21x (5x slower) |
| VAD          | <50μs/frame   | 5 μs     | 15.86 μs | ⚠️     | 0.31x (3x slower) |
| Format Conv  | <100μs/1M     | 1.4 ms   | 22.2 ms  | ❌     | 0.05x (16x slower) |
| Full Pipeline| -             | 0.80 ms  | 4.17 ms  | ❌     | 0.19x (5x slower) |

### Key Findings

1. **Rust is NOT faster than Python for these audio operations**
   - All Rust implementations are 3-18x slower than Python
   - 50-100x speedup claim: **Not achieved**

2. **Root Causes**:
   - **Python advantages**:
     - librosa/numpy use highly optimized C/Fortran/SIMD code underneath
     - Direct memory access without serialization overhead
     - Batch processing optimizations
   
   - **Rust disadvantages**:
     - Node abstraction overhead (JSON serialization/deserialization)
     - Async/await overhead for simple operations
     - Memory allocation and copying
     - Not using SIMD optimizations effectively
     - Individual sample processing vs batch operations

3. **Architecture Issues**:
   - The node-based abstraction adds significant overhead
   - JSON input/output adds serialization costs
   - NodeContext creation and initialization overhead
   - Async executor overhead inappropriate for CPU-bound work

### Recommendations

1. **Optimize Rust Implementation**:
   - Remove JSON serialization for internal nodes
   - Use direct buffer passing instead of Value types
   - Implement SIMD vectorization for format conversion
   - Consider batch processing APIs
   - Remove async overhead for CPU-bound operations

2. **Benchmark Pure Rust Functions**:
   - Test rubato library directly (without node wrapper)
   - Test bytemuck conversions directly
   - Measure overhead of node abstraction separately

3. **Consider Hybrid Approach**:
   - Use Python+numpy for format conversion and VAD
   - Use Rust only where it provides actual benefits
   - Focus on I/O-bound or concurrency-heavy operations

4. **Revise Performance Claims**:
   - Current architecture does not achieve 50-100x speedup
   - Need to either optimize significantly or adjust expectations
   - Document realistic performance characteristics

### Next Steps

To achieve the performance targets:

1. Profile Rust implementation to identify bottlenecks
2. Implement zero-copy buffer passing
3. Add SIMD optimizations for format conversion
4. Consider removing node abstraction for performance-critical paths
5. Benchmark pure library implementations vs node-wrapped versions
6. Re-run benchmarks after optimizations

---

## Raw Benchmark Data

### Rust Criterion Output
See `runtime/target/criterion/` for detailed HTML reports and statistical analysis.

### Python Baseline Script
Located at: `scripts/benchmark_python_baseline.py`

### Rust Benchmark Code
Located at: `runtime/benches/audio_nodes.rs`

---

**Generated**: 2025-01-XX
**Phase**: Phase 5 - Audio Performance Validation (T096-T100)
**Status**: Performance targets not met, optimization required
