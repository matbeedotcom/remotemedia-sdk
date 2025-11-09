# Whisper Transcription Setup Guide

This guide explains how to set up and run the Whisper transcription benchmarks comparing Python (WhisperX) and Rust (rwhisper) implementations.

## Prerequisites

### 1. Base SDK Installation

```bash
cd python-client
pip install -e .
```

### 2. Python WhisperX Setup

WhisperX requires several dependencies:

```bash
# Install ffmpeg (required for audio processing)
# Windows: Download from https://ffmpeg.org/download.html
# Linux: sudo apt install ffmpeg
# macOS: brew install ffmpeg

# Install WhisperX
pip install git+https://github.com/m-bain/whisperx.git

# Additional dependencies
pip install psutil  # For memory tracking
```

**GPU Support (Optional but Recommended):**

For CUDA GPU acceleration:

```bash
# Install PyTorch with CUDA
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu118

# Then install WhisperX
pip install git+https://github.com/m-bain/whisperx.git
```

### 3. Rust rwhisper Setup

#### Step 1: Build Rust Runtime with Whisper Feature

```bash
cd runtime

# Build with whisper feature enabled
maturin develop --release --features whisper
```

#### Step 2: Download Whisper GGML Models

Download pre-converted GGML models from HuggingFace:

**Option A: Direct Download**

Visit: https://huggingface.co/ggerganov/whisper.cpp/tree/main

Download one or more models:
- `ggml-tiny.bin` (~75 MB) - Fastest, lowest accuracy
- `ggml-base.bin` (~142 MB) - Good balance
- `ggml-small.bin` (~466 MB) - Better accuracy
- `ggml-medium.bin` (~1.5 GB) - High accuracy
- `ggml-large-v3.bin` (~3.1 GB) - Best accuracy

**Option B: Using wget/curl**

```bash
# Create models directory
mkdir -p models

# Download tiny model (fast testing)
curl -L -o models/ggml-tiny.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin

# Or download base model (better quality)
curl -L -o models/ggml-base.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
```

**Option C: Convert Your Own Models**

If you want to convert PyTorch Whisper models to GGML format:

```bash
# Clone whisper.cpp
git clone https://github.com/ggerganov/whisper.cpp.git
cd whisper.cpp

# Download and convert model
bash ./models/download-ggml-model.sh base
```

## Running the Benchmarks

### Test Python WhisperX Only

```bash
python examples/rust_runtime/10_whisperx_python_test.py
```

This tests the WhisperX implementation with default settings:
- Model: `tiny` (change in script for better accuracy)
- Device: `cpu` (change to `cuda` if you have GPU)
- Language: `en` (or `None` for auto-detect)

### Run Full Benchmark Comparison

```bash
python examples/rust_runtime/11_whisper_benchmark.py
```

**Before running, update the configuration in the script:**

```python
# Line ~232-234 in 11_whisper_benchmark.py
python_model_size = "tiny"  # Options: tiny, base, small, medium, large-v3
rust_model_path = "models/ggml-tiny.bin"  # Update with your model path
```

## Expected Output

### Successful Benchmark

```
======================================================================
Whisper Transcription Benchmark
Python (WhisperX) vs Rust (rwhisper)
======================================================================

Configuration:
  Audio file: examples/transcribe_demo.wav
  Python model: tiny
  Rust model: models/ggml-tiny.bin

======================================================================
Python WhisperX Benchmark (model: tiny)
======================================================================

[OK] Transcription completed

  Time: 3.45s
  Audio duration: 8.32s
  Real-time factor: 0.415x
  Memory used: 234.5 MB
  Chunks processed: 16

Transcript:
----------------------------------------------------------------------
[Transcribed text will appear here]
----------------------------------------------------------------------

======================================================================
Rust rwhisper Benchmark
======================================================================

[OK] Transcription completed

  Time: 2.15s
  Audio duration: 8.32s
  Real-time factor: 0.258x
  Memory used: 156.3 MB
  Chunks processed: 16

Transcript:
----------------------------------------------------------------------
[Transcribed text will appear here]
----------------------------------------------------------------------

======================================================================
Benchmark Comparison
======================================================================

Metric                    Python WhisperX      Rust rwhisper
----------------------------------------------------------------------
Time                      3.45s                2.15s
Audio Duration            8.32s                8.32s
Real-Time Factor          0.415x               0.258x
Memory Used               234.5 MB             156.3 MB

Speedup:                  Rust is 1.60x faster
Transcript Similarity:    94.5%
```

