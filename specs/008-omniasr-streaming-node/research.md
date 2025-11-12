# Research & Discovery: OmniASR Streaming Transcription Node

**Feature**: OmniASR Streaming Transcription Node
**Branch**: `008-omniasr-streaming-node`
**Date**: 2025-11-11

## Overview

This document consolidates research findings for integrating OmniASR's Wav2Vec2InferencePipeline into the RemoteMedia SDK as a streaming transcription node. The research focused on analyzing the existing HuggingFace space implementation and determining optimal integration patterns.

---

## R1: OmniASR Integration Patterns

### Decision

**Use stateless chunk-by-chunk processing with lazy model initialization**

The Wav2Vec2InferencePipeline fully supports streaming use cases through stateless operation where each audio chunk is processed independently without cross-chunk dependencies.

### Rationale

**Analysis of `examples/omniasr-transcriptions/server/inference/mms_model_pipeline.py`:**

1. **Stateless Operation** (Lines 76-108):
   - `transcribe_audio()` method has no internal state
   - Each call takes audio tensor and returns transcription
   - No buffering or context maintained between calls

2. **Pipeline Architecture**:
   - Input: torch.Tensor or bytes
   - Processing: Independent per chunk
   - Output: List of transcription results

3. **Memory Management**:
   - Model loaded once during initialization (Line 67-70)
   - Stored in `_pipeline` attribute
   - Reused across all transcription calls

### Alternatives Considered

**Alternative 1: Batch Processing**
- **Approach**: Accumulate multiple chunks, process together
- **Rejected**: Adds latency, requires state management, unnecessary for stateless pipeline

**Alternative 2: Streaming API with Callbacks**
- **Approach**: Register callbacks for continuous stream processing
- **Rejected**: OmniASR pipeline doesn't provide streaming API, would require wrapper complexity

**Alternative 3: Model Per Request**
- **Approach**: Load model fresh for each transcription
- **Rejected**: Extremely slow (10-60s load time), wastes memory

### Implementation Approach

```python
class OmniASRNode(Node):
    def __init__(self, model_card="omniASR_LLM_1B", **kwargs):
        super().__init__(**kwargs)
        self.model_card = model_card
        self._model = None  # Lazy loaded

    async def initialize(self):
        """Load model once during node startup"""
        from omnilingual_asr.models.inference.pipeline import Wav2Vec2InferencePipeline
        device = "cuda" if torch.cuda.is_available() else "cpu"
        self._model = Wav2Vec2InferencePipeline(
            model_card=self.model_card,
            device=device
        )

    async def process(self, data: Tuple[np.ndarray, int]):
        """Process each chunk independently"""
        audio_data, sample_rate = data

        # Convert to torch tensor
        audio_tensor = torch.from_numpy(audio_data)

        # Transcribe (non-blocking via executor)
        result = await asyncio.get_event_loop().run_in_executor(
            None, self._transcribe_sync, audio_tensor
        )

        return {"text": result[0]["text"], "language": result[0].get("language")}

    def _transcribe_sync(self, audio_tensor):
        """Synchronous transcription wrapper"""
        from examples.omniasr_transcriptions.server.inference.audio_reading_tools import wav_to_bytes
        audio_bytes = wav_to_bytes(audio_tensor, sample_rate=16000)
        return self._model.transcribe([audio_bytes], batch_size=1)
```

**Key Patterns**:
- Lazy initialization in `initialize()` method
- Async processing with executor for CPU-bound transcription
- Stateless chunk processing
- Reuse existing `wav_to_bytes()` utility

---

## R2: Audio Format Conversion Strategy

### Decision

**Reuse existing `wav_to_bytes()` utility with automatic CPU migration and sample rate validation**

### Rationale

**Analysis of `examples/omniasr-transcriptions/server/inference/audio_reading_tools.py`:**

1. **Conversion Pipeline** (Lines 9-46):
   ```
   Input (torch.Tensor or np.ndarray)
     ↓
   CPU migration (if CUDA tensor)
     ↓
   Type normalization (ensure float32)
     ↓
   Shape handling (mono/stereo)
     ↓
   WAV serialization (soundfile)
     ↓
   Output (bytes as np.int8 array)
   ```

2. **Performance Characteristics**:
   - Zero-copy: torch → numpy conversion (if already CPU)
   - Serialization overhead: ~10-20% for WAV encoding
   - Sample rate: Hardcoded to 16kHz in pipeline

3. **Compatibility**:
   - Handles both torch.Tensor and np.ndarray inputs
   - Automatic CUDA → CPU migration
   - Robust shape normalization (mono/stereo)

### Alternatives Considered

**Alternative 1: Direct Tensor Processing**
- **Approach**: Pass torch.Tensor directly to pipeline without bytes conversion
- **Rejected**: OmniASR pipeline API requires bytes input (confirmed in Line 103)

