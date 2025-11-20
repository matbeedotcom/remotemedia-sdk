# Data Model: OmniASR Streaming Transcription Node

**Feature**: OmniASR Streaming Transcription Node
**Branch**: `008-omniasr-streaming-node`
**Date**: 2025-11-11

## Overview

This document defines the core entities, data structures, and relationships for the OmniASR streaming transcription node integration into the RemoteMedia SDK.

---

## Entity Definitions

### 1. OmniASRNode

**Purpose**: Main node class that integrates OmniASR transcription into RemoteMedia pipelines

**Inheritance**: `remotemedia.core.node.Node`

**Configuration Attributes**:

| Attribute | Type | Default | Description |
|-----------|------|---------|-------------|
| `model_card` | `str` | `"omniASR_LLM_1B"` | OmniASR model variant (1B or 300M parameters) |
| `language` | `Optional[str]` | `None` | ISO 639-3 language code with script (e.g., "eng_Latn", "ara_Arab"). None = auto-detect |
| `chunking_mode` | `str` | `"none"` | Chunking strategy: "none", "static", or "vad" |
| `chunk_duration` | `float` | `30.0` | Target chunk duration in seconds (for static mode) |
| `device` | `Optional[str]` | `None` | Device override: "cuda", "cpu", or None (auto-detect) |
| `enable_alignment` | `bool` | `False` | Whether to generate word-level timestamps |
| `batch_size` | `int` | `1` | Batch size for transcription (streaming=1) |

**State Attributes**:

| Attribute | Type | Description |
|-----------|------|-------------|
| `_model` | `Optional[MMSModel]` | Singleton model instance (lazy loaded) |
| `_chunker` | `Optional[VADChunker]` | Audio chunker instance (lazy loaded if needed) |
| `_initialized` | `bool` | Whether initialize() has been called |
| `is_streaming` | `bool` | Always True (indicates streaming node) |

**Lifecycle Methods**:

```python
async def initialize(self) -> None:
    """
    Load model and resources during node startup.
    Called once by SDK before any process() calls.
    """

async def process(self, data: Tuple[np.ndarray, int]) -> Dict[str, Any]:
    """
    Process single audio chunk and return transcription.
    Called for each chunk in the stream.

    Args:
        data: Tuple of (audio_data, sample_rate)

    Returns:
        TranscriptionOutput dict
    """

async def cleanup(self) -> None:
    """
    Release resources before shutdown.
    Called once by SDK when pipeline stops.
    """
```

**Validation Rules**:
- `model_card` must be supported variant ("omniASR_LLM_1B" or "omniASR_LLM_300M")
- `chunking_mode` must be one of: "none", "static", "vad"
- `chunk_duration` must be > 0.0 seconds
- `batch_size` typically 1 for streaming (batch processing not priority)
- If `language` specified, must be in supported languages list

**State Transitions**:
```
Created → Initialized → Processing → Processing → ... → Cleaned Up
   ↓            ↓            ↓                              ↓
__init__   initialize()  process()                    cleanup()
```

---

### 2. MMSModel

**Purpose**: Singleton wrapper around OmniASR's Wav2Vec2InferencePipeline

**Singleton Pattern**: One instance per process (multiprocess isolation)

**Configuration Attributes**:

| Attribute | Type | Description |
|-----------|------|-------------|
| `model_card` | `str` | Model variant identifier |
| `device` | `str` | Target device ("cuda" or "cpu") |

**State Attributes**:

| Attribute | Type | Description |
|-----------|------|-------------|
| `_instance` | `Optional[MMSModel]` | Class-level singleton instance |
| `_initialized` | `bool` | Whether model has been loaded |
| `_pipeline` | `Optional[Wav2Vec2InferencePipeline]` | OmniASR pipeline object |

**Methods**:

