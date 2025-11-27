# WebRTC Server Implementation - Documentation Index

This directory contains comprehensive documentation of the RemoteMedia WebRTC server implementation.

## Documents

### 1. WEBRTC_SERVER_ANALYSIS.md (36 KB, 1074 lines)

**Complete Technical Deep-Dive** - Start here for comprehensive understanding

Topics covered:
- Section 1: Entry Point (webrtc_server.rs) - Binary initialization and modes
- Section 2: gRPC Signaling Service architecture overview
- Section 3: Request handling flows (announce, offer, ICE, answer)
- Section 4: ServerPeer - Server-side peer management
- Section 5: WebRTC peer connection wrapper
- Section 6: Audio track management
- Section 7: Audio codec (Opus) support
- Section 8: Complete data flow summary
- Section 9: Configuration parameters
- Section 10: Error handling strategies
- Section 11: Critical design patterns
- Section 12: File summary table
- Section 13: Feature flags

**Use this document when:**
- Learning the system architecture
- Understanding specific request flows
- Debugging complex interactions
- Implementing new features

---

### 2. WEBRTC_SERVER_QUICK_REFERENCE.md (8 KB, 247 lines)

**Quick Lookup Guide** - Use for rapid reference

Topics covered:
- Entry points table
- Core classes summary (WebRtcSignalingService, ServerPeer, PeerConnection, AudioTrack)
- Request flow diagram
- Key algorithms (media routing loop, audio encoding flow)
- Critical design decisions table
- Codec configuration details
- Error handling matrix
- Configuration options
- Data channel protocol
- State transitions
- Performance characteristics
- Common issues and solutions
- File organization tree

**Use this document when:**
- Looking up a specific class or method
- Checking configuration options
- Troubleshooting common issues
- Reviewing architecture decisions
- Quick API reference

---

## Source Files Analyzed

All source files are located in: `transports/webrtc/src/`

### Entry Point
```
src/bin/webrtc_server.rs (287 lines)
  - Binary entry point
  - Mode selection (gRPC vs WebSocket)
  - Shutdown handling
  - Configuration parsing
```

### Signaling Service
```
src/signaling/grpc/service.rs (866 lines)
  - WebRtcSignalingService class
  - Peer registry management
  - Request routing (announce, offer, ICE, answer)
  - Broadcasting notifications
```

### Server-Side Peer Management
```
src/peer/server_peer.rs (478 lines)
  - ServerPeer class
  - Offer handling
  - Media routing setup
  - Bidirectional data flow
  - ICE candidate handling
```

### WebRTC Connection
```
src/peer/connection.rs (614 lines)
  - PeerConnection wrapper
  - Track management
  - State tracking
  - SDP creation/parsing
```

### Media Components
```
src/media/tracks.rs (463 lines)
  - AudioTrack class
  - VideoTrack class
  - RTP sample handling
  - Encoding/decoding interface

src/media/audio.rs (211 lines)
  - AudioEncoder (Opus)
  - AudioDecoder (Opus)
  - Codec configuration
```

### Library Organization
```
src/lib.rs (~100 lines)
  - Module organization
  - Public API exports
  - Feature gates
```

---

## Quick Navigation

### By Topic

**Understanding the Overall Architecture:**
- Start with WEBRTC_SERVER_ANALYSIS.md Section 1 (Entry Point)
- Then WEBRTC_SERVER_ANALYSIS.md Section 2 (gRPC Signaling Service)
- Review WEBRTC_SERVER_QUICK_REFERENCE.md Request Flow Diagram

**Implementing Signaling:**
- WEBRTC_SERVER_ANALYSIS.md Section 3 (Request Handling)
- WEBRTC_SERVER_QUICK_REFERENCE.md Request Flow Diagram
- Source: src/signaling/grpc/service.rs

**Handling WebRTC Connections:**
- WEBRTC_SERVER_ANALYSIS.md Section 4 (ServerPeer)
- WEBRTC_SERVER_ANALYSIS.md Section 5 (PeerConnection)
- WEBRTC_SERVER_QUICK_REFERENCE.md Core Classes section

**Audio Streaming:**
- WEBRTC_SERVER_ANALYSIS.md Section 6 (Audio Tracks)
- WEBRTC_SERVER_ANALYSIS.md Section 7 (Opus Codec)
- WEBRTC_SERVER_QUICK_REFERENCE.md Codec Configuration

**Media Routing & Data Flow:**
- WEBRTC_SERVER_ANALYSIS.md Section 4.3 (setup_media_routing_and_data_channel)
- WEBRTC_SERVER_ANALYSIS.md Section 8 (Data Flow Summary)
- WEBRTC_SERVER_QUICK_REFERENCE.md Key Algorithms

**Configuration & Deployment:**
- WEBRTC_SERVER_ANALYSIS.md Section 9 (Configuration)
- WEBRTC_SERVER_QUICK_REFERENCE.md Configuration Options
- WEBRTC_SERVER_QUICK_REFERENCE.md Performance Characteristics

**Troubleshooting:**
- WEBRTC_SERVER_QUICK_REFERENCE.md Common Issues & Solutions
- WEBRTC_SERVER_ANALYSIS.md Section 10 (Error Handling)

### By Implementation Phase

**Phase 1: Understand the System**
1. Read: WEBRTC_SERVER_QUICK_REFERENCE.md (entire document)
2. Read: WEBRTC_SERVER_ANALYSIS.md Sections 1-2

**Phase 2: Deep Dive into Components**
1. Read: WEBRTC_SERVER_ANALYSIS.md Sections 3-7
2. Review relevant source files

