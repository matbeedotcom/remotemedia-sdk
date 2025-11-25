# Tasks: Video Codec Support (AV1/VP8/AVC)

**Input**: Design documents from `/specs/012-video-codec-support/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/video_nodes.md, quickstart.md

**Tests**: Integration and unit tests are included as specified in the feature requirements.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

- Rust library monorepo structure
- Core: `runtime-core/src/`
- Transports: `transports/remotemedia-grpc/src/`, `transports/remotemedia-webrtc/src/`
- Python: `python-client/remotemedia/`
- Tests: `tests/integration/`, `runtime-core/benches/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and dependency setup

- [X] T001 Add ac-ffmpeg 0.19 dependency to runtime-core/Cargo.toml with video feature
- [X] T002 [P] Add rav1e 0.8 (optional) and dav1d-rs 0.10 (optional) dependencies to runtime-core/Cargo.toml
- [X] T003 [P] Update transports/remotemedia-grpc/Cargo.toml with protobuf dependencies for video
- [X] T004 [P] Update transports/remotemedia-webrtc/Cargo.toml to enable media-codec-vpx and openh264 features
- [X] T005 Create runtime-core/src/data/video.rs file (empty module, will be populated in foundational phase)
- [X] T006 Create runtime-core/src/nodes/video/ directory structure (mod.rs, encoder.rs, decoder.rs, scaler.rs, format_converter.rs, codec.rs)
- [X] T007 [P] Create tests/video_samples/ directory and add placeholder test video files (sample_720p30.raw, sample_vp8.webm)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core data types and infrastructure that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Core Data Types

- [X] T008 [P] Implement PixelFormat enum in runtime-core/src/data/video.rs with all variants (Unspecified, Yuv420p, I420, NV12, Rgb24, Rgba32, Encoded)
- [X] T009 [P] Implement VideoCodec enum in runtime-core/src/data/video.rs (Vp8, H264, Av1) with mime_type() and rtp_payload_type() methods
- [X] T010 [P] Implement VideoEncoderConfig struct in runtime-core/src/nodes/video/encoder.rs with Default impl
- [X] T011 [P] Implement VideoDecoderConfig struct in runtime-core/src/nodes/video/decoder.rs with Default impl
- [X] T012 Extend RuntimeData enum in runtime-core/src/lib.rs with Video variant (pixel_data, width, height, format, codec, frame_number, timestamp_us, is_keyframe)
- [X] T013 Implement RuntimeData::validate_video_frame() method in runtime-core/src/lib.rs
- [X] T014 [P] Implement PixelFormat::buffer_size() and requires_even_dimensions() helper methods in runtime-core/src/data/video.rs
- [X] T015 Export video module from runtime-core/src/data/mod.rs and runtime-core/src/lib.rs

### FFmpeg Backend Abstraction

- [X] T016 Create VideoEncoderBackend trait in runtime-core/src/nodes/video/codec.rs for ac-ffmpeg integration
- [X] T017 Create VideoDecoderBackend trait in runtime-core/src/nodes/video/codec.rs
- [X] T018 Implement FFmpegEncoder (ac-ffmpeg wrapper) in runtime-core/src/nodes/video/codec.rs
- [X] T019 Implement FFmpegDecoder (ac-ffmpeg wrapper) in runtime-core/src/nodes/video/codec.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Video Streaming Through Pipeline (Priority: P1) 🎯 MVP

**Goal**: Enable basic video encoding, decoding, and transmission via gRPC. Developers can stream raw video frames through the pipeline, encode to VP8, transmit via gRPC, and decode back to raw frames.

**Independent Test**: Create a pipeline that accepts raw 720p YUV420P frames, encodes to VP8, transmits via gRPC bidirectional stream, decodes, and verifies output matches input (within codec compression tolerance).

### Core Encoding/Decoding Nodes (VP8 Only)

