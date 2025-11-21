# WebRTC Server Quick Reference

## Entry Points

| File | Purpose |
|------|---------|
| `src/bin/webrtc_server.rs` | Binary entry point with mode selection |
| `src/signaling/grpc/service.rs` | gRPC signaling service (peer registry + routing) |
| `src/peer/server_peer.rs` | Server-side peer with pipeline integration |
| `src/peer/connection.rs` | WebRTC peer connection wrapper |

## Core Classes

### WebRtcSignalingService (service.rs)
- **Purpose**: gRPC signaling broker
- **Key fields**: peers, server_peers, pending_offers, config, runner, manifest
- **Main handler**: signal() - bidirectional stream processor
- **Sub-handlers**: 
  - handle_announce() - register peer
  - handle_offer() - SDP exchange
  - handle_ice_candidate() - connectivity
  - handle_answer() - P2P completion
  - handle_disconnect() - cleanup

### ServerPeer (server_peer.rs)
- **Purpose**: Server-side representation of connected client
- **Key fields**: peer_connection, runner, manifest, shutdown_tx
- **Key methods**:
  - handle_offer() - process client offer, create answer
  - setup_media_routing_and_data_channel() - bidirectional media flow
  - handle_ice_candidate() - add candidate to WebRTC
  - send_to_webrtc() - pipeline output to RTP

### PeerConnection (connection.rs)
- **Purpose**: Low-level WebRTC peer connection management
- **Key fields**: peer_connection (Arc<RTCPeerConnection>), audio_track, video_track
- **Key methods**:
  - add_audio_track() - create Opus track
  - on_track() - receive handler for remote tracks

### AudioTrack (tracks.rs)
- **Purpose**: Audio encoding/decoding and RTP transmission
- **Key methods**:
  - send_audio() - encode samples to Opus, send via RTP
  - on_rtp_packet() - decode received Opus RTP packet

## Request Flow Diagram

```
┌─────────────────────────┐
│ 1. Announce             │
│    → Register peer      │
│    → Store response tx  │
│    → Broadcast joined   │
└────────────┬────────────┘
             ↓
┌─────────────────────────┐
│ 2. Offer                │
│    → Create ServerPeer  │
│    → Create session     │
│    → Add audio track    │
│    → Set up routing     │
│    → Generate answer    │
│    → Send answer        │
└────────────┬────────────┘
             ↓
┌─────────────────────────┐
│ 3. ICE Candidates       │
│    → Lookup ServerPeer  │
│    → Add to WebRTC      │
│    → DTLS established   │
└────────────┬────────────┘
             ↓
┌─────────────────────────┐
│ 4. Bidirectional Media  │
│    ↔ RTP audio (Opus)   │
│    ↔ Data channel msgs  │
│    ↔ Pipeline I/O       │
└─────────────────────────┘
```

## Key Algorithms

### Media Routing Loop (ServerPeer)
```rust
loop {
    select! {
        biased;  // Priority: shutdown > inputs > outputs
        
        // Shutdown (highest priority)
        _ = shutdown_rx.recv() => break;
        
        // WebRTC inputs (high priority)
        Some(data) = input_rx.recv() => {
            session_handle.send_input(data).await?;
        }
        
        // Pipeline outputs (with timeout)
        Ok(Some(output)) = timeout(10ms, output_rx.recv()) => {
            send_to_webrtc(output).await?;
        }
    }
}
```

### Audio Encoding Flow
1. Get sample rate from pipeline output
2. Check if encoder needs recreation for sample rate change
3. Calculate frame size: (sample_rate * 20ms) / 1000
4. Split samples into 20ms chunks, padding last chunk
5. Encode each chunk with Opus
6. Create Sample with encoded data
7. Send via WebRTC track

## Critical Design Decisions