```python
@classmethod
def get_instance(cls, model_card: str, device: str) -> MMSModel:
    """
    Get or create singleton instance.

    Args:
        model_card: Model variant
        device: Target device

    Returns:
        Singleton MMSModel instance
    """

def transcribe_audio(
    self,
    audio_tensor: torch.Tensor,
    batch_size: int = 1,
    language_with_scripts: Optional[List[str]] = None
) -> List[Dict[str, Any]]:
    """
    Transcribe audio tensor using OmniASR pipeline.

    Args:
        audio_tensor: 1D audio waveform (16kHz, float32)
        batch_size: Batch size for processing
        language_with_scripts: Optional language codes (None = auto-detect)

    Returns:
        List of transcription results with text and metadata
    """
```

**Validation Rules**:
- `audio_tensor` must be 1D torch.Tensor
- Sample rate assumed to be 16kHz (caller must resample)
- Audio data must be float32 or convertible to float32

---

### 3. VADChunker

**Purpose**: Audio segmentation using Voice Activity Detection

**Source**: Adapted from `examples/omniasr-transcriptions/server/inference/audio_chunker.py`

**Singleton Pattern**: VAD model singleton (shared silero model instance)

**Configuration Constants**:

| Constant | Value | Description |
|----------|-------|-------------|
| `TARGET_CHUNK_DURATION` | `30.0` | Target chunk duration in seconds |
| `MIN_CHUNK_DURATION` | `5.0` | Minimum chunk duration in seconds |
| `SAMPLE_RATE` | `16000` | Required sample rate (16kHz) |

**State Attributes**:

| Attribute | Type | Description |
|-----------|------|-------------|
| `_instance` | `Optional[VADChunker]` | Class-level singleton instance |
| `vad_model` | `Optional[Any]` | Silero VAD model (loaded once) |

**Methods**:

```python
def chunk_audio(
    self,
    audio_tensor: torch.Tensor,
    sample_rate: int = 16000,
    mode: str = "vad",
    chunk_duration: float = 30.0
) -> List[AudioChunk]:
    """
    Chunk audio using specified strategy.

    Args:
        audio_tensor: 1D audio waveform
        sample_rate: Sample rate (must be 16kHz)
        mode: "none", "static", or "vad"
        chunk_duration: Target duration for static mode

    Returns:
        List of AudioChunk dicts
    """
```

**Validation Rules**:
- `sample_rate` must be 16000 Hz
- `audio_tensor` must be 1D
- `mode` must be one of: "none", "static", "vad"
- `chunk_duration` > 0.0

**Chunking Strategies**:

1. **None Mode** (`_create_single_chunk()`):
   - Returns entire audio as single chunk
   - No segmentation
   - Fastest, but may exceed optimal chunk size

2. **Static Mode** (`_chunk_static()`):
   - Fixed-duration chunks (e.g., 30s)
   - Respects MIN_CHUNK_DURATION (5s)
   - Simple, predictable

3. **VAD Mode** (`_chunk_with_vad()`):
   - Speech boundary detection using Silero VAD
   - Creates chunks at silence gaps
   - Respects target duration while finding natural breaks
   - Best quality but higher latency

---

### 4. AudioChunk

**Purpose**: Internal representation of segmented audio with metadata

**Type**: TypedDict (structured dictionary)

**Fields**:

| Field | Type | Description |
|-------|------|-------------|
| `start_time` | `float` | Start time in seconds from beginning of original audio |
| `end_time` | `float` | End time in seconds |
| `duration` | `float` | Chunk duration in seconds |
| `audio_data` | `torch.Tensor` | Audio waveform for this chunk (1D tensor) |
| `sample_rate` | `int` | Sample rate (always 16000) |
| `chunk_index` | `int` | Zero-based chunk index |

**Example**:
```python
{
    "start_time": 0.0,
    "end_time": 5.3,
    "duration": 5.3,
    "audio_data": torch.Tensor([...]),  # 1D tensor, shape (84800,) for 5.3s at 16kHz
    "sample_rate": 16000,
    "chunk_index": 0
}
```

