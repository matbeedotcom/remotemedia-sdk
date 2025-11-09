# WebRtcTransport Public API Contract

**Version:** 1.0.0
**Feature:** WebRTC Multi-Peer Transport
**Status:** Specification
**Created:** 2025-11-07
**Last Updated:** 2025-11-07

## Overview

This contract defines the public API for `WebRtcTransport`, a production-ready WebRTC transport that implements the `PipelineTransport` trait from `remotemedia-runtime-core`. The transport enables multi-peer mesh networking with audio/video streaming and RemoteMedia pipeline integration.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  WebRtcTransport (implements PipelineTransport trait)          │
│                                                                 │
│  ├─ Peer Management (connect_peer, disconnect_peer, list_peers)│
│  ├─ Data Routing (send_to_peer, broadcast)                     │
│  ├─ Session Management (create_session, terminate_session)     │
│  ├─ Configuration (new, configure)                             │
│  └─ Lifecycle (start, shutdown)                                │
│                                                                 │
│  Internal Components:                                           │
│  ├─ SignalingClient (WebSocket + JSON-RPC 2.0)                │
│  ├─ PeerManager (WebRTC peer connections)                     │
│  ├─ SyncManager (per-peer audio/video sync)                   │
│  ├─ SessionRouter (pipeline data routing)                     │
│  └─ PipelineRunner (from remotemedia-runtime-core)            │
└─────────────────────────────────────────────────────────────────┘
```

---

## Core API Methods

### 1. Lifecycle Methods

#### `new(config: WebRtcTransportConfig) -> Result<Self>`

**Purpose**: Create a new WebRtcTransport instance

**Parameters**:
- `config: WebRtcTransportConfig` - Transport configuration

**Return Type**: `Result<WebRtcTransport, Error>`

**Example**:
```rust
use remotemedia_webrtc::transport::{WebRtcTransport, WebRtcTransportConfig};

let config = WebRtcTransportConfig {
    signaling_url: "wss://signaling.example.com".to_string(),
    peer_id: "peer-alice-123".to_string(),
    stun_servers: vec![
        "stun:stun.l.google.com:19302".to_string(),
    ],
    turn_servers: vec![],
    max_peers: 10,
    audio_codec: AudioCodec::Opus,
    video_codec: VideoCodec::VP9,
    enable_data_channel: true,
    jitter_buffer_size_ms: 50,
    ..Default::default()
};

let transport = WebRtcTransport::new(config)?;
```

**Error Conditions**:
- `InvalidConfig`: STUN servers empty or URL invalid
- `SignalingError`: Cannot connect to signaling server
- `NetworkError`: Network initialization failed

**Thread Safety**: Safe for `Send + Sync` use across threads

---

#### `start(&self) -> Result<()>`

**Purpose**: Initialize signaling connection and begin peer discovery

**Example**:
```rust
transport.start().await?;
// Transport is now connected to signaling server and will receive
// peer.announced messages for other peers
```

**Preconditions**:
- Transport must not already be started
- Signaling server must be accessible

**Postconditions**:
- Signaling WebSocket connection established
- Ready to call `connect_peer()`
- Listening for incoming peer announcements

**Error Conditions**:
- `SignalingError`: WebSocket connection failed
- `ConnectionTimeout`: Signaling server unreachable
- `InvalidState`: Already started

**Notes**:
- Non-blocking: returns immediately after connection initiated
- Uses internal background task for signaling communication
- Automatic reconnection on disconnect (configurable retry policy)

---

#### `shutdown(&self) -> Result<()>`

**Purpose**: Gracefully shut down all connections and cleanup resources

**Example**:
```rust
transport.shutdown().await?;
// All peer connections closed
// Signaling connection closed
// Resources released
```

**Preconditions**:
- Transport may be started or not started
- In-progress streams are allowed

**Postconditions**:
- All peer connections closed
- All sessions terminated
- Resources deallocated
- Signaling connection closed

**Error Conditions**:
- `ClosureTimeout`: Shutdown took >5 seconds
- `AlreadyShutdown`: Called multiple times

**Timeout**: 5 seconds maximum

**Notes**:
- Idempotent: safe to call multiple times
- Waits for pending operations (with timeout)
- Closes all sessions before returning

---

### 2. Peer Management Methods

#### `connect_peer(&self, peer_id: &str) -> Result<PeerId>`

**Purpose**: Establish a WebRTC connection to a specific peer

**Parameters**:
- `peer_id: &str` - Remote peer identifier (must be unique)

**Return Type**: `Result<PeerId, Error>`

**Example**:
```rust
let peer_id = transport.connect_peer("peer-bob-456").await?;
println!("Connected to {}", peer_id);

