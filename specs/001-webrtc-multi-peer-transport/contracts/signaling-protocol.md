# WebRTC Signaling Protocol Specification

**Version:** 1.0.0
**Protocol**: JSON-RPC 2.0 over WebSocket
**Status:** Specification
**Created:** 2025-11-07
**Last Updated:** 2025-11-07

## Overview

This contract defines the JSON-RPC 2.0 signaling protocol used by WebRTC peers to discover each other, exchange SDP offers/answers, and negotiate ICE candidates for establishing P2P connections.

**Scope**: Signaling only (not media transport)
**Transport**: WebSocket (ws:// or wss://)
**Encryption**: TLS/SSL for wss:// endpoints (recommended for production)

---

## Protocol Fundamentals

### Message Format

All signaling messages conform to JSON-RPC 2.0 specification (RFC 7919).

```json
{
  "jsonrpc": "2.0",
  "method": "method_name",
  "params": { "param1": "value1" },
  "id": "request-id-123"
}
```

### Message Types

1. **Request**: `method` + `params` + `id` (expects response)
2. **Notification**: `method` + `params` (no `id`, fire-and-forget)
3. **Response**: `result` + `id` (responds to request)
4. **Error Response**: `error` + `id` (error response to request)

### Standard Error Codes

```rust
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;          // Invalid JSON
    pub const INVALID_REQUEST: i32 = -32600;      // Malformed request
    pub const METHOD_NOT_FOUND: i32 = -32601;     // Unknown method
    pub const INVALID_PARAMS: i32 = -32602;       // Bad parameters
    pub const INTERNAL_ERROR: i32 = -32603;       // Server error

    // WebRTC specific
    pub const PEER_NOT_FOUND: i32 = -32000;
    pub const OFFER_INVALID: i32 = -32002;
    pub const ANSWER_INVALID: i32 = -32003;
    pub const ICE_CANDIDATE_INVALID: i32 = -32004;
    pub const SESSION_LIMIT_EXCEEDED: i32 = -32005;
}
```

---

## Signaling Phases

### Phase 1: Peer Discovery & Announcement

**Purpose**: Register peer with signaling server and discover other peers

#### Method: `peer.announce`

**Direction**: Client → Server (request)

**Purpose**: Register local peer with capabilities

```json
{
  "jsonrpc": "2.0",
  "method": "peer.announce",
  "params": {
    "peer_id": "peer-alice-uuid-123",
    "capabilities": ["audio", "video", "data"],
    "user_data": {
      "name": "Alice",
      "location": "US-West"
    }
  },
  "id": "announce-1"
}
```

**Parameters**:
- `peer_id` (string, required): Unique peer identifier
  - Format: UUID or `peer-{random}`
  - Must be unique within signaling server namespace
  - Max length: 64 characters
  - Pattern: `^[a-zA-Z0-9-_]{1,64}$`

- `capabilities` (array, required): Supported media types
  - Valid values: `"audio"`, `"video"`, `"data"`
  - At least one required
  - Examples: `["audio"]`, `["audio", "video"]`, `["video", "data"]`

- `user_data` (object, optional): Application-specific metadata
  - Max size: 1 KB
  - Can be any JSON structure
  - Server may broadcast this to other peers

**Success Response** (from server to client):
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "registered",
    "server_time": "2025-11-07T10:30:00Z",
    "peer_id": "peer-alice-uuid-123",
    "session_key": "abc123def456"
  },
  "id": "announce-1"
}
```

**Response Fields**:
- `status` (string): Always "registered"
- `server_time` (string, ISO 8601): Server timestamp
- `peer_id` (string): Echoed back for confirmation
- `session_key` (string): Optional session identifier for reconnection

**Error Response**:
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32000,
    "message": "Peer ID already registered",
    "data": {
      "registered_at": "2025-11-07T10:29:00Z",
      "registered_from": "192.168.1.100"
    }
  },
  "id": "announce-1"
}
```

**Timeout**: 10 seconds

**Broadcast Notification** (from server to all other peers):
```json
{
  "jsonrpc": "2.0",
  "method": "peer.announced",
  "params": {
    "peer_id": "peer-alice-uuid-123",
    "capabilities": ["audio", "video", "data"],
    "user_data": { "name": "Alice" },
    "announced_at": "2025-11-07T10:30:00Z"
  }
}
```