**Phase 3: Implement Features**
1. Reference: WEBRTC_SERVER_QUICK_REFERENCE.md for design patterns
2. Reference: WEBRTC_SERVER_ANALYSIS.md sections for detailed explanations
3. Review source code for implementation details

**Phase 4: Debug Issues**
1. Check: WEBRTC_SERVER_QUICK_REFERENCE.md Common Issues & Solutions
2. Reference: WEBRTC_SERVER_ANALYSIS.md Section 10 (Error Handling)
3. Trace through data flow (Section 8)

---

## Key Concepts Reference

### Classes

| Class | File | Purpose |
|-------|------|---------|
| WebRtcSignalingService | service.rs | gRPC signaling broker |
| ServerPeer | server_peer.rs | Server-side peer with pipeline integration |
| PeerConnection | connection.rs | WebRTC peer connection wrapper |
| AudioTrack | tracks.rs | Audio encoding/decoding and RTP |
| AudioEncoder | audio.rs | Opus encoder |
| AudioDecoder | audio.rs | Opus decoder |

### Request Types

| Request | Handler | Purpose |
|---------|---------|---------|
| AnnounceRequest | handle_announce() | Peer registration |
| OfferRequest | handle_offer() | SDP offer exchange |
| IceCandidateRequest | handle_ice_candidate() | ICE connectivity |
| AnswerRequest | handle_answer() | SDP answer (P2P) |
| DisconnectRequest | handle_disconnect() | Peer cleanup |
| ListPeersRequest | handle_list_peers() | List connected peers |

### Data Structures

| Structure | Purpose |
|-----------|---------|
| PeerConnection | Peer registry entry |
| PendingOffer | Offer storage |
| ServerPeer | Server-side peer state |
| AudioTrack | Audio track state |

---

## Architecture Highlights

### Request Flow
```
Announce → Register peer
    ↓
Offer → Create ServerPeer → Add audio track → Setup media routing → Generate answer
    ↓
ICE Candidates → Add to WebRTC connection → Enable connectivity
    ↓
Media Flow ↔ RTP audio, Data channel messages, Pipeline I/O
```

### Media Flow
```
WebRTC Client
    ↓ (RTP Opus)
ServerPeer.on_track() → Decode Opus → RuntimeData::Audio
    ↓ (to pipeline)
Pipeline Processing
    ↓ (from pipeline)
RuntimeData::Audio → Encode Opus → RTP packets
    ↓ (to client)
WebRTC Client
```

### Design Patterns

1. **Arc<ServerPeer> Registry** - Multiple references for shared ownership
2. **Response Channels** - Push notifications via mpsc::Sender
3. **Async Tasks** - Non-blocking bidirectional I/O
4. **tokio::select! with biased** - Prevent deadlocks
5. **Timeout-Based Polling** - Avoid blocking on empty outputs

---

## Configuration

### Server Launch
```bash
cargo run --bin webrtc_server --features grpc-signaling -- \
  --mode grpc \
  --grpc-address 0.0.0.0:50051 \
  --manifest ./examples/loopback.yaml \
  --stun-servers stun:stun.l.google.com:19302 \
  --max-peers 20 \
  --enable-data-channel true \
  --jitter-buffer-ms 100
```

### Audio Codec
- **Format**: Opus
- **Sample Rate**: 48000 Hz (WebRTC standard)
- **Channels**: 1 (mono)
- **Bitrate**: 64 kbps (voice quality)
- **Complexity**: 10 (maximum)
- **Frame Size**: 20ms (960 samples @ 48kHz)

---

## Performance Metrics

| Metric | Value | Notes |
|--------|-------|-------|
| Max peers | 10 (configurable) | Default limit |
| Audio latency | 50-100ms | 10ms select timeout + processing |
| Frame interval | 20ms | Standard Opus frame size |
| Bitrate per peer | 64 kbps | Opus voice quality |

---

## Feature Flags

| Flag | Purpose |
|------|---------|
| `grpc-signaling` | Enable gRPC server implementation |
| `opus-codec` | Audio codec support (always enabled) |

---

## How to Use These Documents

### First Time Learning
1. Read WEBRTC_SERVER_QUICK_REFERENCE.md (10 minutes)
2. Study WEBRTC_SERVER_ANALYSIS.md Sections 1-3 (30 minutes)
3. Review source files for Section 4 (30 minutes)
4. Practice with configuration examples

### Implementing a Feature
1. Reference WEBRTC_SERVER_QUICK_REFERENCE.md classes
2. Find detailed implementation in WEBRTC_SERVER_ANALYSIS.md
3. Review source code for exact implementation
4. Check design patterns section for guidance

### Debugging an Issue
1. Check WEBRTC_SERVER_QUICK_REFERENCE.md troubleshooting
2. Reference WEBRTC_SERVER_ANALYSIS.md error handling
3. Trace data flow through Section 8
4. Add logging at key points

### Finding Specific Information
- Use Markdown table of contents
- Ctrl+F for class names or keywords
- Check "By Topic" section above
- Reference quick reference for fast lookup

---

## Document Maintenance

These documents are comprehensive and cover the WebRTC server implementation thoroughly. They should be updated when:

1. Major refactoring changes the architecture
2. New request types are added
3. Audio codec changes
4. Error handling changes
5. Configuration options change

For questions or clarifications, refer to the source code files listed above.

---

Last Updated: November 13, 2025
Analysis Coverage: ~2500 lines of Rust source code
Documentation: 1321 lines across 2 documents
