# WebRTC Multi-Peer Transport - API Contracts

This directory contains three comprehensive API contract documents that define the public interfaces for the WebRTC Multi-Peer Transport feature.

## Document Overview

### 1. [transport-api.md](./transport-api.md) (28 KB)

**Public API Contract for WebRtcTransport**

Defines the main transport API implementing the `PipelineTransport` trait. Covers:

- **Lifecycle Methods**: `new()`, `start()`, `shutdown()`
- **Peer Management**: `connect_peer()`, `disconnect_peer()`, `list_peers()`
- **Data Routing**: `send_to_peer()`, `broadcast()`
- **Pipeline Integration**: `execute()`, `stream()`
- **Configuration**: `configure()`
- **Error Handling**: Error types, recovery patterns
- **Usage Examples**: Complete end-to-end scenarios
- **Performance Targets**: Latency, throughput, resource limits

**Audience**: Developers implementing the WebRTC transport, application developers using the transport

**Key Sections**:
- Architecture overview with component diagram
- Method signatures with parameters, return types, error conditions
- Detailed examples with Rust code
- Thread safety and async model notes
- Resource limits and performance targets

---

### 2. [signaling-protocol.md](./signaling-protocol.md) (26 KB)

**JSON-RPC 2.0 Signaling Protocol Specification**

Defines the complete signaling protocol for peer discovery, SDP exchange, and ICE candidate trickling. Covers:

- **Protocol Fundamentals**: Message format, types, error codes
- **Phase 1: Peer Discovery**: `peer.announce`, `peer.announced`
- **Phase 2: Offer/Answer**: `peer.offer`, `peer.answer` with SDP
- **Phase 3: ICE Trickle**: `peer.ice_candidate` incremental trickling
- **Phase 4: State Management**: `peer.state_changed`
- **Phase 5: Disconnect**: `peer.disconnect`, `peer.disconnected`
- **Trickle ICE Flow Diagram**: Visual timeline of connection establishment
- **Error Handling & Recovery**: Scenarios and recovery patterns
- **WebSocket Connection**: Keep-alive, reconnection, limits
- **Best Practices**: Timing, error handling, session management

**Audience**: Signaling server implementers, transport developers

**Key Sections**:
- Complete JSON examples for each message type
- Trickle ICE flow diagram showing parallel offer/answer/ICE
- Error scenarios with responses
- Connection timing and performance expectations

---

### 3. [sync-manager-api.md](./sync-manager-api.md) (35 KB)

**Synchronization Manager API for Audio/Video Alignment**

Defines the API for per-peer synchronization, jitter buffering, clock drift estimation, and timestamp management. Covers:

- **Lifecycle Methods**: `new()`, `reset()`
- **Audio Processing**: `process_audio_frame()`, `pop_next_audio_frame()`
- **Video Processing**: `process_video_frame()`, `pop_next_video_frame()`
- **Clock Synchronization**: `update_rtcp_sender_report()`, `estimate_clock_drift()`, `apply_clock_drift_correction()`
- **Synchronization Queries**: `get_sync_state()`, `get_buffer_statistics()`
- **Timestamp Conversion**: `rtp_to_wall_clock()`, `wall_clock_to_rtp()`
- **Multi-Peer Sync**: `align_with_peer()`
- **Configuration**: SyncConfig with validation rules
- **Error Recovery**: Patterns for handling discontinuities and overruns
- **Testing**: Unit and integration test examples

**Audience**: Transport developers, audio/video pipeline implementers

**Key Sections**:
- RTP/RTCP timestamp tracking and conversion
- Adaptive jitter buffer with reordering logic
- Clock drift estimation from multiple RTCP Sender Reports
- Lip-sync alignment algorithms
- Configuration examples for different network conditions
- Multi-peer synchronization coordination

---

## Usage

### For Implementation

When implementing the WebRTC transport:

1. **Start with transport-api.md**
   - Understand the public interface
   - Implement WebRtcTransport struct with all listed methods
   - Follow error handling patterns

2. **Refer to signaling-protocol.md**
   - Implement SignalingClient with JSON-RPC 2.0 protocol
   - Follow message format and timing requirements
   - Implement trickle ICE flow correctly

3. **Use sync-manager-api.md**
   - Implement per-peer SyncManager
   - Handle RTP timestamp tracking and RTCP updates
   - Implement jitter buffering and clock drift correction

### For Testing