- [X] T020 [P] [US1] Implement VideoEncoderNode struct in runtime-core/src/nodes/video/encoder.rs with Arc<Mutex<VideoEncoderBackend>>
- [X] T021 [P] [US1] Implement VideoDecoderNode struct in runtime-core/src/nodes/video/decoder.rs with Arc<Mutex<VideoDecoderBackend>>
- [X] T022 [US1] Implement VideoEncoderNode::new() in runtime-core/src/nodes/video/encoder.rs (initialize FFmpeg VP8 encoder)
- [X] T023 [US1] Implement VideoDecoderNode::new() in runtime-core/src/nodes/video/decoder.rs (initialize FFmpeg VP8 decoder)
- [X] T024 [US1] Implement VideoEncoderNode::encode_frame() in runtime-core/src/nodes/video/encoder.rs using tokio::spawn_blocking
- [X] T025 [US1] Implement VideoDecoderNode::decode_frame() in runtime-core/src/nodes/video/decoder.rs using tokio::spawn_blocking
- [X] T026 [US1] Implement StreamingNode trait for VideoEncoderNode in runtime-core/src/nodes/video/encoder.rs (process_streaming, configure, shutdown)
- [X] T027 [US1] Implement StreamingNode trait for VideoDecoderNode in runtime-core/src/nodes/video/decoder.rs (process_streaming with error resilience)

### gRPC Transport Integration

- [X] T028 [US1] Extend protobuf schema in transports/remotemedia-grpc/protos/common.proto with VideoFrame message type
- [X] T029 [US1] Regenerate protobuf code: tonic_build for remotemedia.proto in transports/remotemedia-grpc/build.rs
- [X] T030 [P] [US1] Implement RuntimeData::Video → proto::VideoFrame conversion in transports/remotemedia-grpc/src/adapters.rs
- [X] T031 [P] [US1] Implement proto::VideoFrame → RuntimeData::Video conversion in transports/remotemedia-grpc/src/adapters.rs
- [X] T032 [US1] Modify gRPC StreamPipeline handler in transports/remotemedia-grpc/src/streaming.rs to handle video frames
- [X] T033 [US1] Add video frame routing logic to session_router in transports/remotemedia-grpc/src/session_router.rs

### IPC Serialization (Multiprocess Support)

- [X] T034 [US1] Extend IPC serialization format in runtime-core/src/python/multiprocess/data_transfer.rs with video type (0x03)
- [X] T035 [US1] Implement serialize_video_frame() in runtime-core/src/python/multiprocess/data_transfer.rs (binary format per data-model.md)
- [X] T036 [US1] Implement deserialize_video_frame() in runtime-core/src/python/multiprocess/data_transfer.rs
- [X] T037 [P] [US1] Create VideoFrame Python class in python-client/remotemedia/data/video.py with IPC deserialization
- [X] T038 [US1] Modify iceoryx2 channel handling in runtime-core/src/python/multiprocess/multiprocess_executor.rs to support video frames

### Testing & Validation (User Story 1)

- [X] T039 [P] [US1] Create unit test for VideoEncoderNode (VP8) in runtime-core/src/nodes/video/encoder_tests.rs (encode 720p frame, verify bitstream)
- [X] T040 [P] [US1] Create unit test for VideoDecoderNode (VP8) in runtime-core/src/nodes/video/decoder_tests.rs (decode sample VP8, verify dimensions)
- [X] T041 [P] [US1] Create unit test for RuntimeData::Video validation in runtime-core/src/lib.rs
- [ ] T042 [US1] Create integration test for gRPC video streaming in tests/integration/test_video_grpc.rs (end-to-end: raw → encode → gRPC → decode → verify)
- [ ] T043 [P] [US1] Create benchmark for VP8 encoding in runtime-core/benches/video_encode_decode.rs (measure latency @ 720p30)

### Node Registration

- [X] T044 [US1] Register VideoEncoderNode in runtime-core/src/nodes/registry.rs or runtime-core/src/nodes/streaming_registry.rs
- [X] T045 [US1] Register VideoDecoderNode in runtime-core/src/nodes/registry.rs or runtime-core/src/nodes/streaming_registry.rs

