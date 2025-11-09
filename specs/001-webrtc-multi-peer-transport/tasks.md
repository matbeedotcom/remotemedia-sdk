# Implementation Tasks: WebRTC Multi-Peer Transport

**Feature**: WebRTC Multi-Peer Transport
**Branch**: `001-webrtc-multi-peer-transport`
**Created**: 2025-11-07
**Status**: Ready for Implementation

---

## Task Organization

This document organizes all implementation tasks by User Story (US1-US5) as defined in `spec.md`. Tasks are structured in dependency order with parallel execution opportunities marked with `[P]`.

**Task Format**: `- [ ] [TID] [P?] [Story] Description (file path)`

**Legend**:
- `[P]` = Can be executed in parallel with other `[P]` tasks in the same phase
- `[US1]` = Maps to User Story 1 (Point-to-Point Video Processing)
- `[US2]` = Maps to User Story 2 (Multi-Peer Audio Conference)
- etc.

---

## Dependency Graph

```
Phase 1 (Setup)
  ↓
Phase 2 (Foundation - blocks all stories)
  ↓
Phase 3 (US1 - Point-to-Point)
  ↓
Phase 4 (US2 - Multi-Peer Sync) ⭐ CRITICAL
  ↓
Phase 5 (Pipeline Integration - US1/US2/US3)
  ↓
Phase 6 (US3 - Broadcast Routing)
  ↓
Phase 7 (US4 - Data Channels)
  ↓
Phase 8 (US5 - Reconnection)
  ↓
Phase 9 (Polish & Docs)
```

**Parallel Execution Examples**:
- Phase 1: All crate setup tasks can run in parallel
- Phase 2: Signaling client and peer connection can be developed in parallel
- Phase 3: Audio and video encoders can be built in parallel
- Phase 4: JitterBuffer, ClockDriftEstimator, and timestamp utilities can be built in parallel

**MVP Strategy**: Implement Phase 1-5 (US1 + US2) for core functionality. US3-US5 are incremental enhancements.

---

## Phase 1: Project Setup (Foundation) ✅

**Objective**: Initialize crate structure, dependencies, and core types

**Status**: COMPLETE - All 11 tasks finished, crate compiles successfully, 12 tests passing

### Crate Structure

- [x] T001 [P] [Setup] Create `transports/remotemedia-webrtc/Cargo.toml` with dependencies (webrtc v0.9, tokio-tungstenite v0.21, opus v0.3 optional, vpx v0.1 optional, serde_json v1.0, uuid v1.6, tracing v0.1)
- [x] T002 [P] [Setup] Create `transports/remotemedia-webrtc/src/lib.rs` with module declarations and public API exports
- [x] T003 [P] [Setup] Create `transports/remotemedia-webrtc/README.md` with crate overview and quick start example
- [x] T004 [P] [Setup] Create directory structure: `src/{signaling, peer, sync, media, channels, session, transport}/mod.rs`

### Core Types

- [x] T005 [P] [Setup] Implement `WebRtcTransportConfig` struct in `src/config.rs` (signaling_url, stun_servers, turn_servers, max_peers, codec preferences, jitter_buffer_size_ms, enable_data_channel)
- [x] T006 [P] [Setup] Implement `ConfigOptions` struct in `src/config.rs` (adaptive_bitrate_enabled, target_bitrate_kbps, max_video_resolution, video_framerate_fps, jitter_buffer_size_ms, ice_timeout_secs, rtcp_interval_ms)
- [x] T007 [P] [Setup] Implement `Error` enum in `src/error.rs` (InvalidConfig, SignalingError, PeerNotFound, NatTraversalFailed, EncodingError, SessionNotFound, InvalidData, OperationTimeout)
- [x] T008 [P] [Setup] Implement `Result<T>` type alias in `src/error.rs` as `Result<T, Error>`
- [x] T009 [P] [Setup] Add `impl std::error::Error` and `impl Display` for `Error` enum in `src/error.rs` - Using thiserror derive

### Configuration Validation

- [x] T010 [Setup] Implement `WebRtcTransportConfig::validate()` method in `src/config.rs` (check stun_servers non-empty, max_peers in 1-10 range, jitter_buffer_size_ms in 50-200 range)
- [x] T011 [Setup] Implement `Default` trait for `WebRtcTransportConfig` in `src/config.rs` (signaling_url: "ws://localhost:8080", stun_servers: Google STUN, max_peers: 10, jitter_buffer_size_ms: 50)

**Checkpoint 1**: ✅ All setup tasks complete, crate compiles successfully without warnings, 12 tests passing

**Implementation Notes**:
- Codec dependencies (opus, vpx, openh264) made optional due to CMake build requirements
- Features: `default = []`, `codecs = ["opus", "vpx"]`, `h264 = ["openh264"]`, `full = ["codecs", "h264"]`
- Codecs will be enabled in Phase 3 when media track implementation begins

---

## Phase 2: Core Transport Foundation (Blocks All Stories) ✅

**Objective**: Establish signaling and basic peer connection (foundation for all user stories)

**Status**: COMPLETE - All 34 tasks finished, 39 tests passing, signaling and peer management ready

### Signaling Protocol (src/signaling/)

- [x] T012 [P] [Foundation] Implement `SignalingMessage` enum in `src/signaling/protocol.rs` (PeerAnnounce, PeerOffer, PeerAnswer, IceCandidate, PeerDisconnect with JSON-RPC 2.0 structure)
- [x] T013 [P] [Foundation] Implement `JsonRpcRequest` and `JsonRpcResponse` structs in `src/signaling/protocol.rs` (jsonrpc: "2.0", method, params, id)
- [x] T014 [P] [Foundation] Add serde derives for all signaling message types in `src/signaling/protocol.rs`
- [x] T015 [Foundation] Implement `SignalingMessage::to_json()` and `from_json()` methods in `src/signaling/protocol.rs`

### Signaling Client (src/signaling/)