Each contract includes:
- Unit test examples
- Integration test examples
- Mock data structures
- Testing patterns and assertions

### For Documentation

The contracts serve as:
- API reference for library users
- Contract specification for implementations
- Test specification for correctness verification
- Performance and behavior specification

---

## Related Documents

These contracts are based on and complementary to:

- **[spec.md](../spec.md)** - Feature specification with requirements, user stories, success criteria
- **[data-model.md](../data-model.md)** - Data structures and entity relationships
- **[plan.md](../plan.md)** - Implementation phases and task breakdown
- **[research.md](../../transports/remotemedia-webrtc/research.md)** - Technical research and design decisions

---

## Version Information

| Document | Version | Created | Last Updated |
|----------|---------|---------|--------------|
| transport-api.md | 1.0.0 | 2025-11-07 | 2025-11-07 |
| signaling-protocol.md | 1.0.0 | 2025-11-07 | 2025-11-07 |
| sync-manager-api.md | 1.0.0 | 2025-11-07 | 2025-11-07 |

---

## Implementation Status

These contracts are **specifications** (not implementations):
- They define WHAT to build
- They specify HOW to use the APIs
- They do NOT provide implementation code (except examples and pseudo-code)
- They are implementation-independent (any language can implement to these contracts)

---

## Key Design Principles

### 1. **Multi-Peer Mesh Topology**
- Support 10+ simultaneous peer connections (N:N)
- No central server required for media (P2P direct)
- Signaling server only for discovery and SDP exchange

### 2. **Real-Time Synchronization**
- Explicit RTP/RTCP timestamp management
- Per-peer jitter buffers (50-100ms)
- Clock drift estimation and correction
- Lip-sync alignment for audio/video

### 3. **Zero-Copy Where Possible**
- Arc-based buffer management
- Shared memory (iceoryx2) for multiprocess
- Minimize memory allocations in media path

### 4. **Standard Protocols**
- WebRTC compatible (DTLS-SRTP, VP9/H.264, Opus)
- JSON-RPC 2.0 for signaling
- RTCP for synchronization

### 5. **Production Ready**
- Comprehensive error handling
- Connection quality monitoring
- Automatic reconnection with backoff
- Resource cleanup guarantees

---

## Quick Reference

### Transport API Quick Reference

```rust
// Initialization
let transport = WebRtcTransport::new(config)?;
transport.start().await?;

// Peer Management
let peer = transport.connect_peer("alice").await?;
transport.send_to_peer("alice", &audio_data).await?;
let stats = transport.broadcast(&audio_data).await?;
transport.disconnect_peer("alice").await?;

// Pipeline
let output = transport.execute(manifest, input).await?;
let mut session = transport.stream(manifest).await?;
```

### Signaling Protocol Quick Reference

```json
// Announce
{"jsonrpc": "2.0", "method": "peer.announce", "params": {"peer_id": "...", "capabilities": [...]}, "id": 1}

// Offer/Answer (trickle ICE)
{"jsonrpc": "2.0", "method": "peer.offer", "params": {"from": "...", "to": "...", "sdp": "...", "can_trickle_ice_candidates": true}, "id": 2}

// ICE Candidate (fire-and-forget)
{"jsonrpc": "2.0", "method": "peer.ice_candidate", "params": {"from": "...", "to": "...", "candidate": "...", "sdp_m_line_index": 0}}

// Disconnect
{"jsonrpc": "2.0", "method": "peer.disconnect", "params": {"from": "...", "to": "...", "reason": "user_requested"}}
```

### Sync Manager Quick Reference

```rust
// Initialize
let sync = SyncManager::new("peer-id", config)?;

// Process media
let synced_audio = sync.process_audio_frame(frame)?;
let synced_video = sync.process_video_frame(frame)?;

// Update from RTCP
sync.update_rtcp_sender_report(rtcp_sr)?;

// Query state
if let Some(drift) = sync.estimate_clock_drift() {
    sync.apply_clock_drift_correction(drift.correction_factor)?;
}

// Convert timestamps
let wall_clock_us = sync.rtp_to_wall_clock(rtp_ts)?;
```

---

## See Also

- [Feature Specification](../spec.md)
- [Data Model](../data-model.md)
- [Implementation Plan](../plan.md)
- [Research & Technical Decisions](../../transports/remotemedia-webrtc/research.md)
- [Custom Transport Guide](../../docs/CUSTOM_TRANSPORT_GUIDE.md)

---

Generated: 2025-11-07
