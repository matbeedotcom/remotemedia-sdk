# API Contract: OmniASRNode Interface

**Feature**: OmniASR Streaming Transcription Node
**Branch**: `008-omniasr-streaming-node`
**Date**: 2025-11-11

## Overview

This document defines the public API contract for the OmniASRNode, including initialization parameters, input/output formats, error handling, and behavior guarantees.

---

## Node Registration

**Module**: `remotemedia.nodes.omniasr`
**Class**: `OmniASRNode`
**Node Type String**: `"OmniASRTranscriber"` (for pipeline manifests)

**Import**:
```python
from remotemedia.nodes.omniasr import OmniASRNode
```

---

## Constructor Signature

```python
class OmniASRNode(Node):
    def __init__(
        self,
        model_card: str = "omniASR_LLM_1B",
        language: Optional[str] = None,
        chunking_mode: str = "none",
        chunk_duration: float = 30.0,
        device: Optional[str] = None,
        enable_alignment: bool = False,
        batch_size: int = 1,
        **kwargs
    ) -> None:
        """
        Initialize OmniASR transcription node.

        Args:
            model_card: OmniASR model variant to use
                - "omniASR_LLM_1B" (default): 1B parameter model, higher accuracy
                - "omniASR_LLM_300M": 300M parameter model, faster inference

            language: Language for transcription (ISO 639-3 with script suffix)
                - None (default): Automatic language detection
                - "eng_Latn": English with Latin script
                - "spa_Latn": Spanish with Latin script
                - "ara_Arab": Arabic with Arabic script
                - See omnilingual_asr.models.wav2vec2_llama.lang_ids.supported_langs
                  for complete list of 200+ supported languages

            chunking_mode: How to segment audio for transcription
                - "none" (default): Process entire input as single chunk
                - "static": Fixed-duration chunks (uses chunk_duration)
                - "vad": Voice Activity Detection, chunk at speech boundaries

            chunk_duration: Target chunk duration in seconds (for static mode)
                - Default: 30.0
                - Range: 1.0 - 300.0
                - Ignored when chunking_mode="none" or "vad"

            device: Device for model execution
                - None (default): Auto-detect (GPU if available, else CPU)
                - "cuda": Force GPU execution (raises error if unavailable)
                - "cpu": Force CPU execution

            enable_alignment: Whether to generate word-level timestamps
                - False (default): Text-only transcription
                - True: Include word timestamps for subtitle/karaoke generation

            batch_size: Batch size for transcription
                - Default: 1 (optimal for streaming)
                - Higher values may improve throughput for batch processing

            **kwargs: Additional arguments passed to Node base class
                - name: Node instance name (for logging/debugging)
                - config: Configuration dict (alternative to kwargs)

        Raises:
            ValueError: If model_card not in supported models
            ValueError: If chunking_mode not in ["none", "static", "vad"]
            ValueError: If chunk_duration <= 0.0
        """
```

---

## Lifecycle Methods

### initialize()

```python
async def initialize(self) -> None:
    """
    Load OmniASR model and resources.

    Called once by RemoteMedia SDK before any process() calls.
    Blocks pipeline startup until model loading completes (may take 10-60s).

    Side Effects:
        - Downloads model from HuggingFace if not cached
        - Allocates GPU/CPU memory for model
        - Initializes VAD model if chunking_mode="vad"

    Raises:
        RuntimeError: If model loading fails
        OSError: If model cache directory not writable
        torch.cuda.OutOfMemoryError: If GPU memory insufficient (auto-fallback to CPU attempted)

    Post-Conditions:
        - self._model is loaded and ready for transcription
        - Device (GPU/CPU) is selected and logged
        - Node is ready to accept process() calls
    """
```

---

### process()

