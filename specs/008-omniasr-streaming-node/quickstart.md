# Quickstart Guide: OmniASR Streaming Transcription

**Feature**: OmniASR Streaming Transcription Node
**Branch**: `008-omniasr-streaming-node`
**Audience**: Developers integrating OmniASR transcription into RemoteMedia SDK pipelines

---

## Overview

This guide walks you through adding real-time multilingual speech transcription to your RemoteMedia SDK audio processing pipeline using the OmniASR node. You'll learn how to:

1. Install required dependencies
2. Configure the transcription node
3. Build a basic transcription pipeline
4. Handle different languages and chunking modes
5. Troubleshoot common issues

**Estimated Time**: 15-20 minutes

---

## Prerequisites

### System Requirements

- **Python**: 3.10 or higher
- **Operating System**: Linux, macOS, or Windows
- **Memory**: 8 GB RAM minimum (16 GB recommended)
- **GPU** (optional): NVIDIA GPU with 6 GB+ VRAM for faster processing

### Existing Setup

- RemoteMedia SDK installed (`remotemedia-sdk >= 0.3.0`)
- Basic familiarity with RemoteMedia pipeline manifests

---

## Step 1: Install Dependencies

### Install OmniASR Package

```bash
# Install omnilingual_asr and dependencies
pip install omnilingual-asr torch torchaudio silero-vad librosa soundfile
```

**Package Sizes** (approximate):
- `omnilingual-asr`: ~100 MB
- `torch`: ~1-2 GB (includes CUDA support)
- Other dependencies: ~200 MB

**Note**: First model load will download 1-6 GB model files from HuggingFace.

### Verify Installation

```python
# Test import
python -c "from omnilingual_asr.models.inference.pipeline import Wav2Vec2InferencePipeline; print('✓ OmniASR installed')"

# Check GPU availability
python -c "import torch; print(f'GPU available: {torch.cuda.is_available()}')"
```

**Expected Output**:
```
✓ OmniASR installed
GPU available: True  # or False if CPU-only
```

---

## Step 2: Basic Pipeline Setup

### Create Your First Transcription Pipeline

**File**: `my_transcription_pipeline.yaml`

```yaml
nodes:
  # Audio source (replace with your actual source)
  - id: audio_input
    node_type: AudioFileSource
    params:
      file_path: "/path/to/audio.wav"

  # Resample to 16kHz (required by OmniASR)
  - id: resampler
    node_type: AudioResampleNode
    params:
      target_sample_rate: 16000

  # OmniASR transcription
  - id: transcriber
    node_type: OmniASRTranscriber
    executor: multiprocess  # IMPORTANT: Must use multiprocess
    params:
      model_card: "omniASR_LLM_1B"  # or "omniASR_LLM_300M" for faster
      language: null  # Auto-detect language
      chunking_mode: "none"  # Process entire audio
      enable_alignment: false  # Text only (no word timestamps)

  # Output sink
  - id: output
    node_type: PrintSink

edges:
  - from: audio_input
    to: resampler
  - from: resampler
    to: transcriber
  - from: transcriber
    to: output
```

### Run the Pipeline

```bash
# Run with RemoteMedia CLI
remotemedia run my_transcription_pipeline.yaml

# Or via Python API
python -c "
from remotemedia.pipeline import Pipeline

pipeline = Pipeline.from_yaml('my_transcription_pipeline.yaml')
await pipeline.run()
"
```

**Expected Output**:
```
Loading OmniASR model: omniASR_LLM_1B
Using device: cuda
✓ OmniASR model loaded successfully
Transcribing audio...
Result: {"text": "Your transcribed audio text here...", "language": "eng_Latn", "success": true}
```

**First Run**: Model download may take 5-10 minutes. Subsequent runs are much faster (uses cached model).

---

## Step 3: Language Configuration

### Auto-Detection (Default)

```yaml
- id: transcriber
  node_type: OmniASRTranscriber
  executor: multiprocess
  params:
    language: null  # Automatically detects language
```

**Pros**:
- Works with any supported language
- No configuration needed

**Cons**:
- May be less accurate for short audio clips (< 3 seconds)
- Cannot handle mixed-language audio well

### Explicit Language Selection

```yaml
# English
- id: transcriber
  params:
    language: "eng_Latn"

# Spanish
- id: transcriber
  params:
    language: "spa_Latn"

# Arabic
- id: transcriber
  params:
    language: "ara_Arab"

# French
- id: transcriber
  params:
    language: "fra_Latn"
```

**Benefits**:
- Better accuracy
- Faster processing
- Consistent results

**Find Your Language Code**:

```python
from omnilingual_asr.models.wav2vec2_llama.lang_ids import supported_langs

# Search for your language
for lang in supported_langs:
    if "spanish" in lang.lower() or "spa" in lang.lower():
        print(lang)
# Output: spa_Latn
```

---

## Step 4: Chunking Modes

### Mode 1: None (Default)

**Best For**: Short audio clips (< 30 seconds)