**Checkpoint**: At this point, User Story 1 should be fully functional - VP8 encoding/decoding works via gRPC with IPC support

---

## Phase 4: User Story 2 - Multi-Codec Support (Priority: P2)

**Goal**: Support AV1 and H.264 codecs in addition to VP8. Developers can choose codec based on quality, bandwidth, or compatibility requirements.

**Independent Test**: Configure three pipelines (one per codec), encode the same test video with VP8/AV1/H.264, verify all produce valid output. Use standard decoders (ffmpeg CLI) to validate bitstreams.

### AV1 Codec Support

- [X] T046 [P] [US2] Implement AV1 encoder support in runtime-core/src/nodes/video/codec.rs (FFmpeg libaom backend)
- [X] T047 [P] [US2] Implement AV1 decoder support in runtime-core/src/nodes/video/codec.rs (dav1d-rs bindings)
- [ ] T048 [P] [US2] Add optional rav1e pure Rust encoder in runtime-core/src/nodes/video/codec.rs (feature-gated)
- [X] T049 [US2] Modify VideoEncoderNode::new() to support AV1 codec selection in runtime-core/src/nodes/video/encoder.rs
- [X] T050 [US2] Modify VideoDecoderNode::new() to support AV1 codec detection in runtime-core/src/nodes/video/decoder.rs

### H.264 Codec Support

- [X] T051 [P] [US2] Implement H.264 encoder support in runtime-core/src/nodes/video/codec.rs (FFmpeg libx264 backend)
- [X] T052 [P] [US2] Implement H.264 decoder support in runtime-core/src/nodes/video/codec.rs (FFmpeg decoder)
- [ ] T053 [P] [US2] Add optional openh264 encoder in runtime-core/src/nodes/video/codec.rs for WebRTC compatibility
- [ ] T054 [US2] Modify VideoEncoderNode to support H.264 baseline/main profile configuration in runtime-core/src/nodes/video/encoder.rs
- [ ] T055 [US2] Modify VideoDecoderNode to handle H.264 NAL units in runtime-core/src/nodes/video/decoder.rs

### Codec Selection & Configuration

- [X] T056 [US2] Add codec auto-detection from bitstream in VideoDecoderNode in runtime-core/src/nodes/video/decoder.rs
- [ ] T057 [US2] Implement codec-specific quality presets (VP8: "good"/"best", H.264: "fast"/"medium", AV1: speed 0-10) in runtime-core/src/nodes/video/encoder.rs
- [X] T058 [US2] Add keyframe_interval enforcement across all codecs in runtime-core/src/nodes/video/encoder.rs

### Testing & Validation (User Story 2)

- [X] T059 [P] [US2] Create unit test for AV1 encoder/decoder in runtime-core/src/nodes/video/encoder.rs and decoder.rs
- [X] T060 [P] [US2] Create unit test for H.264 encoder/decoder in runtime-core/src/nodes/video/encoder.rs and decoder.rs
- [ ] T061 [US2] Create codec comparison benchmark in runtime-core/benches/video_encode_decode.rs (VP8 vs AV1 vs H.264 quality/speed)
- [ ] T062 [US2] Create integration test for multi-codec gRPC pipeline in tests/integration/test_video_grpc.rs (encode with each codec, verify interop)
- [ ] T063 [P] [US2] Validate bitstream compatibility with standard decoders (ffmpeg CLI test in tests/integration/)

**Checkpoint**: At this point, User Stories 1 AND 2 should both work - all three codecs (VP8, AV1, H.264) are functional

---

## Phase 5: User Story 3 - WebRTC Video Integration (Priority: P2)

**Goal**: Enable real-time video streaming via WebRTC transport. Developers can send/receive video alongside audio in WebRTC peer connections.

**Independent Test**: Establish WebRTC peer connection, send video frames via RTP, verify remote peer receives and decodes frames correctly. Test audio/video synchronization.

### WebRTC Media Engine Integration

