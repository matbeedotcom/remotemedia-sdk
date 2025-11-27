# Model Requirements & Download Instructions

This document lists the models required for running examples locally.

## Quick Setup

```bash
# Download all models for basic examples
./scripts/download-models.sh

# Or manually download specific models below
```

## Speech-to-Text (STT)

### Whisper Models

| Model | Size | Memory | Quality | Download |
|-------|------|--------|---------|----------|
| tiny | 39M | ~1GB | Fair | Auto-downloaded on first use |
| base | 74M | ~1GB | Good | Auto-downloaded on first use |
| small | 244M | ~2GB | Better | Auto-downloaded on first use |
| medium | 769M | ~5GB | Great | Auto-downloaded on first use |
| large | 1550M | ~10GB | Best | Manual download recommended |

Models are automatically downloaded to `~/.cache/whisper/` on first use.

**Manual Download:**
```bash
# For faster startup, pre-download models:
python -c "import whisper; whisper.load_model('base')"
```

## Voice Activity Detection (VAD)

### Silero VAD

- **Model**: silero_vad.onnx (~2MB)
- **Location**: Auto-downloaded to `~/.cache/silero/`
- **Memory**: ~50MB runtime

## Text-to-Speech (TTS)

### Kokoro TTS

| Voice | Description | Gender |
|-------|-------------|--------|
| af_bella | American Female | F |
| af_sarah | American Female | F |
| am_adam | American Male | M |
| am_michael | American Male | M |
| bf_emma | British Female | F |
| bm_george | British Male | M |

**Model Location**: Auto-downloaded to `~/.cache/kokoro/`

**Memory**: ~500MB per voice loaded

## Large Language Models (LLM)

For voice assistant examples using local LLM:

### Phi-3 Mini (Recommended for Local)

- **Size**: 3.8B parameters (~2.3GB quantized)
- **Memory**: ~4GB VRAM or ~8GB RAM
- **Quality**: Good for simple Q&A

**Download:**
```bash
# Using llama.cpp compatible model
wget https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-gguf/resolve/main/Phi-3-mini-4k-instruct-q4.gguf
```

### Alternative: Use Remote LLM

For better quality responses, configure RemotePipelineNode to use a remote LLM API:

```yaml
# In your pipeline manifest
- id: llm
  node_type: RemotePipelineNode
  params:
    endpoint: "grpc://your-gpu-server:50051"
    pipeline_name: "llama-70b"
```

## Memory Requirements Summary

| Example | Minimum RAM | Recommended RAM | GPU VRAM |
|---------|-------------|-----------------|----------|
| CLI (transcribe) | 2GB | 4GB | Optional |
| Voice Assistant (local) | 8GB | 16GB | 4GB+ |
| Voice Assistant (hybrid) | 2GB | 4GB | N/A |
| Video Transcription | 4GB | 8GB | Optional |

## Troubleshooting

### "Model not found" Error

Models download on first use. If behind a firewall:

1. Download model manually from HuggingFace
2. Place in appropriate cache directory
3. Or set `HF_HOME` environment variable

### Out of Memory

Use smaller models:
```yaml
params:
  model: "tiny"  # Instead of "base" or "small"
```

Or enable hybrid mode to offload to remote server.

### Slow First Run

First run downloads and initializes models. Subsequent runs will be faster.

Pre-warm models:
```bash
# Pre-download all models
remotemedia warmup --all
```