**Note**: Notification has no `id` (fire-and-forget)

---

### Phase 2: Offer/Answer Exchange (Trickle ICE)

#### Method: `peer.offer`

**Direction**: Client A → Server → Client B (request/response)

**Purpose**: Send SDP offer to initiate connection

**Timeline**:
1. Client A calls `create_offer()` on local RTCPeerConnection
2. Client A sends SDP via `peer.offer` (does NOT wait for ICE gathering complete)
3. Client A's ICE candidates trickling in parallel via `peer.ice_candidate`
4. Server forwards offer to Client B
5. Client B receives `peer.offer` notification
6. Client B calls `create_answer()` and sends back
7. ICE candidates trickled from both sides simultaneously

```json
{
  "jsonrpc": "2.0",
  "method": "peer.offer",
  "params": {
    "from": "peer-alice-uuid-123",
    "to": "peer-bob-uuid-456",
    "sdp": "v=0\r\no=- 1234567890 1234567890 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n...",
    "can_trickle_ice_candidates": true,
    "request_id": "req-offer-001"
  },
  "id": "offer-1"
}
```

**Parameters**:
- `from` (string, required): Sender peer ID
- `to` (string, required): Recipient peer ID
- `sdp` (string, required): SDP offer text (multiline)
  - Must be valid SDP format (RFC 4566)
  - Must contain `m=` media lines for each media type
  - Can be up to 64 KB
- `can_trickle_ice_candidates` (boolean, required): Support for trickle ICE
  - If true: recipient can send ICE candidates incrementally
  - If false: recipient must send all candidates before sending answer
- `request_id` (string, required): Unique request ID for tracking
  - Used to match ICE candidates with this offer
  - Format: `req-{uuid}` or similar

**Server Handling**:
1. Server validates `to` peer is registered
2. Server forwards to recipient as notification
3. Server waits for `peer.answer` or timeout (30 seconds)
4. Sends response to sender

**Success Response** (to sender):
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "forwarded",
    "request_id": "req-offer-001"
  },
  "id": "offer-1"
}
```

**Error Response**:
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32000,
    "message": "Peer not found",
    "data": {
      "peer_id": "peer-bob-uuid-456"
    }
  },
  "id": "offer-1"
}
```

**Notification to Recipient** (Bob receives):
```json
{
  "jsonrpc": "2.0",
  "method": "peer.offer",
  "params": {
    "from": "peer-alice-uuid-123",
    "to": "peer-bob-uuid-456",
    "sdp": "v=0\r\n...",
    "can_trickle_ice_candidates": true,
    "request_id": "req-offer-001"
  }
}
```

**Recipient Action**:
1. Parse SDP
2. Call `set_remote_description(offer)`
3. Call `create_answer()`
4. Get local answer SDP
5. Call `set_local_description(answer)`
6. Send `peer.answer` back

---

#### Method: `peer.answer`

**Direction**: Client B → Server → Client A (response to offer)

**Purpose**: Send SDP answer to accept connection

```json
{
  "jsonrpc": "2.0",
  "method": "peer.answer",
  "params": {
    "from": "peer-bob-uuid-456",
    "to": "peer-alice-uuid-123",
    "sdp": "v=0\r\no=- 2345678901 2345678901 IN IP4 127.0.0.2\r\ns=-\r\nt=0 0\r\n...",
    "can_trickle_ice_candidates": true,
    "request_id": "req-offer-001"
  },
  "id": "answer-1"
}
```

**Parameters**:
- `from` (string, required): Responder peer ID
- `to` (string, required): Offerer peer ID
- `sdp` (string, required): SDP answer text
- `can_trickle_ice_candidates` (boolean, required): Trickle ICE support
- `request_id` (string, required): MUST match the offer's request_id

**Server Handling**:
1. Validates offer exists for this request_id
2. Forwards answer to original offerer
3. Sends response to responder