// Peer connection now active and ready for:
// - Media stream transmission
// - Data channel messages
// - Pipeline processing
```

**Preconditions**:
- Transport must be started via `start()`
- Peer must be discoverable via signaling server
- Must not already be connected to this peer
- Peer count < `max_peers` configuration

**Postconditions**:
- WebRTC peer connection established
- ICE candidates exchanged
- Connection state = Connected
- Ready for data transmission

**Error Conditions**:
- `PeerNotFound`: Peer not discoverable via signaling
- `ConnectionFailed`: SDP exchange failed
- `IceCandidateError`: No valid ICE candidates
- `NatTraversalFailed`: Direct P2P and TURN relay both failed
- `MaxPeersExceeded`: Already connected to max_peers
- `ConnectionTimeout`: Timeout waiting for answer (>10s)
- `InvalidPeerId`: Peer ID is empty or invalid format

**Timeout**: 10 seconds total (SDP exchange + ICE gathering)

**Features**:
- Automatic trickle ICE (candidates streamed as discovered)
- Negotiates codec preferences (VP9 primary, H.264 fallback)
- Enables data channel by default
- Creates SyncManager for audio/video synchronization

**Notes**:
- This is a fire-and-forget operation for connection initiation
- Use `get_connection_metrics()` to monitor connection quality
- Connection may remain in "Connecting" state briefly during ICE

---

#### `disconnect_peer(&self, peer_id: &str) -> Result<()>`

**Purpose**: Close WebRTC connection to a peer and cleanup resources

**Parameters**:
- `peer_id: &str` - Peer identifier to disconnect

**Return Type**: `Result<(), Error>`

**Example**:
```rust
transport.disconnect_peer("peer-bob-456").await?;
// Connection closed
// Resources released
// Peer can be reconnected later
```

**Preconditions**:
- Peer must be connected or connecting
- Any active sessions with this peer may be interrupted

**Postconditions**:
- WebRTC connection closed
- RTP/RTCP streams stopped
- Resources deallocated
- Peer may be reconnected

**Error Conditions**:
- `PeerNotFound`: Peer not in connection list
- `AlreadyDisconnected`: Peer not currently connected
- `ClosureTimeout`: Cleanup took >2 seconds

**Timeout**: 2 seconds

**Features**:
- Graceful closure with DTLS close_notify
- Removes peer from all active sessions
- Clears jitter buffers and sync state
- Broadcast notification via signaling (peer.disconnect)

**Notes**:
- Safe to call even if peer already disconnected
- Sessions with this peer are terminated
- Connection state moves to "Closed"

---

#### `list_peers(&self) -> Result<Vec<PeerInfo>>`

**Purpose**: List all currently connected peers with metadata

**Return Type**: `Result<Vec<PeerInfo>, Error>`

**Example**:
```rust
let peers = transport.list_peers().await?;
for peer_info in peers {
    println!(
        "Peer: {}, State: {:?}, Latency: {}ms",
        peer_info.peer_id,
        peer_info.connection_state,
        peer_info.metrics.latency_ms
    );
}
```

**Preconditions**:
- Transport must be started

**Return Value Structure**:
```rust
pub struct PeerInfo {
    pub peer_id: String,
    pub connection_state: ConnectionState,
    pub capabilities: Vec<String>, // "audio", "video", "data"
    pub connected_at: SystemTime,
    pub metrics: ConnectionQualityMetrics,
    pub sync_state: SyncState,
}

