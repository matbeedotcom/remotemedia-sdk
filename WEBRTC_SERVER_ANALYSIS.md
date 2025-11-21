# WebRTC Server Implementation Analysis

## Overview

The RemoteMedia WebRTC server is a sophisticated real-time media streaming transport that handles WebRTC peer connections with gRPC signaling integration. It bridges browser-based WebRTC clients with server-side pipeline processing.

---

## 1. Entry Point: WebRTC Server Binary

**File**: `/Users/mathieugosbee/dev/originals/remotemedia-sdk/transports/remotemedia-webrtc/src/bin/webrtc_server.rs`

### Server Modes

The server supports two operational modes:

#### A. gRPC Signaling Server Mode (Server-Side)
```bash
cargo run --bin webrtc_server --features grpc-signaling -- \
  --mode grpc \
  --grpc-address 0.0.0.0:50051 \
  --manifest ./examples/loopback.yaml
```

- Listens for incoming gRPC connections from WebRTC clients
- Creates a virtual "remotemedia-server" peer that handles all client offers
- Manages the entire peer connection lifecycle server-side

#### B. WebSocket Client Mode (Client-Side)
```bash
cargo run --bin webrtc_server -- \
  --mode websocket \
  --signaling-url ws://localhost:8080
```

- Connects to an external signaling server via WebSocket
- Acts as a client to join existing peer-to-peer networks

### Configuration Options

```rust
struct Args {
    mode: ServerMode,                    // "grpc" or "websocket"
    grpc_address: String,                // Default: 0.0.0.0:50051
    manifest: PathBuf,                   // Pipeline manifest path
    signaling_url: String,               // For WebSocket mode
    stun_servers: Vec<String>,           // STUN server URLs
    max_peers: u32,                      // Default: 10
    enable_data_channel: bool,           // Default: true
    jitter_buffer_ms: u32,               // Default: 100ms
}
```

### Lifecycle

1. **Initialization** (lines 81-152)
   - Parse command-line arguments
   - Set up Ctrl+C handler with graceful shutdown (3-second timeout)
   - Create multi-threaded tokio runtime with CPU-core worker threads
   - Initialize tracing/logging

2. **Mode Selection** (lines 133-149)
   - Route to gRPC server or WebSocket client based on mode

3. **Graceful Shutdown**
   - Ctrl+C handler sets atomic flag
   - Server checks flag periodically and exits cleanly
   - 3-second watchdog ensures forced exit if needed

---

## 2. gRPC Signaling Service

**File**: `/Users/mathieugosbee/dev/originals/remotemedia-sdk/transports/remotemedia-webrtc/src/signaling/grpc/service.rs`

### Architecture

The `WebRtcSignalingService` is the core gRPC service that handles bidirectional signaling:

```
┌─────────────────────────────────────┐
│  gRPC Client (Browser)              │
│  ├─ announce (register peer)        │
│  ├─ offer (SDP with capabilities)   │
│  ├─ ice_candidate (connectivity)    │
│  └─ answer (bidirectional stream)   │
└─────────────────────────────────────┘
        ↓ (tonic bidirectional stream)
┌─────────────────────────────────────┐
│  WebRtcSignalingService             │
│  ├─ peers registry                  │
│  ├─ pending_offers buffer           │
│  ├─ server_peers (ServerPeer mgmt)  │
│  └─ signal() handler (main loop)    │
└─────────────────────────────────────┘
        ↓
┌─────────────────────────────────────┐
│  ServerPeer (per client)            │
│  ├─ WebRTC peer connection          │
│  ├─ Pipeline session handle         │
│  └─ Media routing task              │
└─────────────────────────────────────┘
```

### Key Data Structures

```rust
struct PeerConnection {
    peer_id: String,
    capabilities: Option<PeerCapabilities>,
    metadata: HashMap<String, String>,
    state: i32,                        // PeerState enum
    connected_at: u64,
    tx: mpsc::Sender<Result<SignalingResponse, Status>>,  // Response channel
}

struct PendingOffer {
    from_peer_id: String,
    to_peer_id: String,
    sdp: String,
    timestamp: SystemTime,
}

pub struct WebRtcSignalingService {
    peers: Arc<RwLock<HashMap<String, PeerConnection>>>,
    pending_offers: Arc<RwLock<HashMap<String, PendingOffer>>>,
    server_peers: Arc<RwLock<HashMap<String, Arc<ServerPeer>>>>,  // CRITICAL
    config: Arc<WebRtcTransportConfig>,
    runner: Arc<PipelineRunner>,
    manifest: Arc<Manifest>,
    start_time: SystemTime,
}
```

