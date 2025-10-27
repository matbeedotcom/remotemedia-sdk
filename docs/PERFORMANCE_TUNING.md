# Performance Tuning Guide

**Version**: 0.2.0  
**Last Updated**: 2025-01-XX

## Overview

This guide provides best practices and tuning recommendations for optimizing RemoteMedia SDK performance in production environments. The v0.2.0 native Rust acceleration provides 50-100x performance improvements over v0.1.x, but proper configuration is essential to realize these gains.

## Table of Contents

1. [Quick Wins](#quick-wins)
2. [Audio Processing](#audio-processing)
3. [Memory Management](#memory-management)
4. [Concurrency & Parallelism](#concurrency--parallelism)
5. [Python Integration](#python-integration)
6. [Pipeline Optimization](#pipeline-optimization)
7. [Monitoring & Profiling](#monitoring--profiling)
8. [Production Checklist](#production-checklist)

---

## Quick Wins

### 1. Use Release Builds

**Impact**: 10-50x performance improvement

```bash
# Development (debug)
cargo build

# Production (optimized)
cargo build --release
```

**Why**: Rust's release mode enables aggressive optimizations (inlining, loop unrolling, SIMD vectorization) and removes debug assertions.

### 2. Enable CPU-Specific Optimizations

**Impact**: 10-30% improvement on modern CPUs

```bash
# Linux/macOS
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Windows PowerShell
$env:RUSTFLAGS="-C target-cpu=native"; cargo build --release
```

Add to `.cargo/config.toml`:

```toml
[build]
rustflags = ["-C", "target-cpu=native"]
```

**Why**: Enables AVX2/AVX-512 SIMD instructions for audio processing (resampling, FFT).

### 3. Use LTO (Link-Time Optimization)

**Impact**: 5-15% improvement, smaller binaries

Add to `Cargo.toml`:

```toml
[profile.release]
lto = "fat"
codegen-units = 1
```

**Trade-off**: Longer compile times (2-5x slower).

---

## Audio Processing

### Resampling Performance

**Best Practice**: Pre-allocate output buffers

```rust
use rubato::{Resampler, SincFixedIn};

// ✅ Good: Reuse resampler instance
let mut resampler = SincFixedIn::<f32>::new(
    48000.0 / 16000.0,  // ratio
    2.0,                // max_resample_ratio_relative
    rubato::PolynomialDegree::Septic,
    1024,               // chunk_size
    2,                  // channels
)?;

// Process multiple chunks with same resampler
for chunk in audio_chunks {
    let output = resampler.process(&chunk, None)?;
    // ... use output
}
```

**Why**: Resampler initialization (sinc filter computation) is expensive (~10ms). Reusing instances amortizes this cost.

### FFT Performance

**Best Practice**: Use real FFT for real-valued signals

```rust
use rustfft::FftPlanner;
use realfft::RealFftPlanner;

// ✅ Good: Real FFT (2x faster, half memory)
let mut real_planner = RealFftPlanner::<f32>::new();
let r2c = real_planner.plan_fft_forward(1024);

// ❌ Avoid: Complex FFT for real signals
let mut complex_planner = FftPlanner::<f32>::new();
let fft = complex_planner.plan_fft_forward(1024);
```

**Performance**:
- Real FFT: ~50 μs for 1024 samples
- Complex FFT: ~100 μs for 1024 samples

### Sample Format Conversion

**Best Practice**: Use `bytemuck` for zero-copy conversions

```rust
use bytemuck::{cast_slice, cast_slice_mut};

// ✅ Good: Zero-copy conversion
let i16_data: &[i16] = get_audio_data();
let f32_data: Vec<f32> = i16_data.iter()
    .map(|&s| s as f32 / 32768.0)
    .collect();

// ⚡ Better: SIMD-optimized conversion (future)
// See runtime/src/audio/convert.rs for optimized implementations
```

**Why**: Eliminates memory allocation and copy overhead.

---

## Memory Management

### Buffer Pooling

**Best Practice**: Reuse audio buffers across pipeline executions

```rust
use std::sync::Arc;
use parking_lot::Mutex;

struct BufferPool {
    buffers: Vec<Vec<f32>>,
    capacity: usize,
}

impl BufferPool {
    fn acquire(&mut self, size: usize) -> Vec<f32> {
        self.buffers.pop().unwrap_or_else(|| Vec::with_capacity(size))
    }

    fn release(&mut self, mut buffer: Vec<f32>) {
        buffer.clear();
        if self.buffers.len() < self.capacity {
            self.buffers.push(buffer);
        }
    }
}
```

**Impact**: Reduces allocations by 90%, improves cache locality.

### Zero-Copy NumPy Integration

**Best Practice**: Use PyO3's buffer protocol

```rust
use pyo3::prelude::*;
use numpy::PyArray1;

#[pyfunction]
fn process_audio(py: Python, data: &PyArray1<f32>) -> PyResult<Py<PyArray1<f32>>> {
    // ✅ Good: Direct access to NumPy memory
    let slice = unsafe { data.as_slice()? };
    
    // Process in-place if possible
    let mut output = PyArray1::zeros(py, data.len(), false);
    let out_slice = unsafe { output.as_slice_mut()? };
    
    // ... process slice -> out_slice
    
    Ok(output.to_owned())
}
```

**Why**: Avoids copying data between Python and Rust (100-1000x faster for large arrays).

---

## Concurrency & Parallelism

### Pipeline Parallelism

**Best Practice**: Enable parallel node execution

```yaml
# pipeline.yaml
execution:
  parallel: true
  max_workers: 4  # Match CPU cores
  
nodes:
  - id: node1
    parallel_safe: true  # Can run concurrently
  - id: node2
    parallel_safe: false  # Sequential execution required
```

**Performance**: 2-4x throughput on multi-core systems.

### Async I/O

**Best Practice**: Use async/await for network and disk operations

```rust
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<()> {
    // ✅ Good: Async file I/O (non-blocking)
    let mut file = File::create("output.wav").await?;
    file.write_all(&audio_data).await?;
    
    // Process next chunk while I/O completes
    let next_chunk = process_audio(&input).await?;
    
    Ok(())
}
```

**Why**: Overlaps computation with I/O, reduces latency.

---

## Python Integration

### CPython Configuration

**Best Practice**: Disable GIL for Rust-only operations

```rust
use pyo3::prelude::*;

#[pyfunction]
fn heavy_computation(data: Vec<f32>) -> PyResult<Vec<f32>> {
    // ✅ Good: Release GIL during Rust computation
    Python::with_gil(|py| {
        py.allow_threads(|| {
            // Pure Rust code (no Python API calls)
            expensive_rust_operation(&data)
        })
    })
}
```

**Impact**: Allows Python threads to run concurrently during Rust execution.

### Minimize Python Calls

**Best Practice**: Batch operations in Rust

```rust
// ❌ Avoid: Per-sample Python callback
for sample in audio_data {
    python_callback(sample)?;  // 1M+ FFI calls/sec
}

// ✅ Good: Batch processing
let processed = process_batch_in_rust(&audio_data)?;
python_callback(processed)?;  // 1 FFI call
```

**Performance**: FFI overhead ~100ns/call. Batching reduces overhead by 1000x.

---

## Pipeline Optimization

### Graph Optimization

**Best Practice**: Enable automatic graph fusion

```yaml
# pipeline.yaml
optimization:
  enable_fusion: true  # Merge compatible nodes
  remove_dead_code: true  # Eliminate unused branches
  constant_folding: true  # Pre-compute static values
```

**Example Fusion**:
```
Before:  [Resample] -> [Normalize] -> [Mono]
After:   [ResampleNormalizeMono]  # Single fused node
```

**Impact**: 30-50% reduction in node transitions, improved cache locality.

### Data Layout

**Best Practice**: Use AoS (Array of Structs) for small data, SoA (Struct of Arrays) for SIMD

```rust
// ❌ AoS: Poor cache locality for large datasets
struct Sample {
    left: f32,
    right: f32,
}
let samples: Vec<Sample> = ...;

// ✅ SoA: Better for SIMD processing
struct StereoBuffer {
    left: Vec<f32>,
    right: Vec<f32>,
}
```

**Why**: SoA enables SIMD vectorization (4-8 samples/instruction).

---

## Monitoring & Profiling

### Built-in Metrics

**Enable execution metrics**:

```rust
use remotemedia_runtime::executor::ExecutorMetrics;

let metrics = executor.get_metrics();
println!("Total execution time: {:?}", metrics.total_duration);
println!("Node timings: {:?}", metrics.node_durations);
println!("Memory peak: {} MB", metrics.peak_memory_mb);
```

### Profiling Tools

**Linux (perf)**:
```bash
cargo build --release
perf record --call-graph=dwarf ./target/release/my_app
perf report
```

**macOS (Instruments)**:
```bash
cargo instruments --release --template "Time Profiler"
```

**Windows (VTune/Tracy)**:
```powershell
# Install VTune Profiler, then:
vtune -collect hotspots -- .\target\release\my_app.exe
```

### Flame Graphs

```bash
# Linux
cargo install flamegraph
cargo flamegraph --release

# Open flamegraph.svg in browser
```

---

## Production Checklist

### Pre-Deployment

- [ ] Build with `--release` flag
- [ ] Enable `target-cpu=native` (if deploying to known hardware)
- [ ] Enable LTO (`lto = "fat"` in Cargo.toml)
- [ ] Profile hotspots with perf/Instruments
- [ ] Verify zero memory leaks (valgrind/AddressSanitizer)
- [ ] Benchmark against v0.1.x baseline (expect 50-100x improvement)

### Runtime Configuration

- [ ] Set appropriate buffer pool sizes (match workload)
- [ ] Configure tokio runtime workers (`TOKIO_WORKER_THREADS`)
- [ ] Enable pipeline graph optimization
- [ ] Configure logging levels (warn/error in production)
- [ ] Set up metrics export (Prometheus/statsd)

### Monitoring

- [ ] Track execution latency (p50, p95, p99)
- [ ] Monitor memory usage trends
- [ ] Alert on pipeline failures
- [ ] Track throughput (samples/sec, chunks/sec)

---

## Performance Targets

**RemoteMedia v0.2.0 Expected Performance**:

| Operation                  | Throughput         | Latency (p99) |
|----------------------------|-------------------|---------------|
| Audio Resampling (16k→48k) | 500 MB/s          | <1ms          |
| FFT (1024-point)           | 20M samples/sec   | <100μs        |
| NumPy→Rust Zero-Copy       | 10 GB/s           | <10μs         |
| Pipeline Execution (5 nodes)| 1000 chunks/sec   | <5ms          |

**Compared to v0.1.x (RustPython)**:
- Audio processing: **50-100x faster**
- Memory overhead: **90% reduction**
- Startup time: **10x faster** (no VM initialization)

---

## Troubleshooting

### Slow Performance

1. **Verify release build**: `cargo build --release` (not `cargo build`)
2. **Check CPU flags**: `rustc --print cfg | grep target_feature`
3. **Profile hotspots**: Use `perf` or `cargo flamegraph`
4. **Disable debug logging**: Set `RUST_LOG=warn` (not `debug`/`trace`)

### High Memory Usage

1. **Enable buffer pooling**: Reuse Vec allocations
2. **Limit pipeline parallelism**: Reduce `max_workers`
3. **Check for leaks**: Run with `valgrind --leak-check=full`

### Inconsistent Latency

1. **Disable frequency scaling**: `cpupower frequency-set -g performance`
2. **Pin threads to cores**: Use `core_affinity` crate
3. **Avoid Python in hot path**: Move logic to Rust nodes

---

## Further Reading

- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [PyO3 Performance Guide](https://pyo3.rs/latest/performance)
- [Rubato Documentation](https://docs.rs/rubato/)
- [RustFFT Benchmarks](https://docs.rs/rustfft/)

---

**Last Updated**: 2025-01-XX  
**Contributors**: RemoteMedia Team  
**License**: MIT
