# RemoteMedia WebRTC Transport

**Status**: 🚧 In Development (Phase 1 - Setup Complete)

## Overview

This crate provides WebRTC-based real-time media streaming transport for RemoteMedia pipelines with multi-peer mesh networking support.

## Features

- **Multi-peer mesh topology**: Up to 10 simultaneous peer connections (N:N communication)
- **Audio/Video synchronization**: Per-peer sync managers with jitter buffers and clock drift estimation
- **Media codecs**: Opus audio, VP9/H264 video
- **Data channels**: Reliable/unreliable messaging modes
- **JSON-RPC 2.0 signaling**: WebSocket-based peer discovery and SDP exchange
- **Low latency**: <50ms audio, <100ms video (95th percentile target)
- **RemoteMedia pipeline integration**: Implements PipelineTransport trait

## Architecture

```
┌────────────────────────────────────────────────────────┐
│  WebRTC Peers (Browser/Native)                         │
│  ↓ (WebRTC peer connections - mesh topology)           │
│  WebRtcTransport                                       │
│  ├─ SignalingClient (JSON-RPC 2.0 over WebSocket)     │
│  ├─ PeerManager (mesh of PeerConnections)             │
│  │   └─ Per-peer SyncManager (A/V sync, jitter)       │
│  ├─ SessionManager (pipeline sessions)                 │
│  └─ SessionRouter (routes data: peers ↔ pipeline)     │
│     ↓                                                   │
│  remotemedia-runtime-core::PipelineRunner              │
└────────────────────────────────────────────────────────┘
```

### Key Components

- **SignalingClient**: WebSocket client for JSON-RPC 2.0 peer discovery and SDP exchange
- **PeerManager**: Manages mesh topology of up to 10 peer connections
- **SyncManager**: Per-peer audio/video synchronization with jitter buffers and clock drift compensation
- **SessionManager**: Tracks active streaming sessions and their peer associations
- **SessionRouter**: Routes media data between peers and RemoteMedia pipelines

## Use Cases

1. **Point-to-Point Video Processing** (Priority 1)
   - Real-time video filters with browser preview
   - Speech-to-text from microphone
   - Interactive audio effects

2. **Multi-Peer Audio Conferencing** (Priority 2)
   - Conference rooms with audio mixing
   - Up to 10 participants in mesh topology
   - Synchronized audio/video playback

3. **Broadcast Routing** (Priority 2)
   - One-to-many streaming (presenter to audience)
   - Many-to-one streaming (audience to aggregator)
   - Custom routing patterns via SessionRouter

4. **Data Channel Communication** (Priority 3)
   - Real-time text chat during calls
   - File transfer between peers
   - Custom control protocols

5. **Automatic Reconnection** (Priority 3)
   - Network interruption recovery
   - Exponential backoff retry logic
   - State preservation during reconnection

## Implementation Plan

When implementing this transport:

1. **Dependencies**
   - Add `webrtc` crate (already in workspace)
   - Consider signaling protocol (WebSocket/HTTP)
   - ICE server configuration

2. **Core Components**
   - `WebRTCServer` - Main server struct
   - `PeerConnection` - Per-client connection management
   - `SignalingServer` - SDP exchange and ICE candidates
   - `DataChannelAdapter` - Maps data channels to `StreamSession`
   - `MediaTrackAdapter` - Maps media tracks to pipeline input

3. **Integration Points**
   ```rust
   use remotemedia_runtime_core::transport::{PipelineRunner, StreamSession};

   // Create streaming session
   let session = runner.create_stream_session(manifest)?;

   // Map WebRTC data channel to session
   data_channel.on_message(|msg| {
       session.send_input(msg).await
   });

   // Forward outputs to WebRTC
   while let Some(output) = session.receive_output().await {
       data_channel.send(output).await
   }
   ```

4. **Testing Strategy**
   - Unit tests with mock peer connections
   - Integration tests with real WebRTC stack
   - Browser compatibility tests

## Quick Start

```rust
use remotemedia_webrtc::{WebRtcTransport, WebRtcTransportConfig};
use remotemedia_runtime_core::PipelineRunner;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure transport
    let config = WebRtcTransportConfig {
        signaling_url: "ws://localhost:8080".to_string(),
        stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
        max_peers: 10,
        ..Default::default()
    };

    // Validate configuration
    config.validate()?;

    // Create transport (Phase 2+ implementation)
    // let runner = PipelineRunner::new()?;
    // let transport = WebRtcTransport::new(config, runner).await?;
    //
    // // Connect to peer
    // let peer_id = transport.connect_peer("peer-abc123").await?;
    //
    // // Start streaming session with pipeline
    // let manifest = Arc::new(manifest);
    // let session = transport.stream(manifest).await?;

    Ok(())
}
```

## Related Work

- [webrtc-rs](https://github.com/webrtc-rs/webrtc) - Rust WebRTC implementation
- [gRPC Transport](../remotemedia-grpc/) - Current production transport
- [FFI Transport](../remotemedia-ffi/) - Python SDK transport

## Contributing

If you're interested in implementing WebRTC support:

1. Review the `PipelineTransport` trait in runtime-core
2. Study the gRPC transport implementation for patterns
3. Design the signaling protocol
4. Start with a minimal proof-of-concept
5. Add comprehensive tests

## Implementation Status

**Current Phase**: Phase 2 - Signaling & Peer Foundation ✅

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 | ✅ Complete | Crate structure, config, error types (12 tests) |
| Phase 2 | ✅ Complete | Signaling protocol and peer connections (39 tests) |
| Phase 3 | 📋 Planned | Point-to-point video (User Story 1) |
| Phase 4 | 📋 Planned | Multi-peer A/V synchronization (User Story 2) |
| Phase 5 | 📋 Planned | Pipeline integration |
| Phase 6+ | 📋 Planned | Additional features and polish |

See [specs/001-webrtc-multi-peer-transport/](../../specs/001-webrtc-multi-peer-transport/) for detailed implementation plan and task breakdown.

## Questions?

Open an issue or discussion on the main repository to discuss WebRTC transport requirements and design decisions.