---

## 3. Request Handling Flow

### 3.1 Announce Request (Peer Registration)

**Handler**: `handle_announce()` (lines 339-425)

```
Client sends: AnnounceRequest {
    peer_id: "client-abc123",
    capabilities: { audio: true, video: true, data: true },
    metadata: { ... }
}

Server response:
1. Check if peer already exists (prevent duplicates)
2. Get list of other connected peers
3. Register peer in global registry
4. Store response channel (tx) for future messages
5. Send acknowledgment back to peer
6. Broadcast PeerJoined notification to all other peers
```

**Key code** (lines 347-370):
```rust
let peer_conn = PeerConnection {
    peer_id: announce.peer_id.clone(),
    capabilities: announce.capabilities.clone(),
    metadata: announce.metadata.clone(),
    state: PeerState::Available as i32,
    connected_at: Self::current_timestamp(),
    tx: tx.clone(),  // CRITICAL: Store channel for future messages
};

peers_write.insert(announce.peer_id.clone(), peer_conn);
```

---

### 3.2 Offer Request (SDP Exchange)

**Handler**: `handle_offer()` (lines 427-592)

#### Two Paths Based on Target Peer:

#### Path A: Offer to "remotemedia-server" (Server-Side Processing)

This is the **primary server mode** (lines 466-535):

```
Client offer to "remotemedia-server"
        ↓
    Create ServerPeer {
        peer_id: "client-abc123",
        config: WebRtcTransportConfig,
        runner: PipelineRunner,
        manifest: Manifest,
    }
        ↓
    ServerPeer::handle_offer(sdp: String) {
        1. Create pipeline session
        2. Add audio track to peer connection
        3. Set up bidirectional media routing
        4. Set remote description (parse client offer)
        5. Create SDP answer
        6. Return answer.sdp
    }
        ↓
    Server sends: AnswerNotification {
        from_peer_id: "remotemedia-server",
        sdp: answer_sdp,
        r#type: "answer",
    }
        ↓
    Client receives answer and completes WebRTC handshake
```

**Key code** (lines 469-491):
```rust
let server_peer = match ServerPeer::new(
    from_peer_id.clone(),
    &**config,
    Arc::clone(runner),
    Arc::clone(manifest),
).await {
    Ok(peer) => Arc::new(peer),
    Err(e) => {
        error!("Failed to create ServerPeer: {}", e);
        return Err(Status::internal(...));
    }
};

let answer_sdp = match server_peer.handle_offer(offer.sdp).await {
    Ok(sdp) => sdp,
    Err(e) => {
        error!("ServerPeer failed to handle offer: {}", e);
        return Err(Status::internal(...));
    }
};

// Store server peer for ICE candidates
server_peers.write().await.insert(from_peer_id.clone(), Arc::clone(&server_peer));
```

#### Path B: Peer-to-Peer Offer Forwarding (P2P Mode)

For offers to other peers (lines 536-589):
- Look up target peer in registry
- Forward offer as OfferNotification to target peer
- Target peer responds with their own answer

---

### 3.3 ICE Candidate Handling

**Handler**: `handle_ice_candidate()` (lines 657-784)

#### Path A: ICE Candidates for "remotemedia-server"

```
Client sends: IceCandidateRequest {
    to_peer_id: "remotemedia-server",
    candidate: "candidate:...",
    sdp_mid: "0",
    sdp_mline_index: 0,
}
        ↓
    Lookup ServerPeer from server_peers registry
        ↓
    ServerPeer::handle_ice_candidate() {
        1. Create RTCIceCandidateInit from candidate string
        2. Add to underlying WebRTC peer connection
        3. Triggers ICE connectivity checks
    }
        ↓
    Send acknowledgment back to client
```

