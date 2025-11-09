# Data Model: WebRTC Multi-Peer Transport

**Feature**: WebRTC Multi-Peer Transport
**Branch**: `001-webrtc-multi-peer-transport`
**Created**: 2025-11-07

## Overview

This document defines the core data structures and entities for the WebRTC Multi-Peer Transport implementation. These entities represent the logical data model and their relationships, independent of implementation details.

## Core Entities

### 1. WebRtcTransport

**Purpose**: Main transport implementation coordinating all WebRTC functionality

**Attributes**:
- `transport_id`: Unique identifier for this transport instance
- `config`: Configuration settings (STUN/TURN servers, codec preferences, limits)
- `signaling_client`: Connection to signaling server
- `peer_manager`: Manages mesh of peer connections
- `session_manager`: Tracks active streaming sessions
- `pipeline_runner`: RemoteMedia pipeline executor

**Relationships**:
- HAS-MANY `PeerConnection` via `peer_manager`
- HAS-MANY `StreamSession` via `session_manager`
- USES-ONE `SignalingClient`
- USES-ONE `PipelineRunner`

**State Transitions**:
```
Created → Connected (signaling) → Ready → Shutdown
```

**Validation Rules**:
- Must have at least one STUN server configured
- Max peers must be > 0 and ≤ 10
- Codec preferences must include at least one audio and one video codec

---

### 2. PeerConnection

**Purpose**: Represents a WebRTC connection to a specific remote peer

**Attributes**:
- `peer_id`: Unique identifier for remote peer
- `connection_id`: Unique ID for this specific connection instance
- `state`: Connection state (New, Connecting, Connected, Failed, Closed)
- `local_sdp`: Local SDP offer/answer
- `remote_sdp`: Remote SDP offer/answer
- `ice_candidates`: Collected ICE candidates
- `audio_track`: Audio media track (optional)
- `video_track`: Video media track (optional)
- `data_channel`: Reliable data channel (optional)
- `sync_manager`: Audio/video synchronization manager for this peer
- `connection_quality`: Metrics (latency, packet loss, bandwidth)
- `created_at`: Timestamp when connection initiated
- `connected_at`: Timestamp when fully connected (optional)

**Relationships**:
- BELONGS-TO `PeerManager`
- HAS-ONE `SyncManager`
- HAS-MANY `MediaTrack` (audio, video)
- HAS-ONE `DataChannel` (optional)
- PARTICIPATES-IN-MANY `StreamSession`

**State Transitions**:
```
New → Gathering ICE → Connecting → Connected → [Failed | Closed]
                          ↓
                    (auto-reconnect) → Reconnecting → Connected
```

**Validation Rules**:
- `peer_id` must be unique within a transport instance
- Must have either audio_track OR video_track OR data_channel (at least one)
- Cannot transition to Connected without remote_sdp
- ICE candidates must be valid URI format

---

### 3. StreamSession

**Purpose**: Represents an active streaming session with a specific pipeline

**Attributes**:
- `session_id`: Unique session identifier (session-scoped)
- `manifest`: RemoteMedia pipeline manifest
- `created_at`: Session start timestamp
- `state`: Session state (Initializing, Active, Paused, Closed)
- `connected_peers`: Set of peer IDs in this session
- `input_sequence`: Input sequence counter (for ordering)
- `output_sequence`: Output sequence counter
- `router`: SessionRouter for data flow management

**Relationships**:
- BELONGS-TO `SessionManager`
- ROUTES-TO-MANY `PeerConnection`
- EXECUTES-ONE `Manifest` (pipeline definition)
- USES-ONE `SessionRouter`

**State Transitions**:
```
Initializing → Active → [Paused ⇄ Active] → Closed
```

**Validation Rules**:
- `session_id` must be unique across all active sessions
- Must have valid manifest before transitioning to Active
- Connected peers must reference valid PeerConnection instances
- Cannot close session with pending outputs

---

### 4. SyncManager

**Purpose**: Manages audio/video synchronization for a specific peer

**Attributes**:
- `peer_id`: Associated peer identifier
- `audio_clock`: RTP clock state for audio (48kHz reference)
- `video_clock`: RTP clock state for video (90kHz reference)
- `audio_jitter_buffer`: Jitter buffer for audio frames (50-100ms)
- `video_jitter_buffer`: Jitter buffer for video frames (50-100ms)
- `clock_drift_estimator`: Tracks sender/receiver clock drift
- `ntp_mapping`: NTP-to-RTP timestamp mappings from RTCP
- `last_rtcp_time`: Timestamp of last RTCP Sender Report
- `sync_offset`: Calculated offset to align audio and video