**Validation Rules**:
- `start_time` >= 0.0
- `end_time` > `start_time`
- `duration` = `end_time` - `start_time`
- `audio_data.shape[0]` = `int(duration * sample_rate)`
- `sample_rate` = 16000 (hardcoded)
- `chunk_index` >= 0

---

### 5. TranscriptionOutput

**Purpose**: Output data structure returned by OmniASRNode

**Type**: TypedDict (structured dictionary)

**Fields**:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `text` | `str` | Yes | Transcribed text (UTF-8 encoded) |
| `language` | `str` | Yes | Detected or specified language (ISO 639-3 with script) |
| `success` | `bool` | Yes | Whether transcription succeeded |
| `word_timestamps` | `List[WordTimestamp]` | No | Word-level timing info (if alignment enabled) |
| `chunk_metadata` | `ChunkMetadata` | No | Information about audio chunk processed |
| `error` | `str` | No | Error message if success=False |

**Example (Success)**:
```python
{
    "text": "Hello world, this is a test.",
    "language": "eng_Latn",
    "success": True,
    "word_timestamps": [
        {"word": "Hello", "start": 0.0, "end": 0.5},
        {"word": "world", "start": 0.5, "end": 1.0},
        # ...
    ],
    "chunk_metadata": {
        "chunk_index": 0,
        "duration": 3.2,
        "sample_rate": 16000
    }
}
```

**Example (Error)**:
```python
{
    "text": "",
    "language": "",
    "success": False,
    "error": "GPU out of memory, CPU fallback failed"
}
```

**Validation Rules**:
- `text` always present (empty string if error)
- `success` must be bool
- If `success=True`, `language` must be valid ISO code
- If `success=False`, `error` should be present
- `word_timestamps` only present if `enable_alignment=True`

---

### 6. WordTimestamp

**Purpose**: Word-level timing information for alignment/subtitles

**Type**: TypedDict

**Fields**:

| Field | Type | Description |
|-------|------|-------------|
| `word` | `str` | Word text (UTF-8) |
| `start` | `float` | Start time in seconds (relative to chunk start) |
| `end` | `float` | End time in seconds |
| `confidence` | `float` | Confidence score (0.0-1.0) - optional |

**Example**:
```python
{
    "word": "transcription",
    "start": 1.25,
    "end": 1.89,
    "confidence": 0.95
}
```

**Validation Rules**:
- `start` >= 0.0
- `end` > `start`
- `end` - `start` >= 0.05 (minimum 50ms word duration)
- `confidence` in range [0.0, 1.0] if present

---

### 7. ChunkMetadata

**Purpose**: Metadata about processed audio chunk

**Type**: TypedDict

**Fields**:

| Field | Type | Description |
|-------|------|-------------|
| `chunk_index` | `int` | Zero-based chunk index |
| `duration` | `float` | Chunk duration in seconds |
| `sample_rate` | `int` | Sample rate of processed audio |
| `chunking_mode` | `str` | Mode used: "none", "static", or "vad" |
| `device` | `str` | Device used: "cuda" or "cpu" |

**Example**:
```python
{
    "chunk_index": 2,
    "duration": 7.5,
    "sample_rate": 16000,
    "chunking_mode": "vad",
    "device": "cuda"
}
```

---

## Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│  Audio Pipeline Input                                           │
│  ┌────────────────────────────────┐                            │
│  │ (audio_data: np.ndarray,       │                            │
│  │  sample_rate: int)              │                            │
│  └────────────┬───────────────────┘                            │
└───────────────┼────────────────────────────────────────────────┘
                │
                ↓