**Key code** (lines 671-694):
```rust
if ice.to_peer_id == "remotemedia-server" {
    let server_peers_read = server_peers.read().await;
    
    if let Some(server_peer) = server_peers_read.get(from_peer_id) {
        let server_peer = Arc::clone(server_peer);
        drop(server_peers_read);
        
        match server_peer.handle_ice_candidate(
            ice.candidate.clone(),
            if ice.sdp_mid.is_empty() { None } else { Some(ice.sdp_mid) },
            Some(ice.sdp_mline_index as u16),
        ).await {
            Ok(_) => {
                info!("ICE candidate added to ServerPeer for {}", from_peer_id);
            }
            Err(e) => {
                error!("Failed to add ICE candidate: {}", e);
            }
        }
    } else {
        // ServerPeer not ready yet - silently ignore (candidates may arrive early)
        warn!("ICE candidate arrived before ServerPeer was created");
    }
}
```

#### Path B: P2P ICE Forwarding

For ICE candidates targeting other peers, forward as IceCandidateNotification.

---

### 3.4 Answer Handling

**Handler**: `handle_answer()` (lines 594-655)

```
Client sends: AnswerRequest {
    to_peer_id: "client-xyz789",
    sdp: "v=0\no=...\n...",
    r#type: "answer",
}
        ↓
    Look up target peer in registry
        ↓
    Forward as AnswerNotification to target peer
```

Used in P2P mode to complete handshake.

---

## 4. Server-Side Peer Management (ServerPeer)

**File**: `/Users/mathieugosbee/dev/originals/remotemedia-sdk/transports/remotemedia-webrtc/src/peer/server_peer.rs`

### Purpose

ServerPeer is the **server-side representation** of a connected client. It handles:
1. WebRTC peer connection lifecycle
2. SDP offer/answer exchange
3. Audio track setup
4. Bidirectional media routing between WebRTC and pipeline

### Architecture

```
Client (Browser)
    ↓ (WebRTC peer connection + RTP packets)
ServerPeer {
    peer_connection: PeerConnection,
    runner: PipelineRunner,
    manifest: Manifest,
    shutdown_tx: mpsc::Sender<()>,
}
    ├─ handle_offer(sdp) → creates pipeline session
    ├─ setup_media_routing_and_data_channel()
    ├─ handle_ice_candidate(candidate)
    └─ shutdown()
```

### Key Methods

#### 4.1 ServerPeer::new() (lines 58-80)

```rust
pub async fn new(
    peer_id: String,
    config: &WebRtcTransportConfig,
    runner: Arc<PipelineRunner>,
    manifest: Arc<Manifest>,
) -> Result<Self> {
    // 1. Create underlying WebRTC peer connection
    let peer_connection = Arc::new(PeerConnection::new(peer_id.clone(), config).await?);
    
    // 2. Create shutdown channel for coordinated teardown
    let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
    
    Ok(Self {
        peer_id,
        peer_connection,
        runner,
        manifest,
        shutdown_tx,
        shutdown_rx: Arc::new(RwLock::new(Some(shutdown_rx))),
    })
}
```

#### 4.2 ServerPeer::handle_offer() (lines 94-147)

**Critical flow for establishing connection**:

```rust
pub async fn handle_offer(&self, offer_sdp: String) -> Result<String> {
    // Step 1: Create pipeline session for this peer
    let session_handle = self.runner
        .create_stream_session(Arc::clone(&self.manifest))
        .await?;
    info!("Created pipeline session for peer {}", self.peer_id);
    
    // Step 2: Configure and add audio track (Opus @ 48kHz, mono, 64kbps)
    let audio_config = crate::media::audio::AudioEncoderConfig {
        sample_rate: 48000,
        channels: 1,
        bitrate: 64000,
        complexity: 10,
    };
    
    self.peer_connection.add_audio_track(audio_config).await?;
    info!("Added audio track to peer connection");
    
    // Step 3: Set up bidirectional media routing and data channel
    // This spawns background tasks for continuous data transfer
    self.setup_media_routing_and_data_channel(session_handle).await?;
    
    // Step 4: Parse and set the client's SDP offer as remote description
    let offer = RTCSessionDescription::offer(offer_sdp)?;
    self.peer_connection
        .peer_connection()
        .set_remote_description(offer)
        .await?;
    
    // Step 5: Create and set our answer
    let answer = self.peer_connection
        .peer_connection()
        .create_answer(None)
        .await?;
    
    self.peer_connection
        .peer_connection()
        .set_local_description(answer.clone())
        .await?;
    
    info!("Generated SDP answer for peer {}", self.peer_id);
    Ok(answer.sdp)
}
```