- [ ] T064 [US3] Register VP8 video codec in webrtc-rs MediaEngine in transports/remotemedia-webrtc/src/media/video.rs
- [ ] T065 [P] [US3] Register H.264 video codec in webrtc-rs MediaEngine in transports/remotemedia-webrtc/src/media/video.rs
- [ ] T066 [P] [US3] Register AV1 video codec (if supported by webrtc-rs) in transports/remotemedia-webrtc/src/media/video.rs
- [ ] T067 [US3] Implement send_video_frame() for WebRTC transport in transports/remotemedia-webrtc/src/media/video.rs (RTP packetization)
- [ ] T068 [US3] Implement receive_video_frame() for WebRTC transport in transports/remotemedia-webrtc/src/media/video.rs (RTP depacketization)

### Video Track Management

- [ ] T069 [US3] Create TrackLocalStaticRTP for video in transports/remotemedia-webrtc/src/media/tracks.rs
- [ ] T070 [US3] Implement video track addition to peer connection in transports/remotemedia-webrtc/src/peer/connection.rs
- [ ] T071 [US3] Handle video transceiver setup in SDP offer/answer in transports/remotemedia-webrtc/src/peer/server_peer.rs
- [ ] T072 [US3] Implement video stream routing in session router in transports/remotemedia-webrtc/src/session/router.rs

### RTP Packetization & Quality Control

- [ ] T073 [US3] Implement VP8 RTP payload formatting (RFC 7741) in transports/remotemedia-webrtc/src/media/video.rs
- [ ] T074 [P] [US3] Implement H.264 RTP payload formatting (RFC 6184) in transports/remotemedia-webrtc/src/media/video.rs
- [ ] T075 [US3] Add keyframe request handling (PLI/FIR) in transports/remotemedia-webrtc/src/media/video.rs
- [ ] T076 [US3] Implement adaptive bitrate control (REMB/TWCC feedback) in transports/remotemedia-webrtc/src/peer/connection.rs

### Audio/Video Synchronization

- [ ] T077 [US3] Implement timestamp synchronization between audio/video tracks in transports/remotemedia-webrtc/src/session/session.rs
- [ ] T078 [US3] Add RTP timestamp mapping (90kHz clock for video) in transports/remotemedia-webrtc/src/media/video.rs
- [ ] T079 [US3] Handle clock drift compensation in transports/remotemedia-webrtc/src/session/router.rs

### Testing & Validation (User Story 3)

- [ ] T080 [P] [US3] Create integration test for WebRTC video streaming in tests/integration/test_video_webrtc.rs (local peer → remote peer VP8)
- [ ] T081 [P] [US3] Create integration test for audio/video sync in tests/integration/test_video_webrtc.rs (send both, verify timestamps)
- [ ] T082 [US3] Create WebRTC keyframe request test in tests/integration/test_video_webrtc.rs (simulate packet loss, verify recovery)
- [ ] T083 [US3] Create adaptive bitrate test in tests/integration/test_video_webrtc.rs (simulate network congestion, verify bitrate adjustment)

**Checkpoint**: At this point, User Stories 1, 2, AND 3 should all work - WebRTC video streaming is functional with multi-codec support

---

## Phase 6: User Story 4 - Video Processing Pipeline Integration (Priority: P3)

**Goal**: Enable video transformations (scaling, format conversion) within the pipeline. Developers can preprocess video before encoding or postprocess after decoding.

**Independent Test**: Create pipeline with scaler (1080p→720p) and format converter (RGB→YUV), verify transformations produce correct output dimensions and formats.

### Video Scaling Node

- [ ] T084 [P] [US4] Implement VideoScalerConfig struct in runtime-core/src/nodes/video/scaler.rs (target_width, target_height, algorithm, maintain_aspect_ratio)
- [ ] T085 [P] [US4] Create VideoScalerBackend trait in runtime-core/src/nodes/video/scaler.rs (abstraction for FFmpeg swscale or pure Rust)
- [ ] T086 [US4] Implement FFmpegScaler using swscale in runtime-core/src/nodes/video/scaler.rs (bilinear, bicubic, lanczos algorithms)
- [ ] T087 [US4] Implement VideoScalerNode struct in runtime-core/src/nodes/video/scaler.rs with StreamingNode trait
- [ ] T088 [US4] Add aspect ratio calculation logic in runtime-core/src/nodes/video/scaler.rs
- [ ] T089 [US4] Implement even dimension rounding for YUV formats in runtime-core/src/nodes/video/scaler.rs