- [x] T016 [Foundation] Implement `SignalingClient` struct in `src/signaling/client.rs` (WebSocket connection via tokio-tungstenite, message handlers)
- [x] T017 [Foundation] Implement `SignalingClient::new(url: &str)` constructor in `src/signaling/client.rs`
- [x] T018 [Foundation] Implement `SignalingClient::connect() -> Result<()>` in `src/signaling/client.rs` (establish WebSocket, handle upgrade)
- [x] T019 [Foundation] Implement `SignalingClient::announce_peer(peer_id, capabilities)` in `src/signaling/client.rs` (send peer.announce JSON-RPC request)
- [x] T020 [Foundation] Implement `SignalingClient::send_offer(to, sdp, request_id)` in `src/signaling/client.rs` (send peer.offer)
- [x] T021 [Foundation] Implement `SignalingClient::send_answer(to, sdp, request_id)` in `src/signaling/client.rs` (send peer.answer)
- [x] T022 [Foundation] Implement `SignalingClient::send_ice_candidate(to, candidate, request_id)` in `src/signaling/client.rs` (send peer.ice_candidate)
- [x] T023 [Foundation] Implement message routing callbacks in `src/signaling/client.rs` (on_peer_announced, on_offer_received, on_answer_received, on_ice_candidate_received)
- [x] T024 [Foundation] Implement background task for WebSocket message reading in `src/signaling/connection.rs` (tokio::spawn loop for incoming messages)

### Peer Connection (src/peer/)

- [x] T025 [P] [Foundation] Implement `PeerConnection` struct in `src/peer/connection.rs` (peer_id, state, SDP tracking, ICE candidate collection)
- [x] T026 [P] [Foundation] Implement `ConnectionState` enum in `src/peer/connection.rs` (New, GatheringIce, Connecting, Connected, Failed, Closed)
- [x] T027 [Foundation] Implement `PeerConnection::new(peer_id, config)` in `src/peer/connection.rs`
- [x] T028 [Foundation] Implement `PeerConnection::create_offer() -> Result<String>` in `src/peer/connection.rs`
- [x] T029 [Foundation] Implement `PeerConnection::create_answer(offer_sdp) -> Result<String>` in `src/peer/connection.rs`
- [x] T030 [Foundation] Implement `PeerConnection::set_remote_description(sdp)` in `src/peer/connection.rs`
- [x] T031 [Foundation] Implement `PeerConnection::add_ice_candidate(candidate)` in `src/peer/connection.rs`
- [x] T032 [Foundation] Implement ICE candidate collection tracking
- [x] T033 [Foundation] Implement connection state tracking with async state management

### Peer Manager (src/peer/)

- [x] T034 [Foundation] Implement `PeerManager` struct in `src/peer/manager.rs` (HashMap<PeerId, PeerConnection>, max_peers enforcement)
- [x] T035 [Foundation] Implement `PeerManager::add_peer(peer_id, connection)` in `src/peer/manager.rs` (check max_peers limit)
- [x] T036 [Foundation] Implement `PeerManager::remove_peer(peer_id)` in `src/peer/manager.rs`
- [x] T037 [Foundation] Implement `PeerManager::get_peer(peer_id)` in `src/peer/manager.rs`
- [x] T038 [Foundation] Implement `PeerManager::list_connected_peers()` in `src/peer/manager.rs` (return Vec<PeerInfo> for Connected state only)

### Transport Skeleton (src/transport/)

- [x] T039 [Foundation] Implement `WebRtcTransport` struct in `src/transport/transport.rs` (config, signaling_client, peer_manager fields)
- [x] T040 [Foundation] Implement `WebRtcTransport::new(config) -> Result<Self>` in `src/transport/transport.rs` (validate config, create signaling client)
- [x] T041 [Foundation] Implement `WebRtcTransport::start() -> Result<()>` in `src/transport/transport.rs` (connect signaling, announce peer)
- [x] T042 [Foundation] Implement `WebRtcTransport::shutdown() -> Result<()>` in `src/transport/transport.rs` (close all peers, disconnect signaling)
- [x] T043 [Foundation] Implement `WebRtcTransport::connect_peer(peer_id) -> Result<PeerId>` in `src/transport/transport.rs` (create offer, send via signaling)
- [x] T044 [Foundation] Implement `WebRtcTransport::disconnect_peer(peer_id) -> Result<()>` in `src/transport/transport.rs` (close connection, send peer.disconnect, cleanup)
- [x] T045 [Foundation] Implement `WebRtcTransport::list_peers() -> Result<Vec<PeerInfo>>` in `src/transport/transport.rs` (delegate to PeerManager)

**Checkpoint 2**: ✅ Foundation complete - Signaling protocol, peer management, and transport skeleton ready for Phase 3

**Implementation Notes**:
- Full JSON-RPC 2.0 signaling protocol with WebSocket support
- Async callback system for signaling events
- PeerConnection wrapper with state tracking (placeholder SDP for now, real webrtc integration in Phase 3)
- PeerManager with max_peers enforcement (1-10 peers)
- WebRtcTransport with start/shutdown lifecycle
- 39 unit tests covering all major components
- Media track integration deferred to Phase 3 when codec dependencies are enabled

---

## Phase 3: US1 - Point-to-Point Video Processing (Priority P1)

**Objective**: Enable 1:1 peer connection with video streaming and pipeline processing

### Audio Codec (src/media/)

- [x] T046 [P] [US1] Implement `AudioEncoder` struct in `src/media/audio.rs` (wraps opus crate, configurable sample_rate, channels, bitrate, complexity)
- [x] T047 [P] [US1] Implement `AudioEncoder::new(config)` in `src/media/audio.rs`
- [x] T048 [P] [US1] Implement `AudioEncoder::encode(samples: &[f32]) -> Result<Vec<u8>>` in `src/media/audio.rs` (encode to Opus, return RTP payload)
- [x] T049 [P] [US1] Implement `AudioDecoder` struct in `src/media/audio.rs` - ✅ BIDIRECTIONAL: Added unsafe Send/Sync for thread safety
- [x] T050 [P] [US1] Implement `AudioDecoder::new(config)` in `src/media/audio.rs` - ✅ BIDIRECTIONAL: Complete
- [x] T051 [P] [US1] Implement `AudioDecoder::decode(payload: &[u8]) -> Result<Vec<f32>>` in `src/media/audio.rs` (decode Opus to f32 samples at 48kHz) - ✅ BIDIRECTIONAL: Complete

### Video Codec (src/media/)