**Success Response** (to Bob):
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "forwarded",
    "request_id": "req-offer-001"
  },
  "id": "answer-1"
}
```

**Notification to Offerer** (Alice receives):
```json
{
  "jsonrpc": "2.0",
  "method": "peer.answer",
  "params": {
    "from": "peer-bob-uuid-456",
    "to": "peer-alice-uuid-123",
    "sdp": "v=0\r\n...",
    "can_trickle_ice_candidates": true,
    "request_id": "req-offer-001"
  }
}
```

**Offerer Action**:
1. Parse SDP
2. Call `set_remote_description(answer)`
3. Now connection is in "have-local-pranswer" state
4. Wait for ICE candidates to flow

---

### Phase 3: ICE Candidate Trickle

#### Method: `peer.ice_candidate`

**Direction**: Client → Server → Client (bidirectional, notifications)

**Purpose**: Stream ICE candidates as discovered (trickle ICE)

**Timeline**:
- Alice and Bob send `peer.offer` and `peer.answer` (not waiting for ICE complete)
- As soon as ICE candidates are gathered, both send them immediately
- Each side may connect on first successful candidate pair
- No need to wait for all candidates ("ICE complete")

```json
{
  "jsonrpc": "2.0",
  "method": "peer.ice_candidate",
  "params": {
    "from": "peer-alice-uuid-123",
    "to": "peer-bob-uuid-456",
    "candidate": "candidate:123456 1 UDP 2122260223 192.168.1.100 54321 typ host",
    "sdp_m_line_index": 0,
    "sdp_mid": "audio",
    "user_fragment": "abc123",
    "password": "def456",
    "request_id": "req-offer-001"
  }
}
```

**Parameters**:
- `from` (string, required): Sender peer ID
- `to` (string, required): Recipient peer ID
- `candidate` (string, required): Raw ICE candidate string
  - Format: `candidate:{foundation} {component} {transport} {priority} {ip} {port} typ {type} [...]`
  - May include `raddr`, `rport` for reflexive/relay candidates
  - Must be valid per RFC 5245

- `sdp_m_line_index` (number, required): Media line index (0, 1, 2, ...)
  - 0 = first m= line (audio)
  - 1 = second m= line (video)
  - Must match SDP media order

- `sdp_mid` (string, required): Media ID from SDP
  - Example: `"audio"`, `"video"`, `"0"`, `"1"`
  - For matching candidates to media lines

- `user_fragment` (string, optional): ICE username fragment (for STUN)
  - Part of ICE credentials
  - Used by receiver to correlate candidates

- `password` (string, optional): ICE password (for STUN)
  - Part of ICE credentials

- `request_id` (string, required): Must match the original offer's request_id
  - Allows server to route to correct offer/answer pair

**Server Handling**:
1. Routes candidate to recipient immediately (no buffering)
2. No response needed (fire-and-forget notification)
3. Discards if request_id unknown

**Receiver Action**:
1. Call `add_ice_candidate()`
2. Trigger connectivity checks
3. First successful pair = connection established (can be <100ms)
4. Continue adding candidates as they arrive

**Candidate Examples**:

**Host Candidate** (local IP):
```json
{
  "candidate": "candidate:842163049 1 UDP 1677729535 192.168.1.100 54321 typ host",
  "sdp_m_line_index": 0,
  "sdp_mid": "0"
}
```

**Server Reflexive** (NAT public IP):
```json
{
  "candidate": "candidate:842163050 1 UDP 1677729534 203.0.113.45 54321 typ srflx raddr 192.168.1.100 rport 54321",
  "sdp_m_line_index": 0,
  "sdp_mid": "0"
}
```

**Relay Candidate** (TURN server):
```json
{
  "candidate": "candidate:842163051 1 UDP 50331647 198.51.100.100 50000 typ relay raddr 203.0.113.45 rport 54321",
  "sdp_m_line_index": 0,
  "sdp_mid": "0"
}
```

---

### Phase 4: Connection State Management

#### Method: `peer.state_changed` (Informational)

**Direction**: Either peer → Server (notification, optional)

**Purpose**: Notify signaling server of connection state changes

**Note**: This is optional and primarily for server monitoring/logging

```json
{
  "jsonrpc": "2.0",
  "method": "peer.state_changed",
  "params": {
    "from": "peer-alice-uuid-123",
    "to": "peer-bob-uuid-456",
    "connection_state": "connected",
    "ice_connection_state": "connected",
    "ice_gathering_state": "complete",
    "signaling_state": "stable",
    "request_id": "req-offer-001",
    "timestamp": "2025-11-07T10:30:05Z"
  }
}
```

**Parameters**:
- `from` (string): Local peer ID
- `to` (string): Remote peer ID
- `connection_state` (string): RTCPeerConnectionState
  - Values: `"new"` | `"connecting"` | `"connected"` | `"disconnected"` | `"failed"` | `"closed"`
- `ice_connection_state` (string): RTCIceConnectionState
  - Values: `"new"` | `"checking"` | `"connected"` | `"completed"` | `"failed"` | `"disconnected"` | `"closed"`
- `ice_gathering_state` (string): RTCIceGatheringState
  - Values: `"new"` | `"gathering"` | `"complete"`
- `signaling_state` (string): RTCSignalingState
  - Values: `"stable"` | `"have-local-offer"` | `"have-remote-offer"` | `"have-local-pranswer"` | `"have-remote-pranswer"` | `"closed"`
- `request_id` (string): Original offer request_id
- `timestamp` (string, ISO 8601): When state changed

---

### Phase 5: Disconnect

#### Method: `peer.disconnect`

**Direction**: Either peer → Server (notification)

**Purpose**: Notify peer and server of disconnect

```json
{
  "jsonrpc": "2.0",
  "method": "peer.disconnect",
  "params": {
    "from": "peer-alice-uuid-123",
    "to": "peer-bob-uuid-456",
    "reason": "user_requested",
    "details": {
      "code": 1000,
      "message": "User closed connection"
    },
    "request_id": "req-offer-001"
  }
}
```

**Parameters**:
- `from` (string, required): Disconnecting peer
- `to` (string, required): Other peer
- `reason` (string, required): Reason for disconnect
  - Values: `"user_requested"`, `"network_error"`, `"timeout"`, `"error"`, `"unknown"`
- `details` (object, optional): Additional error details
- `request_id` (string, optional): Original offer request_id (if applicable)

**Server Handling**:
1. Forwards notification to other peer
2. Cleans up offer/answer state for this pair
3. No response required

**Recipient Receives**:
```json
{
  "jsonrpc": "2.0",
  "method": "peer.disconnected",
  "params": {
    "from": "peer-alice-uuid-123",
    "reason": "user_requested",
    "request_id": "req-offer-001"
  }
}
```

---

## Trickle ICE Flow Diagram

```
Timeline: T0             T1             T2             T3             T4
         (ms)           (ms)           (ms)           (ms)           (ms)
