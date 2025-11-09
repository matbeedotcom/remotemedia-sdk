# WebRTC Multi-Peer Transport: Developer Quickstart Guide

**Last Updated**: 2025-11-07
**Target Audience**: Rust developers with WebRTC basics
**Reading Time**: 15-20 minutes
**Hands-on Time**: 30-45 minutes

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Installation (5 minutes)](#installation-5-minutes)
3. [Quick Start: 1:1 Video Call (5 minutes)](#quick-start-11-video-call-5-minutes)
4. [Configuration Guide](#configuration-guide)
5. [Common Use Cases](#common-use-cases)
6. [Pipeline Integration](#pipeline-integration)
7. [Troubleshooting](#troubleshooting)
8. [Testing](#testing)
9. [Performance Tuning](#performance-tuning)
10. [Next Steps](#next-steps)

---

## Prerequisites

### System Requirements

- **OS**: Linux, macOS, or Windows 10+
- **Rust**: 1.70+ (install via [rustup](https://rustup.rs/))
- **CPU**: Modern multi-core processor (2+ cores recommended)
- **RAM**: 4GB minimum (8GB+ recommended for multi-peer)
- **Network**: Internet connectivity (direct or via STUN/TURN)

### Verify Rust Installation

```bash
rustc --version  # Should be 1.70 or higher
cargo --version
```

### Dependencies

The transport depends on:
- `webrtc` (0.9+) - WebRTC protocol implementation
- `tokio` (1.35+) - Async runtime
- `opus` (0.3+) - Audio codec
- `serde_json` - JSON serialization for signaling

All are automatically managed by Cargo.

### Knowledge Requirements

- Basic Rust async/await syntax
- Understanding of WebRTC concepts (SDP, ICE, RTP)
- Familiarity with RemoteMedia pipeline manifests

---

## Installation (5 minutes)

### Step 1: Create a New Project

```bash
cargo new my-webrtc-app
cd my-webrtc-app
```

### Step 2: Add Dependencies to `Cargo.toml`

```toml
[package]
name = "my-webrtc-app"
version = "0.1.0"
edition = "2021"

[dependencies]
# WebRTC transport (not yet published; use path for development)
remotemedia-webrtc = { path = "../remotemedia-sdk-webrtc/transports/remotemedia-webrtc" }

# RemoteMedia runtime core
remotemedia-runtime-core = { path = "../remotemedia-sdk-webrtc/runtime-core" }

# Async runtime
tokio = { version = "1.35", features = ["full"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

[dev-dependencies]
tempfile = "3.8"
```

### Step 3: Verify Build

```bash
cargo build
```

Expected output:
```
Compiling my-webrtc-app v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in 15.23s
```

---

## Quick Start: 1:1 Video Call (5 minutes)

This is the absolute minimal example to establish a peer-to-peer connection and stream video.

### Assumptions

- You have a WebSocket signaling server running at `ws://localhost:8080`
- Peer "alice" and "bob" will connect to it
- Both peers are on the same network (no TURN needed)

### Complete Working Example

Create `src/main.rs`:

```rust
use remotemedia_webrtc::transport::{WebRtcTransport, WebRtcTransportConfig};
use remotemedia_runtime_core::data::RuntimeData;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Step 1: Configure the transport
    let config = WebRtcTransportConfig {
        signaling_url: "ws://localhost:8080".to_string(),
        peer_id: "alice".to_string(),
        stun_servers: vec![
            "stun:stun.l.google.com:19302".to_string(),
            "stun:stun1.l.google.com:19302".to_string(),
        ],
        turn_servers: vec![], // Leave empty for local network
        max_peers: 10,
        enable_data_channel: true,
        jitter_buffer_size_ms: 50,
        ..Default::default()
    };

    // Step 2: Create transport
    let transport = WebRtcTransport::new(config)?;
    println!("Transport created");

    // Step 3: Start and connect to signaling server
    transport.start().await?;
    println!("Connected to signaling server");

    // Step 4: Connect to Bob
    // (Assume Bob is already running in another terminal)
    let peer_id = transport.connect_peer("bob").await?;
    println!("Connected to peer: {}", peer_id);

    // Step 5: Wait for connection to stabilize
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Step 6: Stream dummy video frames (replace with real camera input)
    for i in 0..10 {
        // Create dummy video frame (1280x720, I420 format)
        let frame_size = 1280 * 720 * 3 / 2; // I420
        let frame_data = vec![128u8; frame_size];

        let video_data = RuntimeData::Video {
            frame: Arc::new(video_data_placeholder(frame_data)),
        };

        transport.send_to_peer("bob", &video_data).await?;
        println!("Sent frame {}", i);

        tokio::time::sleep(Duration::from_millis(33)).await; // ~30fps
    }

    // Step 7: Cleanup
    transport.disconnect_peer("bob").await?;
    transport.shutdown().await?;
    println!("Shutdown complete");

    Ok(())
}

// Placeholder for actual video frame structure
fn video_data_placeholder(data: Vec<u8>) -> impl AsRef<[u8]> {
    data
}
```

### Run the Example

Terminal 1 (Bob):
```bash
# Start signaling server (using example signaling server)
cd signaling-server
npm start
```

Terminal 2 (Alice):
```bash
cargo run
```

Expected output:
```
Transport created
Connected to signaling server
Connected to peer: bob
Sent frame 0
Sent frame 1
...
Sent frame 9
Shutdown complete
```

### What Just Happened

1. **Configuration**: Set up WebRTC with STUN servers for NAT traversal
2. **Initialization**: Created transport and connected to signaling server
3. **Peer Discovery**: Alice discovered Bob via signaling server
4. **Connection**: Established WebRTC connection with SDP exchange
5. **Streaming**: Sent 10 video frames at 30fps
6. **Cleanup**: Gracefully closed connection

**Total time**: ~5-10 seconds including connection setup

---

## Configuration Guide

### Audio Configuration

For audio-only applications:

```rust
let config = WebRtcTransportConfig {
    signaling_url: "wss://signaling.example.com".to_string(),
    peer_id: uuid::Uuid::new_v4().to_string(), // Auto-generate unique ID

    // Audio codec (Opus is mandatory in WebRTC)
    audio_codec: AudioCodec::Opus {
        sample_rate: 48_000,  // 8k, 16k, 24k, or 48k
        channels: 1,           // Mono (1) or stereo (2)
        bitrate_kbps: 32,      // 16-128 kbps typical
        complexity: 8,         // 0-10 (higher = better quality, slower)
    },

    // STUN servers for NAT
    stun_servers: vec![
        "stun:stun.l.google.com:19302".to_string(),
        "stun:stun1.l.google.com:19302".to_string(),
        "stun:stun2.l.google.com:19302".to_string(),
    ],

    // No video
    enable_video: false,

    ..Default::default()
};
```

### Video Configuration

For video with adaptive bitrate:

```rust
let config = WebRtcTransportConfig {
    signaling_url: "wss://signaling.example.com".to_string(),

    // Video codec
    video_codec: VideoCodec::VP9 {
        width: 1280,
        height: 720,
        framerate: 30,
        bitrate_kbps: 2000,  // Adaptive between 500-5000
    },

    // TURN for restrictive networks
    turn_servers: vec![
        TurnServer {
            urls: vec!["turn:turn.example.com".to_string()],
            username: "user".to_string(),
            credential: "pass".to_string(),
        }
    ],

    // Jitter buffer (balance latency vs. stability)
    jitter_buffer_size_ms: 100, // 50-200ms typical

    ..Default::default()
};
```

### STUN/TURN Configuration

**STUN** (free public servers):
```rust
stun_servers: vec![
    "stun:stun.l.google.com:19302",
    "stun:stun1.l.google.com:19302",
    "stun:stun2.l.google.com:19302",
    "stun:stun3.l.google.com:19302",
    "stun:stun4.l.google.com:19302",
],
```

**TURN** (for restrictive networks, requires credentials):
```rust
turn_servers: vec![
    TurnServer {
        urls: vec!["turn:turnserver.example.com:3478".to_string()],
        username: "user@example.com".to_string(),
        credential: "password123".to_string(),
    },
    TurnServer {
        urls: vec!["turn:turnserver2.example.com:5349".to_string()],
        username: "user".to_string(),
        credential: "secret".to_string(),
    },
],
```

### Codec Selection

```rust
// Audio: Always Opus (mandatory in WebRTC)
audio_codec: AudioCodec::Opus {
    sample_rate: 48_000,
    channels: 1,
    bitrate_kbps: 32,
    complexity: 8,
}

// Video: VP9 preferred, H.264 fallback
video_codec: VideoCodec::VP9 {
    width: 1920,
    height: 1080,
    framerate: 60,
    bitrate_kbps: 5000,
}

// Or H.264 for lower latency
video_codec: VideoCodec::H264 {
    width: 1280,
    height: 720,
    framerate: 30,
    bitrate_kbps: 2000,
}
```

### Data Channel Configuration

For control messages and coordination:

```rust
let config = WebRtcTransportConfig {
    enable_data_channel: true,
    data_channel_mode: DataChannelMode::Reliable, // vs. Unreliable
    ..Default::default()
};

// Send JSON control message
let message = DataChannelMessage::Json {
    payload: serde_json::json!({
        "command": "change_resolution",
        "width": 1920,
        "height": 1080,
    }),
};

transport.send_data_channel_message("peer-id", &message).await?;
```

---

## Common Use Cases

### Use Case 1: 1:1 Video Call with Background Blur

**Goal**: Two peers exchange video with background blur processing

```rust
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::transport::PipelineTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = WebRtcTransport::new(config)?;
    transport.start().await?;

    // Connect to peer
    transport.connect_peer("remote-peer").await?;

    // Load pipeline manifest with background blur
    let manifest = Arc::new(Manifest::from_file("blur_pipeline.yaml")?);

    // Create streaming session
    let mut session = transport.stream(manifest).await?;

    loop {
        // Receive video from peer
        let input_frame = receive_from_camera().await?;
        let data = RuntimeData::Video { frame: Arc::new(input_frame) };

        // Send through pipeline for blur
        session.send_input(TransportData::new(data)).await?;

        // Get processed output
        while let Some(output) = session.recv_output().await? {
            // Send blurred video back to peer
            transport.send_to_peer("remote-peer", &output.data).await?;
        }

        tokio::time::sleep(Duration::from_millis(33)).await; // 30fps
    }
}
```

**Pipeline manifest** (`blur_pipeline.yaml`):
```yaml
pipeline:
  nodes:
    - id: "background_blur"
      type: "BackgroundBlur"
      executor: "native"
      params:
        blur_strength: 15
        mask_padding: 10

  graph:
    - from: "input"
      to: "background_blur"
    - from: "background_blur"
      to: "output"
```

### Use Case 2: Multi-Peer Audio Conference (5 participants)

**Goal**: 5 peers mix audio and broadcast to all

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = WebRtcTransport::new(audio_config)?;
    transport.start().await?;

    // Connect to all other peers
    let peer_ids = vec!["peer-1", "peer-2", "peer-3", "peer-4"];
    for peer_id in &peer_ids {
        transport.connect_peer(peer_id).await?;
    }

    println!("Connected to {} peers", peer_ids.len());

    // Load audio mixing pipeline
    let manifest = Arc::new(Manifest::from_file("audio_mixer.yaml")?);
    let mut session = transport.stream(manifest).await?;

    loop {
        // Receive audio from all peers
        let mut audio_inputs = Vec::new();

        for peer in transport.list_peers().await? {
            if let Some(audio) = receive_audio_from_peer(&peer.peer_id).await {
                audio_inputs.push(audio);
            }
        }

        // Send mixed audio to pipeline
        let audio_mix = RuntimeData::AudioMix {
            inputs: audio_inputs,
            sample_rate: 48_000,
        };

        session.send_input(TransportData::new(audio_mix)).await?;

        // Broadcast mixed output to all peers
        while let Some(output) = session.recv_output().await? {
            let stats = transport.broadcast(&output.data).await?;
            println!("Broadcast to {} peers (failed: {})",
                     stats.sent_count, stats.failed_count);
        }
    }
}
```

**Audio mixer pipeline** (`audio_mixer.yaml`):
```yaml
pipeline:
  nodes:
    - id: "audio_mixer"
      type: "AudioMixer"
      executor: "native"
      params:
        channels: 1
        sample_rate: 48000
        mix_mode: "average"  # or "maximum"

  graph:
    - from: "input"
      to: "audio_mixer"
    - from: "audio_mixer"
      to: "output"
```

### Use Case 3: Broadcast with Selective Routing (1 source → 3 quality tiers)

**Goal**: One broadcaster sends to multiple viewers with different quality levels

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = WebRtcTransport::new(config)?;
    transport.start().await?;

    // Connect to viewers
    let viewers = vec!["viewer-1", "viewer-2", "viewer-3"];
    for viewer in &viewers {
        transport.connect_peer(viewer).await?;
    }

    // Create pipelines for different qualities
    let manifest_1080p = Arc::new(Manifest::from_file("broadcast_1080p.yaml")?);
    let manifest_720p = Arc::new(Manifest::from_file("broadcast_720p.yaml")?);
    let manifest_480p = Arc::new(Manifest::from_file("broadcast_480p.yaml")?);

    let mut session_1080p = transport.stream(manifest_1080p).await?;
    let mut session_720p = transport.stream(manifest_720p).await?;
    let mut session_480p = transport.stream(manifest_480p).await?;

    loop {
        // Capture source frame
        let source_frame = capture_video_frame().await?;
        let frame_data = RuntimeData::Video {
            frame: Arc::new(source_frame)
        };

        // Process through all pipelines
        session_1080p.send_input(TransportData::new(frame_data.clone())).await?;
        session_720p.send_input(TransportData::new(frame_data.clone())).await?;
        session_480p.send_input(TransportData::new(frame_data)).await?;

        // Send 1080p to viewer-1
        if let Some(output) = session_1080p.recv_output().await? {
            transport.send_to_peer("viewer-1", &output.data).await?;
        }

        // Send 720p to viewer-2
        if let Some(output) = session_720p.recv_output().await? {
            transport.send_to_peer("viewer-2", &output.data).await?;
        }

        // Send 480p to viewer-3
        if let Some(output) = session_480p.recv_output().await? {
            transport.send_to_peer("viewer-3", &output.data).await?;
        }

        tokio::time::sleep(Duration::from_millis(33)).await;
    }
}
```

### Use Case 4: Data Channel for Control Messages

**Goal**: Send pipeline reconfigurations via data channel

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = WebRtcTransport::new(config)?;
    transport.start().await?;

    transport.connect_peer("remote-peer").await?;

    // Start media streaming (audio)
    let manifest = Arc::new(Manifest::from_file("audio_pipeline.yaml")?);
    let mut session = transport.stream(manifest).await?;

    // Listen for control messages in background
    let transport_clone = transport.clone();
    tokio::spawn(async move {
        loop {
            // Receive control message (in real app, use proper event handling)
            if let Ok(Some(msg)) = receive_data_channel_message().await {
                match msg {
                    DataChannelMessage::Json { payload } => {
                        if let Some(bitrate) = payload["bitrate_kbps"].as_i64() {
                            println!("Received bitrate change request: {}kbps", bitrate);
                            // Reconfigure transport bitrate
                            let _ = transport_clone.configure(ConfigOptions {
                                target_bitrate_kbps: Some(bitrate as u32),
                                ..Default::default()
                            }).await;
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    // Main streaming loop
    loop {
        let audio = capture_audio().await?;
        session.send_input(TransportData::new(audio)).await?;

        while let Some(output) = session.recv_output().await? {
            transport.send_to_peer("remote-peer", &output.data).await?;
        }
    }
}
```

---

## Pipeline Integration

### Creating a Simple Pipeline

A RemoteMedia pipeline processes audio/video through a graph of nodes.

**Minimal pipeline YAML** (`simple_pipeline.yaml`):
```yaml
pipeline:
  # Define input/output types
  manifest:
    version: "0.1"
    inputs:
      - type: "audio"
        sample_rate: 48000
        channels: 1
    outputs:
      - type: "audio"
        sample_rate: 48000
        channels: 1

  # Processing nodes
  nodes:
    # Pass-through for testing
    - id: "echo"
      type: "Echo"
      executor: "native"
      params: {}

  # Data flow graph
  graph:
    - from: "input"
      to: "echo"
    - from: "echo"
      to: "output"
```

### Integrating with Transport

```rust
use remotemedia_runtime_core::transport::PipelineTransport;
use remotemedia_runtime_core::manifest::Manifest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create transport
    let transport = WebRtcTransport::new(config)?;
    transport.start().await?;

    // 2. Connect peers
    transport.connect_peer("remote-peer").await?;

    // 3. Load manifest
    let manifest = Arc::new(Manifest::from_file("my_pipeline.yaml")?);

    // 4. Create streaming session (this internally uses PipelineRunner)
    let mut session = transport.stream(manifest).await?;

    // 5. Stream data through pipeline
    loop {
        let input_data = receive_data().await?;

        // Send to pipeline
        session.send_input(TransportData::new(input_data)).await?;

        // Get processed output
        while let Some(output) = session.recv_output().await? {
            // Send to peers
            transport.send_to_peer("remote-peer", &output.data).await?;
        }
    }
}
```

### Unary vs. Streaming Execution

**Unary (single request/response)**:
```rust
let output = transport.execute(manifest, input).await?;
```

Use for: One-off processing, batch jobs

**Streaming (continuous)**:
```rust
let mut session = transport.stream(manifest).await?;
loop {
    session.send_input(data).await?;
    let output = session.recv_output().await?;
}
```

Use for: Real-time media, interactive applications

---

## Troubleshooting

### Issue 1: "Cannot connect to peer"

**Symptoms**: `ConnectionFailed` error when calling `connect_peer()`

**Causes**:
- Signaling server unreachable
- Peer not registered on signaling server
- Network firewall blocking UDP/TCP

**Solutions**:
```rust
// 1. Verify signaling server connection
transport.start().await?;
println!("Signaling connected: {:?}", transport.is_connected());

// 2. Check if peer exists
let peers = transport.list_peers().await?;
println!("Available peers: {:?}", peers);

// 3. Add TURN server for restrictive networks
let config = WebRtcTransportConfig {
    turn_servers: vec![
        TurnServer {
            urls: vec!["turn:your-turn-server.com".to_string()],
            username: "user".to_string(),
            credential: "pass".to_string(),
        }
    ],
    ..Default::default()
};
```

### Issue 2: "Audio/video dropouts"

**Symptoms**: Periodic silence or black frames

**Causes**:
- Network jitter
- Encoding timeout
- Insufficient bandwidth
- Codec mismatch

**Solutions**:
```rust
// 1. Increase jitter buffer
let config = WebRtcTransportConfig {
    jitter_buffer_size_ms: 100, // Increased from 50ms
    ..Default::default()
};

// 2. Reduce bitrate on poor networks
let config = WebRtcTransportConfig {
    video_codec: VideoCodec::VP9 {
        bitrate_kbps: 500, // Reduced from 2000
        framerate: 15,     // Reduced from 30
        ..Default::default()
    },
    ..Default::default()
};

// 3. Enable adaptive bitrate
transport.configure(ConfigOptions {
    adaptive_bitrate_enabled: Some(true),
    target_bitrate_kbps: Some(1000),
    ..Default::default()
}).await?;
```

### Issue 3: "High latency (>100ms)"

**Symptoms**: Audio/video delay noticeable to users

**Causes**:
- TURN relay (instead of direct P2P)
- Jitter buffer too large
- Slow pipeline processing
- Network congestion

**Solutions**:
```rust
// 1. Reduce jitter buffer
let config = WebRtcTransportConfig {
    jitter_buffer_size_ms: 50, // Minimum practical
    ..Default::default()
};

// 2. Use VP9 (faster than H.264)
let config = WebRtcTransportConfig {
    video_codec: VideoCodec::VP9 { .. },
    ..Default::default()
};

// 3. Optimize pipeline
// Ensure pipeline nodes are fast (sub-10ms processing)
// Profile with: `cargo bench --features profiling`

// 4. Monitor connection quality
let peers = transport.list_peers().await?;
for peer in peers {
    println!("Latency: {}ms", peer.metrics.latency_ms);
    println!("Packet loss: {}%", peer.metrics.packet_loss_rate);
}
```

### Issue 4: "ICE candidate gathering fails"

**Symptoms**: Connection stuck in "Connecting" state for >10 seconds

**Causes**:
- STUN server unreachable
- UDP blocked by firewall
- IPv6/IPv4 mismatch
- NAT type incompatible

**Solutions**:
```rust
// 1. Use multiple STUN servers (redundancy)
let config = WebRtcTransportConfig {
    stun_servers: vec![
        "stun:stun.l.google.com:19302".to_string(),
        "stun:stun1.l.google.com:19302".to_string(),
        "stun:stun2.l.google.com:19302".to_string(),
        "stun:stun3.l.google.com:19302".to_string(),
    ],
    ..Default::default()
};

// 2. Add TURN server as fallback
let config = WebRtcTransportConfig {
    turn_servers: vec![
        TurnServer {
            urls: vec!["turn:turn.example.com:3478".to_string()],
            username: "user".to_string(),
            credential: "pass".to_string(),
        }
    ],
    ..Default::default()
};

// 3. Increase ICE timeout
let config = WebRtcTransportConfig {
    ice_timeout_secs: 15, // Increased from 10
    ..Default::default()
};

// 4. Check network diagnostics
println!("Testing STUN connectivity...");
// Use external tool: timeout -s KILL 5 strace -e connect cargo run
```

### Issue 5: "Memory leak during long streaming"

**Symptoms**: Process memory grows continuously over hours

**Causes**:
- Jitter buffer not clearing
- RTP packets not freed
- Session not properly closed

**Solutions**:
```rust
// 1. Explicitly close sessions
session.close().await?;

// 2. Disconnect peers
transport.disconnect_peer("peer-id").await?;

// 3. Graceful shutdown
transport.shutdown().await?;

// 4. Monitor memory usage
use std::alloc::GlobalAlloc;
// Add memory profiling: https://docs.rs/valgrind/latest/valgrind/
```

---

## Testing

### Unit Tests: Transport Configuration

```rust
#[tokio::test]
async fn test_transport_creation() {
    let config = WebRtcTransportConfig {
        signaling_url: "ws://localhost:8080".to_string(),
        peer_id: "test-peer".to_string(),
        stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
        max_peers: 5,
        ..Default::default()
    };

    let transport = WebRtcTransport::new(config);
    assert!(transport.is_ok());
}

#[tokio::test]
async fn test_invalid_config() {
    let config = WebRtcTransportConfig {
        signaling_url: "invalid-url".to_string(), // Invalid
        ..Default::default()
    };

    let transport = WebRtcTransport::new(config);
    assert!(transport.is_err());
}
```

### Integration Tests: Peer Connection

```rust
#[tokio::test]
async fn test_1v1_connection() {
    // Setup two transports (simulate two peers)
    let alice_config = WebRtcTransportConfig {
        peer_id: "alice".to_string(),
        ..Default::default()
    };

    let bob_config = WebRtcTransportConfig {
        peer_id: "bob".to_string(),
        ..Default::default()
    };

    let alice = WebRtcTransport::new(alice_config).unwrap();
    let bob = WebRtcTransport::new(bob_config).unwrap();

    alice.start().await.unwrap();
    bob.start().await.unwrap();

    // Alice connects to Bob
    let peer_id = alice.connect_peer("bob").await.unwrap();
    assert_eq!(peer_id.as_str(), "bob");

    // Verify connection
    let peers = alice.list_peers().await.unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].peer_id, "bob");

    // Cleanup
    alice.disconnect_peer("bob").await.unwrap();
    alice.shutdown().await.unwrap();
    bob.shutdown().await.unwrap();
}
```

### Performance Tests: Latency Measurement

```rust
use std::time::Instant;

#[tokio::test]
async fn test_audio_latency() {
    let transport = setup_transport().await;
    let manifest = Arc::new(load_manifest().await);
    let mut session = transport.stream(manifest).await.unwrap();

    let start = Instant::now();

    // Send audio input
    let audio = create_test_audio();
    session.send_input(TransportData::new(audio)).await.unwrap();

    // Receive output
    let output = session.recv_output().await.unwrap();

    let elapsed = start.elapsed();
    println!("Pipeline latency: {:?}", elapsed);

    assert!(elapsed.as_millis() < 50); // Target: <50ms
}
```

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_1v1_connection

# Run tests with logging
RUST_LOG=debug cargo test -- --nocapture
```

---

## Performance Tuning

### Audio Performance

**Typical latency breakdown**:
- Encoding (Opus): 5-10ms
- Network RTT: 10-50ms
- Decoding: 5-10ms
- Jitter buffer: 50-100ms
- **Total**: 70-170ms

**Optimization**:
```rust
let config = WebRtcTransportConfig {
    audio_codec: AudioCodec::Opus {
        complexity: 4,          // Lower complexity = faster
        bitrate_kbps: 24,       // Lower bitrate = faster encoding
        ..Default::default()
    },
    jitter_buffer_size_ms: 50, // Minimum for stability
    ..Default::default()
};
```

### Video Performance

**Typical latency breakdown**:
- Encoding (VP9): 20-50ms
- Network RTT: 10-50ms
- Decoding: 10-30ms
- Jitter buffer: 50-100ms
- **Total**: 90-230ms

**Optimization**:
```rust
let config = WebRtcTransportConfig {
    video_codec: VideoCodec::VP9 {
        width: 640,             // Lower resolution = faster
        height: 480,
        framerate: 15,          // Lower fps = lower bitrate
        bitrate_kbps: 500,      // Lower bitrate = faster encoding
        ..Default::default()
    },
    jitter_buffer_size_ms: 50,
    ..Default::default()
};
```

### Bandwidth Optimization

**Adaptive bitrate strategy**:
```rust
loop {
    let peers = transport.list_peers().await?;

    for peer in peers {
        if peer.metrics.packet_loss_rate > 0.05 {
            // Loss >5%: reduce bitrate
            transport.configure(ConfigOptions {
                target_bitrate_kbps: Some(1000),
                ..Default::default()
            }).await?;
        } else if peer.metrics.packet_loss_rate < 0.01 {
            // Loss <1%: increase bitrate
            transport.configure(ConfigOptions {
                target_bitrate_kbps: Some(3000),
                ..Default::default()
            }).await?;
        }
    }

    tokio::time::sleep(Duration::from_secs(5)).await;
}
```

### CPU Optimization

**Profiling**:
```bash
# Use perf (Linux)
perf record -g target/release/my-webrtc-app
perf report

# Or cargo-flamegraph
cargo install flamegraph
cargo flamegraph -- --help
```

**Bottleneck elimination**:
1. Codec: VP9 slow? → Use H.264
2. Manifest: Complex pipeline? → Simplify, use native nodes
3. Signaling: Constant reconnects? → Increase timeout
4. Memory: Frequent allocations? → Use Arc buffers

---

## Next Steps

### 1. Explore the Specification Documents

- **Feature Spec**: `specs/001-webrtc-multi-peer-transport/spec.md`
  - User scenarios and requirements
  - Success criteria

- **Transport API**: `specs/001-webrtc-multi-peer-transport/contracts/transport-api.md`
  - Complete method reference
  - Error types and recovery patterns

- **Signaling Protocol**: `specs/001-webrtc-multi-peer-transport/contracts/signaling-protocol.md`
  - JSON-RPC 2.0 message formats
  - State machine details

- **Research Document**: `transports/remotemedia-webrtc/research.md`
  - Audio/video synchronization
  - Codec selection rationale
  - Production hardening strategies

### 2. Set Up a Signaling Server

The transport requires a WebSocket signaling server. Choose one:

**Option A: Use existing reference**
```bash
cd transports/remotemedia-webrtc/signaling-server
npm install
npm start
```

**Option B: Implement minimal signaling**
```rust
// Use gotham or actix-web for WebSocket + JSON-RPC 2.0
// See examples/signaling-server.rs
```

### 3. Build Multi-Peer Applications

Start with simpler use cases, then scale:
1. 1:1 audio call (this quickstart)
2. 1:1 video call with processing
3. 3-person audio conference
4. 5+ person mesh network
5. Broadcast with 10+ viewers

### 4. Integrate with RemoteMedia Pipelines

- **Audio processing**: VAD, noise gate, echo cancellation
- **Video processing**: Background blur, face detection, format conversion
- **Hybrid**: Audio + video + data channels simultaneously

See `CLAUDE.md` (project root) for pipeline execution patterns.

### 5. Production Deployment

Checklist before going live:

- [ ] Error handling for all `WebRtcError` types
- [ ] Automatic reconnection with exponential backoff
- [ ] Monitor metrics (`list_peers()`, latency, packet loss)
- [ ] Rate limiting on signaling server
- [ ] HTTPS/WSS (TLS) for signaling
- [ ] TURN server configured for restrictive networks
- [ ] Load testing (5+ simultaneous connections)
- [ ] Graceful shutdown on SIGTERM
- [ ] Logging enabled for debugging

### 6. Performance Benchmarking

```bash
# Run benchmarks
cargo bench --release

# Typical results (on modern hardware):
# - Connection setup: <2 seconds
# - Audio latency: <50ms
# - Video latency: <100ms
# - Memory per peer: <100MB
# - CPU (30fps video): <30% of single core
```

### 7. Contribute Back

The WebRTC transport is actively developed. Contributions welcome:
- Bug fixes
- Performance optimizations
- Additional codec support
- Platform-specific improvements

---

## Quick Reference

### Configuration Defaults

```rust
WebRtcTransportConfig {
    signaling_url: "ws://localhost:8080",
    peer_id: auto-generated UUID,
    stun_servers: [Google STUN],
    turn_servers: [],
    max_peers: 10,
    audio_codec: Opus { sample_rate: 48k, bitrate: 32k },
    video_codec: VP9 { 1280x720, 30fps, 2000kbps },
    enable_data_channel: true,
    jitter_buffer_size_ms: 50,
}
```

### Common Methods

| Method | Purpose | Returns |
|--------|---------|---------|
| `new()` | Create transport | `Result<Self>` |
| `start()` | Connect to signaling | `Result<()>` |
| `connect_peer()` | Establish peer connection | `Result<PeerId>` |
| `send_to_peer()` | Send media to one peer | `Result<()>` |
| `broadcast()` | Send to all peers | `Result<BroadcastStats>` |
| `stream()` | Create streaming session | `Result<StreamSession>` |
| `shutdown()` | Cleanup and close | `Result<()>` |

### Error Handling

```rust
match transport.connect_peer("peer-id").await {
    Ok(id) => println!("Connected: {}", id),
    Err(WebRtcError::PeerNotFound(_)) => eprintln!("Peer not discoverable"),
    Err(WebRtcError::NatTraversalFailed) => eprintln!("STUN/TURN failed"),
    Err(e) => eprintln!("Other error: {}", e),
}
```

---

## Getting Help

- **Documentation**: Read spec documents in `specs/001-webrtc-multi-peer-transport/`
- **Examples**: Check `examples/` directory for complete working examples
- **Debugging**: Enable logging with `RUST_LOG=debug cargo run`
- **Issues**: Open GitHub issue with error log and minimal reproduction
- **Research**: See `transports/remotemedia-webrtc/research.md` for deep dives

---

## Glossary

**SDP** - Session Description Protocol (offer/answer for codec negotiation)
**ICE** - Interactive Connectivity Establishment (NAT traversal)
**STUN** - Simple Traversal of UDP through NAT (public server for IP discovery)
**TURN** - Traversal Using Relays around NAT (relay server for restrictive networks)
**RTP** - Real-time Transport Protocol (media packet format)
**RTCP** - RTP Control Protocol (statistics and synchronization)
**DTLS-SRTP** - Encrypted RTP over datagram TLS
**Jitter Buffer** - Reordering buffer for network-out-of-order packets
**Adaptive Bitrate** - Dynamic quality adjustment based on network conditions
**Mesh Topology** - Every peer connects to every other peer (N:N)
**Trickle ICE** - Streaming ICE candidates incrementally instead of waiting for all

---

**Happy streaming!**
