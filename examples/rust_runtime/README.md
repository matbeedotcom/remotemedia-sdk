# Rust Runtime Examples

This directory contains examples demonstrating the RemoteMedia SDK's Rust runtime integration.

## Overview

The Rust runtime provides **significant performance improvements** for pipeline execution while maintaining **100% Python compatibility**. All examples in this directory demonstrate zero-code-change migration - they work identically whether the Rust runtime is available or not.

## Prerequisites

### Python SDK Installation

```bash
cd python-client
pip install -e .
```

### Rust Runtime Installation (Optional but Recommended)

```bash
cd runtime
pip install maturin
maturin develop --release
```

If you don't install the Rust runtime, all examples will automatically fall back to the Python executor.

## Examples

### 01_basic_pipeline.py

**Purpose:** Simplest possible pipeline demonstrating Rust runtime usage

**Key Concepts:**
- Creating a basic pipeline
- Automatic runtime detection
- Transparent fallback

**Run:**
```bash
python examples/rust_runtime/01_basic_pipeline.py
```

**Expected Output:**
```
✓ Rust runtime available
✓ Result: [1, 2, 3, 4, 5]
✓ Execution successful!
```

---

### 02_calculator_pipeline.py

**Purpose:** Demonstrates data transformation with stateful nodes

**Key Concepts:**
- Nodes with parameters
- Chaining transformations
- Result verification

**Run:**
```bash
python examples/rust_runtime/02_calculator_pipeline.py
```

**What It Does:**
- Input: `[1, 2, 3]`
- Multiply by 2: `[2, 4, 6]`
- Add 10: `[12, 14, 16]`
- Verifies computation correctness

---

### 03_runtime_comparison.py

**Purpose:** Compare Rust vs Python execution performance

**Key Concepts:**
- Explicit runtime selection (`use_rust` parameter)
- Performance benchmarking
- Result equivalence verification

**Run:**
```bash
python examples/rust_runtime/03_runtime_comparison.py
```

**Expected Output:**
```
Rust runtime:   4.32 ms
Python runtime: 12.45 ms
Speedup:        2.88x faster with Rust!
```

---

### 04_async_streaming.py

**Purpose:** Demonstrates async streaming node support

**Key Concepts:**
- Async generator nodes
- Streaming data flow
- Async/await in Python nodes

**Run:**
```bash
python examples/rust_runtime/04_async_streaming.py
```

**What It Does:**
- Generates a stream of numbers asynchronously
- Transforms each item in the stream
- Shows Rust runtime handling async operations

---

### 05_fallback_behavior.py

**Purpose:** Demonstrates graceful fallback when Rust unavailable

**Key Concepts:**
- Automatic fallback behavior
- Explicit runtime control
- Cross-runtime result verification

**Run:**
```bash
python examples/rust_runtime/05_fallback_behavior.py
```

**What It Tests:**
- Default behavior (try Rust, fall back to Python)
- Forced Python execution
- Result consistency

---

### 06_rust_vs_python_nodes.py

**Purpose:** Performance benchmark comparing Rust-native nodes vs Python nodes

**Key Concepts:**
- Rust-native node implementations
- Performance measurement and comparison
- Scalability with pipeline complexity

**Run:**
```bash
python examples/rust_runtime/06_rust_vs_python_nodes.py
```

**What It Shows:**
- Rust-native `MultiplyNode` and `AddNode` are **193-361x faster** than Python equivalents
- Both implementations produce identical results
- Performance benefits increase with pipeline complexity
- No code changes needed - same Python API

**Example Output:**
```
Simple pipeline:  Rust 361.68x faster
Complex pipeline: Rust 193.21x faster
```

---

### 07_audio_vad_performance.py

**Purpose:** Real-world audio/VAD pipeline performance benchmark

**Key Concepts:**
- I/O-bound vs compute-bound operations
- Audio processing pipeline performance
- Understanding when Rust provides benefits

**Run:**
```bash
python examples/rust_runtime/07_audio_vad_performance.py
```

**What It Shows:**
- Audio I/O pipelines are I/O-bound (file reading, native C libraries)
- Rust runtime has minimal overhead for I/O operations
- Performance is comparable between Rust and Python for audio pipelines
- Compute-intensive nodes (see example 06) show 100x+ speedup with Rust

**Key Insight:**
The Rust runtime excels at **compute-intensive** operations, while I/O-bound operations
show comparable performance. The optimal strategy is to implement Rust-native nodes for
computationally expensive operations (custom audio effects, image processing, ML inference)
while using CPython executor for I/O-bound operations (file reading, existing native libraries).

**Example Output:**
```
Average Speedup: 0.79x faster (I/O-bound operations)

Compare with example 06:
- Math operations: 193-361x faster (compute-bound operations)
```

