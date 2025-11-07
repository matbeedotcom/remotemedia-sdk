# RemoteMedia WebRTC Transport

**Status**: 🚧 Placeholder - Not Yet Implemented

## Overview

This crate will provide WebRTC-based real-time media streaming transport for RemoteMedia pipelines.

## Planned Features

- **Real-time Communication**: WebRTC peer-to-peer connections
- **Low Latency**: Sub-100ms audio/video streaming
- **Browser Support**: Direct browser-to-server communication
- **NAT Traversal**: ICE/STUN/TURN support
- **Data Channels**: For pipeline control and results
- **Media Tracks**: For audio/video streaming

## Architecture (Planned)

```
┌─────────────────────────────────────────────────────┐
│  Browser/Native Client                              │
│  ├─ WebRTC PeerConnection                           │
│  ├─ Data Channels (pipeline I/O)                    │
│  └─ Media Tracks (audio/video)                      │
│     ↓                                                │
│  remotemedia-webrtc (Server)                        │
│  ├─ Signaling Server (WebSocket/HTTP)               │
│  ├─ ICE/STUN/TURN handling                          │
│  ├─ Data Channel → StreamSession mapping            │
│  └─ Media Track → PipelineRunner integration        │
│     ↓                                                │
│  remotemedia-runtime-core                           │
│  └─ PipelineRunner (streaming execution)            │
└─────────────────────────────────────────────────────┘
```

## Use Cases

1. **Browser-Based Real-Time Processing**
   - Speech-to-text from microphone
   - Real-time audio effects
   - Video processing with visual feedback

2. **Low-Latency Streaming**
   - Interactive voice applications
   - Live transcription
   - Real-time translation

3. **Peer-to-Peer Media**
   - Direct client-to-server processing
   - Minimal latency for interactive apps
   - NAT traversal for home networks

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

## API Design (Future)

```rust
use remotemedia_webrtc::{WebRTCServer, WebRTCConfig};
use remotemedia_runtime_core::transport::PipelineRunner;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runner = PipelineRunner::new()?;

    let config = WebRTCConfig {
        signaling_addr: "0.0.0.0:8080".parse()?,
        ice_servers: vec![
            "stun:stun.l.google.com:19302".to_string(),
        ],
        ..Default::default()
    };

    let server = WebRTCServer::new(config, runner)?;
    server.serve().await?;

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

## Timeline

- **Q1 2025**: Design phase
- **Q2 2025**: Initial implementation
- **Q3 2025**: Testing and refinement
- **Q4 2025**: Production release

## Questions?

Open an issue or discussion on the main repository to discuss WebRTC transport requirements and design decisions.