┌───────────────────────────────────────────────────────────────┐
│  OmniASRNode.process()                                        │
│                                                                │
│  1. Validate sample rate (must be 16kHz)                      │
│  2. Convert np.ndarray → torch.Tensor                         │
│  3. Optional: Apply chunking (VAD/static)                     │
│     ┌──────────────────┐                                      │
│     │  VADChunker       │                                      │
│     │  chunk_audio()    │                                      │
│     └─────┬────────────┘                                      │
│           ↓                                                    │
│     List[AudioChunk]                                           │
│                                                                │
│  4. For each chunk:                                            │
│     ┌──────────────────────────────────┐                      │
│     │  MMSModel.transcribe_audio()     │                      │
│     │  ┌──────────────────────────┐    │                      │
│     │  │ wav_to_bytes()           │    │                      │
│     │  │ torch.Tensor → bytes     │    │                      │
│     │  └──────┬───────────────────┘    │                      │
│     │         ↓                         │                      │
│     │  Wav2Vec2InferencePipeline       │                      │
│     │  .transcribe([audio_bytes])      │                      │
│     │         ↓                         │                      │
│     │  Raw transcription results        │                      │
│     └─────────┬───────────────────────┘                      │
│               ↓                                                │
│     TranscriptionOutput dict                                   │
│                                                                │
│  5. Merge results from all chunks                             │
│  6. Return final TranscriptionOutput                           │
└─────────────────┬─────────────────────────────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────────────────────────────┐
│  Output to Next Node / Consumer                                 │
│  ┌────────────────────────────────────────────────────┐        │
│  │  {                                                  │        │
│  │    "text": "transcribed text...",                  │        │
│  │    "language": "eng_Latn",                         │        │
│  │    "success": true,                                 │        │
│  │    "word_timestamps": [...],                        │        │
│  │    "chunk_metadata": {...}                          │        │
│  │  }                                                  │        │
│  └────────────────────────────────────────────────────┘        │
└─────────────────────────────────────────────────────────────────┘
```

---

## Relationships

### Composition

- **OmniASRNode** contains (lazy loads):
  - 1× **MMSModel** (singleton)
  - 0 or 1× **VADChunker** (if chunking enabled)

- **MMSModel** contains:
  - 1× **Wav2Vec2InferencePipeline** (OmniASR library object)

- **VADChunker** contains:
  - 0 or 1× Silero VAD model (if VAD mode used)

### Dependencies

- **OmniASRNode** depends on:
  - RemoteMedia SDK `Node` base class
  - **MMSModel** for transcription
  - **VADChunker** for optional chunking
  - `wav_to_bytes()` utility for format conversion

- **MMSModel** depends on:
  - `omnilingual_asr.Wav2Vec2InferencePipeline`
  - PyTorch (`torch`)
  - `wav_to_bytes()` utility

- **VADChunker** depends on:
  - `silero_vad` library
  - PyTorch (`torch`)
  - `soundfile` (for audio I/O if needed)

### Data Transformations

```
Input Data Flow:
(np.ndarray, int)                           # SDK format
    ↓
torch.Tensor                                # Converted for chunker
    ↓
List[AudioChunk]                            # After chunking
    ↓
List[bytes]                                 # wav_to_bytes conversion
    ↓
List[Dict]                                  # OmniASR raw results
    ↓
TranscriptionOutput                         # Formatted output
```

---

## Storage and Persistence

### Model Caching

- **Location**: Controlled by `FAIRSEQ2_CACHE_DIR` environment variable
- **Format**: OmniASR model files (HuggingFace format)
- **Size**: 1-6 GB depending on model variant
- **Lifecycle**: Persistent across sessions, loaded on demand

### Runtime Memory

- **Model Instance**: 1-6 GB GPU/RAM (singleton per process)
- **VAD Model**: ~50 MB (singleton per process)
- **Per-Chunk Audio**: ~200 KB - 2 MB (16kHz float32, 5-30s chunks)
- **Results**: Minimal (~1-10 KB per chunk for text output)

### Multiprocess Considerations

- Each worker process loads **independent** model copies
- No shared memory between processes (iceoryx2 for IPC data only)
- Memory usage = N workers × model size
- **Recommendation**: Limit transcription nodes to 1-2 workers

---

## Validation and Constraints

### Input Validation

**OmniASRNode.process() Entry Point**:

```python
async def process(self, data: Tuple[np.ndarray, int]):
    # 1. Type validation
    if not isinstance(data, (tuple, list)):
        raise TypeError(f"Expected tuple, got {type(data)}")

    if len(data) != 2:
        raise ValueError(f"Expected (audio, sr) tuple, got length {len(data)}")

    audio_data, sample_rate = data

    # 2. Audio data validation
    if not isinstance(audio_data, np.ndarray):
        raise TypeError(f"Expected numpy array, got {type(audio_data)}")

    if audio_data.dtype not in [np.float32, np.float64]:
        raise ValueError(f"Expected float32/64, got {audio_data.dtype}")

    # 3. Sample rate validation
    if sample_rate != 16000:
        raise ValueError(
            f"OmniASR requires 16kHz audio, got {sample_rate}Hz. "
            f"Add AudioResampleNode before this node."
        )

    # 4. Proceed with transcription...