#### 4.3 setup_media_routing_and_data_channel() (lines 149-355)

**Establishes bidirectional media flow**:

```
┌─────────────────────────────────────────────────────────┐
│  WebRTC Client                                          │
│  ├─ Audio track (incoming RTP Opus)                    │
│  ├─ Audio track (outgoing RTP Opus)                    │
│  └─ Data channel (Protobuf messages)                   │
└────────────────┬────────────────────────────────────────┘
                 │ (RTP packets)
┌────────────────▼────────────────────────────────────────┐
│  ServerPeer (Rust async context)                        │
│  ├─ Data Channel Handler                               │
│  │   └─ on_message(): Protobuf → RuntimeData            │
│  ├─ Track Handlers                                       │
│  │   ├─ on_track(): RTP → decode Opus → RuntimeData    │
│  │   └─ Audio reception task (per track)               │
│  └─ Media Routing Task (select! loop)                  │
│      ├─ Recv inputs from WebRTC                        │
│      ├─ Send to pipeline                                │
│      ├─ Recv outputs from pipeline                      │
│      └─ Send to WebRTC                                  │
└────────────────┬────────────────────────────────────────┘
                 │ (RuntimeData)
┌────────────────▼────────────────────────────────────────┐
│  Pipeline Session (StreamSessionHandle)                │
│  ├─ send_input(TransportData) → pipeline input         │
│  └─ recv_output() → pipeline output                     │
└──────────────────────────────────────────────────────────┘
```

##### A. Data Channel Handler (lines 163-214)

```rust
self.peer_connection
    .peer_connection()
    .on_data_channel(Box::new(move |data_channel| {
        // Called when data channel opens
        
        // Set message handler
        data_channel.on_message(Box::new(move |msg| {
            // 1. Deserialize Protobuf DataBuffer from msg.data
            let data_buffer = crate::generated::DataBuffer::decode(&msg.data[..])?;
            
            // 2. Convert DataBuffer → RuntimeData
            let runtime_data = crate::adapters::data_buffer_to_runtime_data(&data_buffer)?;
            
            // 3. Wrap in TransportData (with metadata)
            let transport_data = TransportData {
                data: runtime_data,
                sequence: None,
                metadata: HashMap::new(),
            };
            
            // 4. Forward to pipeline via dc_input_tx channel
            dc_input_tx.send(transport_data).await?;
        }));
    }));
```

##### B. Audio Track Handler (lines 216-288)

```rust
// on_track is called when remote peer adds an audio track
self.peer_connection.on_track(move |track, _receiver, _transceiver| {
    Box::pin(async move {
        // Filter for audio tracks only
        if track.kind() != RTPCodecType::Audio {
            return;
        }
        
        // Spawn task to continuously read and decode audio
        tokio::spawn(async move {
            loop {
                // Read RTP packet from remote track
                let (rtp_packet, _) = track.read_rtp().await?;
                
                // Decode Opus to f32 samples @ 48kHz
                match audio_track.on_rtp_packet(&rtp_packet.payload).await {
                    Ok(samples) => {
                        // Create RuntimeData::Audio
                        let transport_data = TransportData {
                            data: RuntimeData::Audio {
                                samples,
                                sample_rate: 48000,
                                channels: 1,
                            },
                            sequence: None,
                            metadata: HashMap::new(),
                        };
                        
                        // Forward to pipeline
                        dc_input_tx.send(transport_data).await?;
                    }
                    Err(e) => {
                        warn!("Opus decode error: {}", e);
                        // Continue processing next packets
                    }
                }
            }
        });
    })
});
```

##### C. Media Routing Task (lines 296-352)