**Relationships**:
- BELONGS-TO `PeerConnection`
- HAS-ONE `ClockDriftEstimator`
- HAS-ONE `JitterBuffer` (audio)
- HAS-ONE `JitterBuffer` (video)

**State Transitions**:
```
Unsynced → Syncing → Synced → [Drift Detected] → Resyncing → Synced
```

**Validation Rules**:
- Audio clock rate must be 48kHz (Opus standard)
- Video clock rate must be 90kHz (RTP standard)
- Jitter buffer size must be 50-200ms
- NTP mapping must be updated every 5 seconds via RTCP
- Clock drift must be monitored every 10 seconds

---

### 5. JitterBuffer

**Purpose**: Reorders and buffers media frames to handle network jitter

**Attributes**:
- `buffer_size_ms`: Target buffer duration (50-100ms)
- `frames`: Ordered queue of buffered frames
- `head_sequence`: Sequence number of next frame to output
- `last_received_sequence`: Highest received sequence number
- `packet_loss_count`: Count of detected lost packets
- `late_arrival_count`: Count of packets that arrived too late
- `playout_delay_ms`: Current measured playout delay

**Relationships**:
- BELONGS-TO `SyncManager`
- STORES-MANY `MediaFrame`

**State Transitions**:
```
Empty → Buffering → Stable → [Underrun | Overrun] → Adjusting → Stable
```

**Validation Rules**:
- Buffer size must be 50-200ms
- Frames must be ordered by RTP sequence number
- Must handle sequence number wraparound (16-bit)
- Late packets (>200ms old) should be discarded
- Must emit frames in sequence order

---

### 6. MediaFrame

**Purpose**: Container for a single audio or video frame with metadata

**Attributes**:
- `frame_type`: Audio or Video
- `rtp_timestamp`: RTP timestamp from WebRTC
- `sequence_number`: RTP sequence number (16-bit, wraps)
- `payload`: Encoded media data (Opus, VP9, or H.264)
- `received_at`: Local timestamp when frame received
- `marker_bit`: RTP marker bit (frame boundary indicator)
- `ssrc`: Synchronization source identifier

**Relationships**:
- STORED-IN `JitterBuffer`
- DECODED-BY `MediaCodec`

**Validation Rules**:
- RTP timestamp must be valid for codec clock rate (48kHz audio, 90kHz video)
- Sequence numbers must be continuous (allowing for wraparound)
- Payload must be valid encoded data for specified codec
- Audio frames typically 20ms duration (960 samples @ 48kHz)
- Video frames typically 33ms duration (30fps)

---

### 7. ClockDriftEstimator

**Purpose**: Tracks and estimates clock drift between sender and receiver

**Attributes**:
- `peer_id`: Associated peer identifier
- `sample_count`: Number of observations collected
- `sender_rate`: Estimated sender clock rate (Hz)
- `receiver_rate`: Local receiver clock rate (Hz)
- `drift_ppm`: Estimated drift in parts per million (±100-1000 ppm typical)
- `last_observation_time`: Timestamp of last observation
- `correction_factor`: Calculated sample rate adjustment factor

**Relationships**:
- BELONGS-TO `SyncManager`

**State Transitions**:
```
Initializing → Collecting Samples → Stable → [Drift Detected] → Adjusting → Stable
```

**Validation Rules**:
- Requires minimum 10 observations before estimating drift
- Observations should be spaced ~10 seconds apart
- Drift estimates should be updated incrementally (low-pass filter)
- Correction factor must be limited to ±1% to avoid abrupt changes
- Must handle clock rate resets (e.g., after reconnection)

---

### 8. SignalingMessage

**Purpose**: JSON-RPC 2.0 message for signaling protocol

**Attributes**:
- `jsonrpc`: Protocol version ("2.0")
- `method`: RPC method name (e.g., "peer.offer", "peer.answer")
- `params`: Method parameters (varies by method)
- `id`: Request ID for request/response matching (optional)

**Message Types**:

**peer.announce**:
```json
{
  "peer_id": "string",
  "capabilities": ["audio", "video", "data"]
}
```

**peer.offer**:
```json
{
  "from": "sender_peer_id",
  "to": "receiver_peer_id",
  "sdp": "v=0\r\n..."
}
```

