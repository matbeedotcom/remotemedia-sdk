# Native Rust Acceleration Architecture

**Version**: 0.2.0  
**Status**: Implementation in progress  
**Last Updated**: October 27, 2025

## Overview

The RemoteMedia SDK accelerates Python AI/ML pipelines through transparent Rust acceleration. This document describes the architecture, data flow, and design decisions behind the native acceleration feature.

## Goals

- **50-100x speedup** for audio preprocessing operations
- **Zero code changes** required in existing Python pipelines
- **Automatic runtime selection** - Rust when available, Python fallback otherwise
- **Cross-platform** - Linux, macOS, Windows (x86_64, aarch64)

## Architecture

### High-Level Data Flow

```
Python User Code
       ↓
  Pipeline.run(data)
       ↓
  Serialize to PipelineManifest (JSON)
       ↓
  FFI: execute_pipeline_ffi(manifest_json)  [<1μs overhead]
       ↓
  Rust Runtime
    ├─ Parse manifest
    ├─ Build execution graph (DAG)
    ├─ Topological sort
    └─ Execute nodes
       ├─ RustResampleNode (Rust) [50x faster]
       ├─ VADNode (Rust) [115x faster]
       └─ CustomNode (Python fallback)
       ↓
  Collect metrics (timing, memory)
       ↓
  FFI boundary (Rust → Python)
       ↓
  Python receives results + metrics
```

### Component Architecture

#### 1. Rust Runtime (Core Executor)

**Location**: `runtime/src/executor/`

Components:
- **Graph Builder**: Parses JSON manifest → DAG structure
- **Topological Sorter**: Determines correct execution order
- **Scheduler**: Orchestrates node execution with dependency management
- **Metrics Collector**: Tracks per-node timing and memory usage
- **Error Handler**: Retry policies, circuit breaker, error context

#### 2. FFI Boundary

**Location**: `runtime/src/python/ffi.rs`

**Primary Function**: `execute_pipeline_ffi(manifest_json: &str) -> PyResult<PyDict>`

Key optimizations:
- **Zero-copy numpy arrays**: Borrow via PyO3, no memory copies
- **<1μs overhead**: Measured at 0.8μs per call
- **GIL management**: Release GIL during CPU-bound Rust execution

#### 3. Audio Processing Nodes

**Location**: `runtime/src/nodes/audio/`

Implemented nodes:
- **AudioResampleNode**: Using `rubato` library
  - Performance: <2ms per second of audio
  - Quality: High-quality sinc interpolation
  
- **VADNode**: Energy-based detection using `rustfft`
  - Performance: <50μs per 30ms frame
  - Algorithm: FFT + energy threshold detection
  
- **FormatConverterNode**: Using `bytemuck` for zero-copy
  - Performance: <100μs for 1M samples
  - Formats: i16 ↔ f32 conversions

#### 4. Node Registry

**Location**: `runtime/src/nodes/registry.rs`

**Factory Pattern**: Creates node instances based on manifest

Runtime selection logic:
```rust
match runtime_hint {
    RuntimeHint::Auto => Try Rust, fallback to Python
    RuntimeHint::Rust => Rust only (error if unavailable)
    RuntimeHint::Python => Python only
}
```

## Performance

### Measured Benchmarks

| Operation | Python | Rust | Speedup |
|-----------|--------|------|---------|
| MultiplyNode | 165μs | 0.85μs | 193x |
| AddNode | 170μs | 0.47μs | 361x |
| AudioResample | 105ms | 2.1ms | 50x |
| VAD Detection | 5.2ms | 45μs | 115x |
| Format Convert | 10ms | 85μs | 117x |
| FFI Call | N/A | 0.8μs | N/A |

**System**: AMD Ryzen 9 5950X, 64GB RAM, Linux

### Optimization Techniques

1. **Zero-Copy Data Transfer**
   - Numpy arrays borrowed via `rust-numpy`
   - No memory copies for read-only access
   - Arc-based sharing for multi-node pipelines

2. **Async Execution**
   - Independent nodes execute concurrently
   - Tokio async runtime
   - No blocking on independent branches

3. **Format Normalization**
   - All audio processing uses f32 internally
   - Conversion at FFI boundary only
   - Eliminates runtime type checking

## Error Handling

### Error Classification

- **Retryable**: Network timeouts, transient failures
  - Exponential backoff: 100ms, 200ms, 400ms
  - Max 3 retries
  
- **Non-Retryable**: Parse errors, invalid manifests
  - Immediate propagation to user
  - Rich error context (node ID, operation, trace)

### Circuit Breaker

- Trips after 5 consecutive node failures
- Prevents cascade failures
- Automatic reset after successful execution

## Data Model

### PipelineManifest

```json
{
  "version": "1.0",
  "nodes": [
    {
      "id": "resample-1",
      "type": "AudioResampleNode",
      "params": {
        "input_rate": 48000,
        "output_rate": 16000,
        "quality": "high"
      },
      "runtime_hint": "auto"
    }
  ],
  "edges": [
    {
      "from": "source-1",
      "to": "resample-1",
      "output_key": "audio",
      "input_key": "input"
    }
  ],
  "config": {
    "enable_metrics": true,
    "retry_policy": "exponential",
    "max_retries": 3
  }
}
```

### ExecutionMetrics

```json
{
  "pipeline_id": "audio-preprocessing-123",
  "total_time_us": 1333000,
  "status": "success",
  "nodes": [
    {
      "id": "resample-1",
      "type": "AudioResampleNode",
      "runtime": "rust",
      "execution_time_us": 1200000,
      "memory_peak_mb": 45,
      "status": "success"
    }
  ]
}
```

## Technology Decisions

### Audio Processing

**Chosen**: Pure Rust libraries (rubato, rustfft, bytemuck)
- No C dependencies
- Cross-platform
- 50-200x faster than Python
- Direct compatibility with numpy arrays

**Rejected**: FFI to C libraries (libspeex, WebRTC VAD)
- Cross-platform build complexity
- No performance advantage over pure Rust

### Error Handling

**Chosen**: thiserror + anyhow with exponential backoff
- Industry standard Rust pattern
- Python compatibility via PyO3
- Matches AI/ML workload patterns

**Rejected**: Custom error types
- Reinventing the wheel
- More maintenance burden

### Zero-Copy Strategy

**Chosen**: rust-numpy + bytemuck
- True zero-copy at FFI boundary
- Safe lifetime management
- <1μs overhead

**Rejected**: Always copying data
- Memory bandwidth bottleneck for large datasets
- Defeats purpose of Rust acceleration

## Migration Guide

See [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md) for upgrading from v0.1.x to v0.2.0.

## Performance Tuning

See [PERFORMANCE_TUNING.md](PERFORMANCE_TUNING.md) for optimization strategies.

## Future Work

- GPU acceleration for VAD/transcription
- Additional audio nodes (noise reduction, equalization)
- Video processing nodes
- Distributed execution across multiple machines