**Main event loop** using `tokio::select!` for bidirectional multiplexing:

```rust
tokio::spawn(async move {
    loop {
        tokio::select! {
            biased;  // Prioritize shutdown and inputs over outputs
            
            // Shutdown signal (highest priority)
            _ = shutdown_rx.recv() => {
                info!("Shutting down media routing for peer {}", peer_id);
                break;
            }
            
            // Inputs from WebRTC (high priority)
            Some(transport_data) = dc_input_rx.recv() => {
                debug!("Forwarding WebRTC input to pipeline");
                session_handle.send_input(transport_data).await?;
            }
            
            // Outputs from pipeline (with timeout to avoid blocking inputs)
            output_result = tokio::time::timeout(
                Duration::from_millis(10),
                session_handle.recv_output()
            ) => {
                match output_result {
                    Ok(Ok(Some(transport_data))) => {
                        // Send to WebRTC
                        Self::send_to_webrtc(&peer_connection, transport_data).await?;
                    }
                    Ok(Ok(None)) => {
                        // Output temporarily empty - normal behavior
                        // DON'T break loop
                    }
                    Err(_timeout) => {
                        // Timeout - normal, continue checking inputs
                    }
                }
            }
        }
    }
});
```

Key design decisions:
- **biased**: Prioritizes shutdown and inputs over outputs to prevent deadlock
- **10ms timeout**: Avoids blocking input processing while checking for outputs
- **None handling**: Don't break on empty outputs (channels can be temporarily empty)

#### 4.4 send_to_webrtc() (lines 357-390)

Converts pipeline output (RuntimeData) to RTP packets and sends via WebRTC:

```rust
async fn send_to_webrtc(
    peer_connection: &Arc<PeerConnection>,
    transport_data: TransportData,
) -> Result<()> {
    match &transport_data.data {
        RuntimeData::Audio { samples, sample_rate, channels } => {
            if let Some(audio_track) = peer_connection.audio_track().await {
                // Encode and send audio samples via RTP
                audio_track.send_audio(Arc::new(samples.clone()), *sample_rate).await?;
            }
        }
        RuntimeData::Video { .. } => {
            // TODO: Implement video transmission
        }
        _ => {
            debug!("Unsupported RuntimeData type");
        }
    }
    Ok(())
}
```

#### 4.5 handle_ice_candidate() (lines 392-426)

Adds ICE candidates from client to WebRTC peer connection:

```rust
pub async fn handle_ice_candidate(
    &self,
    candidate: String,
    sdp_mid: Option<String>,
    sdp_mline_index: Option<u16>,
) -> Result<()> {
    // Create ICE candidate init from candidate string
    let ice_candidate_init = RTCIceCandidateInit {
        candidate: candidate.clone(),
        sdp_mid,
        sdp_mline_index,
        username_fragment: None,
    };
    
    // Add to WebRTC peer connection (triggers ICE connectivity checks)
    self.peer_connection
        .peer_connection()
        .add_ice_candidate(ice_candidate_init)
        .await?;
    
    info!("ICE candidate added successfully");
    Ok(())
}
```

---

## 5. WebRTC Peer Connection

**File**: `/Users/mathieugosbee/dev/originals/remotemedia-sdk/transports/remotemedia-webrtc/src/peer/connection.rs`

### Purpose

Low-level WebRTC peer connection management with audio/video track support.

### Key Methods

#### 5.1 PeerConnection::new() (lines 88-196)