### Pixel Format Conversion Node

- [ ] T090 [P] [US4] Implement VideoFormatConverterConfig struct in runtime-core/src/nodes/video/format_converter.rs (target_format, color_matrix, color_range)
- [ ] T091 [US4] Implement RGB24 ↔ YUV420P conversion in runtime-core/src/nodes/video/format_converter.rs (using FFmpeg swscale or colorspace math)
- [ ] T092 [P] [US4] Implement RGB24 ↔ RGBA32 conversion in runtime-core/src/nodes/video/format_converter.rs (alpha channel handling)
- [ ] T093 [P] [US4] Implement YUV420P ↔ NV12 conversion in runtime-core/src/nodes/video/format_converter.rs (planar ↔ semi-planar)
- [ ] T094 [US4] Implement VideoFormatConverterNode struct with StreamingNode trait in runtime-core/src/nodes/video/format_converter.rs
- [ ] T095 [US4] Add color matrix selection (BT.601, BT.709, BT.2020) in runtime-core/src/nodes/video/format_converter.rs

### Zero-Copy Optimizations

- [ ] T096 [US4] Optimize video frame transfer for iceoryx2 shared memory in runtime-core/src/python/multiprocess/data_transfer.rs
- [ ] T097 [US4] Implement zero-copy numpy array marshaling for Python video nodes in transports/remotemedia-ffi/src/numpy_bridge.rs (if exists)
- [ ] T098 [US4] Add mmap support for large video frames (>10MB) in runtime-core/src/nodes/video/scaler.rs

### Node Registration & Integration

- [ ] T099 [US4] Register VideoScalerNode in runtime-core/src/nodes/registry.rs or streaming_registry.rs
- [ ] T100 [US4] Register VideoFormatConverterNode in runtime-core/src/nodes/registry.rs or streaming_registry.rs
- [ ] T101 [US4] Create Python wrapper VideoProcessorNode in python-client/remotemedia/nodes/video_processor.py

### Testing & Validation (User Story 4)

- [ ] T102 [P] [US4] Create unit test for VideoScalerNode in runtime-core/src/nodes/video/scaler.rs (1080p→720p, verify dimensions)
- [ ] T103 [P] [US4] Create unit test for VideoFormatConverterNode in runtime-core/src/nodes/video/format_converter.rs (RGB→YUV→RGB roundtrip)
- [ ] T104 [US4] Create integration test for video processing pipeline in tests/integration/test_video_processing.rs (raw → scale → convert → encode → decode)
- [ ] T105 [P] [US4] Create zero-copy IPC test in tests/integration/test_video_ipc.rs (multiprocess Python node, verify memoryview usage)
- [ ] T106 [US4] Create performance benchmark for scaling/conversion in runtime-core/benches/video_processing.rs (latency targets)

**Checkpoint**: All user stories should now be independently functional - complete video processing pipeline with transformations

---

## Phase 7: Hardware Acceleration & Performance

**Purpose**: Optimize video encoding/decoding with hardware acceleration

- [ ] T107 [P] Implement VAAPI hardware encoding (Linux) in runtime-core/src/nodes/video/codec.rs
- [ ] T108 [P] Implement VideoToolbox hardware encoding (macOS) in runtime-core/src/nodes/video/codec.rs
- [ ] T109 [P] Implement NVENC hardware encoding (NVIDIA GPUs) in runtime-core/src/nodes/video/codec.rs (optional)
- [ ] T110 Add hardware acceleration auto-detection in runtime-core/src/nodes/video/encoder.rs
- [ ] T111 Implement graceful fallback to software encoding in runtime-core/src/nodes/video/encoder.rs
- [ ] T112 [P] Create hardware acceleration benchmark in runtime-core/benches/video_hw_accel.rs (compare HW vs SW latency)
- [ ] T113 Add hardware encoder config validation in runtime-core/src/nodes/video/encoder.rs

