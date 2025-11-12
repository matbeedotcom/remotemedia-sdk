# Tasks: OmniASR Streaming Transcription Node

**Input**: Design documents from `/specs/008-omniasr-streaming-node/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Architecture Context**: This implementation integrates OmniASR with the existing real-time streaming architecture featuring SileroVAD Node and SpeculativeVADGate, leveraging ultra-fast Rust runtime capabilities for minimal latency.

**Tests**: Not explicitly requested in spec - focus on implementation with manual validation.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Python nodes**: `python-client/remotemedia/nodes/`
- **Tests**: `python-client/tests/nodes/`
- **Rust nodes** (if needed): `runtime-core/src/nodes/`
- **Examples**: `examples/omniasr-transcriptions/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and dependency setup

- [x] T001 Add omnilingual-asr dependency to python-client/setup.py with version constraints
- [x] T002 [P] Add silero-vad dependency to python-client/setup.py (required for Python VAD fallback)
- [x] T003 [P] Document environment variables in python-client/README.md (FAIRSEQ2_CACHE_DIR, HF_TOKEN)
- [x] T004 Create python-client/remotemedia/nodes/omniasr/ package directory structure
- [x] T005 [P] Add pytest fixtures for OmniASR testing in python-client/tests/conftest.py

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core utilities and base implementations that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T006 Create MMSModel singleton wrapper in python-client/remotemedia/nodes/omniasr/model.py (adapts examples/omniasr-transcriptions/server/inference/mms_model_pipeline.py)
- [ ] T007 Implement lazy model loading in MMSModel.get_instance() with GPU/CPU auto-detection
- [ ] T008 Add GPU OOM exception handling in MMSModel with automatic CPU fallback
- [ ] T009 Implement audio format conversion utilities in python-client/remotemedia/nodes/omniasr/audio_utils.py (reuse wav_to_bytes from examples)
- [ ] T010 Add sample rate validation helper (enforce 16kHz) in audio_utils.py
- [ ] T011 Create base OmniASRNode class in python-client/remotemedia/nodes/omniasr/node.py extending remotemedia.core.node.Node
- [ ] T012 Implement OmniASRNode.__init__() with configuration validation (model_card, language, device, chunking_mode)
- [ ] T013 Implement OmniASRNode.initialize() with model loading and device selection logging
- [ ] T014 Implement OmniASRNode.cleanup() with model reference release and GPU cache clearing
- [ ] T015 Register OmniASRNode in python-client/remotemedia/nodes/__init__.py as "OmniASRTranscriber"

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Real-time Audio Transcription (Priority: P1) 🎯 MVP

**Goal**: Basic chunk-by-chunk transcription with word-level timestamps, integrated with Rust runtime's SileroVAD and SpeculativeVADGate for ultra-low latency

**Independent Test**: Send 10-second audio chunk (16kHz) through pipeline with SpeculativeVADGate → OmniASRNode and verify transcribed text is returned with timestamps within 2 seconds

**Real-Time Architecture Integration**:
```
Audio → SpeculativeVADGate → [Immediate Forward] → OmniASRNode
           ↑                                            ↓
      SileroVAD → [VAD Decision] → [Confirm/Cancel] → Transcription
```

### Implementation for User Story 1

- [ ] T016 [P] [US1] Implement basic process() method in OmniASRNode.process() accepting (audio_data, sample_rate) tuple
- [ ] T017 [US1] Add sample rate validation in process() (raise ValueError if != 16000Hz)
- [ ] T018 [US1] Add audio format validation in process() (check dtype, shape)
- [ ] T019 [US1] Implement synchronous _transcribe_sync() wrapper for blocking OmniASR inference
- [ ] T020 [US1] Wrap _transcribe_sync() with asyncio executor in process() for non-blocking operation
- [ ] T021 [US1] Add timeout protection (30s default) using asyncio.wait_for()
- [ ] T022 [US1] Implement TranscriptionOutput dict formatting with text, language, success fields
- [ ] T023 [US1] Add word timestamp extraction when enable_alignment=True
- [ ] T024 [US1] Add chunk metadata population (chunk_index, duration, sample_rate, device)
- [ ] T025 [US1] Implement error handling with graceful error dict return (no exceptions for transcription failures)
- [ ] T026 [US1] Add comprehensive logging (model loading, device selection, processing errors)
- [ ] T027 [US1] Create example pipeline manifest in examples/omniasr-streaming/basic_transcription.yaml with SpeculativeVADGate → OmniASRNode
- [ ] T028 [US1] Test manual validation: Run example pipeline with 10s English audio, verify transcription accuracy and latency <2s

**Checkpoint**: At this point, User Story 1 should be fully functional - basic real-time transcription with SpeculativeVADGate working end-to-end