```rust
pub async fn new(peer_id: String, config: &WebRtcTransportConfig) -> Result<Self> {
    // 1. Create MediaEngine with default codecs (Opus, VP8, VP9, H.264)
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    
    // 2. Register interceptors (RTCP, bandwidth estimation, etc.)
    let interceptor_registry = register_default_interceptors(
        Default::default(),
        &mut media_engine
    )?;
    
    // 3. Build WebRTC API
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(interceptor_registry)
        .build();
    
    // 4. Configure ICE servers (STUN/TURN)
    let ice_servers: Vec<RTCIceServer> = config
        .stun_servers
        .iter()
        .map(|url| RTCIceServer {
            urls: vec![url.clone()],
            ..Default::default()
        })
        .chain(config.turn_servers.iter().map(|turn| RTCIceServer {
            urls: vec![turn.url.clone()],
            username: turn.username.clone(),
            credential: turn.credential.clone(),
            ..Default::default()
        }))
        .collect();
    
    // 5. Create peer connection
    let rtc_config = RTCConfiguration { ice_servers, ..Default::default() };
    let peer_connection = Arc::new(api.new_peer_connection(rtc_config).await?);
    
    // 6. Set up connection state change handler
    peer_connection.on_peer_connection_state_change(Box::new(move |s| {
        // Update connection state (New → Connecting → Connected → Closed)
        // Record connected_at timestamp when state reaches Connected
    }));
    
    Ok(Self { ... })
}
```

#### 5.2 add_audio_track() (lines 390-423)

Creates an audio track for sending Opus-encoded audio:

```rust
pub async fn add_audio_track(&self, config: AudioEncoderConfig) -> Result<Arc<AudioTrack>> {
    // 1. Create TrackLocalStaticSample with Opus codec parameters
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: "audio/opus".to_string(),
            clock_rate: config.sample_rate,        // Usually 48000
            channels: config.channels,             // Usually 1 (mono)
            sdp_fmtp_line: String::new(),
            rtcp_feedback: vec![],
        },
        format!("audio-{}", self.peer_id),
        format!("stream-{}", self.connection_id),
    ));
    
    // 2. Add track to peer connection
    // This modifies the pending offer/answer SDP
    let sender = self
        .peer_connection
        .add_track(track.clone() as Arc<dyn TrackLocal + Send + Sync>)
        .await?;
    
    // 3. Create AudioTrack wrapper with encoder/decoder
    let audio_track = Arc::new(AudioTrack::new(track, config)?);
    
    // 4. Store for later access
    *self.audio_track.write().await = Some(audio_track.clone());
    *self.audio_sender.write().await = Some(sender);
    
    Ok(audio_track)
}
```

#### 5.3 on_track() (lines 513-519)

Register handler for incoming remote tracks (receiving audio from client):

```rust
pub async fn on_track<F>(&self, handler: F)
where
    F: Fn(Arc<TrackRemote>, Arc<RTCRtpReceiver>, Arc<RTCRtpTransceiver>)
        -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send + Sync + 'static,
{
    self.peer_connection.on_track(Box::new(handler));
}
```

---

## 6. Audio Track Management

**File**: `/Users/mathieugosbee/dev/originals/remotemedia-sdk/transports/remotemedia-webrtc/src/media/tracks.rs`

### AudioTrack (lines 16-167)

```rust
pub struct AudioTrack {
    track: Arc<TrackLocalStaticSample>,     // WebRTC track
    encoder: Arc<RwLock<AudioEncoder>>,     // Opus encoder
    decoder: Arc<RwLock<AudioDecoder>>,     // Opus decoder
    timestamp: Arc<RwLock<u32>>,            // RTP timestamp
}
```

#### send_audio() (lines 65-129)

Sends audio samples through WebRTC:

```rust
pub async fn send_audio(&self, samples: Arc<Vec<f32>>, sample_rate: u32) -> Result<()> {
    // 1. Check if encoder needs recreation for sample rate change
    {
        let encoder = self.encoder.read().await;
        if encoder.config.sample_rate != sample_rate {
            let mut encoder_write = self.encoder.write().await;
            *encoder_write = AudioEncoder::new(AudioEncoderConfig {
                sample_rate,
                channels: encoder_write.config.channels,
                bitrate: encoder_write.config.bitrate,
                complexity: encoder_write.config.complexity,
            })?;
        }
    }
    
    // 2. Calculate frame size for 20ms @ given sample rate
    let frame_size = (sample_rate as usize * 20) / 1000;  // 20ms frame
    
    // 3. Process audio in chunks (Opus requires exact frame sizes)
    for chunk in samples.chunks(frame_size) {
        // Pad last chunk to frame size if needed
        let samples_to_encode = if chunk.len() < frame_size {
            let mut padded = chunk.to_vec();
            padded.resize(frame_size, 0.0);  // Pad with silence
            padded
        } else {
            chunk.to_vec()
        };
        
        // 4. Encode chunk with Opus
        let encoded = self.encoder.write().await.encode(&samples_to_encode)?;
        
        // 5. Update RTP timestamp
        let mut ts = self.timestamp.write().await;
        *ts = ts.wrapping_add(chunk.len() as u32);
        
        // 6. Create Sample and send via WebRTC
        let sample = Sample {
            data: encoded.into(),
            duration: Duration::from_millis(20),
            timestamp: SystemTime::now(),
            ..Default::default()
        };
        
        self.track.write_sample(&sample).await?;
    }
    
    Ok(())
}
```