```yaml
params:
  chunking_mode: "none"
```

**Behavior**:
- Processes entire audio as single chunk
- Fastest for short clips
- May struggle with very long audio

### Mode 2: Static

**Best For**: Consistent chunk sizes, simple use cases

```yaml
params:
  chunking_mode: "static"
  chunk_duration: 30.0  # 30 second chunks
```

**Behavior**:
- Splits audio every N seconds
- Fixed, predictable chunks
- May cut mid-sentence

### Mode 3: VAD (Voice Activity Detection)

**Best For**: Long-form audio with natural pauses

```yaml
params:
  chunking_mode: "vad"
```

**Behavior**:
- Chunks at speech boundaries (silence gaps)
- Respects sentence/phrase boundaries
- Higher quality but more processing overhead

**Example Output** (with VAD chunking):
```json
{
  "text": "First sentence with natural pause. Second sentence after pause.",
  "chunk_metadata": {
    "chunking_mode": "vad",
    "chunks_created": 2
  }
}
```

---

## Step 5: Word-Level Timestamps (Alignment)

**Use Case**: Subtitle generation, karaoke, precise timing

### Enable Alignment

```yaml
- id: transcriber
  params:
    enable_alignment: true
```

### Output Format

```json
{
  "text": "Hello world, this is a test.",
  "language": "eng_Latn",
  "success": true,
  "word_timestamps": [
    {
      "word": "Hello",
      "start": 0.0,
      "end": 0.42,
      "confidence": 0.98
    },
    {
      "word": "world",
      "start": 0.42,
      "end": 0.86,
      "confidence": 0.95
    },
    {
      "word": "this",
      "start": 0.92,
      "end": 1.12,
      "confidence": 0.97
    }
    // ...
  ]
}
```

### Generate SRT Subtitles

```python
def create_srt(transcription_result):
    """Convert word timestamps to SRT subtitle format."""
    srt_output = []
    words = transcription_result["word_timestamps"]

    # Group words into subtitle segments (e.g., 5 words each)
    segment_size = 5
    for i in range(0, len(words), segment_size):
        segment_words = words[i:i+segment_size]
        start_time = segment_words[0]["start"]
        end_time = segment_words[-1]["end"]
        text = " ".join(w["word"] for w in segment_words)

        # Format as SRT
        srt_output.append(f"{i//segment_size + 1}")
        srt_output.append(f"{format_timestamp(start_time)} --> {format_timestamp(end_time)}")
        srt_output.append(text)
        srt_output.append("")  # Blank line

    return "\n".join(srt_output)

def format_timestamp(seconds):
    """Convert seconds to SRT timestamp format (HH:MM:SS,mmm)."""
    hours = int(seconds // 3600)
    minutes = int((seconds % 3600) // 60)
    secs = int(seconds % 60)
    millis = int((seconds % 1) * 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{millis:03d}"
```

---

## Step 6: Model Selection

### 1B Model (Default)

**Best For**: Highest accuracy

```yaml
params:
  model_card: "omniASR_LLM_1B"
```

**Specs**:
- Size: ~4-6 GB
- Memory: 6 GB GPU / 8 GB RAM
- Speed: 500ms-2s per 5s chunk (GPU)

### 300M Model

**Best For**: Faster processing, resource-constrained environments

```yaml
params:
  model_card: "omniASR_LLM_300M"
```

**Specs**:
- Size: ~1-2 GB
- Memory: 2 GB GPU / 4 GB RAM
- Speed: 300ms-1s per 5s chunk (GPU)
- Accuracy: ~90-95% of 1B model

---

## Step 7: Advanced Configuration

### Force GPU or CPU

```yaml
params:
  device: "cuda"  # Force GPU (error if unavailable)
  # OR
  device: "cpu"   # Force CPU (slower but always works)
  # OR
  device: null    # Auto-detect (default)
```

### Custom Model Cache

```bash
# Set cache directory before running
export FAIRSEQ2_CACHE_DIR=/path/to/custom/cache

# Then run pipeline
remotemedia run my_pipeline.yaml
```

### Batch Processing (Advanced)

```yaml
# For offline batch processing (not streaming)
params:
  batch_size: 4  # Process 4 chunks at once
```

**Note**: Only useful for batch processing. Streaming pipelines should use `batch_size: 1`.

---

## Complete Example: Real-Time Transcription

**Use Case**: Live microphone transcription with Spanish language

```yaml
nodes:
  # Real-time audio from microphone
  - id: microphone
    node_type: AudioInputDevice
    params:
      device_index: 0  # Default microphone
      sample_rate: 48000
      chunk_duration_ms: 1000  # 1 second chunks

  # Resample to 16kHz
  - id: resampler
    node_type: AudioResampleNode
    params:
      target_sample_rate: 16000

  # Buffer audio (accumulate 5 seconds before transcribing)
  - id: buffer
    node_type: AudioBuffer
    params:
      buffer_size_samples: 80000  # 5 seconds at 16kHz

  # Transcribe with OmniASR
  - id: transcriber
    node_type: OmniASRTranscriber
    executor: multiprocess
    params:
      model_card: "omniASR_LLM_300M"  # Faster for real-time
      language: "spa_Latn"  # Spanish
      chunking_mode: "none"  # Buffer already chunks
      enable_alignment: false

  # Display results
  - id: display
    node_type: TextDisplay

edges:
  - from: microphone
    to: resampler
  - from: resampler
    to: buffer
  - from: buffer
    to: transcriber
  - from: transcriber
    to: display
```