---

## Phase 8: Python Bindings & Examples

**Purpose**: Python developer experience and examples

- [ ] T114 [P] Implement VideoEncoderNode Python wrapper in python-client/remotemedia/nodes/video_encoder.py
- [ ] T115 [P] Implement VideoDecoderNode Python wrapper in python-client/remotemedia/nodes/video_decoder.py
- [ ] T116 [P] Implement VideoFrame dataclass in python-client/remotemedia/data/video.py (matches RuntimeData::Video)
- [ ] T117 Add PixelFormat and VideoCodec Python enums in python-client/remotemedia/data/video.py
- [ ] T118 [P] Create Python example: encode camera frames in examples/python/encode_camera.py
- [ ] T119 [P] Create Python example: gRPC video streaming in examples/python/grpc_video_stream.py
- [ ] T120 [P] Create Python example: decode and display video in examples/python/decode_display.py
- [ ] T121 Add Python documentation strings for all video classes in python-client/remotemedia/

---

## Phase 9: Documentation & Polish

**Purpose**: Developer documentation and code quality

- [ ] T122 [P] Update README.md with video codec feature overview
- [ ] T123 [P] Create video codec usage guide in docs/VIDEO_CODEC_GUIDE.md
- [ ] T124 [P] Document FFmpeg installation requirements in docs/INSTALLATION.md
- [ ] T125 [P] Add video node examples to quickstart.md validation (ensure examples work)
- [ ] T126 Run cargo clippy on all video-related code in runtime-core/src/nodes/video/
- [ ] T127 Run cargo fmt on all modified files
- [ ] T128 [P] Add inline documentation (rustdoc) for all public video APIs
- [ ] T129 [P] Create video codec troubleshooting guide in docs/TROUBLESHOOTING.md
- [ ] T130 Update CHANGELOG.md with video codec feature additions

---

## Phase 10: Error Handling & Robustness

**Purpose**: Production-ready error handling and edge cases

- [ ] T131 [P] Implement corrupted frame handling in runtime-core/src/nodes/video/decoder.rs (lenient vs strict mode)
- [ ] T132 [P] Add resolution change detection in runtime-core/src/nodes/video/decoder.rs (reinitialize decoder)
- [ ] T133 [P] Implement keyframe request on decoder error in runtime-core/src/nodes/video/decoder.rs
- [ ] T134 Add IPC buffer overflow handling in runtime-core/src/python/multiprocess/data_transfer.rs (chunking for large frames)
- [ ] T135 [P] Add encoder initialization retry logic in runtime-core/src/nodes/video/encoder.rs
- [ ] T136 Implement frame drop metrics in runtime-core/src/nodes/video/encoder.rs and decoder.rs
- [ ] T137 Add logging for all error paths in video nodes (encode failures, decode errors, etc.)
- [ ] T138 [P] Create edge case integration tests in tests/integration/test_video_edge_cases.rs (resolution changes, corruption, missing keyframes)

---

## Phase 11: Performance Benchmarks & Validation

**Purpose**: Verify success criteria from spec.md