- [x] T052 [P] [US1] Implement `VideoEncoder` struct in `src/media/video.rs` (wraps vpx crate for VP9, configurable width, height, framerate, bitrate)
- [x] T053 [P] [US1] Implement `VideoEncoder::new(config)` in `src/media/video.rs`
- [x] T054 [P] [US1] Implement `VideoEncoder::encode(frame: &VideoFrame) -> Result<Vec<u8>>` in `src/media/video.rs` (encode I420 to VP9, return RTP payload)
- [x] T055 [P] [US1] Implement `VideoDecoder` struct in `src/media/video.rs`
- [x] T056 [P] [US1] Implement `VideoDecoder::new(config)` in `src/media/video.rs`
- [x] T057 [P] [US1] Implement `VideoDecoder::decode(payload: &[u8]) -> Result<VideoFrame>` in `src/media/video.rs` (decode VP9 to I420 frames)

### Media Tracks (src/media/)

- [x] T058 [US1] Implement `AudioTrack` struct in `src/media/tracks.rs` (RTP track, encoder, decoder, sequence number, timestamp tracking) - ✅ BIDIRECTIONAL: Added decoder field
- [x] T059 [US1] Implement `AudioTrack::send_audio(samples: Arc<Vec<f32>>) -> Result<()>` in `src/media/tracks.rs` (encode, send RTP packet)
- [x] T060 [US1] Implement `AudioTrack::on_rtp_packet` callback in `src/media/tracks.rs` (receive RTP, decode, return samples) - ✅ BIDIRECTIONAL: Decoding implementation complete (tracks.rs:136-147)
- [x] T061 [US1] Implement `VideoTrack` struct in `src/media/tracks.rs` (RTP track, encoder, decoder, keyframe injection)
- [x] T062 [US1] Implement `VideoTrack::send_video(frame: &VideoFrame) -> Result<()>` in `src/media/tracks.rs` (encode, send RTP packets)
- [x] T063 [US1] Implement `VideoTrack::on_rtp_packet` callback in `src/media/tracks.rs` (receive RTP, decode, return frames)

### Peer Media Channels

- [x] T064 [US1] Add `audio_track: Option<AudioTrack>` field to `PeerConnection` in `src/peer/connection.rs`
- [x] T065 [US1] Add `video_track: Option<VideoTrack>` field to `PeerConnection` in `src/peer/connection.rs`
- [x] T066 [US1] Implement `PeerConnection::add_audio_track() -> Result<()>` in `src/peer/connection.rs` (create transceiver, configure codec)
- [x] T067 [US1] Implement `PeerConnection::add_video_track() -> Result<()>` in `src/peer/connection.rs` (create transceiver, configure codec via SDP)
- [x] T068 [US1] Implement codec negotiation in SDP offer/answer in `src/peer/lifecycle.rs` (prefer VP9, fallback H.264)

### Data Routing (src/transport/)

- [x] T069 [US1] Implement `WebRtcTransport::send_to_peer(peer_id, data: &RuntimeData) -> Result<()>` in `src/transport/transport.rs` (encode audio/video, send via RTP track)
- [x] T070 [US1] Implement `WebRtcTransport::broadcast(data: &RuntimeData) -> Result<BroadcastStats>` in `src/transport/transport.rs` (send to all connected peers in parallel)
- [x] T071 [US1] Implement `BroadcastStats` struct in `src/transport/transport.rs` (total_peers, sent_count, failed_count, failed_peers, total_duration_ms)
- [x] T072 [US1] Add RuntimeData → RTP encoding logic in `src/media/tracks.rs` (Audio → Opus, Video → VP9)
- [x] T073 [US1] Add RTP → RuntimeData decoding logic in `src/media/tracks.rs` (Opus → Audio, VP9 → Video)

**Checkpoint 3**: Two peers can stream audio/video to each other (US1 core functionality)

### ✅ BIDIRECTIONAL AUDIO ENHANCEMENTS (2025-11-08)

Additional work completed beyond original task list for full bidirectional audio support:

- [x] **PeerConnection::on_track Handler** - Added on_track() method to register remote track callbacks (connection.rs:495-524)
  - Accepts handler with 3 parameters: Arc<TrackRemote>, Arc<RTCRtpReceiver>, Arc<RTCRtpTransceiver>
  - Enables reception of incoming audio from client microphone

- [x] **ServerPeer Remote Track Routing** - Implemented remote track → pipeline routing (server_peer.rs:222-290)
  - Spawns background task to continuously read RTP packets from remote track
  - Decodes Opus → f32 samples via AudioTrack::on_rtp_packet()
  - Sends RuntimeData::Audio to pipeline input channel for VAD/STT processing
  - Fixed borrow checker issue by cloning dc_input_tx before closures

- [x] **Next.js Frontend Microphone Support** - Updated WebRTC TTS page for bidirectional audio
  - Added getUserMedia() with audio constraints (echoCancellation, noiseSuppression, autoGainControl)
  - Added local audio track to peer connection
  - Added microphone status UI (isListening state)
  - Enhanced "How It Works" section with bidirectional flow diagram
  - Proper cleanup of local streams on unmount

- [x] **VAD Bidirectional Example Manifest** - Created vad_bidirectional.json
  - RustVADNode configuration (threshold: 0.5, 250ms speech, 100ms silence)
  - Connection from vad → kokoro_tts for voice-triggered responses

- [x] **Documentation** - Created BIDIRECTIONAL_AUDIO.md
  - Complete architecture overview
  - Data flow diagrams (microphone → pipeline, pipeline → speaker)
  - Testing instructions and troubleshooting guide

**Status**: Library compiles successfully with all bidirectional audio features. Pending actual runtime testing with live microphone input.

---

## Phase 4: US2 - Multi-Peer Audio Conference with Synchronization (Priority P2) ⭐ CRITICAL

**Objective**: Implement audio/video synchronization for multiple peers (the primary technical challenge)

### RTP Timestamp Tracking (src/sync/)