---

## Phase 4: User Story 2 - Multilingual Speech Recognition (Priority: P2)

**Goal**: Support 200+ languages with auto-detection and explicit language specification

**Independent Test**: Process identical speech in English, Spanish, and Arabic with both auto-detect and explicit language modes, verify correct transcriptions

### Implementation for User Story 2

- [ ] T029 [P] [US2] Add language parameter support in OmniASRNode.__init__()
- [ ] T030 [P] [US2] Implement language code validation against omnilingual_asr.models.wav2vec2_llama.lang_ids.supported_langs
- [ ] T031 [US2] Pass language parameter to MMSModel.transcribe_audio() when specified
- [ ] T032 [US2] Implement auto-detection fallback when language=None
- [ ] T033 [US2] Add language detection result to TranscriptionOutput
- [ ] T034 [US2] Handle invalid language codes gracefully (log warning, fallback to auto-detect)
- [ ] T035 [US2] Create multilingual example pipeline in examples/omniasr-streaming/multilingual_transcription.yaml
- [ ] T036 [US2] Add supported languages documentation to python-client/remotemedia/nodes/omniasr/README.md
- [ ] T037 [US2] Test manual validation: Transcribe 5s clips in 3+ languages, verify accuracy

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently - basic transcription + multilingual support

---

## Phase 5: User Story 3 - VAD-based Intelligent Chunking (Priority: P2)

**Goal**: Integrate with AudioBufferAccumulator to chunk at speech boundaries using SileroVAD

**Independent Test**: Process 2-minute audio with natural pauses, verify chunks align with speech boundaries rather than arbitrary time cuts

**Architecture**:
```
Audio → SpeculativeVADGate → AudioBufferAccumulator → OmniASRNode
           ↑                         ↑
      SileroVAD ──────────────────────┘
```

### Implementation for User Story 3

- [ ] T038 [P] [US3] Create VADChunker adapter in python-client/remotemedia/nodes/omniasr/chunker.py (simplified from examples/omniasr-transcriptions/server/inference/audio_chunker.py)
- [ ] T039 [P] [US3] Implement _create_single_chunk() for "none" mode in VADChunker
- [ ] T040 [US3] Implement _chunk_static() for fixed-duration mode in VADChunker
- [ ] T041 [US3] Add chunking_mode parameter support in OmniASRNode.__init__() (none, static, vad)
- [ ] T042 [US3] Add chunk_duration parameter for static mode
- [ ] T043 [US3] Integrate VADChunker in OmniASRNode.process() when chunking_mode != "none"
- [ ] T044 [US3] Implement multi-chunk processing loop in process() (transcribe each chunk, merge results)
- [ ] T045 [US3] Add chunk_index tracking in output metadata
- [ ] T046 [US3] Document integration with AudioBufferAccumulator in examples/omniasr-streaming/vad_chunking.yaml
- [ ] T047 [US3] Create example pipeline using Rust SileroVAD + AudioBufferAccumulator + OmniASRNode
- [ ] T048 [US3] Test manual validation: Process long audio, verify chunking at speech boundaries

**Checkpoint**: All core user stories complete - basic transcription + multilingual + VAD chunking working with Rust runtime nodes

---

## Phase 6: User Story 4 - Model Selection and Configuration (Priority: P3)

**Goal**: Support multiple model sizes (1B vs 300M) with configurable quality/speed tradeoffs

**Independent Test**: Transcribe same audio with both models, measure quality difference and processing time (2-3x faster for 300M)

### Implementation for User Story 4

- [ ] T049 [P] [US4] Add model_card parameter validation in OmniASRNode.__init__() (omniASR_LLM_1B, omniASR_LLM_300M)
- [ ] T050 [US4] Implement device parameter override (cuda, cpu, None for auto-detect)
- [ ] T051 [US4] Add model selection logic in MMSModel.get_instance() based on model_card
- [ ] T052 [US4] Add GPU memory monitoring and logging in initialize()
- [ ] T053 [US4] Document model comparison in python-client/remotemedia/nodes/omniasr/README.md (accuracy, speed, memory)
- [ ] T054 [US4] Create performance benchmark example in examples/omniasr-streaming/benchmark_models.py
- [ ] T055 [US4] Test manual validation: Benchmark both models on GPU and CPU, document results

**Checkpoint**: Optimization features complete - users can choose model size based on requirements

---

## Phase 7: User Story 5 - Forced Alignment for Subtitle Generation (Priority: P3)

**Goal**: Generate precise word-level timestamps accurate to ±100ms for subtitle/karaoke use cases

**Independent Test**: Transcribe 30s audio with alignment, verify timestamps align within 100ms of actual word positions

### Implementation for User Story 5

