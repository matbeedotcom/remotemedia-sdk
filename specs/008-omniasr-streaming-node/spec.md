# Feature Specification: OmniASR Streaming Transcription Node

**Feature Branch**: `008-omniasr-streaming-node`
**Created**: 2025-11-11
**Status**: Draft
**Input**: User description: "Implement an OmniASR Python Streaming Node, huggingface space code available here: @examples\omniasr-transcriptions\server\server.py"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Real-time Audio Transcription (Priority: P1)

A developer integrates the OmniASR node into their audio processing pipeline to transcribe spoken content from live audio streams. The transcription should support multiple languages and produce word-level timestamps for accurate alignment.

**Why this priority**: Core functionality that delivers immediate value - without basic transcription, the node cannot fulfill its primary purpose. This is the MVP that enables all other use cases.

**Independent Test**: Can be fully tested by sending a 10-second audio chunk through the node and verifying transcription text is returned with accurate word timestamps, delivering immediate value for any real-time transcription use case.

**Acceptance Scenarios**:

1. **Given** a streaming pipeline with an OmniASR node configured, **When** a 5-second audio chunk with clear English speech is received, **Then** the node outputs transcribed text with word-level timestamps within 2 seconds
2. **Given** an OmniASR node in a pipeline, **When** audio chunks arrive continuously over 30 seconds, **Then** each chunk is transcribed independently and outputs are streamed without buffering delays
3. **Given** a 10-second audio segment with speech in the first 3 seconds and silence afterwards, **When** processed through the node, **Then** transcription is generated only for the speech portion with accurate start/end timestamps

---

### User Story 2 - Multilingual Speech Recognition (Priority: P2)

A user needs to transcribe audio content in various languages including Arabic, Chinese, Spanish, and 200+ other languages supported by OmniASR. The system should support both automatic language detection and explicit language specification.

**Why this priority**: Critical differentiator for OmniASR compared to other ASR systems. Enables global use cases but requires P1 basic transcription to function.

**Independent Test**: Can be tested independently by processing identical speech content in different languages (e.g., same sentence in English, Spanish, Arabic) and verifying correct transcription for each language, delivering value for multilingual content creators.

**Acceptance Scenarios**:

1. **Given** an OmniASR node with language parameter set to "spa_Latn" (Spanish), **When** Spanish speech audio is processed, **Then** the transcription is accurate with language-appropriate characters
2. **Given** an OmniASR node with no language specified (auto-detection mode), **When** audio containing Arabic speech is processed, **Then** the system detects the language and produces Arabic text transcription
3. **Given** an audio stream switching between English and Mandarin Chinese, **When** processed with auto-detection, **Then** each chunk is transcribed in the detected language with language metadata included in output

---

### User Story 3 - VAD-based Intelligent Chunking (Priority: P2)

A developer working with long-form audio content needs efficient processing that respects speech boundaries. The system should use Voice Activity Detection to create optimal chunks that don't cut words mid-sentence.

**Why this priority**: Essential for production quality and user experience but depends on P1 basic transcription. Improves accuracy and reduces processing costs.

**Independent Test**: Can be tested by processing a 2-minute audio file with natural pauses between sentences and verifying chunks align with speech boundaries rather than arbitrary time intervals, delivering immediate quality improvements.

**Acceptance Scenarios**:

1. **Given** a 60-second audio file with speech segments separated by 2-second pauses, **When** processed with VAD chunking mode, **Then** chunks are created at silence boundaries, not mid-sentence
2. **Given** a continuous 30-second speech with no pauses, **When** VAD chunking is applied with 30-second target duration, **Then** a single chunk is created covering the entire segment
3. **Given** an audio file with background noise and intermittent speech, **When** VAD chunking is enabled, **Then** only segments exceeding 5 seconds minimum duration are created, filtering out noise-only segments

---

### User Story 4 - Model Selection and Configuration (Priority: P3)

Users with different performance requirements need to select appropriate model sizes. The system should support multiple OmniASR model variants (1B, 300M parameters) with configurable quality vs speed tradeoffs.

**Why this priority**: Optimization feature that enhances flexibility but not required for core functionality. Users can start with default model and optimize later.

**Independent Test**: Can be tested by processing the same audio with different model configurations and measuring transcription quality vs processing time, allowing users to choose optimal tradeoffs independently of other features.

**Acceptance Scenarios**:

1. **Given** an OmniASR node configured with the 1B parameter model, **When** audio is transcribed, **Then** higher accuracy is achieved compared to the 300M model but with 2-3x longer processing time
2. **Given** a pipeline manifest specifying the model card "omniASR_LLM_300M", **When** the node initializes, **Then** the 300M model is loaded and used for all subsequent transcriptions
3. **Given** an OmniASR node with GPU acceleration available, **When** initialized, **Then** the model is loaded on GPU and processing time is 5-10x faster than CPU mode

---

### User Story 5 - Forced Alignment for Subtitle Generation (Priority: P3)

Content creators need precise word-level timing for subtitle and caption generation. The system should provide accurate start/end timestamps for each word aligned with the audio waveform.

**Why this priority**: Advanced feature for specialized use cases (subtitles, karaoke). Valuable but not essential for basic transcription workflows.

**Independent Test**: Can be tested independently by transcribing a 30-second audio clip and verifying each word has timestamp metadata that aligns within 100ms of actual audio position, delivering immediate value for subtitle generation workflows.

**Acceptance Scenarios**:

1. **Given** a transcription result from OmniASR, **When** alignment is requested, **Then** each word has start_time and end_time fields accurate to within 100 milliseconds
2. **Given** aligned transcription output, **When** exported to SRT subtitle format, **Then** subtitle timing matches actual speech timing when played back with video
3. **Given** a multi-sentence audio chunk, **When** aligned, **Then** sentence boundaries are detected and can be used to group words into subtitle segments

---

### Edge Cases

- What happens when audio chunks are too short (< 0.5 seconds)? System should buffer until minimum duration or return empty transcription
- How does the system handle audio format mismatches (sample rate != 16kHz)? Automatic resampling should occur transparently
- What happens when GPU memory is insufficient for the model? System should fall back to CPU processing with warning logged
- How are transcription failures handled for corrupted audio chunks? Error should be logged and empty result returned without crashing pipeline
- What happens when language code is invalid or unsupported? System should fall back to auto-detection mode with warning
- How does the node behave when audio contains only music/noise with no speech? Empty transcription is returned with VAD metadata indicating no speech detected
- What happens when processing extremely long audio (> 5 minutes) in a single chunk? System should use chunking strategy to process in manageable segments

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST integrate OmniASR's Wav2Vec2InferencePipeline for multilingual speech transcription supporting 200+ languages
- **FR-002**: System MUST accept audio input as numpy arrays with accompanying sample rate metadata in the format (audio_data, sample_rate)
- **FR-003**: System MUST automatically resample input audio to 16kHz when input sample rate differs from model requirements
- **FR-004**: System MUST support three chunking modes: "none" (single chunk), "vad" (Voice Activity Detection), and "static" (fixed duration)
- **FR-005**: System MUST output transcription results as text strings with optional word-level timestamp metadata
- **FR-006**: System MUST support both automatic language detection and explicit language specification via ISO 639-3 codes with script suffixes (e.g., "eng_Latn", "ara_Arab")
- **FR-007**: System MUST load the OmniASR model as a singleton to avoid redundant memory usage when multiple instances exist
- **FR-008**: System MUST integrate with the RemoteMedia SDK's multiprocess execution framework for Python nodes
- **FR-009**: System MUST support configurable model selection between available OmniASR variants (1B, 300M parameter models)
- **FR-010**: System MUST convert audio tensors to WAV bytes format before passing to the OmniASR pipeline
- **FR-011**: System MUST detect and utilize GPU acceleration when CUDA is available, falling back to CPU processing otherwise
- **FR-012**: VAD chunking MUST respect minimum chunk duration of 5 seconds and target duration of 30 seconds
- **FR-013**: VAD chunking MUST use Silero VAD model for speech detection with configurable silence thresholds
- **FR-014**: System MUST return chunk metadata including start_time, end_time, duration, and chunk_index for each processed segment
- **FR-015**: System MUST support streaming operation where audio chunks are processed independently without maintaining session state
- **FR-016**: System MUST handle audio format conversion for mono/multi-channel inputs, converting to mono for VAD analysis
- **FR-017**: System MUST log model loading status, device selection (CPU/GPU), and processing errors for debugging
- **FR-018**: System MUST preserve audio data on its original device (CPU/GPU) during processing, only moving to CPU when required by libraries
- **FR-019**: System MUST support optional forced alignment to generate word-level timestamps for subtitle generation
- **FR-020**: System MUST expose configuration parameters through node initialization: model_card, language, chunking_mode, chunk_duration, device

### Key Entities *(include if feature involves data)*

- **OmniASRNode**: The primary streaming node component that integrates OmniASR into RemoteMedia pipelines. Contains configuration for model selection, language, and chunking mode. Inherits from RemoteMedia SDK's Node base class.