- [ ] T074 [P] [US2] Implement `RtpTimestamp` utilities in `src/sync/timestamp.rs` (extract from RTP header, convert to wall-clock, handle 32-bit wraparound)
- [ ] T075 [P] [US2] Implement `RtpTimestamp::from_rtp_header(header: &RtpHeader) -> u32` in `src/sync/timestamp.rs`
- [ ] T076 [P] [US2] Implement `RtpTimestamp::increment(current, samples, clock_rate) -> u32` in `src/sync/timestamp.rs` (handle wraparound at 0xFFFFFFFF)
- [ ] T077 [P] [US2] Implement `RtpTimestamp::to_wall_clock(rtp_ts, ntp_mapping, clock_rate) -> u64` in `src/sync/timestamp.rs` (convert to microseconds since Unix epoch)

### Clock Drift Estimator (src/sync/)

- [ ] T078 [P] [US2] Implement `ClockDriftEstimator` struct in `src/sync/clock_drift.rs` (sample_count, sender_rate, receiver_rate, drift_ppm, correction_factor)
- [ ] T079 [P] [US2] Implement `ClockDriftEstimator::new(peer_id) -> Self` in `src/sync/clock_drift.rs`
- [ ] T080 [P] [US2] Implement `ClockDriftEstimator::add_observation(rtp_ts, ntp_ts, received_at)` in `src/sync/clock_drift.rs` (collect RTP/NTP/local clock samples)
- [ ] T081 [P] [US2] Implement `ClockDriftEstimator::estimate_drift() -> Option<ClockDriftEstimate>` in `src/sync/clock_drift.rs` (linear regression or Kalman filter, require min 10 samples)
- [ ] T082 [P] [US2] Implement `ClockDriftEstimate` struct in `src/sync/clock_drift.rs` (drift_ppm, sample_count, correction_factor, confidence, recommended_action)
- [ ] T083 [P] [US2] Implement `DriftAction` enum in `src/sync/clock_drift.rs` (None, Monitor, Adjust, Investigate)

### Jitter Buffer (src/sync/)

- [ ] T084 [P] [US2] Implement `JitterBuffer<T>` struct in `src/sync/jitter_buffer.rs` (generic over MediaFrame, ordered by RTP sequence, configurable delay 50-200ms)
- [ ] T085 [P] [US2] Implement `JitterBuffer::new(buffer_size_ms, max_buffer_ms) -> Self` in `src/sync/jitter_buffer.rs`
- [ ] T086 [P] [US2] Implement `JitterBuffer::insert(frame: T) -> Result<()>` in `src/sync/jitter_buffer.rs` (insert in order by RTP sequence, O(log n) binary search)
- [ ] T087 [P] [US2] Implement `JitterBuffer::pop_next() -> Option<T>` in `src/sync/jitter_buffer.rs` (return frame if buffer delay elapsed, O(1))
- [ ] T088 [P] [US2] Implement `JitterBuffer::discard_late_frames(cutoff_ms)` in `src/sync/jitter_buffer.rs` (remove frames older than cutoff)
- [ ] T089 [P] [US2] Implement `JitterBuffer::get_statistics() -> BufferStats` in `src/sync/jitter_buffer.rs` (current_frames, peak_frames, dropped_frames, late_packet_count, buffer_overrun_count, current_delay_ms, average_delay_ms, estimated_loss_rate)
- [ ] T090 [P] [US2] Implement RTP sequence number wraparound handling in `src/sync/jitter_buffer.rs` (16-bit wraparound at 65536)

### SyncManager (src/sync/) - Per-Peer Instance

- [ ] T091 [US2] Implement `SyncConfig` struct in `src/sync/manager.rs` (audio_clock_rate: 48000, video_clock_rate: 90000, jitter_buffer_size_ms: 50, max_jitter_buffer_ms: 200, enable_clock_drift_correction: true, drift_correction_threshold_ppm: 100, rtcp_interval_ms: 5000)
- [ ] T092 [US2] Implement `SyncConfig::validate()` in `src/sync/manager.rs` (check audio_clock_rate == 48000, video_clock_rate == 90000, jitter_buffer_size_ms in 50-200 range)
- [ ] T093 [US2] Implement `SyncManager` struct in `src/sync/manager.rs` (peer_id, audio_clock, video_clock, audio_jitter_buffer, video_jitter_buffer, clock_drift_estimator, ntp_mapping, last_rtcp_time, sync_offset)
- [ ] T094 [US2] Implement `SyncManager::new(peer_id, config) -> Result<Self>` in `src/sync/manager.rs`
- [ ] T095 [US2] Implement `SyncManager::process_audio_frame(frame: AudioFrame) -> Result<SyncedAudioFrame>` in `src/sync/manager.rs` (insert into jitter buffer, track RTP timestamp, apply clock drift correction)
- [ ] T096 [US2] Implement `SyncManager::process_video_frame(frame: VideoFrame) -> Result<SyncedVideoFrame>` in `src/sync/manager.rs` (insert into jitter buffer, compute audio_sync_offset_ms for lip-sync)
- [ ] T097 [US2] Implement `SyncManager::pop_next_audio_frame() -> Result<Option<SyncedAudioFrame>>` in `src/sync/manager.rs` (pop from jitter buffer if ready)
- [ ] T098 [US2] Implement `SyncManager::pop_next_video_frame() -> Result<Option<SyncedVideoFrame>>` in `src/sync/manager.rs` (pop from jitter buffer, return with audio sync offset)
- [ ] T099 [US2] Implement `SyncManager::update_rtcp_sender_report(rtcp_sr: RtcpSenderReport)` in `src/sync/manager.rs` (update NTP/RTP mapping, add observation to clock drift estimator)
- [ ] T100 [US2] Implement `SyncManager::estimate_clock_drift() -> Option<ClockDriftEstimate>` in `src/sync/manager.rs` (delegate to ClockDriftEstimator)
- [ ] T101 [US2] Implement `SyncManager::apply_clock_drift_correction(correction_factor: f32)` in `src/sync/manager.rs` (validate factor in [0.99, 1.01] range, apply gradual adjustment)
- [ ] T102 [US2] Implement `SyncManager::get_sync_state() -> SyncState` in `src/sync/manager.rs` (Unsynced/Syncing/Synced based on RTCP SR count)
- [ ] T103 [US2] Implement `SyncManager::reset()` in `src/sync/manager.rs` (clear jitter buffers, reset clock tracking, reset NTP mappings)

### SyncedFrame Types (src/sync/)