```

### Configuration Validation

**OmniASRNode.__init__()**:

```python
def __init__(self, model_card="omniASR_LLM_1B", chunking_mode="none", **kwargs):
    # Validate model card
    SUPPORTED_MODELS = ["omniASR_LLM_1B", "omniASR_LLM_300M"]
    if model_card not in SUPPORTED_MODELS:
        raise ValueError(f"Unsupported model: {model_card}. Choose from {SUPPORTED_MODELS}")

    # Validate chunking mode
    CHUNKING_MODES = ["none", "static", "vad"]
    if chunking_mode not in CHUNKING_MODES:
        raise ValueError(f"Invalid chunking_mode: {chunking_mode}. Choose from {CHUNKING_MODES}")

    # Store config
    self.model_card = model_card
    self.chunking_mode = chunking_mode
```

---

## Error States

| Error Condition | Detection Point | Handling Strategy |
|----------------|----------------|-------------------|
| Invalid sample rate | `process()` entry | Raise ValueError with clear message |
| GPU OOM | `initialize()` | Catch RuntimeError, fallback to CPU |
| VAD model failure | `_get_chunker()` | Log warning, use static chunking |
| Transcription timeout | `process()` | AsyncIO timeout, return error result |
| Invalid audio format | `process()` entry | Raise TypeError/ValueError |
| Unsupported language | `process()` | Log warning, fallback to auto-detect |

**Error Result Format**:
```python
{
    "text": "",
    "language": "",
    "success": False,
    "error": "Descriptive error message"
}
```

---

## Performance Characteristics

### Memory Usage (Per Process)

| Component | Size | Notes |
|-----------|------|-------|
| OmniASR 1B model | 4-6 GB | GPU VRAM or RAM |
| OmniASR 300M model | 1-2 GB | GPU VRAM or RAM |
| Silero VAD model | ~50 MB | RAM |
| Audio chunk buffer | ~200 KB - 2 MB | Temporary, per chunk |
| **Total (1B model)** | **4-6 GB** | Per worker process |

### Processing Latency (Target)

| Chunk Size | GPU (1B model) | CPU (1B model) | GPU (300M model) |
|------------|----------------|----------------|------------------|
| 1 second | ~200-500 ms | ~1-2 s | ~100-300 ms |
| 5 seconds | ~500-1000 ms | ~2-5 s | ~300-800 ms |
| 30 seconds | ~1-2 s | ~5-15 s | ~800 ms-1.5 s |

**Note**: Actual latency depends on hardware (GPU model, CPU cores) and audio complexity.

---

## Future Extensions

### Potential Additions (Out of Current Scope)

1. **Confidence Scores**: Expose per-word or per-segment confidence from OmniASR
2. **Speaker Diarization**: Identify multiple speakers (requires different model)
3. **Punctuation Enhancement**: Post-process to improve punctuation quality
4. **Streaming Partial Results**: Yield incremental transcriptions (requires pipeline changes)
5. **Custom Vocabulary**: Support domain-specific terminology (requires model fine-tuning)

These extensions would require new entities or fields but are not part of initial implementation.