- **TranscriptionOutput**: Output data structure containing transcribed text, optional word-level timestamps, language metadata, and chunk timing information. Format compatible with RemoteMedia SDK's RuntimeData types.

- **AudioChunk**: Internal representation of segmented audio data with metadata including start_time, end_time, duration, audio_data tensor, sample_rate, and chunk_index. Used during VAD-based segmentation.

- **MMSModel**: Singleton wrapper around OmniASR's Wav2Vec2InferencePipeline managing model lifecycle, device placement, and inference execution. Ensures only one model instance exists in memory per process.

- **VADChunker**: Component responsible for intelligent audio segmentation using Silero VAD model. Detects speech boundaries, creates optimal chunks respecting target duration, and handles edge cases like short segments.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: System transcribes 10-second audio chunks with 95%+ accuracy for clear speech in supported languages
- **SC-002**: Processing latency is under 2 seconds for 5-second audio chunks when GPU acceleration is available
- **SC-003**: VAD-based chunking creates segments at natural speech boundaries (silence gaps) rather than arbitrary time cuts in 90%+ of cases
- **SC-004**: System successfully transcribes audio in at least 50 different languages from the 200+ supported by OmniASR
- **SC-005**: Node integrates seamlessly with existing RemoteMedia SDK pipelines without requiring custom executor modifications
- **SC-006**: Memory usage remains constant when processing continuous streams (no memory leaks over 1 hour of operation)
- **SC-007**: GPU memory usage does not exceed 6GB for the 1B parameter model and 2GB for the 300M model
- **SC-008**: System falls back to CPU processing within 1 second when GPU is unavailable, maintaining functionality
- **SC-009**: Word-level timestamps are accurate to within 100 milliseconds of actual audio positions when alignment is enabled
- **SC-010**: Model loading completes in under 30 seconds on GPU and under 60 seconds on CPU
- **SC-011**: System handles at least 10 concurrent transcription sessions without performance degradation when using multiprocess execution
- **SC-012**: Error rate for corrupted or invalid audio input is below 1% (system gracefully handles errors without crashing)

## Assumptions *(mandatory)*

1. **Model Availability**: OmniASR models are accessible via the omnilingual_asr Python package and can be downloaded from HuggingFace or local cache directories
2. **Compute Resources**: Target deployment environment has at least 8GB RAM for CPU operation or 6GB+ VRAM for GPU operation with the 1B model
3. **Audio Format**: Input audio is provided as numpy arrays with float32 sample values, as per RemoteMedia SDK audio node conventions
4. **Sample Rate**: While 16kHz is required by OmniASR, the node will handle resampling from common rates (8kHz, 22.05kHz, 44.1kHz, 48kHz) automatically
5. **Language Support**: The 200+ languages supported by OmniASR cover user requirements; no additional language training is needed
6. **Streaming Semantics**: The node operates in a stateless streaming mode where each audio chunk is independent (no cross-chunk context)
7. **VAD Dependencies**: Silero VAD model is available via the silero_vad Python package and is compatible with OmniASR processing requirements
8. **Device Selection**: The system can auto-detect CUDA availability and device selection is handled automatically with optional manual override
9. **Integration Pattern**: The node follows RemoteMedia SDK's existing Python node patterns (similar to transcription.py and audio.py nodes)
10. **Multiprocess Execution**: The node will be executed using RemoteMedia's multiprocess executor (not in-process CPython executor) for production use
11. **Output Format**: Transcription text output is UTF-8 encoded and compatible with RemoteMedia SDK's Text RuntimeData type
12. **Batch Size**: Default batch size of 1 is appropriate for streaming use cases; batch processing is not a priority for this implementation

## Dependencies *(mandatory)*

### External Dependencies

- **omnilingual_asr**: Python package providing OmniASR models and inference pipeline (Wav2Vec2InferencePipeline)
- **silero_vad**: Voice Activity Detection library for intelligent audio chunking
- **torch**: PyTorch framework for model execution and tensor operations
- **torchaudio**: Audio I/O and transformation utilities
- **librosa**: Audio resampling and preprocessing
- **soundfile**: Audio file reading/writing for chunk persistence
- **numpy**: Numerical array operations for audio data

### Internal Dependencies

- **RemoteMedia SDK Core**: Node base class, RuntimeData types, and pipeline framework
- **RemoteMedia SDK Audio Nodes**: AudioResampleNode, AudioBuffer for preprocessing
- **Multiprocess Executor**: IPC framework for Python node execution in isolated processes
- **Python FFI Layer**: Data marshalling between Rust runtime and Python nodes

### System Dependencies