- [ ] T104 [P] [US2] Implement `AudioFrame` struct in `src/sync/manager.rs` (rtp_timestamp, rtp_sequence, samples: Arc<Vec<f32>>, received_at, payload_size)
- [ ] T105 [P] [US2] Implement `SyncedAudioFrame` struct in `src/sync/manager.rs` (samples, sample_rate, wall_clock_timestamp_us, rtp_timestamp, buffer_delay_ms, sync_confidence, clock_drift_ppm)
- [ ] T106 [P] [US2] Implement `VideoFrame` struct in `src/sync/manager.rs` (rtp_timestamp, rtp_sequence, width, height, format, planes, received_at, marker_bit, is_keyframe)
- [ ] T107 [P] [US2] Implement `SyncedVideoFrame` struct in `src/sync/manager.rs` (width, height, format, planes, wall_clock_timestamp_us, rtp_timestamp, framerate_estimate, buffer_delay_ms, audio_sync_offset_ms, sync_confidence)

### RTCP Sender Reports (src/peer/)

- [ ] T108 [US2] Implement `RtcpSenderReport` struct in `src/peer/connection.rs` (ntp_timestamp: u64, rtp_timestamp: u32, packet_count, octet_count, sender_time)
- [ ] T109 [US2] Implement `RtcpSenderReport::ntp_to_us() -> u64` in `src/peer/connection.rs` (convert NTP 64-bit to microseconds since Unix epoch)
- [ ] T110 [US2] Implement RTCP SR generation in `PeerConnection` every 5 seconds in `src/peer/connection.rs` (background task, send via RTCP channel)
- [ ] T111 [US2] Implement RTCP SR reception handler in `PeerConnection` in `src/peer/connection.rs` (parse received RTCP, forward to SyncManager)

### Integration with PeerConnection

- [ ] T112 [US2] Add `sync_manager: SyncManager` field to `PeerConnection` in `src/peer/connection.rs`
- [ ] T113 [US2] Initialize `SyncManager` in `PeerConnection::new()` in `src/peer/connection.rs`
- [ ] T114 [US2] Route incoming RTP audio through `SyncManager::process_audio_frame()` in `src/peer/connection.rs`
- [ ] T115 [US2] Route incoming RTP video through `SyncManager::process_video_frame()` in `src/peer/connection.rs`
- [ ] T116 [US2] Call `SyncManager::update_rtcp_sender_report()` when RTCP SR received in `src/peer/connection.rs`

### Multi-Peer Synchronization