| Decision | Why |
|----------|-----|
| **ServerPeer per client** | Server-side representation allows pipeline integration |
| **Arc<ServerPeer> in registry** | Multiple references (signaling service + ICE handler) |
| **Async tasks for media routing** | Non-blocking bidirectional I/O |
| **tokio::select! with biased** | Prevents deadlock between inputs/outputs/shutdown |
| **10ms timeout on output** | Avoid blocking inputs while waiting for output |
| **20ms Opus frames** | Standard WebRTC audio frame size |
| **Opus @ 48kHz** | WebRTC standard, highest quality |

## Codec Configuration

### Audio (Opus)
- **Sample rate**: 48000 Hz (standard for WebRTC)
- **Channels**: 1 (mono)
- **Bitrate**: 64000 bps (voice quality)
- **Complexity**: 10 (maximum)
- **Frame size**: 20ms (960 samples @ 48kHz)
- **Application**: VOIP

## Error Handling

| Error | Handling |
|-------|----------|
| Peer already exists | Status::already_exists() |
| Peer not announced | Status::failed_precondition() |
| Target peer not found | Status::not_found() |
| ICE candidate before offer | Silently ignore + acknowledge |
| WebRTC/pipeline errors | Status::internal() |

## Configuration Options

```bash
cargo run --bin webrtc_server --features grpc-signaling -- \
  --mode grpc                           # "grpc" or "websocket"
  --grpc-address 0.0.0.0:50051         # Listen address
  --manifest ./examples/loopback.yaml  # Pipeline definition
  --stun-servers stun:...              # ICE servers
  --max-peers 10                       # Max connections
  --enable-data-channel true           # Data channel support
  --jitter-buffer-ms 100               # Buffer size
```

## Data Channel Protocol

**Message Format**: Protobuf DataBuffer

```rust
// Client sends
on_message(msg: RTCDataChannelMessage) {
    data_buffer = DataBuffer::decode(msg.data)?;
    runtime_data = adapters::to_runtime_data(data_buffer)?;
    // Forward to pipeline
}

// Server sends (pipeline output)
transport_data = session_handle.recv_output();
data_buffer = adapters::from_runtime_data(transport_data);
data_channel.send(data_buffer.encode());
```

## State Transitions

### Connection State
```
New → Connecting → Connected → Closed
                  ↓ (error)
                Failed
```

### Peer State
```
Announced → Available → Disconnected
```

## Performance Characteristics

| Metric | Value | Notes |
|--------|-------|-------|
| Max peers | 10 (configurable) | Default, can increase |
| Audio latency | ~50-100ms | 10ms timeout + processing |
| Frame interval | 20ms | Opus frame size |
| Sample rate | 48000 Hz | WebRTC standard |
| Bitrate | 64 kbps | Per connection |

## Common Issues & Solutions

### ICE Candidate Arrives Before Offer
- **Status**: Expected behavior
- **Handling**: Silently ignore candidate (error logged at WARN level)
- **Why**: Candidates may arrive in any order

### ServerPeer Not Found for ICE
- **Status**: Timing issue
- **Cause**: Candidate arrived before offer processed
- **Solution**: Client retransmits candidate

### No Audio Transmission
- **Checklist**:
  1. Audio track added (add_audio_track())
  2. Encoder sample rate matches pipeline output
  3. send_audio() called with correct sample rate
  4. RTP packets reaching client

### Data Channel Not Opening
- **Checklist**:
  1. on_data_channel handler registered
  2. Offer/answer completed
  3. ICE connected (verified)
  4. Data channel created by offerer

## File Organization

```
transports/remotemedia-webrtc/src/
├── bin/webrtc_server.rs              # Entry point
├── signaling/
│   └── grpc/service.rs                # gRPC service
├── peer/
│   ├── server_peer.rs                # Server-side peer
│   ├── connection.rs                 # WebRTC wrapper
│   └── manager.rs                    # Peer registry
├── media/
│   ├── tracks.rs                     # RTP tracks
│   ├── audio.rs                      # Opus codec
│   └── video.rs                      # VP9 codec
├── session/
│   └── router.rs                     # Session routing
└── lib.rs                            # Public API
```