**peer.answer**:
```json
{
  "from": "responder_peer_id",
  "to": "offerer_peer_id",
  "sdp": "v=0\r\n..."
}
```

**peer.ice_candidate**:
```json
{
  "from": "sender_peer_id",
  "to": "receiver_peer_id",
  "candidate": "candidate:..."
}
```

**peer.disconnect**:
```json
{
  "peer_id": "string"
}
```

**Validation Rules**:
- `jsonrpc` must be "2.0"
- `method` must be a valid signaling method
- `from`/`to` peer IDs must be non-empty strings
- SDP must be valid SDP format
- ICE candidates must be valid URI format

---

### 9. TransportConfig

**Purpose**: Configuration settings for WebRtcTransport

**Attributes**:
- `signaling_url`: WebSocket signaling server URL (ws:// or wss://)
- `stun_servers`: List of STUN server URLs
- `turn_servers`: List of TURN server configurations (optional)
- `peer_id`: Local peer ID (auto-generated if None)
- `max_peers`: Maximum peers in mesh (default: 10, max: 10)
- `audio_codec`: Audio codec preference (default: Opus)
- `video_codec`: Video codec preference (default: VP9)
- `enable_data_channel`: Enable data channel (default: true)
- `data_channel_mode`: Reliable/Ordered or Unreliable (default: Reliable)
- `jitter_buffer_size_ms`: Jitter buffer target size (default: 50ms)
- `enable_rtcp`: Enable RTCP Sender Reports (default: true, required for sync)
- `rtcp_interval_ms`: RTCP report interval (default: 5000ms)

**Validation Rules**:
- `signaling_url` must be valid WebSocket URL
- `stun_servers` must have at least one entry
- `max_peers` must be 1-10
- `jitter_buffer_size_ms` must be 50-200
- `rtcp_interval_ms` must be 1000-10000 (1-10 seconds)
- `audio_codec` must be Opus (only supported audio codec)
- `video_codec` must be VP9 or H264

---

### 10. ConnectionQualityMetrics

**Purpose**: Tracks connection quality metrics for a peer

**Attributes**:
- `peer_id`: Associated peer identifier
- `latency_ms`: Round-trip time (RTT) in milliseconds
- `packet_loss_rate`: Packet loss percentage (0.0-100.0)
- `jitter_ms`: Packet arrival jitter in milliseconds
- `bandwidth_kbps`: Estimated bandwidth in kilobits per second
- `video_resolution`: Current video resolution (e.g., "720p")
- `video_framerate`: Current video framerate (fps)
- `audio_bitrate_kbps`: Current audio bitrate
- `video_bitrate_kbps`: Current video bitrate
- `updated_at`: Timestamp of last metric update

**Relationships**:
- BELONGS-TO `PeerConnection`

**Validation Rules**:
- Metrics should be updated every 1-5 seconds
- Latency should be calculated from RTCP reports
- Packet loss rate should trigger bitrate adjustment if >5%
- Bandwidth estimates should use sliding window (last 10 seconds)

---

## Data Flow Diagrams

### Incoming Media Processing

```
Remote Peer
  │
  ├─ Audio RTP Packet (seq=N, ts=T1, payload=opus_data)
  │    ↓
  │  PeerConnection.audio_track.on_rtp()
  │    ↓
  │  SyncManager.process_audio_frame()
  │    ↓
  │  JitterBuffer.insert(frame)  [reorder, buffer 50ms]
  │    ↓
  │  JitterBuffer.pop_next()  [when ready]
  │    ↓
  │  OpusDecoder.decode(opus_data) → Vec<f32>
  │    ↓
  │  Arc::new(RuntimeData::Audio { samples, sample_rate, channels })
  │    ↓
  │  SessionRouter.route_to_pipeline(session_id, data)
  │    ↓
  │  PipelineRunner.execute_streaming(manifest, data)
  │    ↓
  │  Arc<RuntimeData> (processed output)
  │    ↓
  │  SessionRouter.route_to_peers(output, target_peers)
  │    ↓
  │  OpusEncoder.encode(samples) → Vec<u8>
  │    ↓
  │  PeerConnection.audio_track.write_rtp(payload, new_ts, new_seq)
  │    ↓
  └─ Remote Peer(s)
```

### Multi-Peer Synchronization

```
Peer A                    Peer B                    Peer C
  │                         │                         │
  ├─ Audio (seq=10, ts=480)─┼─ Audio (seq=15, ts=720)─┤
  ├─ Video (seq=5, ts=900) ─┼─ Video (seq=7, ts=1260)─┤
  │                         │                         │
  ↓                         ↓                         ↓
SyncManager A             SyncManager B             SyncManager C
  │                         │                         │
  ├─ audio_jitter_buffer ───┼─ audio_jitter_buffer ───┤
  ├─ video_jitter_buffer ───┼─ video_jitter_buffer ───┤
  ├─ clock_drift_estimator ─┼─ clock_drift_estimator ─┤
  │                         │                         │
  ↓ (align A/V)             ↓ (align A/V)             ↓ (align A/V)
  │                         │                         │
  └───────────────> SessionRouter.collect_aligned_frames()
                           │
                           ↓
                    PipelineRunner (process all peers together)
                           │
                           ↓
                    Route outputs to all peers
```

---

## Key Invariants

1. **Session Isolation**: Each `session_id` must create independent channel namespaces (no cross-session data leakage)

2. **RTP Timestamp Continuity**: RTP timestamps must increment monotonically per stream (allowing for wraparound)

3. **Jitter Buffer Ordering**: Frames in jitter buffer must be strictly ordered by RTP sequence number

4. **Clock Drift Bounds**: Clock drift correction must be limited to ±1% adjustment per update to avoid audio glitches

5. **RTCP Synchronization**: NTP/RTP mappings must be updated at least every 5 seconds for accurate A/V sync

6. **Peer Limit**: Maximum 10 peers per session (mesh topology constraint)

7. **Zero-Copy Paths**: Media data must use Arc<RuntimeData> references where possible (no unnecessary copies)

8. **State Consistency**: PeerConnection state must reflect actual WebRTC connection state (no stale states)

9. **Resource Cleanup**: All resources (connections, sessions, buffers) must be cleaned up within 1 second of session termination

10. **Sequence Number Wraparound**: All code handling RTP sequence numbers must correctly handle 16-bit wraparound (0xFFFF → 0x0000)

---

## Index and Query Patterns

### Lookup Patterns

1. **Find peer by ID**: `peer_manager.get_peer(peer_id) → Option<PeerConnection>`
2. **Find session by ID**: `session_manager.get_session(session_id) → Option<StreamSession>`
3. **Find sync manager for peer**: `peer_connection.sync_manager → SyncManager`
4. **List active sessions**: `session_manager.list_active() → Vec<SessionId>`
5. **List connected peers**: `peer_manager.list_connected() → Vec<PeerId>`
6. **Query connection quality**: `peer_connection.get_metrics() → ConnectionQualityMetrics`

### Iteration Patterns

1. **Route to all peers in session**:
   ```rust
   for peer_id in session.connected_peers {
       let peer = peer_manager.get_peer(peer_id)?;
       peer.send(data).await?;
   }
   ```

2. **Collect aligned frames from all peers**:
   ```rust
   for peer in peer_manager.list_connected() {
       let sync = peer.sync_manager;
       if let Some(frame) = sync.pop_next_aligned() {
           frames.push((peer.peer_id, frame));
       }
   }
   ```

3. **Monitor connection quality**:
   ```rust
   for peer in peer_manager.list_connected() {
       let metrics = peer.get_metrics();
       if metrics.packet_loss_rate > 5.0 {
           peer.reduce_bitrate().await?;
       }
   }
   ```

---

## Persistence and Caching

**Storage**: This transport is entirely in-memory (no persistence)

**Caching Strategy**:
- Jitter buffers cache frames for 50-100ms (automatically expire old frames)
- Clock drift estimates are cached and updated incrementally every 10 seconds
- RTCP NTP mappings are cached for 5 seconds (refreshed by Sender Reports)
- Connection quality metrics are cached for 1-5 seconds

**No Long-Term Storage**:
- No session history
- No media recording
- No connection logs (use tracing for observability)
- Sessions are transient (exist only while streaming)

---

## Version and Migration

**Initial Version**: 1.0.0

**Future Considerations**:
- Add support for simulcast (multiple quality tiers per stream)
- Add support for SFU mode (selective forwarding instead of mesh)
- Add support for screen sharing tracks
- Add support for file transfer via data channels
- Add persistent session recovery (reconnect to existing session)

**Backward Compatibility**: Not applicable (greenfield implementation)