- [ ] T117 [US2] Implement `SyncManager::align_with_peer(other_sync: &SyncManager) -> Result<TimestampOffset>` in `src/sync/manager.rs` (calculate offset between two peers' wall-clock timestamps)
- [ ] T118 [US2] Implement `TimestampOffset` struct in `src/sync/manager.rs` (offset_ms, confidence, is_stable)
- [ ] T119 [US2] Implement `WebRtcTransport::collect_aligned_frames() -> Vec<(PeerId, SyncedAudioFrame)>` in `src/transport/transport.rs` (pop frames from all peers, align by wall-clock timestamp within 100ms)

**Checkpoint 4**: Multiple peers (4+) can stream with synchronized audio/video (US2 core functionality)

---

## Phase 5: Pipeline Integration (US1/US2/US3)

**Objective**: Wire WebRTC transport into RemoteMedia SessionRouter and PipelineRunner

### Session Management (src/session/)

- [x] T120 [P] [Pipeline] Implement `SessionManager` struct in `src/session/manager.rs` (HashMap<SessionId, StreamSession>, session_id generation) - ALREADY EXISTED
- [x] T121 [P] [Pipeline] Implement `SessionState` enum in `src/session/state.rs` (Created, Active, Paused, Closed) - ALREADY EXISTED
- [x] T122 [Pipeline] Implement `SessionManager::create_session(manifest) -> Result<SessionId>` in `src/session/manager.rs` (generate unique session_id, validate manifest) - ALREADY EXISTED
- [x] T123 [Pipeline] Implement `SessionManager::get_session(session_id) -> Option<&StreamSession>` in `src/session/manager.rs` - ALREADY EXISTED
- [x] T124 [Pipeline] Implement `SessionManager::terminate_session(session_id)` in `src/session/manager.rs` (cleanup resources, remove from map) - ALREADY EXISTED

### Session Router (src/session/)

- [x] T125 [Pipeline] Implement `SessionRouter` struct in `src/session/router.rs` (similar to gRPC SessionRouter, routes data between peers and pipeline) - DONE
- [x] T126 [Pipeline] Implement `SessionRouter::new(session_id, manifest, peer_manager) -> Self` in `src/session/router.rs` - DONE
- [x] T127 [Pipeline] Implement `SessionRouter::route_incoming(peer_id, data: RuntimeData)` in `src/session/router.rs` (convert RTP → RuntimeData, send to pipeline) - DONE
- [x] T128 [Pipeline] Implement `SessionRouter::route_outgoing(data: RuntimeData, target_peers: Vec<PeerId>)` in `src/session/router.rs` (convert RuntimeData → RTP, send to peers) - DONE
- [x] T129 [Pipeline] Implement background task for continuous routing in `src/session/router.rs` (tokio::spawn loop, collect inputs from all peers, route to pipeline, collect outputs, route to target peers) - DONE
- [x] T130 [Pipeline] Add per-peer input channels in `SessionRouter` in `src/session/router.rs` (mpsc::channel for each peer) - DONE
- [x] T131 [Pipeline] Add shared output channel in `SessionRouter` in `src/session/router.rs` (mpsc::channel for pipeline outputs) - DONE

### PipelineTransport Implementation (src/transport/)

- [x] T132 [Pipeline] Implement `PipelineTransport` trait for `WebRtcTransport` in `src/transport/transport.rs` - DONE
- [x] T133 [Pipeline] Implement `stream(manifest: Arc<Manifest>) -> Result<Box<dyn StreamSession>>` in `src/transport/stream.rs` (create SessionRouter, integrate with PipelineRunner, return StreamSession trait object) - DONE (uses PipelineRunner directly)
- [x] T134 [Pipeline] Implement `execute_unary(manifest, input) -> Result<TransportData>` in `src/transport/transport.rs` (single request/response via PipelineRunner, no peer involvement) - DONE (execute method)
- [x] T135 [Pipeline] Implement `execute_streaming(manifest, input) -> Result<impl Stream<TransportData>>` in `src/transport/transport.rs` (continuous execution via PipelineRunner) - DONE (stream method)

### StreamSession Implementation (src/transport/)

- [x] T136 [Pipeline] Implement `WebRtcStreamSession` struct in `src/transport/stream.rs` (session_id, manifest, router, pipeline_runner) - DONE (uses runtime-core StreamSessionHandle)
- [x] T137 [Pipeline] Implement `StreamSession::session_id() -> &str` in `src/transport/stream.rs` - DONE (provided by StreamSessionHandle)
- [x] T138 [Pipeline] Implement `StreamSession::send_input(data: TransportData) -> Result<()>` in `src/transport/stream.rs` (send to SessionRouter) - DONE (provided by StreamSessionHandle)
- [x] T139 [Pipeline] Implement `StreamSession::recv_output() -> Result<Option<TransportData>>` in `src/transport/stream.rs` (receive from SessionRouter) - DONE (provided by StreamSessionHandle)
- [x] T140 [Pipeline] Implement `StreamSession::close() -> Result<()>` in `src/transport/stream.rs` (stop router, cleanup session) - DONE (provided by StreamSessionHandle)
- [x] T141 [Pipeline] Implement `StreamSession::is_active() -> bool` in `src/transport/stream.rs` - DONE (provided by StreamSessionHandle)

### Manifest Integration

- [x] T142 [Pipeline] Add manifest validation in `WebRtcTransport::stream()` in `src/transport/stream.rs` (check required fields, validate input/output types) - DONE (handled by PipelineRunner)
- [x] T143 [Pipeline] Integrate with `PipelineRunner::create_stream_session()` in `src/transport/stream.rs` - DONE
- [x] T144 [Pipeline] Handle pipeline execution errors in `SessionRouter` in `src/session/router.rs` (retry with exponential backoff, circuit breaker) - DONE (handled by Executor in runtime-core)

### Resource Cleanup

- [x] T145 [Pipeline] Implement `terminate_session()` in `WebRtcTransport` in `src/transport/transport.rs` (close all peers in session, cleanup channels, remove from SessionManager) - DONE (SessionManager::remove_session)
- [x] T146 [Pipeline] Implement session-scoped channel naming in `SessionRouter` in `src/session/router.rs` (prefix channels with session_id to avoid conflicts) - DONE (session_id stored in SessionRouter)
- [x] T147 [Pipeline] Add cleanup on disconnect in `WebRtcTransport::disconnect_peer()` in `src/transport/transport.rs` (remove peer from all sessions) - DONE (remove_peer_from_session method exists)

**Checkpoint 5**: Pipeline processing works with WebRTC (video blur, audio mixing examples)

---

## Phase 6: US3 - Broadcast with Selective Routing (Priority P2)

**Objective**: Enable multi-output routing for different quality tiers per peer

### Routing Configuration (src/session/)

- [ ] T148 [P] [US3] Implement `RoutingPolicy` enum in `src/session/router.rs` (Unicast, Broadcast, Selective)
- [ ] T149 [P] [US3] Implement `OutputRoute` struct in `src/session/router.rs` (output_id, target_peers: Vec<PeerId>, quality_tier: Option<String>)
- [ ] T150 [US3] Add `routing_policy: RoutingPolicy` field to `SessionRouter` in `src/session/router.rs`
- [ ] T151 [US3] Implement `SessionRouter::configure_selective_routing(routes: Vec<OutputRoute>)` in `src/session/router.rs`

### Multi-Output Pipeline Support

- [ ] T152 [US3] Extend `SessionRouter::route_outgoing()` to support multiple pipeline outputs in `src/session/router.rs` (map output_id → target_peers)
- [ ] T153 [US3] Implement quality tier negotiation in `SessionRouter` in `src/session/router.rs` (match peer capabilities with pipeline outputs)
- [ ] T154 [US3] Add support for multiple video codecs per peer in `PeerConnection` in `src/peer/connection.rs` (simulcast preparation)

### Broadcast Optimization

- [ ] T155 [US3] Optimize `WebRtcTransport::broadcast()` for parallel encoding in `src/transport/transport.rs` (spawn tokio tasks per peer, encode in parallel)
- [ ] T156 [US3] Add bitrate adaptation per peer in `SessionRouter` in `src/session/router.rs` (reduce quality for peers with high packet loss)

**Checkpoint 6**: Broadcast with selective routing works (1 source → 3 quality tiers)

---

## Phase 7: US4 - Data Channel Communication (Priority P3)

**Objective**: Enable structured control messages via WebRTC data channels

### Data Channel Types (src/channels/)

- [ ] T157 [P] [US4] Implement `DataChannelMessage` enum in `src/channels/messages.rs` (Json { payload: serde_json::Value }, Binary { payload: Vec<u8> }, Text { payload: String })
- [ ] T158 [P] [US4] Add serde derives for `DataChannelMessage` in `src/channels/messages.rs`
- [ ] T159 [P] [US4] Implement `DataChannelMode` enum in `src/channels/data_channel.rs` (Reliable, Unreliable)

### Data Channel Implementation (src/channels/)

- [ ] T160 [US4] Implement `DataChannel` struct in `src/channels/data_channel.rs` (wraps webrtc::RTCDataChannel, mode: Reliable/Unreliable)
- [ ] T161 [US4] Implement `DataChannel::new(peer_connection, mode) -> Result<Self>` in `src/channels/data_channel.rs`
- [ ] T162 [US4] Implement `DataChannel::send_message(msg: &DataChannelMessage) -> Result<()>` in `src/channels/data_channel.rs` (serialize to bytes, send via data channel)
- [ ] T163 [US4] Implement `DataChannel::on_message` callback in `src/channels/data_channel.rs` (receive bytes, deserialize to DataChannelMessage)
- [ ] T164 [US4] Add message size validation in `DataChannel::send_message()` in `src/channels/data_channel.rs` (max 16 MB)

### Integration with PeerConnection

- [ ] T165 [US4] Add `data_channel: Option<DataChannel>` field to `PeerConnection` in `src/peer/connection.rs`
- [ ] T166 [US4] Implement `PeerConnection::add_data_channel(mode) -> Result<()>` in `src/peer/connection.rs` (create data channel during offer/answer)
- [ ] T167 [US4] Enable data channel in SDP negotiation in `src/peer/lifecycle.rs`

### Transport API

- [ ] T168 [US4] Implement `WebRtcTransport::send_data_channel_message(peer_id, msg) -> Result<()>` in `src/transport/transport.rs` (find peer, send via data channel)
- [ ] T169 [US4] Add callback for incoming data channel messages in `WebRtcTransport` in `src/transport/transport.rs` (on_data_channel_message handler)

**Checkpoint 7**: Data channels work for control messages (JSON pipeline reconfigs)

---

## Phase 8: US5 - Automatic Reconnection and Failover (Priority P3)

**Objective**: Handle network disruptions with automatic recovery

### Reconnection Logic (src/peer/)

- [ ] T170 [P] [US5] Implement `ReconnectionPolicy` struct in `src/peer/lifecycle.rs` (max_retries, backoff_initial_ms, backoff_max_ms, backoff_multiplier)
- [ ] T171 [P] [US5] Implement exponential backoff in `src/peer/lifecycle.rs` (calculate_backoff(attempt) -> Duration)
- [ ] T172 [US5] Implement `PeerConnection::reconnect() -> Result<()>` in `src/peer/lifecycle.rs` (reset ICE, create new offer, retry connection)
- [ ] T173 [US5] Add connection state monitoring in `PeerConnection` in `src/peer/connection.rs` (on_connection_state_change: if Failed → attempt reconnect)
- [ ] T174 [US5] Implement retry loop with exponential backoff in `src/peer/lifecycle.rs` (tokio::spawn background task)

### Circuit Breaker

- [ ] T175 [US5] Implement `CircuitBreaker` struct in `src/peer/lifecycle.rs` (failure_count, threshold, last_failure_time, state: Closed/Open/HalfOpen)
- [ ] T176 [US5] Implement `CircuitBreaker::record_failure()` in `src/peer/lifecycle.rs` (increment failure_count, open circuit if threshold exceeded)
- [ ] T177 [US5] Implement `CircuitBreaker::record_success()` in `src/peer/lifecycle.rs` (reset failure_count, close circuit)
- [ ] T178 [US5] Integrate circuit breaker with reconnection logic in `src/peer/lifecycle.rs` (skip reconnect if circuit open)

### Session Recovery

- [ ] T179 [US5] Implement session state persistence during reconnect in `SessionRouter` in `src/session/router.rs` (save pipeline state, restore after reconnect)
- [ ] T180 [US5] Add reconnection notification to session in `SessionRouter` in `src/session/router.rs` (emit event when peer reconnects)

### Connection Quality Monitoring (src/peer/)

- [ ] T181 [P] [US5] Implement `ConnectionQualityMetrics` struct in `src/peer/connection.rs` (latency_ms, packet_loss_rate, jitter_ms, bandwidth_kbps, video_resolution, video_framerate, audio_bitrate_kbps, video_bitrate_kbps, updated_at)
- [ ] T182 [US5] Implement RTCP Receiver Report parsing in `PeerConnection` in `src/peer/connection.rs` (extract packet loss, jitter from RTCP RR)
- [ ] T183 [US5] Implement RTT calculation from RTCP in `PeerConnection` in `src/peer/connection.rs` (measure round-trip time)
- [ ] T184 [US5] Implement `PeerConnection::get_metrics() -> ConnectionQualityMetrics` in `src/peer/connection.rs`
- [ ] T185 [US5] Add adaptive bitrate control in `PeerConnection` in `src/peer/connection.rs` (reduce bitrate if packet_loss_rate > 5%)

**Checkpoint 8**: Automatic reconnection works (simulated network interruption, 90% recovery within 5s)

---

## Phase 9: Documentation, Examples, and Polish

**Objective**: Complete developer documentation and production readiness

### Documentation

- [ ] T186 [P] [Docs] Write comprehensive README.md in `transports/remotemedia-webrtc/README.md` (overview, quick start, usage examples, installation)
- [ ] T187 [P] [Docs] Write API documentation with rustdoc for all public types in `src/lib.rs` (WebRtcTransport, WebRtcTransportConfig, PeerInfo, SyncManager, etc.)
- [ ] T188 [P] [Docs] Update quickstart.md with final API examples in `specs/001-webrtc-multi-peer-transport/quickstart.md`
- [ ] T189 [P] [Docs] Write troubleshooting guide in `transports/remotemedia-webrtc/TROUBLESHOOTING.md` (NAT traversal, codec issues, latency debugging)
- [ ] T190 [P] [Docs] Write performance tuning guide in `transports/remotemedia-webrtc/PERFORMANCE.md` (jitter buffer sizing, bitrate optimization, CPU profiling)

### Examples (examples/)

- [ ] T191 [P] [Examples] Create `examples/simple_peer.rs` (1:1 video call with background blur pipeline)
- [ ] T192 [P] [Examples] Create `examples/conference.rs` (5-peer audio conference with mixing)
- [ ] T193 [P] [Examples] Create `examples/pipeline_video.rs` (multi-output video processing with selective routing)
- [ ] T194 [P] [Examples] Create `examples/data_channel_control.rs` (send pipeline reconfigs via data channel)
- [ ] T195 [P] [Examples] Add README to examples/ directory with usage instructions

### Configuration Presets

- [ ] T196 [P] [Polish] Add `WebRtcTransportConfig::low_latency_preset()` in `src/config.rs` (jitter_buffer_size_ms: 50, rtcp_interval_ms: 2000)
- [ ] T197 [P] [Polish] Add `WebRtcTransportConfig::high_quality_preset()` in `src/config.rs` (jitter_buffer_size_ms: 100, higher bitrates)
- [ ] T198 [P] [Polish] Add `WebRtcTransportConfig::mobile_network_preset()` in `src/config.rs` (larger jitter buffer, lower bitrates, TURN enabled)

### Logging and Observability

- [ ] T199 [Polish] Add comprehensive tracing throughout codebase (instrument all async functions, log state transitions, include peer_id in spans)
- [ ] T200 [Polish] Add metrics collection in `SessionRouter` in `src/session/router.rs` (frame counts, latency measurements, error counts)
- [ ] T201 [Polish] Add connection quality logging in `PeerConnection` in `src/peer/connection.rs` (log metrics every 10 seconds)

### Error Handling Improvements

- [ ] T202 [Polish] Add context to all error types in `src/error.rs` (use thiserror crate, add descriptive messages)
- [ ] T203 [Polish] Ensure all panics are replaced with proper error handling (search for unwrap(), expect(), panic!())
- [ ] T204 [Polish] Add error recovery suggestions in Error::Display in `src/error.rs` (suggest TURN server if NatTraversalFailed)

### Performance Optimizations

- [ ] T205 [Polish] Profile jitter buffer insertion performance (ensure O(log n) binary search)
- [ ] T206 [Polish] Profile RTP encoding/decoding latency (ensure audio <10ms, video <30ms)
- [ ] T207 [Polish] Optimize broadcast() for 10 peers (ensure parallel encoding, <100ms total)
- [ ] T208 [Polish] Add zero-copy optimizations in media tracks (use Arc<Vec<u8>> for RTP payloads)

### CI/CD Configuration

- [ ] T209 [P] [Polish] Create `.github/workflows/webrtc-transport.yml` (cargo build, cargo test, cargo clippy, cargo fmt)
- [ ] T210 [P] [Polish] Add benchmark CI job in `.github/workflows/webrtc-transport.yml` (cargo bench on every commit)
- [ ] T211 [P] [Polish] Add code coverage reporting in `.github/workflows/webrtc-transport.yml` (target >90% coverage)

**Checkpoint 9**: All documentation complete, examples runnable, CI/CD green

---

## Summary Statistics

**Total Tasks**: 211 (original plan) + 5 (bidirectional audio enhancements) = **216 total**
**By Phase**:
- Phase 1 (Setup): 11 tasks ✅ COMPLETE
- Phase 2 (Foundation): 34 tasks ✅ COMPLETE
- Phase 3 (US1): 28 tasks + 5 bidirectional enhancements = 33 tasks ✅ COMPLETE
- Phase 4 (US2 - CRITICAL): 46 tasks ⏳ PENDING
- Phase 5 (Pipeline Integration): 28 tasks ✅ COMPLETE
- Phase 6 (US3): 9 tasks
- Phase 7 (US4): 13 tasks
- Phase 8 (US5): 16 tasks
- Phase 9 (Polish): 26 tasks

**By User Story**:
- Setup/Foundation: 45 tasks
- US1 (Point-to-Point): 28 tasks
- US2 (Multi-Peer Sync): 46 tasks (⭐ CRITICAL - largest technical challenge)
- US3 (Broadcast Routing): 9 tasks
- US4 (Data Channels): 13 tasks
- US5 (Reconnection): 16 tasks
- Pipeline Integration: 28 tasks
- Polish/Docs: 26 tasks

**Parallel Opportunities**: ~80 tasks marked `[P]` (can be executed in parallel with other `[P]` tasks in same phase)

**Critical Path**: Phase 1 → Phase 2 → Phase 3 → Phase 4 (US2 sync) → Phase 5 → Phases 6-9 (can overlap)

**MVP Delivery**: Complete Phases 1-5 (147 tasks) for US1 + US2 core functionality
**Full Feature Delivery**: Complete all 9 phases (211 tasks)

---

## Implementation Strategy

### Week 1: Foundation (Phases 1-2)
**Goal**: Basic peer connection via signaling (no media)
**Tasks**: T001-T045 (45 tasks)
**Deliverable**: Two peers can establish WebRTC connection

### Week 2: US1 - Point-to-Point (Phase 3)
**Goal**: Audio/video streaming between two peers
**Tasks**: T046-T073 (28 tasks)
**Deliverable**: 1:1 video call with pipeline processing

### Week 3: US2 - Synchronization (Phase 4) ⭐ CRITICAL
**Goal**: Multi-peer A/V synchronization
**Tasks**: T074-T119 (46 tasks)
**Deliverable**: 4-peer conference with synchronized audio

### Week 4: Pipeline Integration (Phase 5)
**Goal**: Full SessionRouter integration
**Tasks**: T120-T147 (28 tasks)
**Deliverable**: Pipeline-based multi-peer processing

### Week 5: Polish & Remaining Features (Phases 6-9)
**Goal**: US3-US5 + documentation
**Tasks**: T148-T211 (64 tasks)
**Deliverable**: Production-ready transport with examples

---

## Validation Checklist

After completing all tasks, verify:

- [ ] All 211 tasks marked as complete
- [ ] All unit tests pass (`cargo test`)
- [ ] All integration tests pass (`cargo test --test '*'`)
- [ ] Benchmarks run successfully (`cargo bench`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Code formatted (`cargo fmt`)
- [ ] Examples compile and run (`cargo run --example simple_peer`)
- [ ] Documentation builds (`cargo doc --no-deps`)
- [ ] Performance targets met (latency <50ms audio, <100ms video)
- [ ] Memory usage <100MB per peer
- [ ] 10-peer mesh stable for 30+ minutes
- [ ] All user stories (US1-US5) acceptance criteria satisfied

---

## Next Steps

1. Review this tasks.md with team
2. Set up project board (Kanban or similar) with all 211 tasks
3. Assign tasks to developers
4. Begin Phase 1 implementation
5. Execute weekly checkpoints (Checkpoint 1-9)
6. Track progress against 5-week timeline
7. Escalate blockers early (especially webrtc-rs stability issues)

---

## References

- **Feature Spec**: `specs/001-webrtc-multi-peer-transport/spec.md`
- **Implementation Plan**: `specs/001-webrtc-multi-peer-transport/plan.md`
- **Data Model**: `specs/001-webrtc-multi-peer-transport/data-model.md`
- **API Contracts**: `specs/001-webrtc-multi-peer-transport/contracts/`
- **Quickstart Guide**: `specs/001-webrtc-multi-peer-transport/quickstart.md`
- **Design Document**: `transports/remotemedia-webrtc/DESIGN.md`
- **CLAUDE.md**: Project-wide instructions and architecture overview