**Alternative 2: Custom Resampling with torchaudio**
- **Approach**: Use `torchaudio.transforms.Resample` instead of librosa
- **Consideration**: Potentially faster, but would need benchmarking
- **Decision**: Stick with proven `wav_to_bytes()` initially, optimize later if needed

**Alternative 3: Pre-conversion in Upstream Node**
- **Approach**: Add dedicated format converter node before transcription
- **Rejected**: Adds pipeline complexity, `wav_to_bytes()` already handles conversion

### Implementation Approach

1. **Import Utility**:
   ```python
   from examples.omniasr_transcriptions.server.inference.audio_reading_tools import wav_to_bytes
   ```

2. **Sample Rate Validation**:
   ```python
   async def process(self, data: Tuple[np.ndarray, int]):
       audio_data, sample_rate = data

       # Validate sample rate (OmniASR requires 16kHz)
       if sample_rate != 16000:
           logger.warning(f"Sample rate {sample_rate}Hz != 16kHz, resampling required")
           # Option 1: Raise error requiring upstream resampling
           # Option 2: Auto-resample here (adds latency)
   ```

3. **Conversion Flow**:
   ```python
   # RemoteMedia SDK audio format: (np.ndarray, int)
   audio_data, sample_rate = data

   # Convert to torch tensor
   audio_tensor = torch.from_numpy(audio_data)

   # Convert to bytes for OmniASR pipeline
   audio_bytes = wav_to_bytes(audio_tensor, sample_rate=16000)

   # Transcribe
   results = self._model.transcribe([audio_bytes], batch_size=1)
   ```

**Recommendation**: Require upstream `AudioResampleNode` for sample rate conversion to 16kHz. This keeps transcription node simple and leverages existing SDK components.

---

## R3: VAD Chunking Adaptation

### Decision

**Make VAD chunking optional with three modes: 'none', 'static', 'vad' - default to 'none' for simplicity**

### Rationale

**Analysis of `examples/omniasr-transcriptions/server/inference/audio_chunker.py`:**

1. **Existing Implementation** (Lines 56-101):
   - Three modes: none (single chunk), static (fixed duration), vad (speech boundaries)
   - VAD uses Silero model for speech detection
   - Target chunk duration: 30 seconds, minimum: 5 seconds

2. **Stateful vs Stateless**:
   - **Current**: Stateless - each audio file chunked independently
   - **Streaming Challenge**: No cross-chunk context for VAD decisions
   - **Solution**: Process chunks as-is in 'none' mode, or apply VAD within each chunk in 'vad' mode

3. **VAD Model Lifecycle**:
   - Singleton pattern (Lines 28-40)
   - Loaded once, reused across calls
   - Safe for multiprocess (each process loads own instance)

### Alternatives Considered

**Alternative 1: Always Apply VAD**
- **Approach**: Force VAD chunking for all inputs
- **Rejected**: Adds latency, may cause issues with pre-chunked streams, user preference varies

**Alternative 2: Stateful VAD Across Chunks**
- **Approach**: Maintain VAD state between chunks for cross-chunk decisions
- **Rejected**: Complex state management, breaks multiprocess isolation, unnecessary for most use cases

**Alternative 3: Remove VAD Entirely**
- **Approach**: Only support 'none' mode (no chunking)
- **Rejected**: VAD provides value for long-form audio, should be available as option

### Implementation Approach

**Node Configuration**:
```python
class OmniASRNode(Node):
    def __init__(
        self,
        chunking_mode: str = "none",  # 'none', 'static', 'vad'
        chunk_duration: float = 30.0,  # for static mode
        **kwargs
    ):
        self.chunking_mode = chunking_mode
        self.chunk_duration = chunk_duration
        self._chunker = None  # Lazy loaded if needed
```

**Chunking Logic**:
```python
async def process(self, data: Tuple[np.ndarray, int]):
    audio_data, sample_rate = data

    if self.chunking_mode == "none":
        # Process entire chunk as-is
        chunks = [{"audio_data": audio_data, "start_time": 0.0}]
    else:
        # Apply chunking (VAD or static)
        if self._chunker is None:
            from .omniasr_chunker import AudioChunker
            self._chunker = AudioChunker()

        audio_tensor = torch.from_numpy(audio_data)
        chunks = self._chunker.chunk_audio(
            audio_tensor,
            sample_rate=sample_rate,
            mode=self.chunking_mode,
            chunk_duration=self.chunk_duration
        )

    # Transcribe each chunk
    results = []
    for chunk in chunks:
        result = await self._transcribe_chunk(chunk)
        results.append(result)

    return self._merge_results(results)
```

