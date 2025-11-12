# Implementation Plan: OmniASR Streaming Transcription Node

**Branch**: `008-omniasr-streaming-node` | **Date**: 2025-11-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/008-omniasr-streaming-node/spec.md`

## Summary

Integrate OmniASR multilingual speech recognition (200+ languages) into the RemoteMedia SDK as a Python streaming node. The node will accept audio chunks from pipelines, perform automatic transcription using the Wav2Vec2InferencePipeline, and output transcribed text with optional word-level timestamps. Key features include Voice Activity Detection for intelligent chunking, automatic sample rate conversion to 16kHz, GPU/CPU auto-detection, and singleton model loading for memory efficiency. The implementation follows RemoteMedia SDK's existing Python node patterns and integrates with the multiprocess executor for isolated process execution.

## Technical Context

**Language/Version**: Python 3.10+ (matching RemoteMedia SDK requirements)
**Primary Dependencies**:
- omnilingual_asr (OmniASR inference pipeline)
- torch >= 2.0.0 (PyTorch for model execution)
- silero_vad (Voice Activity Detection)
- librosa (audio resampling)
- soundfile (audio I/O)
- numpy (array operations)
- torchaudio (audio utilities)

**Storage**: N/A (stateless streaming node, model cache in FAIRSEQ2_CACHE_DIR)
**Testing**: pytest (RemoteMedia SDK standard), integration tests with multiprocess executor
**Target Platform**: Linux/Windows with Python multiprocess execution (iceoryx2 IPC)
**Project Type**: Python library extension to RemoteMedia SDK
**Performance Goals**:
- <2 second latency for 5-second audio chunks (GPU)
- <5 second latency for 5-second audio chunks (CPU)
- Support 10+ concurrent sessions via multiprocess isolation

**Constraints**:
- GPU memory: 6GB for 1B model, 2GB for 300M model
- Minimum audio chunk: 0.5 seconds (buffer shorter segments)
- Sample rate: Must convert all inputs to 16kHz for OmniASR

**Scale/Scope**:
- Single Python module (~500-800 lines)
- Integration with existing RemoteMedia SDK node base classes
- Reuse existing audio chunking components from examples/omniasr-transcriptions

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Status**: No project-specific constitution defined. Using RemoteMedia SDK architectural principles from CLAUDE.md:

### Architecture Compliance

✅ **Follows SDK Node Pattern**: Inherits from RemoteMedia Node base class
✅ **Multiprocess Execution**: Designed for multiprocess executor (not in-process CPython)
✅ **Stateless Streaming**: Each audio chunk processed independently
✅ **Zero-Copy Where Possible**: Uses numpy arrays directly, GPU tensors preserved
✅ **Error Handling**: Graceful fallbacks (GPU→CPU, VAD→static chunking)

### Integration Points

✅ **IPC Compatible**: Works with iceoryx2 zero-copy shared memory transport
✅ **RuntimeData Format**: Outputs conform to RemoteMedia SDK data types
✅ **Singleton Pattern**: Model loading uses singleton to avoid redundant memory usage
✅ **Logging Standards**: Uses Python logging module with appropriate levels

**No Constitution Violations Detected**

## Project Structure

### Documentation (this feature)

```text
specs/008-omniasr-streaming-node/
├── spec.md              # Feature specification (completed)
├── plan.md              # This file (in progress)
├── research.md          # Phase 0 output (to be generated)
├── data-model.md        # Phase 1 output (to be generated)
├── quickstart.md        # Phase 1 output (to be generated)
├── contracts/           # Phase 1 output (to be generated)
│   └── node-interface.md
└── tasks.md             # Phase 2 output (not created by /speckit.plan)
```

### Source Code (repository root)

```text
python-client/remotemedia/nodes/
├── omniasr.py                    # NEW: Main OmniASRNode implementation
├── omniasr_model.py              # NEW: Singleton MMSModel wrapper
├── omniasr_chunker.py            # NEW: VAD-based audio chunking (adapted from examples)
└── __init__.py                   # MODIFIED: Register new node

python-client/remotemedia/
└── core/
    └── node.py                   # EXISTING: Base Node class (reference)

examples/omniasr-transcriptions/
└── server/
    ├── inference/
    │   ├── mms_model_pipeline.py     # REFERENCE: Model loading patterns
    │   ├── audio_chunker.py          # REFERENCE: VAD chunking logic
    │   └── audio_reading_tools.py    # REFERENCE: Audio conversion utilities
    └── server.py                      # REFERENCE: Integration patterns

tests/
└── python-client/
    └── nodes/
        ├── test_omniasr.py           # NEW: Unit tests for OmniASRNode
        ├── test_omniasr_model.py     # NEW: Model loading tests
        └── test_omniasr_chunker.py   # NEW: Chunking tests