pub enum ConnectionState {
    New,
    Connecting,
    Connected,
    Failed,
    Closed,
}

pub struct ConnectionQualityMetrics {
    pub latency_ms: f64,
    pub packet_loss_rate: f32,
    pub jitter_ms: f64,
    pub bandwidth_kbps: f32,
    pub video_resolution: Option<String>,
    pub video_framerate: Option<u32>,
    pub audio_bitrate_kbps: Option<u32>,
    pub video_bitrate_kbps: Option<u32>,
}

pub enum SyncState {
    Unsynced,
    Syncing,
    Synced,
}
```

**Error Conditions**:
- `NotStarted`: Transport not started yet

**Performance**:
- O(n) where n = number of connected peers
- Snapshot taken at call time (excludes Connecting state peers)

**Notes**:
- Only returns Connected peers (not Connecting or Failed)
- Metrics updated every 1-5 seconds from RTCP reports
- Safe to call frequently without performance impact

---

### 3. Data Routing Methods

#### `send_to_peer(&self, peer_id: &str, data: &RuntimeData) -> Result<()>`

**Purpose**: Send media data to a specific peer via WebRTC media track

**Parameters**:
- `peer_id: &str` - Target peer identifier
- `data: &RuntimeData` - Media data (Audio, Video, Text)

**Return Type**: `Result<(), Error>`

**Example**:
```rust
use remotemedia_runtime_core::data::RuntimeData;
use std::sync::Arc;

// Send audio data (post-pipeline processing)
let audio_data = RuntimeData::Audio {
    samples: Arc::new(vec![0.1, 0.2, 0.3, ...]), // f32 samples
    sample_rate: 48_000,
    channels: 1,
};

transport.send_to_peer("peer-bob-456", &audio_data).await?;

// Send video data
let video_data = RuntimeData::Video {
    frame: Arc::new(VideoFrame {
        width: 1280,
        height: 720,
        format: PixelFormat::I420,
        planes: vec![y_plane, u_plane, v_plane],
        timestamp: 12345,
    }),
};

transport.send_to_peer("peer-bob-456", &video_data).await?;
```

**Preconditions**:
- Peer must be connected and in Connected state
- Data must be valid for the encoded media type
- For audio: samples must be f32, 48kHz
- For video: frame must be I420 or compatible format

**Postconditions**:
- Data encoded (Opus for audio, VP9/H.264 for video)
- Sent via RTP to peer
- Sequence number incremented
- Timestamp updated

**Error Conditions**:
- `PeerNotFound`: Peer not in connection list
- `PeerNotConnected`: Peer not in Connected state
- `InvalidDataType`: Data type not supported for this peer
- `EncodingError`: Codec encoding failed
- `SendBufferFull`: RTP send buffer exceeded (backpressure)
- `EncodingTimeout`: Encoding took >30ms (video)

**Timeout**: 30ms (video) / 10ms (audio)

**Features**:
- RTP timestamp automatically managed (incremental per clock rate)
- Sequence number automatically incremented
- DTLS-SRTP encryption applied transparently
- Adaptive bitrate adjustment on packet loss

**Zero-Copy Optimization**:
- Audio: Direct reference to Arc<Vec<f32>> (no copy)
- Video: Direct reference to frame buffer (no copy)
- Encoding only when necessary (mandatory for media track)

**Notes**:
- For multi-peer broadcast, use `broadcast()` instead
- Backpressure: blocks if RTP buffer full (typical <1ms)
- Timestamp continuity maintained across calls

---

#### `broadcast(&self, data: &RuntimeData) -> Result<BroadcastStats>`

**Purpose**: Send data to all connected peers simultaneously

**Parameters**:
- `data: &RuntimeData` - Media data to broadcast

**Return Type**: `Result<BroadcastStats, Error>`

**Example**:
```rust
let mixed_audio = RuntimeData::Audio {
    samples: Arc::new(vec![...]), // Mixed audio from multiple peers
    sample_rate: 48_000,
    channels: 1,
};