```python
async def process(
    self,
    data: Tuple[np.ndarray, int]
) -> Dict[str, Any]:
    """
    Transcribe audio chunk.

    Args:
        data: Tuple containing:
            - audio_data: numpy.ndarray
                - Shape: (samples,) for mono, (channels, samples) for multi-channel
                - Dtype: np.float32 or np.float64
                - Value range: [-1.0, 1.0] (normalized audio samples)
            - sample_rate: int
                - MUST be 16000 (16kHz)
                - Use AudioResampleNode upstream if source is different rate

    Returns:
        TranscriptionOutput dict with fields:
            {
                "text": str,                    # Transcribed text (UTF-8)
                "language": str,                # ISO 639-3 language code (e.g., "eng_Latn")
                "success": bool,                # True if transcription succeeded
                "word_timestamps": [            # Optional, if enable_alignment=True
                    {
                        "word": str,
                        "start": float,         # Seconds from chunk start
                        "end": float,
                        "confidence": float     # 0.0-1.0, optional
                    },
                    ...
                ],
                "chunk_metadata": {             # Optional metadata
                    "chunk_index": int,
                    "duration": float,
                    "sample_rate": int,
                    "chunking_mode": str,
                    "device": str
                },
                "error": str                    # Present only if success=False
            }

    Raises:
        TypeError: If data is not tuple of (ndarray, int)
        ValueError: If sample_rate != 16000
        ValueError: If audio_data dtype invalid
        asyncio.TimeoutError: If transcription exceeds 30s timeout

    Behavior Guarantees:
        - Stateless: Each call is independent, no cross-chunk context
        - Non-blocking: Runs CPU-bound transcription in thread executor
        - Timeout: Maximum 30 seconds per chunk (configurable in future)
        - Error recovery: Returns error dict instead of raising for transcription failures

    Performance:
        - GPU (1B model): ~500ms-2s for 5s chunk
        - CPU (1B model): ~2s-5s for 5s chunk
        - Varies with hardware and audio complexity
    """
```

---

### cleanup()

```python
async def cleanup(self) -> None:
    """
    Release model and resources.

    Called once by RemoteMedia SDK when pipeline stops or errors.

    Side Effects:
        - Releases reference to OmniASR model
        - Clears GPU cache if applicable
        - Resets internal state

    Post-Conditions:
        - Model memory can be reclaimed by garbage collector
        - Node cannot be reused after cleanup (must create new instance)
    """
```

---

## Input Format Contract

### Audio Data Requirements

**Required Format**: Tuple `(audio_data, sample_rate)`

**audio_data** (numpy.ndarray):
- **Shape**: `(samples,)` for mono OR `(channels, samples)` for stereo/multi-channel
- **Dtype**: `np.float32` or `np.float64`
- **Value Range**: `[-1.0, 1.0]` (normalized PCM samples)
- **Encoding**: Linear PCM (not compressed)

**sample_rate** (int):
- **Value**: MUST be `16000` (16kHz)
- **Enforcement**: Strict validation - raises ValueError if != 16000
- **Reason**: OmniASR model trained on 16kHz audio

**Channel Handling**:
- Mono: Used directly
- Stereo/Multi-channel: Converted to mono internally (average channels)

**Example Valid Inputs**:

```python
# Mono audio, 1 second at 16kHz
audio_mono = np.random.randn(16000).astype(np.float32)
input_data = (audio_mono, 16000)

# Stereo audio, 2 seconds at 16kHz
audio_stereo = np.random.randn(2, 32000).astype(np.float32)
input_data = (audio_stereo, 16000)
```

**Invalid Inputs** (will raise errors):

```python
# Wrong sample rate
(audio, 44100)  # ValueError: OmniASR requires 16kHz

# Wrong dtype
(np.array([...], dtype=np.int16), 16000)  # ValueError: Expected float32/64

# Wrong shape
(np.array([[[...]]], ndim=3), 16000)  # ValueError: Expected 1D or 2D array
```

---

## Output Format Contract

### Success Response

**Type**: `Dict[str, Any]`

**Required Fields**:

| Field | Type | Always Present? | Description |
|-------|------|----------------|-------------|
| `text` | `str` | Yes | Transcribed text (UTF-8 encoded) |
| `language` | `str` | Yes | Detected/specified language code |
| `success` | `bool` | Yes | `True` for successful transcription |

**Optional Fields**:

| Field | Type | Condition | Description |
|-------|------|-----------|-------------|
| `word_timestamps` | `List[Dict]` | `enable_alignment=True` | Word-level timing info |
| `chunk_metadata` | `Dict` | Always | Chunk processing metadata |

**Example Success Response**:

```json
{
  "text": "The quick brown fox jumps over the lazy dog.",
  "language": "eng_Latn",
  "success": true,
  "word_timestamps": [
    {"word": "The", "start": 0.0, "end": 0.12, "confidence": 0.98},
    {"word": "quick", "start": 0.12, "end": 0.36, "confidence": 0.95},
    {"word": "brown", "start": 0.36, "end": 0.64, "confidence": 0.97}
  ],
  "chunk_metadata": {
    "chunk_index": 0,
    "duration": 3.5,
    "sample_rate": 16000,
    "chunking_mode": "none",
    "device": "cuda"
  }
}
```

### Error Response

**Required Fields**:

| Field | Type | Value | Description |
|-------|------|-------|-------------|
| `text` | `str` | `""` (empty) | No transcription available |
| `language` | `str` | `""` (empty) | Language unknown |
| `success` | `bool` | `False` | Indicates failure |
| `error` | `str` | Error message | Human-readable error description |

**Example Error Response**:

```json
{
  "text": "",
  "language": "",
  "success": false,
  "error": "GPU out of memory, CPU fallback failed: Model loading failed"
}
```

**Common Error Messages**:

| Error | Trigger | Recovery |
|-------|---------|----------|
| `"Sample rate must be 16000Hz, got {rate}Hz"` | Invalid sample rate | Add AudioResampleNode upstream |
| `"GPU out of memory, CPU fallback failed"` | Insufficient GPU RAM | Use smaller model, reduce workers |
| `"Transcription timeout after 30s"` | Very long/complex audio | Split into smaller chunks |
| `"Invalid audio format: expected float32/64"` | Wrong dtype | Convert audio to float32 |
| `"VAD model loading failed, using static chunking"` | Silero VAD unavailable | Warning only, continues with static |

---

## Configuration Contract

### Pipeline Manifest Example

**YAML Format** (RemoteMedia SDK pipelines):

```yaml
nodes:
  # Resample to 16kHz (required if source != 16kHz)
  - id: resampler
    node_type: AudioResampleNode
    params:
      target_sample_rate: 16000

  # OmniASR transcription
  - id: transcriber
    node_type: OmniASRTranscriber
    executor: multiprocess  # REQUIRED: Use multiprocess executor
    params:
      model_card: "omniASR_LLM_1B"
      language: null  # Auto-detect
      chunking_mode: "vad"
      enable_alignment: true
      device: null  # Auto-detect GPU/CPU

edges:
  - from: audio_source
    to: resampler
  - from: resampler
    to: transcriber
  - from: transcriber
    to: output_sink
```

**Python API** (Programmatic usage):

```python
from remotemedia.nodes.omniasr import OmniASRNode
from remotemedia.pipeline import Pipeline

# Create node
transcriber = OmniASRNode(
    name="omniasr_transcriber",
    model_card="omniASR_LLM_300M",  # Faster model
    language="eng_Latn",  # Force English
    chunking_mode="none",  # No chunking
    enable_alignment=False  # Text only
)

# Add to pipeline
pipeline = Pipeline()
pipeline.add_node(transcriber)
pipeline.connect("audio_source", "omniasr_transcriber")

# Run
await pipeline.initialize()
result = await transcriber.process((audio_data, 16000))
print(result["text"])
```

---

## Supported Languages

**Source**: `omnilingual_asr.models.wav2vec2_llama.lang_ids.supported_langs`

**Total**: 200+ languages

**Common Examples**:

| Language | Code | Script |
|----------|------|--------|
| English | `eng_Latn` | Latin |
| Spanish | `spa_Latn` | Latin |
| French | `fra_Latn` | Latin |
| German | `deu_Latn` | Latin |
| Chinese (Mandarin) | `cmn_Hans` | Simplified Hanzi |
| Arabic | `ara_Arab` | Arabic |
| Hindi | `hin_Deva` | Devanagari |
| Russian | `rus_Cyrl` | Cyrillic |
| Japanese | `jpn_Jpan` | Japanese (mixed scripts) |
| Korean | `kor_Hang` | Hangul |

**Full List**: Query at runtime:
```python
from omnilingual_asr.models.wav2vec2_llama.lang_ids import supported_langs
print(supported_langs)  # List of all supported language codes
```

---

## Behavioral Contracts

### Guarantees

1. **Stateless Processing**:
   - Each `process()` call is independent
   - No cross-chunk state or context
   - Results depend only on input chunk