Alice     |              |              |              |              |
  |       |              |              |              |              |
  +----offer(no wait)----+              |              |              |
  |                      |              |              |              |
  |      [ICE gathering starts]         |              |              |
  |                      |              |              |              |
  +--------ice_candidate(host)----------+              |              |
  |                      |     [Bob receives offer]     |              |
  |                      |              |              |              |
  |                      |        [Bob: ICE gathering]  |              |
  |                      |              |              |              |
  +--------ice_candidate(srflx)--------+              |              |
  |                      |              |              |              |
  |                      |              +----answer---+              |
  |                      |              |              |              |
  |                      |              |      [Bob trickles ICE]     |
  |                      |              |              |              |
  |                      |              +--ice_cand---+              |
  |                      |              |              |              |
  |      [Connectivity checks]          |              |              |
  |                      |              |              |              |
  |                      |<---ice_cand--+              |              |
  |                      |              |              |              |
  |                      +- CONNECTION -+              |              |
  |                      | ESTABLISHED  |              |              |
  |                      |              |              |              |
  +-----state_changed(connected)--------+              |              |
  |                      |              |              |              |
  +--------ice_candidate(relay)--------+              |              |
  |                      |              |              |              |
  |                      +-------ice_cand------+              |
  |                      |              |              |              |
  |                      |              |        (more candidates)    |
  |                      |              |              |              |