let stats = transport.broadcast(&mixed_audio).await?;
println!(
    "Broadcast to {} peers, {} failed",
    stats.sent_count, stats.failed_count
);
```

**Preconditions**:
- At least one peer connected
- Data must be valid (same as `send_to_peer()`)

**Postconditions**:
- Data sent to all Connected peers
- BroadcastStats populated with results

**Return Value Structure**:
```rust
pub struct BroadcastStats {
    pub total_peers: usize,
    pub sent_count: usize,
    pub failed_count: usize,
    pub failed_peers: Vec<(String, String)>, // (peer_id, error_reason)
    pub total_duration_ms: f64,
}
```

**Error Conditions**:
- `NoPeersConnected`: No connected peers
- `InvalidData`: Data invalid for broadcast
- `PartialBroadcast`: Some peers received data, others failed
- `NoCapability`: Peers don't support data type

**Performance**:
- Parallel encoding: ~O(n) where n = number of peers
- Typical: 1000 peers/second on single core

**Features**:
- Parallel encoding to multiple peers
- Atomic at call level: either all get data or none
- Stats include failure reasons for debugging

**Notes**:
- Not truly atomic at packet level (delivery may vary slightly)
- Failed sends do NOT prevent other peers from receiving
- Use `BroadcastStats.failed_peers` to retry or handle failures

---

### 4. Data Channel Methods (Optional)

#### `send_data_channel_message(&self, peer_id: &str, message: &DataChannelMessage) -> Result<()>`

**Purpose**: Send structured message via reliable ordered data channel

**Parameters**:
- `peer_id: &str` - Target peer
- `message: &DataChannelMessage` - Message to send

**Return Type**: `Result<(), Error>`

**Example**:
```rust
use remotemedia_webrtc::data_channel::DataChannelMessage;

let message = DataChannelMessage::Json {
    payload: serde_json::json!({
        "command": "pipeline_config_change",
        "params": {
            "bitrate_kbps": 2000,
            "video_codec": "VP9"
        }
    }),
};

transport.send_data_channel_message("peer-bob-456", &message).await?;
```

**Message Types**:
```rust
pub enum DataChannelMessage {
    Json {
        payload: serde_json::Value,
    },
    Binary {
        payload: Vec<u8>,
    },
    Text {
        payload: String,
    },
}
```

**Preconditions**:
- Peer must be connected
- Data channel enabled in config
- Message size < 16 MB

**Postconditions**:
- Message queued for delivery
- Delivered in order with all previous messages
- Guaranteed delivery (no loss)

**Error Conditions**:
- `PeerNotFound`: Peer not connected
- `DataChannelNotAvailable`: Peer lacks data channel support
- `MessageTooLarge`: Payload > 16 MB
- `SendBufferFull`: Data channel buffer exceeded

**Guarantees**:
- Ordered: messages delivered in send order
- Reliable: no message loss
- DTLS encrypted: same as media tracks

**Notes**:
- Data channels carry JSON/binary control messages, not media
- Typical latency: 10-50ms (depends on network)
- Used for pipeline reconfiguration, metadata, coordination

---

### 5. Session Management Methods

#### `execute(&self, manifest: Arc<Manifest>, input: TransportData) -> Result<TransportData>`

**Purpose**: Unary execution (single request/response) via `PipelineTransport` trait

**Parameters**:
- `manifest: Arc<Manifest>` - RemoteMedia pipeline definition
- `input: TransportData` - Input data

**Return Type**: `Result<TransportData, Error>`

**Example**:
```rust
use remotemedia_runtime_core::transport::{PipelineTransport, TransportData};
use remotemedia_runtime_core::data::RuntimeData;
use std::sync::Arc;

let audio_input = RuntimeData::Audio {
    samples: Arc::new(vec![...]),
    sample_rate: 48_000,
    channels: 1,
};