2. **Thread Safety**:
   - Not thread-safe (uses PyTorch models)
   - Safe for multiprocess execution (process isolation)
   - Each worker process loads independent model copy

3. **Resource Management**:
   - Model loaded once in `initialize()`
   - Model memory released in `cleanup()`
   - GPU cache cleared on cleanup

4. **Error Handling**:
   - Transcription errors return error dict, don't raise exceptions
   - Configuration errors raise ValueError in `__init__`
   - Initialization errors raise in `initialize()`

5. **Performance**:
   - Non-blocking processing (uses executor)
   - Timeout protection (30s max per chunk)
   - Latency proportional to chunk size

### Limitations

1. **Sample Rate Restriction**:
   - ONLY 16kHz audio accepted
   - No automatic resampling built-in
   - Must use AudioResampleNode upstream if needed

2. **Memory Usage**:
   - Each worker process loads full model (1-6 GB)
   - No model sharing across processes
   - Recommend 1-2 transcription workers max

3. **Model Loading Time**:
   - Initial `initialize()` may take 10-60 seconds
   - Downloads model from HuggingFace on first use
   - Subsequent loads faster (uses cache)

4. **Language Detection Accuracy**:
   - Auto-detection may fail on very short clips (< 1s)
   - Mixed-language audio uses first detected language
   - Specify language explicitly for best results

---

## Integration Requirements

### Dependencies

**Python Package**:
```bash
pip install omnilingual_asr torch silero_vad librosa soundfile numpy
```

**Environment Variables**:
```bash
# Optional: Model cache directory (default: ~/.cache/fairseq2)
export FAIRSEQ2_CACHE_DIR=/path/to/model/cache

# Optional: Hugging Face token (if models require authentication)
export HF_TOKEN=your_token_here
```

### Upstream Nodes

**Required**:
- Audio source providing `(np.ndarray, int)` tuple format

**Recommended**:
- `AudioResampleNode` to ensure 16kHz sample rate

**Example Chain**:
```
AudioSource → AudioResampleNode(16kHz) → OmniASRNode → TextSink
```

### Downstream Nodes

**Compatible Outputs**:
- Text processing nodes (accept `Dict[str, Any]` or extract `result["text"]`)
- Logging/storage nodes
- Analysis nodes (sentiment, NER, etc.)

---

## Versioning and Compatibility

**API Version**: 1.0.0
**Stability**: Beta (may change based on feedback)

**Breaking Change Policy**:
- Constructor parameters: Additive only (new params with defaults)
- Input format: No changes planned
- Output format: May add new fields, won't remove existing

**Compatibility**:
- RemoteMedia SDK: >= 0.3.0
- Python: >= 3.10
- PyTorch: >= 2.0.0
- omnilingual_asr: As specified in package

---

## Testing Contract

### Unit Test Coverage

**Required Tests**:
1. Initialization with various model_card values
2. Sample rate validation (accept 16kHz, reject others)
3. Audio format validation (dtype, shape)
4. Chunking mode variations (none, static, vad)
5. Error handling (GPU fallback, VAD failure)
6. Output format validation

### Integration Test Scenarios

**Required Scenarios**:
1. End-to-end transcription (real audio → text)
2. Multiprocess execution (IPC communication)
3. GPU and CPU device modes
4. Multiple languages
5. Word alignment generation
6. Error recovery paths

**Test Data Requirements**:
- Sample audio clips (1s, 5s, 30s)
- Multiple languages (English, Spanish, Arabic min)
- Various sample rates (8kHz, 16kHz, 44.1kHz)
- Edge cases (silence, noise, very short clips)

---

## Security Considerations

### Input Validation

- All inputs validated before processing
- Type checking enforced
- Value range validation (sample rate, chunk duration)

### Resource Limits

- Transcription timeout (30s default)
- Memory monitoring (log warnings)
- No arbitrary code execution

### Data Privacy

- Audio data not persisted (stateless)
- No logging of audio content
- Model caching via HuggingFace (respects cache policies)

---

## Changelog

### Version 1.0.0 (2025-11-11)

**Initial Release**:
- OmniASR integration for 200+ languages
- Three chunking modes (none, static, vad)
- GPU/CPU auto-detection with fallback
- Word-level alignment support
- Multiprocess execution compatibility