#### on_rtp_packet() (lines 150-161)

Decodes received RTP packets:

```rust
pub async fn on_rtp_packet(&self, payload: &[u8]) -> Result<Vec<f32>> {
    // Decode Opus RTP payload to f32 samples @ 48kHz
    let samples = self.decoder.write().await.decode(payload)?;
    Ok(samples)
}
```

---

## 7. Audio Codec Support (Opus)

**File**: `/Users/mathieugosbee/dev/originals/remotemedia-sdk/transports/remotemedia-webrtc/src/media/audio.rs`

### AudioEncoder (lines 32-107)

```rust
pub struct AudioEncoder {
    config: AudioEncoderConfig,
    encoder: opus::Encoder,  // Opus FFI binding
}

pub fn encode(&mut self, samples: &[f32]) -> Result<Vec<u8>> {
    // 1. Validate input samples are in range [-1.0, 1.0]
    const MAX_PACKET_SIZE: usize = 4000;
    let mut output = vec![0u8; MAX_PACKET_SIZE];
    
    // 2. Encode with Opus library
    let len = self.encoder.encode_float(samples, &mut output)?;
    
    // 3. Truncate to actual size
    output.truncate(len);
    Ok(output)
}
```

Default configuration:
- **Sample rate**: 48000 Hz (also supports 24000, 16000)
- **Channels**: 1 (mono) or 2 (stereo)
- **Bitrate**: 64000 bps (typical for voice)
- **Complexity**: 10 (maximum quality)
- **Application**: VOIP

---

## 8. Data Flow Summary

### Complete Request-Response Cycle

```
┌────────────────────────────────────────────────────────────────┐
│ 1. Client announces via gRPC signal()                          │
│    - Sends: AnnounceRequest { peer_id, capabilities, metadata }│
│    - Server: Registers peer, stores response channel           │
│    - Server: Broadcasts PeerJoined to other peers             │
└────────────────────────────────────────────────────────────────┘
                              ↓
┌────────────────────────────────────────────────────────────────┐
│ 2. Client sends SDP offer to "remotemedia-server"              │
│    - Sends: OfferRequest { to_peer_id, sdp }                  │
│    - Server: Creates ServerPeer with pipeline session          │
│    - Server: Adds audio track (Opus @ 48kHz, 64kbps)           │
│    - Server: Sets up bidirectional media routing               │
│    - Server: Parses offer as remote description                │
│    - Server: Generates answer                                  │
│    - Server: Sends AnswerNotification back                     │
└────────────────────────────────────────────────────────────────┘
                              ↓
┌────────────────────────────────────────────────────────────────┐
│ 3. Client sends ICE candidates                                 │
│    - Sends: IceCandidateRequest { to_peer_id, candidate, ... } │
│    - Server: Looks up ServerPeer                               │
│    - Server: Adds candidate to WebRTC peer connection          │
│    - ICE: Gathers candidates, completes connectivity checks    │
│    - WebRTC: Establishes DTLS connection                       │
└────────────────────────────────────────────────────────────────┘
                              ↓
┌────────────────────────────────────────────────────────────────┐
│ 4. Media Flow (Bidirectional)                                  │
│                                                                 │
│    Incoming (Client → Server → Pipeline):                       │
│    - Client RTP Opus packets → ServerPeer audio track          │
│    - on_track handler reads RTP, decodes Opus → f32 samples   │
│    - Audio sent to pipeline via TransportData                  │
│    - Pipeline processes (VAD, TTS, etc.)                       │
│                                                                 │
│    Outgoing (Pipeline → Server → Client):                       │
│    - Pipeline output RuntimeData::Audio                        │
│    - send_to_webrtc() encodes Opus, creates RTP Sample         │
│    - write_sample() sends via WebRTC track                     │
│    - Client receives RTP packets and plays audio               │
│                                                                 │
│    Data Channel:                                                │
│    - Receives Protobuf DataBuffer on data channel              │
│    - on_message handler deserializes → RuntimeData             │
│    - Forwards to pipeline                                      │
└────────────────────────────────────────────────────────────────┘
```