let manifest = Arc::new(Manifest::from_json(r#"{
    "pipeline": {
        "nodes": [
            { "id": "audio_processor", "type": "AudioProcessor" }
        ],
        "graph": [{ "from": "input", "to": "audio_processor", "to": "output" }]
    }
}"#)?);

let input = TransportData::new(audio_input);
let output = transport.execute(manifest, input).await?;

println!("Processed: {:?}", output.data);
```

**Trait Implementation**: Delegates to internal `PipelineRunner::execute_unary()`

**Preconditions**:
- Manifest must be valid and parseable
- Input data matches manifest input types

**Postconditions**:
- Pipeline executed end-to-end
- Output data returned

**Error Conditions**:
- `InvalidManifest`: Manifest missing required fields
- `ExecutionError`: Pipeline node execution failed
- `ValidationError`: Input data invalid for manifest
- `TimeoutError`: Execution exceeded 30 seconds

**Timeout**: 30 seconds

**Notes**:
- This method does NOT use WebRTC peers
- Purely local pipeline execution
- For multi-peer scenarios, use `stream()` instead

---

#### `stream(&self, manifest: Arc<Manifest>) -> Result<Box<dyn StreamSession>>`

**Purpose**: Create streaming session for continuous pipeline execution

**Parameters**:
- `manifest: Arc<Manifest>` - Pipeline definition

**Return Type**: `Result<Box<dyn StreamSession>, Error>`

**Example**:
```rust
use remotemedia_runtime_core::transport::{PipelineTransport, StreamSession};

let manifest = Arc::new(Manifest::from_json(pipeline_json)?);
let mut session = transport.stream(manifest).await?;

// Continuous streaming
loop {
    let input_data = receive_from_peer().await?;
    let input = TransportData::new(input_data);

    session.send_input(input).await?;

    while let Some(output) = session.recv_output().await? {
        send_to_peers(&output).await?;
    }
}

session.close().await?;
```

**Trait Implementation**: Delegates to internal `PipelineRunner::create_stream_session()`

**Return Value**: StreamSession trait object

```rust
pub trait StreamSession: Send + Sync {
    fn session_id(&self) -> &str;
    async fn send_input(&mut self, data: TransportData) -> Result<()>;
    async fn recv_output(&mut self) -> Result<Option<TransportData>>;
    async fn close(&mut self) -> Result<()>;
    fn is_active(&self) -> bool;
}
```

**Preconditions**:
- Manifest must be valid
- At least one peer connected (for useful streaming)

**Postconditions**:
- StreamSession created with unique session_id
- Ready for `send_input()` / `recv_output()` loops

**Error Conditions**:
- `InvalidManifest`: Manifest validation failed
- `SessionCreationFailed`: Internal runner error

**Session Lifecycle**:
```
Created
  ↓
Active (send_input ↔ recv_output)
  ↓
Closed (after close())
```

**Notes**:
- Multi-peer streaming: each peer's input routed through same session
- Session ID used for iceoryx2 channel naming (prevents conflicts)
- Sessions are long-lived (seconds to hours)

---

### 6. Configuration Methods

#### `configure(&mut self, options: ConfigOptions) -> Result<()>`

**Purpose**: Reconfigure transport settings (post-initialization)

**Parameters**:
- `options: ConfigOptions` - Settings to update

**Return Type**: `Result<(), Error>`

**Example**:
```rust
use remotemedia_webrtc::transport::ConfigOptions;

let mut transport = WebRtcTransport::new(config)?;

// Update bitrate strategy
transport.configure(ConfigOptions {
    adaptive_bitrate_enabled: Some(true),
    target_bitrate_kbps: Some(2000),
    max_video_resolution: Some("1080p".to_string()),
    jitter_buffer_size_ms: Some(100),
    ..Default::default()
}).await?;
```

**Supported Options**:
```rust
pub struct ConfigOptions {
    pub adaptive_bitrate_enabled: Option<bool>,
    pub target_bitrate_kbps: Option<u32>,
    pub max_video_resolution: Option<String>, // "1080p", "720p", "480p"
    pub video_framerate_fps: Option<u32>,
    pub audio_bitrate_kbps: Option<u32>,
    pub jitter_buffer_size_ms: Option<u32>,
    pub ice_timeout_secs: Option<u32>,
    pub rtcp_interval_ms: Option<u32>,
    ..Default::default()
}
```

**Preconditions**:
- Transport started or not

**Postconditions**:
- Settings updated immediately
- Applied to all future connections
- Existing connections may be affected

**Error Conditions**:
- `InvalidOption`: Option value out of valid range
- `NotSupported`: Option not supported by codec/peer

**Constraints**:
- Bitrate: 16 kbps - 50 Mbps
- Jitter buffer: 50 - 200 ms
- Resolution: must be 16:9 or 4:3 aspect ratio
- Framerate: 10 - 60 fps

**Notes**:
- Changes take effect on next connection attempt
- Existing peer connections not affected
- Use `connect_peer()` after reconfiguration to apply to new peers

---

## Error Handling

### Error Types

```rust
pub enum WebRtcError {
    // Configuration
    InvalidConfig(String),

    // Signaling
    SignalingError(String),
    ConnectionFailed(String),

    // Peer management
    PeerNotFound(String),
    PeerNotConnected(String),
    MaxPeersExceeded,

    // Network
    NatTraversalFailed,
    IceCandidateError(String),

    // Data
    InvalidData(String),
    EncodingError(String),
    DecodingError(String),

    // Session
    SessionNotFound(String),
    SessionCreationFailed(String),

    // Lifecycle
    NotStarted,
    AlreadyStarted,
    AlreadyShutdown,

    // Timeout
    OperationTimeout(Duration),
}
```

### Error Recovery Patterns

**Pattern 1: Retry with Backoff**
```rust
use std::time::Duration;

async fn connect_with_retry(
    transport: &WebRtcTransport,
    peer_id: &str,
) -> Result<PeerId> {
    let mut backoff = Duration::from_millis(100);
    let mut attempt = 0;

    loop {
        match transport.connect_peer(peer_id).await {
            Ok(id) => return Ok(id),
            Err(e) if attempt < 3 => {
                eprintln!("Attempt {} failed: {}", attempt, e);
                tokio::time::sleep(backoff).await;
                backoff *= 2;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

**Pattern 2: Graceful Fallback**
```rust
let result = match transport.connect_peer(peer_id).await {
    Ok(id) => Ok(id),
    Err(WebRtcError::NatTraversalFailed) => {
        // Try with different TURN server
        transport.reconfigure_turn_servers(backup_servers).await?;
        transport.connect_peer(peer_id).await
    }
    Err(e) => Err(e),
};
```

---

## Usage Examples

### Example 1: Basic Peer-to-Peer Audio Streaming

```rust
use remotemedia_webrtc::transport::{WebRtcTransport, WebRtcTransportConfig};
use remotemedia_runtime_core::data::RuntimeData;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize
    let config = WebRtcTransportConfig {
        signaling_url: "wss://signaling.example.com".to_string(),
        peer_id: "alice".to_string(),
        stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
        ..Default::default()
    };

    let transport = WebRtcTransport::new(config)?;
    transport.start().await?;

    // Connect to peer
    let peer_id = transport.connect_peer("bob").await?;
    println!("Connected to {}", peer_id);

    // Stream audio
    loop {
        let audio = receive_audio_from_mic().await?;
        let data = RuntimeData::Audio {
            samples: Arc::new(audio),
            sample_rate: 48_000,
            channels: 1,
        };

        transport.send_to_peer("bob", &data).await?;
    }
}
```

### Example 2: Pipeline-Based Processing

```rust
use remotemedia_runtime_core::transport::{PipelineTransport, StreamSession};
use remotemedia_runtime_core::manifest::Manifest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = WebRtcTransport::new(config)?;
    transport.start().await?;

    // Create pipeline manifest
    let manifest = Arc::new(Manifest::from_json(r#"{
        "pipeline": {
            "nodes": [
                { "id": "noise_gate", "type": "NoiseGate", "params": {} }
            ],
            "graph": [
                { "from": "input", "to": "noise_gate" },
                { "from": "noise_gate", "to": "output" }
            ]
        }
    }"#)?);

    // Create streaming session
    let mut session = transport.stream(manifest).await?;

    // Process audio through pipeline
    loop {
        let input_data = receive_audio().await?;
        session.send_input(TransportData::new(input_data)).await?;

        while let Some(output) = session.recv_output().await? {
            transport.broadcast(&output.data).await?;
        }
    }
}
```

### Example 3: Multi-Peer Conference

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = WebRtcTransport::new(config)?;
    transport.start().await?;

    // Connect to multiple peers
    let peers = vec!["peer-1", "peer-2", "peer-3"];
    for peer_id in &peers {
        transport.connect_peer(peer_id).await?;
    }

    // Broadcast mixed audio to all
    loop {
        let mixed_audio = mix_peer_audio(&transport).await?;
        let stats = transport.broadcast(&mixed_audio).await?;
        println!("Broadcast to {} peers", stats.sent_count);
    }
}
```

---

## Implementation Notes

### Thread Safety

- All methods are `async` and thread-safe
- `WebRtcTransport` is `Send + Sync`
- Can be wrapped in `Arc<WebRtcTransport>` for shared ownership
- Internal state protected by `RwLock` and `Mutex`

### Async Model

- All I/O is async via tokio
- Methods should be awaited: `.await`
- Non-blocking: returns control immediately
- Background tasks handle async work

### Resource Limits

| Resource | Limit | Notes |
|----------|-------|-------|
| Max peers per transport | 10 | Configurable, mesh topology constraint |
| Max sessions per transport | 1 per manifest | Can reuse session for same manifest |
| Max data channel message size | 16 MB | Per WebRTC spec |
| RTP buffer per peer | 100 packets | ~100ms at 30fps video |
| Jitter buffer | 50-200ms | Configurable |

### Performance Targets

| Operation | Target | Notes |
|-----------|--------|-------|
| Connect peer | <2s | Includes SDP exchange + ICE |
| Audio frame send | <10ms | Encoding + RTP delivery |
| Video frame send | <30ms | Encoding + RTP delivery |
| Broadcast to 10 peers | <100ms | Parallel encoding |
| Session create | <50ms | Manifest parsing + runner init |
| Session close | <1s | Resource cleanup |

---

## Testing

### Unit Tests

```rust
#[tokio::test]
async fn test_connect_peer_success() {
    let transport = WebRtcTransport::new(test_config()).await.unwrap();
    transport.start().await.unwrap();

    let peer_id = transport.connect_peer("test-peer").await.unwrap();
    assert_eq!(peer_id.as_str(), "test-peer");
}

#[tokio::test]
async fn test_broadcast_multiple_peers() {
    let transport = WebRtcTransport::new(test_config()).await.unwrap();
    // Setup multiple connected peers...

    let audio = RuntimeData::Audio { ... };
    let stats = transport.broadcast(&audio).await.unwrap();
    assert_eq!(stats.sent_count, 3);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_streaming() {
    let alice = WebRtcTransport::new(alice_config()).await.unwrap();
    let bob = WebRtcTransport::new(bob_config()).await.unwrap();

    alice.start().await.unwrap();
    bob.start().await.unwrap();

    alice.connect_peer("bob").await.unwrap();
    // Verify connection and streaming...
}
```

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2025-11-07 | Initial contract specification |

---

## See Also

- [Signaling Protocol Contract](./signaling-protocol.md)
- [Sync Manager API Contract](./sync-manager-api.md)
- [WebRTC Research Document](../../transports/remotemedia-webrtc/research.md)
- [Feature Specification](../spec.md)
- [Data Model](../data-model.md)