**Adaptation Strategy**:
1. **Copy** `audio_chunker.py` to `python-client/remotemedia/nodes/omniasr_chunker.py`
2. **Modify** to remove file I/O dependencies (already uses tensors)
3. **Keep** VAD model singleton pattern (safe for multiprocess)
4. **Simplify** to focus on three modes only

---

## R4: Multiprocess Executor Integration

### Decision

**Follow WhisperXTranscriber pattern with lazy model loading in initialize() and proper cleanup**

### Rationale

**Analysis of RemoteMedia SDK Multiprocess Architecture:**

1. **Node Lifecycle** (from `python-client/remotemedia/core/multiprocessing/node.py`):
   ```
   __init__() → initialize() → process() → process() → ... → cleanup()
        ↓            ↓            ↓                              ↓
   Config only  Load resources  Process data              Release resources
   ```

2. **Initialization Sequence** (Lines 293-346):
   - Messages queued during `initialize()`
   - Queued messages processed after init complete
   - No data loss even with slow model loading (10-60s)

3. **Process Isolation**:
   - Each node runs in separate Python process
   - Independent memory space (no shared state)
   - IPC via iceoryx2 shared memory transport
   - Session-scoped channel naming prevents conflicts

### Alternatives Considered

**Alternative 1: Eager Loading in __init__()**
- **Approach**: Load model immediately when node object created
- **Rejected**: Blocks multiprocess initialization, doesn't fit SDK lifecycle

**Alternative 2: On-Demand Loading in process()**
- **Approach**: Load model on first process() call
- **Rejected**: First request has huge latency, no clear initialization signal

**Alternative 3: Separate Model Process**
- **Approach**: Dedicated process for model, node proxies requests
- **Consideration**: Useful for memory optimization, but adds complexity
- **Decision**: Start simple with in-process model, consider later if memory issues

### Implementation Approach

**Lifecycle Methods**:
```python
class OmniASRNode(Node):
    def __init__(self, model_card="omniASR_LLM_1B", **kwargs):
        """Configuration only - no resource allocation"""
        super().__init__(**kwargs)
        self.model_card = model_card
        self._model = None
        self._chunker = None
        self.is_streaming = True

    async def initialize(self):
        """Load model and resources once during startup"""
        logger.info(f"Loading OmniASR model: {self.model_card}")

        from omnilingual_asr.models.inference.pipeline import Wav2Vec2InferencePipeline

        # Detect device
        device = "cuda" if torch.cuda.is_available() else "cpu"
        logger.info(f"Using device: {device}")

        # Load pipeline
        self._model = Wav2Vec2InferencePipeline(
            model_card=self.model_card,
            device=device
        )

        logger.info("✓ OmniASR model loaded successfully")

    async def process(self, data: Tuple[np.ndarray, int]):
        """Process audio chunk (called repeatedly)"""
        # Model guaranteed to be loaded here (SDK ensures initialize() completes first)
        assert self._model is not None, "Model not initialized"

        # Process chunk...

    async def cleanup(self):
        """Release resources before shutdown"""
        logger.info("Cleaning up OmniASR node")
        self._model = None
        self._chunker = None
        torch.cuda.empty_cache()  # Release GPU memory if used
```

**Session Management**:
- SDK handles session lifecycle automatically
- No explicit session state needed in node
- Channel naming includes session_id (handled by executor)

**Resource Cleanup**:
- `cleanup()` called when pipeline stops or errors
- Release model references
- Clear GPU cache if applicable

---

## R5: Error Handling and Fallbacks

### Decision

**Implement graceful fallbacks for all major failure modes with comprehensive logging**

### Rationale

**Critical Failure Modes Identified:**

1. **GPU OOM (Out of Memory)**:
   - Large models (1B params) may exceed VRAM
   - Multiple concurrent sessions compound issue

2. **VAD Model Loading Failure**:
   - Silero VAD may fail to download
   - Network issues or cache corruption

3. **Invalid Audio Format**:
   - Wrong sample rate (!=16kHz)
   - Corrupted data
   - Unsupported encoding

4. **Language Detection Failure**:
   - Auto-detection may fail on short/noisy clips
   - Unsupported language codes

5. **Transcription Timeout**:
   - Very long audio chunks
   - Slow CPU processing

### Alternatives Considered

**Alternative 1: Fail Fast**
- **Approach**: Raise exceptions immediately on any error
- **Rejected**: Crashes pipeline, bad user experience, loses partial results

**Alternative 2: Silent Failure**
- **Approach**: Return empty strings on errors
- **Rejected**: Users unaware of failures, no debugging info

**Alternative 3: Retry Logic**
- **Approach**: Automatically retry failed transcriptions
- **Consideration**: Useful for transient errors, but may waste time on persistent issues
- **Decision**: Don't retry (user can implement at pipeline level if needed)

### Implementation Approach