---

### 08_realistic_media_benchmark.py

**Purpose:** Benchmark concurrent media processing showing GIL bottleneck

**Key Concepts:**
- Multiple concurrent media streams
- CPU-intensive simulated processing (Whisper)
- Python GIL impact demonstration
- Scalability testing (1, 2, 4, 8 streams)

**Run:**
```bash
python examples/rust_runtime/08_realistic_media_benchmark.py
```

**What It Shows:**
- Tests processing multiple audio streams concurrently
- Simulates CPU-intensive ML inference work
- Demonstrates how Python GIL affects concurrent processing
- Compares efficiency across stream counts

---

### 09_realtime_transcription_benchmark.py

**Purpose:** Realistic real-time audio transcription pipeline benchmark

**Key Concepts:**
- Production-realistic pipeline: Audio -> Resample -> VAD -> Buffer -> Whisper -> Text
- Real-time processing constraints (RTF < 1.0 required)
- Concurrent stream handling (simulating multiple users)
- Smart buffering based on voice activity

**Run:**
```bash
python examples/rust_runtime/09_realtime_transcription_benchmark.py
```

**What It Does:**
- Processes 1, 2, 4, 8 concurrent audio streams
- Applies Voice Activity Detection (VAD)
- Buffers audio intelligently before transcription
- Simulates Whisper ML inference (~150ms per second of audio)
- Measures Real-Time Factor (processing_time / audio_duration)

**Pipeline Flow:**
```
Audio Input (16kHz mono)
    -> VAD (30ms frames, speech detection)
    -> Smart Buffer (accumulate 500ms)
    -> Whisper Transcription (CPU-intensive)
    -> Transcribed Text Output
```

**Key Findings:**
- **Single stream:** Both runtimes handle real-time well (RTF ~0.51x)
- **Multiple streams (2-4):** Both maintain good performance with asyncio+ThreadPoolExecutor
- **Heavy load (8 streams):** Both start exceeding real-time (RTF > 1.0)
- **Python GIL impact:** Minimal for this workload due to ThreadPoolExecutor distribution
- **Important insight:** Mixed I/O+compute workloads with proper async design show good Python concurrency

**Performance Summary:**
```
Streams    Rust RTF     Python RTF   Speedup
1          0.511x       0.512x       1.00x
2          0.518x       0.512x       0.99x
4          0.610x       0.576x       0.94x
8          1.067x       1.021x       0.95x
```

**Production Impact:**
This benchmark demonstrates that for mixed I/O+compute workloads with asyncio:
- Python's ThreadPoolExecutor provides good concurrency
- GIL impact is minimal when work is distributed across threads
- Both runtimes handle real-time transcription well up to 4 streams
- For pure compute workloads, see Example 06 (193-361x Rust speedup)

---

### 10_whisperx_python_test.py

**Purpose:** Test Python WhisperX transcription implementation

**Key Concepts:**
- Real Whisper model integration (not simulated)
- WhisperX with CTranslate2 optimization
- Lazy model loading
- Batch inference for efficiency

**Run:**
```bash
# Install WhisperX first
pip install git+https://github.com/m-bain/whisperx.git psutil

python examples/rust_runtime/10_whisperx_python_test.py
```

**What It Does:**
- Loads WhisperX model (tiny/base/small/medium/large)
- Processes audio through pipeline (Resample -> Transcribe)
- Measures transcription time and real-time factor
- Outputs full transcript with timestamps

**Configuration Options:**
```python
WhisperXTranscriber(
    model_size="tiny",     # tiny, base, small, medium, large-v3
    device="cpu",          # cpu or cuda
    compute_type="float32",# float32, float16, int8
    batch_size=16,
    language="en",         # or None for auto-detect
    align_model=True,      # Enable word-level timestamps
)
```

---

### 11_whisper_benchmark.py

**Purpose:** Comprehensive benchmark comparing Python WhisperX vs Rust rwhisper

**Key Concepts:**
- Real Whisper model comparison (not simulated)
- Python: WhisperX with CTranslate2
- Rust: rwhisper (whisper.cpp bindings)
- Memory usage tracking
- Transcript similarity comparison

**Setup:**

See [WHISPER_SETUP.md](WHISPER_SETUP.md) for detailed setup instructions.

**Quick Start:**

```bash
# 1. Install Python dependencies
pip install git+https://github.com/m-bain/whisperx.git psutil

# 2. Build Rust runtime with whisper feature
cd runtime
maturin develop --release --features whisper

# 3. Download GGML model
mkdir -p models
curl -L -o models/ggml-tiny.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin

# 4. Run benchmark
cd ../..
python examples/rust_runtime/11_whisper_benchmark.py
```

