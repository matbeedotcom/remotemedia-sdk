# WebRTC Transport Specification

## ADDED Requirements

### Requirement: WebRTC Peer Connection Management
The system SHALL establish and maintain WebRTC peer connections for real-time media and data streaming.

#### Scenario: Establish peer connection automatically
- **GIVEN** a pipeline node configured with host="remote.example.com"
- **WHEN** pipeline execution starts
- **THEN** runtime SHALL automatically establish WebRTC peer connection with signaling

#### Scenario: Handle peer reconnection
- **GIVEN** an established peer connection
- **WHEN** connection drops temporarily
- **THEN** runtime SHALL automatically attempt reconnection with exponential backoff

#### Scenario: Support multiple simultaneous peers
- **GIVEN** a pipeline with nodes on different remote hosts
- **WHEN** pipeline executes
- **THEN** runtime SHALL maintain separate peer connections to each host

### Requirement: Automatic Signaling
The system SHALL handle WebRTC signaling transparently without user configuration.

#### Scenario: Discover signaling server
- **GIVEN** a remote host specified without explicit signaling config
- **WHEN** establishing connection
- **THEN** runtime SHALL attempt default signaling endpoints or use environment config

#### Scenario: Exchange SDP offer/answer
- **GIVEN** two peers initiating connection
- **WHEN** WebRTC handshake begins
- **THEN** runtime SHALL exchange SDP offer/answer via signaling channel

#### Scenario: ICE candidate gathering
- **GIVEN** peers behind NAT
- **WHEN** establishing connection
- **THEN** runtime SHALL gather and exchange ICE candidates until connection succeeds

### Requirement: Data Channel Communication
The system SHALL use WebRTC data channels for structured pipeline messages and control flow.

#### Scenario: Send pipeline control messages
- **GIVEN** an established data channel
- **WHEN** runtime needs to send node execution request
- **THEN** it SHALL serialize request to JSON/msgpack and send via ordered data channel

#### Scenario: Handle large message fragmentation
- **GIVEN** a message exceeding 16KB
- **WHEN** sending via data channel
- **THEN** runtime SHALL fragment message and reassemble on receiver side

#### Scenario: Bidirectional streaming control
- **GIVEN** a streaming pipeline
- **WHEN** data flows in both directions
- **THEN** runtime SHALL maintain separate data channels for upstream and downstream

### Requirement: Media Track Streaming
The system SHALL stream audio and video data via WebRTC media tracks.

#### Scenario: Stream audio from AudioSource node
- **GIVEN** an AudioSource node producing audio frames
- **WHEN** connected to remote peer
- **THEN** runtime SHALL encode frames and send via audio media track

#### Scenario: Receive video from remote pipeline
- **GIVEN** a remote pipeline producing video
- **WHEN** local pipeline consumes it
- **THEN** runtime SHALL receive video track and decode frames for processing

#### Scenario: Adaptive bitrate for media
- **GIVEN** varying network conditions
- **WHEN** streaming media
- **THEN** WebRTC SHALL automatically adjust bitrate and quality

### Requirement: End-to-End Encryption
The system SHALL use DTLS-SRTP for encrypted media and data transmission.

#### Scenario: Encrypt all transmissions by default
- **GIVEN** a WebRTC connection
- **WHEN** media or data is transmitted
- **THEN** all traffic SHALL be encrypted with DTLS-SRTP

#### Scenario: Verify peer fingerprints
- **GIVEN** SDP offer with fingerprint
- **WHEN** establishing connection
- **THEN** runtime SHALL verify certificate fingerprint matches SDP

#### Scenario: Reject unencrypted connections
- **GIVEN** a peer attempting unencrypted connection
- **WHEN** connection negotiation occurs
- **THEN** runtime SHALL reject connection if encryption cannot be established

### Requirement: Transport Fallback
The system SHALL support graceful fallback between WebRTC and gRPC transports.

#### Scenario: Prefer WebRTC for streaming nodes
- **GIVEN** a node with is_streaming=True
- **WHEN** remote execution is configured
- **THEN** runtime SHALL use WebRTC transport if available

#### Scenario: Fall back to gRPC for non-streaming
- **GIVEN** a batch processing node
- **WHEN** WebRTC is unavailable or unsuitable
- **THEN** runtime SHALL use gRPC transport instead

#### Scenario: Explicit transport selection
- **GIVEN** RemoteExecutorConfig with transport="grpc"
- **WHEN** establishing connection
- **THEN** runtime SHALL use gRPC regardless of node streaming capability

### Requirement: STUN/TURN Server Configuration
The system SHALL support STUN/TURN servers for NAT traversal.

#### Scenario: Use default STUN servers
- **GIVEN** no explicit STUN/TURN configuration
- **WHEN** establishing WebRTC connection
- **THEN** runtime SHALL use default public STUN servers (e.g., Google, Mozilla)

#### Scenario: Configure custom TURN server
- **GIVEN** environment variable REMOTEMEDIA_TURN_URL set
- **WHEN** connecting through strict firewall
- **THEN** runtime SHALL use configured TURN server for relay

#### Scenario: Try TURN as fallback
- **GIVEN** direct connection and STUN fail
- **WHEN** ICE gathering completes
- **THEN** runtime SHALL attempt TURN relay if configured

### Requirement: Backpressure and Flow Control
The system SHALL implement backpressure mechanisms for streaming data.

#### Scenario: Pause sender on buffer full
- **GIVEN** a fast producer sending to slow consumer
- **WHEN** receiver buffer reaches threshold
- **THEN** runtime SHALL signal sender to pause via data channel message

#### Scenario: Resume on buffer drain
- **GIVEN** a paused sender
- **WHEN** receiver buffer drops below threshold
- **THEN** runtime SHALL signal sender to resume

#### Scenario: Drop frames under extreme backpressure
- **GIVEN** real-time audio/video with latency requirements
- **WHEN** buffers exceed maximum latency
- **THEN** runtime MAY drop frames to maintain real-time performance

### Requirement: Connection Quality Monitoring
The system SHALL monitor WebRTC connection quality and provide metrics.

#### Scenario: Track packet loss
- **GIVEN** an active WebRTC connection
- **WHEN** packets are lost
- **THEN** runtime SHALL track loss rate per track/channel

#### Scenario: Measure round-trip time
- **GIVEN** an active connection
- **WHEN** RTCP reports arrive
- **THEN** runtime SHALL calculate and expose RTT metrics

#### Scenario: Detect connection degradation
- **GIVEN** a connection with increasing packet loss
- **WHEN** loss exceeds threshold (e.g., 5%)
- **THEN** runtime SHALL log warning and optionally trigger quality reduction

### Requirement: WebRTC Endpoint Registration
The system SHALL allow runtimes to register as WebRTC endpoints for receiving connections.

#### Scenario: Start WebRTC server endpoint
- **GIVEN** `remotemedia serve --webrtc --port 9000`
- **WHEN** server starts
- **THEN** it SHALL listen for WebRTC signaling on specified port

#### Scenario: Register with signaling service
- **GIVEN** a server endpoint starting
- **WHEN** connected to signaling service
- **THEN** it SHALL register its endpoint ID for peer discovery

#### Scenario: Accept incoming peer connections
- **GIVEN** a registered endpoint
- **WHEN** remote peer sends connection request
- **THEN** server SHALL accept connection and establish peer session