Key Events:
- T0: Alice sends offer immediately (without waiting for ICE complete)
- T0: ICE gathering starts in background
- T1: Alice sends first host candidate
- T1: Bob receives offer, starts ICE gathering
- T2: Alice sends reflexive candidate (from NAT)
- T2: Bob sends answer back
- T2: First successful candidate pair matches -> connection established
- T3: Remaining candidates trickled (relay, additional addresses)
```

---

## Error Handling & Recovery

### Connection Failures

**Scenario 1: Peer Not Found**

Request:
```json
{
  "jsonrpc": "2.0",
  "method": "peer.offer",
  "params": { "from": "alice", "to": "charlie", ... },
  "id": "offer-1"
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32000,
    "message": "Peer not found",
    "data": {
      "peer_id": "charlie",
      "registered_peers": ["bob", "dave"]
    }
  },
  "id": "offer-1"
}
```

**Recovery**: Client should:
1. Retry `peer.announce` if peer list changed
2. Check peer capabilities match requirements
3. Implement UI to discover peers first

---

**Scenario 2: Invalid SDP**

Request:
```json
{
  "jsonrpc": "2.0",
  "method": "peer.offer",
  "params": { "sdp": "invalid json not sdp", ... },
  "id": "offer-1"
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32002,
    "message": "Offer invalid",
    "data": {
      "reason": "Missing v= line",
      "position": 0
    }
  },
  "id": "offer-1"
}
```

---

**Scenario 3: ICE Timeout (No Connection)**

If no ICE candidates succeed within timeout:

```json
{
  "jsonrpc": "2.0",
  "method": "peer.state_changed",
  "params": {
    "connection_state": "failed",
    "ice_connection_state": "failed",
    "details": "No valid candidate pair"
  }
}
```

**Recovery**:
1. Check STUN/TURN server configuration
2. Verify firewall/NAT settings
3. Try with explicit TURN relay
4. Implement reconnect logic

---

## WebSocket Connection

### Establishing Connection

**Client**:
```
GET / HTTP/1.1
Host: signaling.example.com
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Key: x3JJHMbDL1EzLkh9GBhXDw==
Sec-WebSocket-Version: 13
```

**Server**:
```
HTTP/1.1 101 Switching Protocols
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Accept: HSmrc0sMlYUkAGmm5OPpG2HaGWk=
```

### Connection Management

**Keep-Alive**:
- No application-level heartbeat required
- WebSocket frame-level ping/pong (RFC 6455)
- Clients should respond to server ping with pong

**Reconnection**:
- Client should implement exponential backoff
- If disconnected, reconnect and re-announce peer
- Resume pending offers/answers if within timeout

**Connection Limits**:
- Timeout for inactive connections: 5 minutes
- Max simultaneous peer connections: 10 (per transport)
- Max message size: 1 MB

---

## Best Practices

### 1. Trickle ICE Timing

**DO**:
```rust
// Send offer immediately
peer_connection.set_local_description(offer).await?;
signaling_client.send_offer(&offer).await?;

// Send candidates as they arrive (immediate)
peer_connection.on_ice_candidate(|candidate| {
    signaling_client.send_ice_candidate(&candidate).await.ok();
});
```

**DON'T**:
```rust
// Wait for ICE gathering complete (slow)
let candidates = peer_connection.gather_all_ice_candidates().await?;
signaling_client.send_offer(&offer).await?;
signaling_client.send_all_candidates(&candidates).await?;
```

### 2. Error Handling

**DO**:
```rust
match signaling_client.send_offer(&offer).await {
    Ok(_) => println!("Offer sent"),
    Err(SignalingError::PeerNotFound) => {
        // Retry peer discovery
        signaling_client.announce_peers().await?;
    }
    Err(e) => eprintln!("Signaling error: {}", e),
}
```

**DON'T**:
```rust
// Silent failures
let _ = signaling_client.send_offer(&offer).await;
```

### 3. Session Management

**DO**:
```rust
// Use request_id to correlate offers/answers/candidates
let request_id = uuid::Uuid::new_v4().to_string();
signaling_client.send_offer(&offer, &request_id).await?;

// Handle incoming answer with same request_id
on_answer_received(|answer| {
    if answer.request_id == request_id {
        peer_connection.set_remote_description(&answer.sdp).await?;
    }
});
```

**DON'T**:
```rust
// Confuse offer/answer pairs
send_offer(...).await?;
send_offer(...).await?;  // Second offer replaces first
// Answer might match wrong offer!
```

### 4. Capability Matching

**DO**:
```rust
let my_capabilities = vec!["audio", "video"];
let peer_capabilities = peer_info.capabilities;