**What It Tests:**
- Transcription time (both implementations)
- Real-time factor (must be < 1.0 for live audio)
- Memory usage
- Transcript quality (word overlap similarity)

**Expected Results:**

```
Metric                    Python WhisperX      Rust rwhisper
----------------------------------------------------------------------
Time                      3.45s                2.15s
Real-Time Factor          0.415x               0.258x
Memory Used               234.5 MB             156.3 MB

Speedup:                  Rust is 1.60x faster
Transcript Similarity:    94.5%
```

**Key Findings:**

- **Rust rwhisper:** Lower memory footprint, better CPU efficiency
- **Python WhisperX:** Better accuracy with CTranslate2, GPU support
- **Both:** Achieve real-time transcription (RTF < 1.0)
- **Production choice:** Depends on requirements:
  - CPU-only deployment → Rust rwhisper
  - Maximum accuracy with GPU → Python WhisperX
  - Mixed workloads → Use both (Rust for compute, Python for accuracy)

**Model Comparison:**

| Model    | Size   | Speed   | Accuracy | Memory   | Use Case              |
|----------|--------|---------|----------|----------|-----------------------|
| tiny     | 75 MB  | Fastest | Lowest   | ~200 MB  | Testing, real-time    |
| base     | 142 MB | Fast    | Good     | ~300 MB  | General purpose       |
| small    | 466 MB | Medium  | Better   | ~600 MB  | Better accuracy       |
| medium   | 1.5 GB | Slow    | High     | ~1.5 GB  | Professional          |
| large-v3 | 3.1 GB | Slowest | Best     | ~3 GB    | Maximum accuracy      |

---

## Runtime Selection

All examples use `pipeline.run()` which supports:

```python
# Try Rust first, fall back to Python (default)
result = await pipeline.run(data)
result = await pipeline.run(data, use_rust=True)

# Force Python executor
result = await pipeline.run(data, use_rust=False)
```

## Zero-Code-Change Migration

These examples demonstrate that existing Python pipeline code requires **no modifications** to benefit from the Rust runtime:

```python
# Your existing code (works with or without Rust)
pipeline = Pipeline("my_pipeline")
pipeline.add_node(MyNode())
result = await pipeline.run(data)  # Automatically uses Rust if available!
```

## Performance Benefits

Performance improvements vary significantly based on workload type:

### Compute-Intensive Operations (Example 06)
- **Rust-native nodes:** 193-361x faster than Python
- **Use case:** Mathematical operations, custom filters, image transformations
- **Recommendation:** Implement Rust-native nodes for CPU-heavy operations

### I/O-Bound Operations (Example 07)
- **Performance:** Comparable between Rust and Python (~0.79-1.03x)
- **Reason:** Native C libraries (libav, webrtc-audio-processing) release GIL
- **Use case:** File I/O, audio/video decoding, existing native libraries
- **Recommendation:** Python nodes work well for I/O operations

### Mixed Workloads (Example 09)
- **Performance:** Similar between Rust and Python with proper async design
- **Reason:** ThreadPoolExecutor distributes work across threads effectively
- **Use case:** Real-time processing pipelines with mixed I/O+compute
- **Recommendation:** Focus Rust optimization on pure compute nodes

### Summary
- **Rust excels at:** Pure CPU-intensive operations (100x+ speedup)
- **Python works well for:** I/O operations, mixed workloads with asyncio
- **Best strategy:** Use Rust-native nodes for compute bottlenecks, Python for I/O

## Troubleshooting

### Rust Runtime Not Available

If you see "Rust runtime not available":

1. Check installation:
   ```bash
   python -c "import remotemedia_runtime; print(remotemedia_runtime.__version__)"
   ```

2. Rebuild if needed:
   ```bash
   cd runtime
   maturin develop --release
   ```

3. Verify correct Python environment is active

### Import Errors

If you see `ModuleNotFoundError: No module named 'remotemedia'`:

```bash
cd python-client
pip install -e .
```

### Build Errors

If `maturin develop` fails:
- Ensure Rust is installed: `rustc --version`
- Update Rust: `rustup update`
- Check Python development headers are installed

## Next Steps

- Explore the [Migration Guide](../../docs/MIGRATION_GUIDE.md)
- Read [FFI Usage Guide](../../docs/FFI_USAGE.md) for advanced use cases
- Check out [Performance Benchmarks](../../docs/BENCHMARKS.md)
- Review [RustPython Compatibility Report](../../docs/RUSTPYTHON_COMPATIBILITY.md)

## Support

For issues or questions:
- Check the main [README](../../README.md)
- Review [documentation](../../docs/)
- Open an issue on GitHub