**1. GPU OOM → CPU Fallback**:
```python
async def initialize(self):
    try:
        # Try GPU first
        if torch.cuda.is_available():
            device = "cuda"
            self._model = Wav2Vec2InferencePipeline(
                model_card=self.model_card,
                device=device
            )
            logger.info("✓ Model loaded on GPU")
    except RuntimeError as e:
        if "out of memory" in str(e).lower():
            logger.warning(f"GPU OOM, falling back to CPU: {e}")
            torch.cuda.empty_cache()
            device = "cpu"
            self._model = Wav2Vec2InferencePipeline(
                model_card=self.model_card,
                device=device
            )
            logger.info("✓ Model loaded on CPU (fallback)")
        else:
            raise
```

**2. VAD Loading Failure → Static Chunking**:
```python
def _get_chunker(self):
    if self._chunker is not None:
        return self._chunker

    try:
        from .omniasr_chunker import AudioChunker
        self._chunker = AudioChunker()  # Loads VAD model internally
        return self._chunker
    except Exception as e:
        logger.error(f"Failed to load VAD model: {e}")
        logger.warning("Falling back to static chunking mode")
        # Create chunker with VAD disabled
        self._chunker = AudioChunker()
        self._chunker.vad_model = None  # Force static mode
        return self._chunker
```

**3. Invalid Audio Format → Clear Error**:
```python
async def process(self, data: Tuple[np.ndarray, int]):
    try:
        audio_data, sample_rate = data

        # Validate sample rate
        if sample_rate != 16000:
            raise ValueError(
                f"OmniASR requires 16kHz audio, got {sample_rate}Hz. "
                f"Add AudioResampleNode before OmniASRNode in pipeline."
            )

        # Validate audio data
        if not isinstance(audio_data, np.ndarray):
            raise TypeError(f"Expected numpy array, got {type(audio_data)}")

        if audio_data.dtype not in [np.float32, np.float64]:
            raise ValueError(f"Expected float32/float64 audio, got {audio_data.dtype}")

        # Process...

    except Exception as e:
        logger.error(f"Transcription failed: {e}", exc_info=True)
        # Return error result instead of crashing
        return {
            "text": "",
            "error": str(e),
            "success": False
        }
```

**4. Language Detection Failure → Auto-Detect**:
```python
async def process(self, data):
    # ...
    language = self.language  # User-specified or None

    if language and language not in SUPPORTED_LANGUAGES:
        logger.warning(f"Unsupported language '{language}', using auto-detection")
        language = None  # Fall back to auto-detect

    results = self._model.transcribe(
        [audio_bytes],
        batch_size=1,
        lang=[language] if language else None  # None = auto-detect
    )
```

**5. Timeout Protection**:
```python
async def process(self, data):
    try:
        # Run with timeout
        result = await asyncio.wait_for(
            asyncio.get_event_loop().run_in_executor(
                None, self._transcribe_sync, audio_tensor
            ),
            timeout=30.0  # 30 second max per chunk
        )
        return result
    except asyncio.TimeoutError:
        logger.error(f"Transcription timeout after 30s")
        return {"text": "", "error": "timeout", "success": False}
```

---

## Summary of Key Decisions

| Research Area | Decision | Rationale |
|--------------|----------|-----------|
| **Pipeline Integration** | Stateless chunk-by-chunk processing | Pipeline supports independent chunk processing, no cross-chunk dependencies |
| **Audio Conversion** | Reuse `wav_to_bytes()` utility | Proven implementation, handles format variations, compatible with pipeline API |
| **VAD Chunking** | Optional (none/static/vad modes), default 'none' | Flexibility for different use cases, simple default behavior |
| **Multiprocess Integration** | Lazy loading in `initialize()`, follow WhisperX pattern | Fits SDK lifecycle, proven pattern, no data loss during init |
| **Error Handling** | Graceful fallbacks with logging | GPU→CPU, VAD→static, auto-detect on invalid language |

---

## Implementation Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Memory explosion (N workers × model size)** | High | Start with single worker, document memory requirements, consider model server pattern |
| **Sample rate mismatch** | Medium | Validate 16kHz requirement, clear error messages, document need for upstream resampling |
| **Model load time (10-60s)** | Low | SDK queues messages during init, no data loss, document expected startup time |
| **GPU OOM** | Medium | Automatic CPU fallback, memory monitoring, support smaller 300M model |
| **VAD false negatives** | Low | Make VAD optional, default to 'none' mode, configurable sensitivity |

---

## Next Steps

With research complete, proceed to **Phase 1: Design & Contracts**:
1. Generate `data-model.md` - detailed entity definitions
2. Create `contracts/` - API contracts and interfaces
3. Write `quickstart.md` - user-facing getting started guide
4. Update agent context with new technology decisions