- [ ] T139 [P] Benchmark encoding latency @ 720p30 (all codecs) in runtime-core/benches/video_encode_decode.rs (target: <50ms)
- [ ] T140 [P] Benchmark decoding latency @ 720p30 (all codecs) in runtime-core/benches/video_encode_decode.rs (target: <50ms)
- [ ] T141 Benchmark WebRTC streaming throughput in runtime-core/benches/video_webrtc.rs (target: 30fps sustained)
- [ ] T142 [P] Benchmark zero-copy IPC transfer in runtime-core/benches/video_ipc.rs
- [ ] T143 Memory leak test: 1-hour continuous encoding in tests/integration/test_video_stress.rs (verify stable memory)
- [ ] T144 gRPC frame loss test under normal conditions in tests/integration/test_video_grpc.rs (target: zero loss)
- [ ] T145 Create quality comparison test in tests/integration/ (VP8 vs AV1 vs H.264 PSNR metrics)
- [ ] T146 [P] Validate success criteria SC-001 through SC-010 from spec.md (create validation checklist)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational (Phase 2) - VP8 encoding/decoding + gRPC
- **User Story 2 (Phase 4)**: Depends on Foundational (Phase 2) - Can start in parallel with US1, but builds on US1 patterns
- **User Story 3 (Phase 5)**: Depends on User Story 1 completion (needs encoder/decoder nodes) - WebRTC integration
- **User Story 4 (Phase 6)**: Depends on Foundational (Phase 2) - Can start in parallel with US1/US2, independent transformations
- **Hardware Acceleration (Phase 7)**: Depends on User Story 1 or 2 completion (needs base encoder)
- **Python Bindings (Phase 8)**: Depends on User Story 1 completion (needs Rust nodes)
- **Documentation (Phase 9)**: Can proceed in parallel with any phase, finalize after US4
- **Error Handling (Phase 10)**: Depends on User Story 1 completion
- **Benchmarks (Phase 11)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories - **MVP TARGET**
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Extends US1 encoder/decoder to support more codecs
- **User Story 3 (P2)**: Depends on User Story 1 (needs video encoder/decoder nodes) - Adds WebRTC transport
- **User Story 4 (P3)**: Can start after Foundational (Phase 2) - Independent transformations (scaler, converter)

### Within Each User Story

- Tests can run in parallel (marked [P])
- Core node implementations before transport integration
- gRPC/WebRTC integration after node implementations
- IPC support after basic node functionality

### Parallel Opportunities

- **Setup Phase**: Tasks T002, T003, T004, T007 can run in parallel
- **Foundational Phase**: Tasks T008-T011, T014, T016-T017 can run in parallel
- **User Story 1**: T020/T021 (encoder/decoder structs), T030/T031 (protobuf conversions), T039/T040/T041 (tests) in parallel
- **User Story 2**: T046/T047/T048 (AV1), T051/T052/T053 (H.264), T059/T060 (tests) in parallel
- **User Story 3**: T065/T066 (codec registration), T073/T074 (RTP formatting), T080/T081 (tests) in parallel
- **User Story 4**: T084/T085 (scaler), T090/T091 (converter), T102/T103 (tests) in parallel
- **Hardware Accel**: T107, T108, T109 can run in parallel
- **Python**: T114-T117, T118-T120 can run in parallel
- **Documentation**: T122-T125, T128-T129 can run in parallel

---

## Parallel Example: User Story 1