- **CUDA Toolkit** (optional): For GPU acceleration, CUDA 11.8+ or 12.x compatible with PyTorch
- **FFmpeg** (optional): For handling diverse audio format conversions if needed upstream

## Risks and Mitigations *(mandatory)*

### Risk 1: Model Download and Caching Failures

**Risk**: OmniASR models may fail to download from HuggingFace due to network issues or cache directory permissions, preventing node initialization.

**Impact**: High - Node cannot function without model access

**Mitigation**:
- Implement retry logic with exponential backoff for model downloads
- Support offline mode with local model cache directories via FAIRSEQ2_CACHE_DIR environment variable
- Provide clear error messages with instructions for manual model download
- Include model verification step during installation/setup

### Risk 2: GPU Memory Exhaustion

**Risk**: Large models (1B parameters) may exceed available GPU memory, especially with concurrent sessions or when other GPU processes are running.

**Impact**: Medium - Causes runtime errors and potential pipeline crashes

**Mitigation**:
- Implement automatic fallback to CPU when GPU memory allocation fails
- Support model selection (300M vs 1B) for memory-constrained environments
- Add GPU memory monitoring and warning logs when approaching limits
- Document GPU memory requirements clearly in deployment guide

### Risk 3: Audio Format Compatibility Issues

**Risk**: Input audio may arrive in unexpected formats (sample rates, bit depths, channel counts) causing processing failures or quality degradation.

**Impact**: Medium - Results in transcription errors or degraded accuracy

**Mitigation**:
- Implement robust audio format detection and automatic conversion
- Add validation layer to verify audio tensor shapes and data types
- Support common sample rates with automatic resampling to 16kHz
- Provide clear error messages for unsupported formats with conversion suggestions

### Risk 4: Latency Requirements for Real-time Use Cases

**Risk**: Transcription latency may exceed acceptable thresholds for real-time applications (e.g., live captioning) especially on CPU.

**Impact**: Medium - Limits applicability for time-sensitive use cases

**Mitigation**:
- Document expected latency benchmarks for different hardware configurations
- Recommend GPU acceleration for real-time use cases
- Support configurable chunk sizes to balance latency vs accuracy
- Consider implementing parallel processing for multiple chunks when feasible

### Risk 5: Language Detection Accuracy

**Risk**: Automatic language detection may misidentify languages, especially for short audio segments or mixed-language content.

**Impact**: Low-Medium - Results in incorrect transcriptions but doesn't break functionality

**Mitigation**:
- Provide option to explicitly specify language when known
- Document limitations of auto-detection for short segments (< 3 seconds)
- Return language detection confidence scores in output metadata
- Support language hints or priors when available from context

### Risk 6: VAD False Negatives

**Risk**: Voice Activity Detection may incorrectly filter out quiet speech or speech with heavy accents, leading to missing transcriptions.

**Impact**: Medium - Loss of content in transcription output

**Mitigation**:
- Make VAD chunking optional with fallback to static chunking
- Provide configurable VAD sensitivity thresholds
- Include VAD metadata in output for debugging (speech_ratio, energy levels)
- Document recommended settings for different audio quality scenarios
- Default to "none" chunking mode for most reliable (though less optimized) results

## Out of Scope *(mandatory)*

The following are explicitly **not** included in this specification:

1. **Speaker Diarization**: Identifying and separating multiple speakers in audio is not supported. Output is a single transcription stream without speaker labels.

2. **Punctuation and Capitalization**: While OmniASR models may provide some punctuation, automatic punctuation enhancement or restoration is not guaranteed or optimized.

3. **Translation**: The node performs transcription only. Translation between languages is out of scope (would require separate translation node).

4. **Custom Model Training**: Users cannot train or fine-tune OmniASR models within this node. Only pre-trained models from the omnilingual_asr package are supported.

5. **Audio Source Recording**: The node does not handle audio capture from microphones or other input devices. Audio must be provided by upstream nodes or sources.

6. **Real-time Streaming Protocol Support**: The node processes discrete audio chunks. Integration with streaming protocols (WebRTC, RTSP, etc.) is handled by other pipeline components.

7. **Confidence Scores**: While the underlying model may produce confidence scores, exposing and calibrating these scores is not included in initial implementation.

8. **Noise Reduction/Enhancement**: Audio quality improvement or noise filtering should be handled by dedicated preprocessing nodes, not within the transcription node.

9. **Batch File Processing**: The node is designed for streaming pipelines. Batch processing of audio files is handled by pipeline orchestration, not node-level functionality.

10. **Custom Vocabulary/Domain Adaptation**: Users cannot provide custom vocabularies or domain-specific language models. Only the base OmniASR models are supported.