## Troubleshooting

### Python WhisperX Issues

**Error: `ModuleNotFoundError: No module named 'whisperx'`**

```bash
pip install git+https://github.com/m-bain/whisperx.git
```

**Error: `ffmpeg not found`**

Install ffmpeg for your platform (see Prerequisites above).

**Error: CUDA out of memory**

Reduce batch size or switch to CPU:

```python
WhisperXTranscriber(
    model_size="tiny",  # Use smaller model
    device="cpu",       # Switch to CPU
    batch_size=8,       # Reduce batch size
    ...
)
```

### Rust rwhisper Issues

**Error: `Whisper feature not enabled`**

Rebuild with the whisper feature:

```bash
cd runtime
maturin develop --release --features whisper
```

**Error: `Model file not found`**

Download GGML models (see Step 2 above) and update the path in the benchmark script.

**Error: `Failed to load Whisper model`**

- Verify the model file is a valid GGML format (not PyTorch .pt files)
- Check file permissions
- Ensure sufficient disk space

### General Issues

**Error: `Audio file not found: examples/transcribe_demo.wav`**

Provide your own audio file or update the path in the benchmark script.

**Performance Issues**

For better performance:

1. **Use GPU acceleration** (CUDA)
   - Python: Set `device="cuda"` in WhisperX
   - Rust: rwhisper uses CPU by default (CUDA support coming)

2. **Adjust thread count** (Rust)
   ```python
   RustWhisperTranscriber(
       n_threads=8,  # Increase for more CPU cores
       ...
   )
   ```

3. **Batch processing** (Python)
   ```python
   WhisperXTranscriber(
       batch_size=32,  # Increase for better GPU utilization
       ...
   )
   ```

## Model Comparison

| Model      | Size    | Speed  | Accuracy | Memory (CPU) | Best For                    |
|------------|---------|--------|----------|--------------|------------------------------|
| tiny       | 75 MB   | Fastest| Lowest   | ~200 MB      | Testing, real-time apps     |
| base       | 142 MB  | Fast   | Good     | ~300 MB      | General purpose              |
| small      | 466 MB  | Medium | Better   | ~600 MB      | Better accuracy needs        |
| medium     | 1.5 GB  | Slow   | High     | ~1.5 GB      | Professional transcription   |
| large-v3   | 3.1 GB  | Slowest| Best     | ~3 GB        | Maximum accuracy required    |

## Performance Tips

### For Real-Time Transcription

- Use `tiny` or `base` models
- Enable GPU acceleration
- Process audio in small chunks (30ms frames)
- Use VAD to skip silence

### For Batch Processing

- Use larger models (`medium` or `large-v3`)
- Increase batch size (GPU)
- Process multiple files concurrently
- Consider accuracy over speed

### For Production Deployments

- **Python WhisperX:**
  - Better accuracy with CTranslate2 optimization
  - Supports word-level timestamps and alignment
  - GPU acceleration available
  - Good for high-accuracy requirements

- **Rust rwhisper:**
  - Lower memory footprint
  - Better CPU efficiency
  - Easier deployment (single binary)
  - Good for CPU-only environments

## Next Steps

1. Test with your own audio files
2. Experiment with different model sizes
3. Try GPU acceleration (CUDA)
4. Implement concurrent stream processing (see example 09)
5. Integrate with your production pipeline

## References

- **WhisperX**: https://github.com/m-bain/whisperx
- **rwhisper**: https://docs.rs/rwhisper/latest/rwhisper/
- **whisper.cpp**: https://github.com/ggerganov/whisper.cpp
- **OpenAI Whisper**: https://github.com/openai/whisper
- **GGML Models**: https://huggingface.co/ggerganov/whisper.cpp