---

## 9. Key Configuration Parameters

### WebRtcTransportConfig

```rust
pub struct WebRtcTransportConfig {
    // Signaling
    pub signaling_url: String,              // gRPC server address
    pub stun_servers: Vec<String>,          // ICE connectivity
    pub turn_servers: Vec<TurnServerConfig>,
    
    // Limits
    pub max_peers: u32,                     // Default: 10
    
    // Media
    pub enable_data_channel: bool,          // Default: true
    pub audio_codec: AudioCodec,            // Only: Opus
    pub video_codec: VideoCodec,            // VP9, H.264
    pub video_resolution: VideoResolution,  // 640x480, 1280x720, etc.
    
    // Buffer
    pub jitter_buffer_size_ms: u32,         // Default: 100ms
}
```

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

---

## 10. Error Handling

All errors map to `remotemedia_webrtc::Error` enum:

```rust
pub enum Error {
    WebRtcError(String),
    SdpError(String),
    IceCandidateError(String),
    MediaTrackError(String),
    EncodingError(String),
    InvalidConfig(String),
    InternalError(String),
    PeerConnectionError(String),
}
```

gRPC errors are converted to `tonic::Status`:
- `Status::invalid_argument()` - Invalid request format
- `Status::already_exists()` - Peer already announced
- `Status::not_found()` - Target peer not found
- `Status::failed_precondition()` - Announce required first
- `Status::internal()` - WebRTC/pipeline errors

---

## 11. Critical Design Patterns

### Pattern 1: Arc<RwLock<>> for Shared State

```rust
peers: Arc<RwLock<HashMap<String, PeerConnection>>>
server_peers: Arc<RwLock<HashMap<String, Arc<ServerPeer>>>>
```

Allows multiple concurrent readers, exclusive writers.

### Pattern 2: tokio::select! with biased

```rust
tokio::select! {
    biased;  // Process shutdown and inputs before outputs
    
    _ = shutdown_rx.recv() => { break; }
    Some(data) = input_rx.recv() => { send_to_pipeline(); }
    _ = timeout(..., output_rx.recv()) => { send_to_webrtc(); }
}
```

Prevents deadlocks by prioritizing shutdown and inputs.

### Pattern 3: Response Channel in Peer Registry

```rust
tx: mpsc::Sender<Result<SignalingResponse, Status>>
```

Allows service to send notifications to specific peers asynchronously.

### Pattern 4: Continuous Output Draining

```rust
// Media routing task continuously polls for pipeline outputs
// Independent of input processing (fire-and-forget)
let output_result = tokio::time::timeout(
    Duration::from_millis(10),
    session_handle.recv_output()
).await;
```

Ensures outputs reach client even during slow input processing.

---

## 12. Files Summary

| File | Lines | Purpose |
|------|-------|---------|
| `src/bin/webrtc_server.rs` | 287 | Server entry point, mode selection, shutdown handling |
| `src/signaling/grpc/service.rs` | 866 | gRPC service, peer registry, request routing |
| `src/peer/server_peer.rs` | 478 | Server-side peer, pipeline integration, media routing |
| `src/peer/connection.rs` | 614 | Low-level WebRTC peer connection, track management |
| `src/media/tracks.rs` | 463 | Audio/video track encoding/decoding |
| `src/media/audio.rs` | 211 | Opus codec wrapper |
| `src/lib.rs` | ~100 | Module organization, public API |

---

## 13. Feature Flags

- `grpc-signaling` - Enables gRPC server implementation (required for server mode)
- `opus-codec` - Audio codec support (always enabled for WebRTC)