```bash
# Launch encoder and decoder node implementations together:
Task: "Implement VideoEncoderNode struct in runtime-core/src/nodes/video/encoder.rs"
Task: "Implement VideoDecoderNode struct in runtime-core/src/nodes/video/decoder.rs"

# Launch protobuf conversions together:
Task: "RuntimeData::Video → proto::VideoFrame in transports/remotemedia-grpc/src/adapters.rs"
Task: "proto::VideoFrame → RuntimeData::Video in transports/remotemedia-grpc/src/adapters.rs"

# Launch all unit tests together:
Task: "Unit test for VideoEncoderNode (VP8) in runtime-core/src/nodes/video/encoder.rs"
Task: "Unit test for VideoDecoderNode (VP8) in runtime-core/src/nodes/video/decoder.rs"
Task: "Unit test for RuntimeData::Video validation in runtime-core/src/lib.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T007)
2. Complete Phase 2: Foundational (T008-T019) - CRITICAL - blocks all stories
3. Complete Phase 3: User Story 1 (T020-T045)
4. **STOP and VALIDATE**:
   - Test VP8 encoding/decoding independently
   - Run integration test for gRPC video streaming (T042)
   - Verify <50ms latency benchmark (T043)
5. Deploy/demo MVP: **Basic video streaming through pipeline works**

**MVP Validation Checklist**:
- ✅ Can encode raw 720p YUV frame to VP8 bitstream
- ✅ Can decode VP8 bitstream back to raw frame
- ✅ Can send video via gRPC bidirectional stream
- ✅ Can receive video via gRPC
- ✅ IPC serialization works for multiprocess nodes
- ✅ Latency < 50ms @ 720p30

### Incremental Delivery

1. **Complete Setup + Foundational** (T001-T019) → Foundation ready
2. **Add User Story 1** (T020-T045) → Test independently → Deploy/Demo **(MVP! Basic VP8 video streaming works)**
3. **Add User Story 2** (T046-T063) → Test independently → Deploy/Demo (Multi-codec support: VP8 + AV1 + H.264)
4. **Add User Story 3** (T064-T083) → Test independently → Deploy/Demo (WebRTC real-time video calls)
5. **Add User Story 4** (T084-T106) → Test independently → Deploy/Demo (Video transformations: scaling, format conversion)
6. **Add Performance & Polish** (T107-T146) → Production-ready release

Each increment adds value without breaking previous features.

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup (Phase 1) + Foundational (Phase 2) together (T001-T019)
2. Once Foundational is done:
   - **Developer A**: User Story 1 (T020-T045) - VP8 + gRPC
   - **Developer B**: User Story 4 (T084-T106) - Scaler + Format Converter (independent)
   - **Developer C**: Python Bindings (T114-T121) in parallel (depends on US1 completing)
3. After US1 completes:
   - **Developer A**: User Story 2 (T046-T063) - AV1 + H.264 codecs
   - **Developer D**: User Story 3 (T064-T083) - WebRTC integration (depends on US1)
4. Polish & Performance together (T107-T146)

---

## Task Summary

### Total Tasks: 146

**By Phase**:
- Phase 1 (Setup): 7 tasks
- Phase 2 (Foundational): 12 tasks (CRITICAL PATH)
- Phase 3 (User Story 1 - P1): 26 tasks ← **MVP TARGET**
- Phase 4 (User Story 2 - P2): 18 tasks
- Phase 5 (User Story 3 - P2): 20 tasks
- Phase 6 (User Story 4 - P3): 23 tasks
- Phase 7 (Hardware Acceleration): 7 tasks
- Phase 8 (Python Bindings): 8 tasks
- Phase 9 (Documentation): 9 tasks
- Phase 10 (Error Handling): 8 tasks
- Phase 11 (Benchmarks): 8 tasks

**By User Story**:
- User Story 1 (P1 - VP8 + gRPC): 26 tasks
- User Story 2 (P2 - Multi-codec): 18 tasks
- User Story 3 (P2 - WebRTC): 20 tasks
- User Story 4 (P3 - Transformations): 23 tasks
- Infrastructure (Setup + Foundational): 19 tasks
- Polish & Cross-cutting: 40 tasks

**Parallel Opportunities**: 58 tasks marked [P] (40% of total)

**Suggested MVP Scope**: Phases 1-3 (45 tasks, User Story 1 only)

**Estimated Effort** (from plan.md):
- MVP (US1): 2 weeks (1 developer)
- Full Feature (US1-US4): 8 weeks (1 developer)
- Parallel Team: 4-5 weeks (3-4 developers)

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Integration tests validate each user story works end-to-end
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Hardware acceleration (Phase 7) is optional but recommended for production
- WASM support (ffmpeg-wasi) deferred to future iteration per plan.md

---

## Format Validation

✅ All 146 tasks follow required checklist format:
- Checkbox: `- [ ]`
- Task ID: Sequential (T001-T146)
- [P] marker: Present on 58 parallelizable tasks
- [Story] label: Present on all user story tasks (US1-US4)
- Description: Includes exact file paths
- Organization: Grouped by phase and user story

✅ Independent Test Criteria: Defined for each user story
✅ Dependencies: Clearly documented per phase
✅ MVP Scope: User Story 1 (26 tasks + infrastructure)
✅ Parallel Opportunities: 58 tasks identified