- [ ] T056 [P] [US5] Add enable_alignment parameter to OmniASRNode.__init__()
- [ ] T057 [US5] Extract word-level timestamps from OmniASR transcription results in _transcribe_sync()
- [ ] T058 [US5] Format word timestamps as List[WordTimestamp] dicts in TranscriptionOutput
- [ ] T059 [US5] Add confidence scores to word timestamps if available from OmniASR
- [ ] T060 [US5] Create SRT subtitle generation utility in examples/omniasr-streaming/generate_subtitles.py
- [ ] T061 [US5] Create example pipeline for subtitle generation in examples/omniasr-streaming/subtitle_generation.yaml
- [ ] T062 [US5] Test manual validation: Generate SRT file, verify timing accuracy with video playback

**Checkpoint**: All user stories complete - full feature set implemented and validated

---

## Phase 8: Real-Time Architecture Integration & Optimization

**Purpose**: Deep integration with Rust runtime components for ultra-low latency

- [ ] T063 [P] Implement ControlMessage handling in OmniASRNode.process_control_message() for CancelSpeculation support
- [ ] T064 Detect CancelSpeculation messages and terminate in-progress inference
- [ ] T065 Discard partial transcription results for cancelled audio segments
- [ ] T066 [P] Add speculation acceptance metrics tracking (cancelled vs. completed transcriptions)
- [ ] T067 [P] Log P99 latency metrics using SDK metrics infrastructure
- [ ] T068 Create comprehensive real-time pipeline example in examples/omniasr-streaming/realtime_pipeline.yaml (SpeculativeVADGate + SileroVAD + AudioBufferAccumulator + OmniASRNode)
- [ ] T069 Add queue depth monitoring to prevent inference backlog
- [ ] T070 Document latency optimization strategies in examples/omniasr-streaming/PERFORMANCE.md

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, examples, and production readiness

- [ ] T071 [P] Create comprehensive API documentation in python-client/remotemedia/nodes/omniasr/README.md
- [ ] T072 [P] Add troubleshooting guide to python-client/remotemedia/nodes/omniasr/TROUBLESHOOTING.md
- [ ] T073 [P] Create example pipelines directory in examples/omniasr-streaming/ with README
- [ ] T074 [P] Add environment setup guide in examples/omniasr-streaming/SETUP.md (model cache, GPU drivers)
- [ ] T075 Document integration with Rust runtime nodes in examples/omniasr-streaming/ARCHITECTURE.md
- [ ] T076 [P] Add performance benchmarks to examples/omniasr-streaming/BENCHMARKS.md (GPU vs CPU, model sizes, latency)
- [ ] T077 Code review and refactoring pass across all OmniASR modules
- [ ] T078 Validate all examples in examples/omniasr-streaming/ directory work end-to-end
- [ ] T079 Run memory leak check (1 hour continuous transcription)
- [ ] T080 Update main project documentation in CLAUDE.md with OmniASR integration details
- [ ] T081 Create migration guide from examples/omniasr-transcriptions to production node in specs/008-omniasr-streaming-node/MIGRATION.md

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-7)**: All depend on Foundational phase completion
  - User stories CAN proceed in parallel (if staffed)
  - Or sequentially in priority order (US1 → US2 → US3 → US4 → US5)
- **Real-Time Integration (Phase 8)**: Depends on US1 completion (basic transcription working)
- **Polish (Phase 9)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Independent of US1
- **User Story 3 (P2)**: Can start after Foundational (Phase 2) - Builds on US1 but independently testable
- **User Story 4 (P3)**: Can start after Foundational (Phase 2) - Independent of other stories
- **User Story 5 (P3)**: Can start after US1 completion - Requires basic transcription working

### Within Each User Story

- Foundation tasks before story-specific implementation
- Core implementation before integration
- Story complete before moving to next priority

### Parallel Opportunities

**Phase 1 (Setup)**:
- T002, T003, T005 can all run in parallel (different files)

**Phase 2 (Foundational)**:
- After T006 complete: T007-T010 can run in parallel
- After T011 complete: T012-T014 can run in parallel

**User Story Phases**:
- US2 can start in parallel with US3, US4 (all depend only on Phase 2)
- Within each story, tasks marked [P] can run in parallel

**Phase 8 (Real-Time Integration)**:
- T063, T066, T067, T068 can run in parallel (different concerns)

**Phase 9 (Polish)**:
- T071-T076 can all run in parallel (different documentation files)

---

## Parallel Example: User Story 1

```bash
# After T016-T025 complete, these can run in parallel:
Task T026: Add logging
Task T027: Create example manifest
Task T028: Manual validation testing
```

---

## Parallel Example: Real-Time Integration