**Run**:
```bash
remotemedia run realtime_transcription.yaml
```

**Expected Flow**:
1. Microphone captures audio (1s chunks at 48kHz)
2. Resampler converts to 16kHz
3. Buffer accumulates 5 seconds
4. Transcriber processes 5s chunk
5. Text appears on screen ~500ms-1s later

---

## Troubleshooting

### Issue 1: "Sample rate must be 16000Hz"

**Cause**: OmniASR requires 16kHz audio

**Solution**: Add `AudioResampleNode` before transcriber:

```yaml
- id: resampler
  node_type: AudioResampleNode
  params:
    target_sample_rate: 16000

edges:
  - from: audio_source
    to: resampler  # MUST resample first
  - from: resampler
    to: transcriber
```

---

### Issue 2: "GPU out of memory"

**Cause**: Model too large for GPU

**Solutions**:

1. **Use smaller model**:
   ```yaml
   params:
     model_card: "omniASR_LLM_300M"  # 2GB instead of 6GB
   ```

2. **Force CPU mode**:
   ```yaml
   params:
     device: "cpu"
   ```

3. **Reduce concurrent workers**:
   ```yaml
   # In pipeline config
   execution:
     max_workers: 1  # Only 1 transcription worker
   ```

---

### Issue 3: Slow First Run

**Cause**: Model downloading from HuggingFace

**Solution**: Wait for download (one-time only):

```
Downloading model...
████████████████████████████████████  4.2GB/4.2GB
✓ Model cached at ~/.cache/fairseq2/
```

**Subsequent runs** use cached model and start in seconds.

---

### Issue 4: Poor Accuracy

**Possible Causes & Solutions**:

1. **Wrong language detected**:
   ```yaml
   params:
     language: "eng_Latn"  # Specify explicitly
   ```

2. **Audio quality too low**:
   - Ensure sample rate is 16kHz (not 8kHz)
   - Check audio isn't clipped or distorted
   - Use noise reduction upstream if needed

3. **Audio too short**:
   - Auto-detection fails on < 1 second clips
   - Accumulate longer chunks (3-5 seconds minimum)

---

### Issue 5: "Module not found: omnilingual_asr"

**Solution**: Install package:

```bash
pip install omnilingual-asr
```

**If still failing**, check Python environment:

```bash
# Verify correct environment
which python
pip list | grep omnilingual
```

---

## Performance Optimization

### Best Practices

1. **Use GPU**: 5-10x faster than CPU
   ```yaml
   params:
     device: "cuda"
   ```

2. **Smaller model for real-time**:
   ```yaml
   params:
     model_card: "omniASR_LLM_300M"  # Faster
   ```

3. **Optimal chunk size**: 3-10 seconds
   - Too short: Poor accuracy, overhead
   - Too long: High latency

4. **Limit workers**: 1-2 transcription nodes max
   - Each loads full model copy
   - Memory usage = N workers × model size

### Benchmarks

**GPU (NVIDIA RTX 3090, 24GB VRAM)**:

| Chunk Size | 1B Model | 300M Model |
|------------|----------|------------|
| 1 second | ~300ms | ~150ms |
| 5 seconds | ~800ms | ~400ms |
| 30 seconds | ~2s | ~1s |

**CPU (Intel i9-12900K, 32GB RAM)**:

| Chunk Size | 1B Model | 300M Model |
|------------|----------|------------|
| 1 second | ~1.5s | ~700ms |
| 5 seconds | ~4s | ~2s |
| 30 seconds | ~15s | ~7s |

---

## Next Steps

### Explore Advanced Features

1. **Multi-Language Support**: Try different languages from 200+ supported
2. **Custom Pipelines**: Combine with NER, sentiment analysis, translation
3. **Streaming Optimizations**: Fine-tune chunking and buffering

### Related Documentation

- [Full API Reference](./contracts/node-interface.md)
- [Data Model](./data-model.md)
- [Research Findings](./research.md)

### Get Help

- **Issues**: File at RemoteMedia SDK GitHub
- **Questions**: Community Discord/Forum
- **OmniASR Docs**: [OmniASR Documentation](https://huggingface.co/omnilingual-asr)

---

## Summary

You've learned how to:

✅ Install OmniASR dependencies
✅ Create a basic transcription pipeline
✅ Configure language detection and explicit selection
✅ Use different chunking modes (none, static, VAD)
✅ Generate word-level timestamps for subtitles
✅ Troubleshoot common issues

**Ready to build?** Start with the basic example and customize for your use case!