```

**Structure Decision**: Single project extension to existing RemoteMedia SDK Python client. The implementation adds three new modules to the `remotemedia/nodes/` package following the established pattern of existing nodes (e.g., `audio.py`, `transcription.py`). We'll adapt proven code from `examples/omniasr-transcriptions/server/inference/` rather than starting from scratch, ensuring compatibility with the OmniASR library usage patterns.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No constitution violations detected. Implementation follows existing SDK patterns.

---

## Phase 0: Research & Discovery

**Objective**: Resolve unknowns from Technical Context and establish design decisions.

### Research Tasks

#### R1: OmniASR Integration Patterns
**Question**: How should we integrate the Wav2Vec2InferencePipeline for streaming use cases?
**Current Understanding**: Examples show batch processing with file uploads
**Investigation Needed**:
- Can pipeline handle chunk-by-chunk streaming?
- Memory implications of persistent model in multiprocess execution
- Thread safety of pipeline methods

#### R2: Audio Format Conversion Strategy
**Question**: What's the most efficient way to handle audio resampling and format conversion?
**Current Understanding**: Examples use librosa and custom wav_to_bytes
**Investigation Needed**:
- Performance comparison: librosa vs torchaudio resampling
- Zero-copy conversion paths from numpy→torch→bytes
- Impact of format conversions on latency budget

#### R3: VAD Chunking Adaptation
**Question**: How to adapt existing VAD chunker for streaming pipeline use?
**Current Understanding**: Examples have AudioChunker with three modes (none, vad, static)
**Investigation Needed**:
- Stateful vs stateless chunking for streaming
- Handling incomplete chunks at stream boundaries
- VAD model lifecycle in multiprocess executor

#### R4: Multiprocess Executor Integration
**Question**: What's required for reliable multiprocess execution?
**Current Understanding**: CLAUDE.md documents IPC thread architecture
**Investigation Needed**:
- Node initialization sequence in multiprocess context
- Model loading timing (eager vs lazy)
- Session cleanup and resource management

#### R5: Error Handling and Fallbacks
**Question**: What failure modes need graceful degradation?
**Investigation Needed**:
- GPU OOM → CPU fallback mechanism
- VAD model loading failure → static chunking
- Invalid audio format → error vs auto-conversion
- Language detection failure → auto-detect fallback

**Output**: `research.md` documenting decisions, rationales, and implementation approaches.

---

## Phase 1: Design & Contracts

**Prerequisites**: Research phase complete

### Data Model (data-model.md)

**Entities**:

1. **OmniASRNode** (Main node class)
   - Configuration: model_card, language, chunking_mode, chunk_duration, device
   - State: _model_instance (singleton), _chunker_instance
   - Lifecycle: initialize() → process() → cleanup()

2. **MMSModel** (Singleton wrapper)
   - Configuration: model_card, device
   - State: _pipeline, _initialized
   - Methods: transcribe_audio(), get_instance()

3. **VADChunker** (Audio segmentation)
   - Configuration: mode, chunk_duration, min_duration, target_duration
   - State: _vad_model (singleton)
   - Methods: chunk_audio(), _chunk_with_vad(), _chunk_static()

4. **TranscriptionOutput** (Output format)
   - Fields: text, language, word_timestamps[], chunk_metadata
   - Format: Compatible with RemoteMedia SDK Text RuntimeData

### API Contracts (contracts/)

**Node Interface Contract**:
```python
class OmniASRNode(Node):
    def __init__(
        self,
        model_card: str = "omniASR_LLM_1B",
        language: Optional[str] = None,
        chunking_mode: str = "none",
        chunk_duration: float = 30.0,
        device: Optional[str] = None,
        **kwargs
    )

    async def initialize(self) -> None

    async def process(self, data: Tuple[np.ndarray, int]) -> Dict[str, Any]

    async def cleanup(self) -> None
```

**Input/Output Contract**:
- Input: `(audio_data: np.ndarray, sample_rate: int)` tuple
- Output: `{"text": str, "language": str, "word_timestamps": List[Dict], "chunk_metadata": Dict}`

### Quickstart (quickstart.md)

**User Journey**: Developer adds OmniASR transcription to an existing audio pipeline

**Key Steps**:
1. Install omnilingual_asr dependency
2. Add OmniASRNode to pipeline manifest
3. Configure language/model parameters
4. Run pipeline with audio input
5. Consume transcription outputs

---

## Phase 2: Task Breakdown

**Note**: Task generation is handled by `/speckit.tasks` command, NOT by `/speckit.plan`.

The implementation will be broken into dependency-ordered tasks covering:
- Task 1: Adapt MMSModel singleton from examples
- Task 2: Implement audio format conversion utilities
- Task 3: Adapt VADChunker for streaming use
- Task 4: Implement OmniASRNode with multiprocess support
- Task 5: Add node registration and __init__ updates
- Task 6: Write unit tests for all components
- Task 7: Write integration tests with multiprocess executor
- Task 8: Update documentation and examples

**These tasks will be generated in `tasks.md` by the `/speckit.tasks` command.**

---

## Implementation Notes

### Key Design Decisions

1. **Singleton Pattern**: Both MMSModel and VADChunker use singletons to avoid redundant memory usage when multiple node instances exist in the same process.

2. **Stateless Processing**: Each audio chunk is processed independently without cross-chunk state. This simplifies multiprocess execution but means no context across chunks.

3. **Lazy Model Loading**: Model is loaded on first process() call, not during __init__, to support multiprocess initialization patterns.

4. **Automatic Fallbacks**:
   - GPU unavailable → CPU
   - VAD model fails → static chunking
   - Invalid language → auto-detection

5. **Code Reuse**: Maximum reuse of proven code from examples/omniasr-transcriptions rather than reimplementation.

### Integration Points

1. **Node Base Class**: Inherits from `remotemedia.core.node.Node`
2. **RuntimeData Types**: Uses SDK's data type conventions for I/O
3. **Multiprocess Executor**: Compatible with iceoryx2 IPC transport
4. **Logging**: Uses Python logging module with SDK conventions

### Testing Strategy

1. **Unit Tests**: Mock OmniASR pipeline, test node logic in isolation
2. **Integration Tests**: Real model loading, actual audio transcription
3. **Multiprocess Tests**: Verify IPC communication and session cleanup
4. **Performance Tests**: Latency benchmarks for GPU/CPU modes

### Risks and Mitigations

1. **Model Download Failures**: Document offline setup, provide cache verification
2. **GPU Memory Issues**: Implement OOM detection and CPU fallback
3. **Latency Variability**: Profile and document expected performance ranges
4. **VAD False Negatives**: Make chunking mode configurable, default to "none" for reliability