```bash
# Launch together after US1 complete:
Task T063: Implement ControlMessage handling
Task T066: Add metrics tracking
Task T067: Log P99 latency
Task T068: Create real-time pipeline example
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1 (Basic transcription with SpeculativeVADGate)
4. **STOP and VALIDATE**: Test with real audio, measure latency
5. Demo real-time transcription capability

**Estimated Effort**: 16 tasks (T001-T015 + T016-T028) = ~3-5 days

### Incremental Delivery

1. Complete Setup + Foundational → Foundation ready (T001-T015)
2. Add User Story 1 → Real-time transcription working (T016-T028) → **Demo MVP**
3. Add User Story 2 → Multilingual support (T029-T037) → Demo
4. Add User Story 3 → VAD chunking with Rust runtime (T038-T048) → Demo
5. Add User Story 4 → Model selection (T049-T055) → Demo
6. Add User Story 5 → Subtitle alignment (T056-T062) → Demo
7. Add Real-Time Integration → Ultra-low latency (T063-T070) → Demo
8. Polish → Production ready (T071-T081)

### Parallel Team Strategy

With 3 developers:

1. **Week 1**: Team completes Setup + Foundational together (T001-T015)
2. **Week 2**: Once Foundational done:
   - **Dev A**: User Story 1 (T016-T028) - Real-time transcription
   - **Dev B**: User Story 2 (T029-T037) - Multilingual support
   - **Dev C**: User Story 4 (T049-T055) - Model selection
3. **Week 3**:
   - **Dev A**: User Story 3 (T038-T048) - VAD chunking
   - **Dev B**: User Story 5 (T056-T062) - Subtitle alignment
   - **Dev C**: Real-Time Integration (T063-T070)
4. **Week 4**: Team collaborates on Polish (T071-T081)

---

## Real-Time Architecture Notes

### Key Integration Points

1. **SpeculativeVADGate** (Rust):
   - Forwards audio immediately to OmniASRNode (<1ms latency)
   - Sends cancellation messages for false positives
   - OmniASRNode must handle `ControlMessage::CancelSpeculation`

2. **SileroVAD** (Rust):
   - Runs in parallel with transcription (~19ms per chunk)
   - Provides VAD decisions to SpeculativeVADGate
   - No direct integration with OmniASRNode

3. **AudioBufferAccumulator** (Rust):
   - Accumulates audio during speech
   - Releases complete utterances on VAD silence detection
   - OmniASRNode receives complete utterances (not individual chunks)

### Performance Targets

- **P99 Latency**: <250ms end-to-end (audio input → transcription output)
- **Speculation Acceptance**: >95% (minimize cancellations)
- **Throughput**: 100+ concurrent sessions per GPU
- **Memory**: <6GB per worker (1B model), <2GB per worker (300M model)

### Cancellation Handling Strategy

```python
async def process_control_message(self, message, session_id):
    if message.message_type == "CancelSpeculation":
        # 1. Check if inference in progress for this timestamp range
        # 2. If yes, set cancellation flag
        # 3. Discard partial results
        # 4. Log cancellation event
        return True  # Handled
    return False  # Not handled
```

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Integration with Rust runtime nodes (SileroVAD, SpeculativeVADGate, AudioBufferAccumulator) is critical for real-time performance
- Focus on US1 first for MVP - basic transcription with speculative forwarding
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Measure latency at each phase to ensure <250ms P99 target

---

## Task Count Summary

- **Phase 1 (Setup)**: 5 tasks
- **Phase 2 (Foundational)**: 10 tasks (T006-T015)
- **Phase 3 (US1)**: 13 tasks (T016-T028)
- **Phase 4 (US2)**: 9 tasks (T029-T037)
- **Phase 5 (US3)**: 11 tasks (T038-T048)
- **Phase 6 (US4)**: 7 tasks (T049-T055)
- **Phase 7 (US5)**: 7 tasks (T056-T062)
- **Phase 8 (Real-Time Integration)**: 8 tasks (T063-T070)
- **Phase 9 (Polish)**: 11 tasks (T071-T081)

**Total**: 81 tasks

**MVP Scope** (Setup + Foundational + US1): 28 tasks
**Full Feature Set**: All 81 tasks

---

## Validation Checklist

✅ All tasks follow checklist format: `- [ ] [TaskID] [P?] [Story?] Description`
✅ File paths included in all implementation tasks
✅ User stories mapped from spec.md (P1-P5 priorities)
✅ Dependencies clearly documented
✅ Parallel opportunities identified with [P] markers
✅ Independent test criteria per user story
✅ MVP scope defined (User Story 1)
✅ Integration with Rust runtime architecture (SileroVAD, SpeculativeVADGate)
✅ Real-time performance targets documented