if peer_capabilities.contains(&"video") && my_capabilities.contains(&"video") {
    // Safe to create video tracks
}
```

**DON'T**:
```rust
// Assume all peers support all media
create_video_track();  // May fail if peer has no video capability
```

---

## Implementation Reference

### Pseudo-Code: Trickle ICE Flow

```rust
async fn establish_connection(peer_id: &str) {
    // 1. Create offer immediately (async)
    let offer = peer_connection.create_offer().await?;
    peer_connection.set_local_description(&offer).await?;

    // 2. Send offer without waiting for ICE
    let request_id = uuid::Uuid::new_v4().to_string();
    signaling.send_offer(&offer, &request_id).await?;

    // 3. Setup ICE candidate handler (background task)
    peer_connection.on_ice_candidate(|candidate| {
        // Send each candidate immediately as discovered
        signaling.send_ice_candidate(&candidate, &request_id).await.ok();
    });

    // 4. Wait for answer (can arrive anytime)
    let answer_fut = signaling.wait_for_answer(&request_id);
    let answer = tokio::time::timeout(Duration::from_secs(30), answer_fut).await??;

    // 5. Set remote description
    peer_connection.set_remote_description(&answer).await?;

    // 6. Connection state changes happen automatically
    peer_connection.on_ice_connection_state_change(|state| {
        match state {
            IceConnectionState::Connected => println!("Connected!"),
            IceConnectionState::Failed => println!("Failed to connect"),
            _ => {}
        }
    });
}
```

---

## Testing

### Unit Tests

```rust
#[test]
fn test_peer_announce_message() {
    let msg = json!({
        "jsonrpc": "2.0",
        "method": "peer.announce",
        "params": {
            "peer_id": "alice",
            "capabilities": ["audio", "video"]
        },
        "id": 1
    });

    // Validate JSON-RPC structure
    assert_eq!(msg["jsonrpc"].as_str(), Some("2.0"));
    assert_eq!(msg["method"].as_str(), Some("peer.announce"));
}

#[test]
fn test_offer_message_format() {
    let offer = json!({
        "jsonrpc": "2.0",
        "method": "peer.offer",
        "params": {
            "from": "alice",
            "to": "bob",
            "sdp": "v=0\r\n...",
            "can_trickle_ice_candidates": true,
            "request_id": "req-1"
        },
        "id": "offer-1"
    });

    // Validate required fields
    let params = &offer["params"];
    assert!(params["from"].is_string());
    assert!(params["to"].is_string());
    assert!(params["sdp"].is_string());
    assert!(params["request_id"].is_string());
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_trickle_ice_flow() {
    let mut server = MockSignalingServer::new();
    let alice = SignalingClient::new("alice").await?;
    let bob = SignalingClient::new("bob").await?;

    // 1. Both peers announce
    alice.announce(&["audio"]).await?;
    bob.announce(&["audio"]).await?;

    // 2. Alice sends offer
    let offer = create_test_offer();
    alice.send_offer("bob", &offer).await?;

    // 3. Bob receives offer (verify notification)
    let received_offer = bob.wait_for_offer().await?;
    assert_eq!(received_offer.from, "alice");

    // 4. Bob sends answer
    let answer = create_test_answer(&received_offer);
    bob.send_answer("alice", &answer).await?;

    // 5. Verify both sides connected
    assert!(alice.is_connected("bob").await);
    assert!(bob.is_connected("alice").await);
}
```

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2025-11-07 | Initial JSON-RPC 2.0 specification with trickle ICE |

---

## References

- [RFC 7920 - JSON-RPC 2.0 Specification](https://www.jsonrpc.org/specification)
- [RFC 5245 - Interactive Connectivity Establishment (ICE)](https://tools.ietf.org/html/rfc5245)
- [RFC 3264 - Offer/Answer Model](https://tools.ietf.org/html/rfc3264)
- [RFC 4566 - SDP (Session Description Protocol)](https://tools.ietf.org/html/rfc4566)
- [W3C WebRTC Spec - RTCPeerConnection](https://w3c.github.io/webrtc-pc/)

---

## See Also

- [Transport API Contract](./transport-api.md)
- [Sync Manager API Contract](./sync-manager-api.md)
- [Feature Specification](../spec.md)
- [Research Document](../../transports/remotemedia-webrtc/research.md)
