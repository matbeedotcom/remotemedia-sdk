# WebRTC Transport Troubleshooting Guide

This guide helps diagnose and resolve common issues with the WebRTC multi-peer transport.

## Quick Diagnostics

Before diving into specific issues, run these diagnostic checks:

```bash
# 1. Verify the crate compiles
cargo check -p remotemedia-webrtc

# 2. Run tests to check basic functionality
cargo test -p remotemedia-webrtc

# 3. Check for runtime issues with logging
RUST_LOG=debug cargo run --example simple_peer
```

---

## Connection Issues

### Issue: "Cannot connect to peer" / ConnectionFailed

**Symptoms:**
- `Error::PeerNotFound` when calling `connect_peer()`
- Connection hangs indefinitely
- ICE gathering never completes

**Diagnostic Steps:**

1. **Check signaling server connectivity:**
```rust
// Verify signaling URL is correct
let config = WebRtcTransportConfig {
    signaling_url: "ws://localhost:8080".to_string(), // Must be ws:// or wss://
    ..Default::default()
};

// Start transport and check for errors
match transport.start().await {
    Ok(_) => println!("Signaling connected"),
    Err(e) => println!("Signaling failed: {} - {}", e, e.recovery_suggestion()),
}
```

2. **Verify peer is registered:**
```rust
// List available peers before connecting
let peers = transport.list_peers().await?;
println!("Available peers: {:?}", peers);
```

3. **Check firewall/NAT settings:**
```bash
# Test STUN connectivity (Linux)
stun stun.l.google.com:19302

# Check UDP port availability
netstat -an | grep -E ":(49152|65535)"
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| Signaling server down | Verify server is running at the configured URL |
| Peer not registered | Ensure both peers call `transport.start()` before connecting |
| Firewall blocking UDP | Open UDP ports 49152-65535 or use TURN relay |
| Wrong signaling URL | Use `ws://` for local, `wss://` for production |

**Configuration Fix:**
```rust
let config = WebRtcTransportConfig {
    signaling_url: "wss://signaling.example.com".to_string(),
    stun_servers: vec![
        "stun:stun.l.google.com:19302".to_string(),
        "stun:stun1.l.google.com:19302".to_string(),
    ],
    // Add TURN for restrictive networks
    turn_servers: vec![
        TurnServerConfig {
            url: "turn:turn.example.com:3478".to_string(),
            username: "user".to_string(),
            credential: "pass".to_string(),
        }
    ],
    ..Default::default()
};
```

---

### Issue: ICE Candidate Gathering Fails

**Symptoms:**
- Connection stuck in "GatheringIce" state for >10 seconds
- No ICE candidates collected
- `Error::NatTraversalFailed`

**Diagnostic Steps:**

1. **Check STUN server accessibility:**
```bash
# Test STUN (requires stun-client package)
stun stun.l.google.com 19302

# Or use webrtc-test tools
npx wrtc-test stun stun.l.google.com:19302
```

2. **Verify ICE servers in config:**
```rust
let config = transport.config();
println!("STUN servers: {:?}", config.stun_servers);
println!("TURN servers: {:?}", config.turn_servers.len());
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| STUN unreachable | Try multiple STUN servers (Google, Twilio, etc.) |
| UDP blocked | Use TURN with TCP transport |
| IPv6/IPv4 mismatch | Force IPv4 or add both server types |
| Corporate firewall | Use TURN with TLS (turns://) on port 443 |

**Configuration Fix:**
```rust
let config = WebRtcTransportConfig {
    stun_servers: vec![
        "stun:stun.l.google.com:19302".to_string(),
        "stun:stun1.l.google.com:19302".to_string(),
        "stun:stun2.l.google.com:19302".to_string(),
        "stun:stun3.l.google.com:19302".to_string(),
    ],
    // TURN fallback for restrictive networks
    turn_servers: vec![
        TurnServerConfig {
            url: "turns:turn.example.com:443".to_string(), // TLS on 443
            username: "user".to_string(),
            credential: "pass".to_string(),
        }
    ],
    options: ConfigOptions {
        ice_timeout_secs: 30, // Increase timeout
        ..Default::default()
    },
    ..Default::default()
};
```

---

## Media Issues

### Issue: Audio/Video Dropouts

**Symptoms:**
- Periodic silence or black frames
- Choppy audio playback
- Video freezes then catches up

**Diagnostic Steps:**

1. **Check jitter buffer statistics:**
```rust
let peers = transport.list_peers().await?;
for peer in peers {
    let metrics = peer.metrics;
    println!("Peer {}: latency={}ms, packet_loss={}%, jitter={}ms",
        peer.peer_id, metrics.latency_ms, metrics.packet_loss_rate, metrics.jitter_ms);
}
```

2. **Monitor network conditions:**
```bash
# Check network latency
ping -c 10 signaling.example.com

# Check for packet loss
mtr --report signaling.example.com
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| High network jitter | Increase `jitter_buffer_size_ms` (100-150ms) |
| Insufficient bandwidth | Reduce bitrate or resolution |
| Codec mismatch | Verify both peers support the same codec |
| CPU overload | Use hardware acceleration or lower quality |

**Configuration Fix:**
```rust
// For unstable networks
let config = WebRtcTransportConfig {
    jitter_buffer_size_ms: 100, // Increased from 50ms
    video_codec: VideoCodec::VP8, // Lower CPU than VP9
    options: ConfigOptions {
        adaptive_bitrate_enabled: true,
        target_bitrate_kbps: 1000, // Reduced bitrate
        max_video_resolution: VideoResolution::P480, // Lower resolution
        video_framerate_fps: 24, // Reduced framerate
        ..Default::default()
    },
    ..Default::default()
};
```

---

### Issue: High Latency (>100ms)

**Symptoms:**
- Noticeable audio/video delay
- Echo during voice communication
- Lip-sync issues

**Diagnostic Steps:**

1. **Measure end-to-end latency:**
```rust
use std::time::Instant;

let start = Instant::now();
transport.send_to_peer("peer-id", &audio_data).await?;
// ... receive response
let latency = start.elapsed();
println!("Round-trip latency: {:?}", latency);
```

2. **Check if using TURN relay:**
```rust
// TURN adds ~50-100ms latency vs direct P2P
let peers = transport.list_peers().await?;
for peer in peers {
    println!("Connection type: {:?}", peer.connection_type); // Direct or Relay
}
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| Large jitter buffer | Reduce to 50ms minimum |
| TURN relay | Improve NAT traversal to avoid relay |
| Slow codec | Use VP8 instead of VP9 |
| Pipeline processing | Optimize pipeline nodes |

**Configuration Fix:**
```rust
// Use low-latency preset
let config = WebRtcTransportConfig::low_latency_preset("ws://localhost:8080");
// jitter_buffer_size_ms: 50
// rtcp_interval_ms: 2000
// video_codec: VP8
// data_channel_mode: Unreliable
```

---

### Issue: Audio/Video Out of Sync (Lip-Sync)

**Symptoms:**
- Video lips don't match audio
- Audio arrives before/after video
- Sync drift over time

**Diagnostic Steps:**

1. **Check sync manager state:**
```rust
// Get sync statistics from peer connection
let peer = transport.get_peer("peer-id").await?;
let sync_stats = peer.get_sync_stats();
println!("Sync state: {:?}", sync_stats.state);
println!("Audio/Video offset: {}ms", sync_stats.av_offset_ms);
println!("Clock drift: {} ppm", sync_stats.clock_drift_ppm);
```

2. **Verify RTCP is enabled:**
```rust
let config = transport.config();
assert!(config.enable_rtcp, "RTCP required for A/V sync");
println!("RTCP interval: {}ms", config.rtcp_interval_ms);
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| RTCP disabled | Enable `enable_rtcp: true` |
| Large clock drift | Check hardware clocks, reduce session duration |
| Different jitter for A/V | Use same jitter buffer for both |
| Missing RTCP SR | Verify signaling exchanges RTCP packets |

---

## Data Channel Issues

### Issue: Data Channel Messages Not Delivered

**Symptoms:**
- `send_data_channel_message()` succeeds but peer doesn't receive
- Messages arrive out of order
- Large messages fail silently

**Diagnostic Steps:**

1. **Check data channel state:**
```rust
let peer = transport.get_peer("peer-id").await?;
println!("Data channel open: {}", peer.is_data_channel_open());
```

2. **Check message size:**
```rust
// Maximum message size is 16 MB
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
if message_bytes.len() > MAX_MESSAGE_SIZE {
    println!("Message too large: {} bytes", message_bytes.len());
}
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| Data channel not open | Wait for connection to be `Connected` |
| Message too large | Split into chunks < 16 MB |
| Unreliable mode | Use `DataChannelMode::Reliable` for important messages |
| Peer disconnected | Check connection state before sending |

**Configuration Fix:**
```rust
let config = WebRtcTransportConfig {
    enable_data_channel: true,
    data_channel_mode: DataChannelMode::Reliable, // Guaranteed delivery
    ..Default::default()
};
```

---

## Performance Issues

### Issue: High CPU Usage

**Symptoms:**
- CPU usage >30% per peer
- System becomes unresponsive
- Thermal throttling

**Diagnostic Steps:**

1. **Profile CPU usage:**
```bash
# Linux perf
perf record -g target/release/my-webrtc-app
perf report

# Or use cargo-flamegraph
cargo flamegraph --bin my-webrtc-app
```

2. **Check encoding settings:**
```rust
let config = transport.config();
println!("Video codec: {:?}", config.video_codec);
println!("Resolution: {:?}", config.options.max_video_resolution);
println!("Framerate: {} fps", config.options.video_framerate_fps);
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| VP9 encoding | Switch to VP8 or H.264 |
| High resolution | Reduce to 720p or 480p |
| High framerate | Reduce to 24 or 15 fps |
| Too many peers | Limit `max_peers` to 5 or fewer |

**Configuration Fix:**
```rust
let config = WebRtcTransportConfig {
    video_codec: VideoCodec::VP8, // Lower CPU than VP9
    max_peers: 5, // Limit concurrent peers
    options: ConfigOptions {
        max_video_resolution: VideoResolution::P480,
        video_framerate_fps: 24,
        target_bitrate_kbps: 800,
        ..Default::default()
    },
    ..Default::default()
};
```

---

### Issue: Memory Leak / Growing Memory Usage

**Symptoms:**
- Memory usage grows over time
- Eventually OOM or crash
- Works fine for short sessions

**Diagnostic Steps:**

1. **Monitor memory:**
```bash
# Watch memory usage
watch -n 1 'ps -o rss,vsz,pid,cmd -p $(pgrep my-webrtc-app)'

# Or use valgrind
valgrind --leak-check=full target/release/my-webrtc-app
```

2. **Check session cleanup:**
```rust
// Ensure sessions are properly closed
session.close().await?;
transport.disconnect_peer("peer-id").await?;
transport.shutdown().await?;
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| Sessions not closed | Call `session.close()` when done |
| Peers not disconnected | Call `disconnect_peer()` on exit |
| Jitter buffer overflow | Enable `discard_late_frames` |
| Callback leaks | Use weak references in callbacks |

---

## Reconnection Issues

### Issue: Auto-Reconnect Not Working

**Symptoms:**
- Connection drops and doesn't recover
- Reconnection attempts fail repeatedly
- Circuit breaker opens

**Diagnostic Steps:**

1. **Check reconnection state:**
```rust
let peer = transport.get_peer("peer-id").await?;
println!("Reconnection state: {:?}", peer.reconnection_state());
println!("Retry count: {}", peer.retry_count());
```

2. **Check circuit breaker:**
```rust
println!("Circuit state: {:?}", peer.circuit_breaker_state());
// Closed = normal, Open = blocking retries, HalfOpen = testing
```

**Solutions:**

| Cause | Solution |
|-------|----------|
| Max retries exceeded | Increase `max_reconnect_retries` |
| Circuit breaker open | Wait for recovery timeout or reset manually |
| Network permanently down | Handle gracefully, notify user |
| Signaling server down | Implement signaling failover |

**Configuration Fix:**
```rust
let config = WebRtcTransportConfig {
    options: ConfigOptions {
        max_reconnect_retries: 15, // More attempts
        reconnect_backoff_initial_ms: 1000,
        reconnect_backoff_max_ms: 60000, // Up to 1 minute
        reconnect_backoff_multiplier: 1.5,
        ..Default::default()
    },
    ..Default::default()
};
```

---

## Debugging Tips

### Enable Detailed Logging

```bash
# All WebRTC transport logs
RUST_LOG=remotemedia_webrtc=debug cargo run

# Include webrtc-rs internals
RUST_LOG=remotemedia_webrtc=debug,webrtc=debug cargo run

# Trace-level for maximum detail
RUST_LOG=remotemedia_webrtc=trace cargo run
```

### Capture Network Traffic

```bash
# Capture WebRTC traffic (STUN/TURN/RTP)
sudo tcpdump -i any -w webrtc.pcap 'udp port 3478 or udp portrange 49152-65535'

# Analyze with Wireshark
wireshark webrtc.pcap
```

### Use Browser DevTools

When debugging browser-to-Rust connections:

1. Open `chrome://webrtc-internals` in Chrome
2. Look for:
   - ICE candidate pairs and their states
   - Codec negotiation results
   - RTP statistics (packets sent/received, loss)
   - RTCP reports

---

## Getting Help

If issues persist after trying these solutions:

1. **Check existing issues:** Search the GitHub repository
2. **Collect diagnostics:**
   - Full error message with stack trace
   - Configuration used
   - Network environment (NAT type, firewall)
   - Platform and Rust version
3. **Create minimal reproduction:** Smallest code that shows the issue
4. **File an issue:** Include all collected information

---

## Quick Reference: Error Codes

| Error Code | Common Cause | Quick Fix |
|------------|--------------|-----------|
| `INVALID_CONFIG` | Bad configuration values | Use `config.validate()` |
| `SIGNALING_ERROR` | Can't reach signaling server | Check URL and network |
| `PEER_NOT_FOUND` | Peer not connected | Verify peer is online |
| `NAT_TRAVERSAL_FAILED` | ICE failed | Add TURN servers |
| `ENCODING_ERROR` | Codec issue | Check codec features |
| `SESSION_NOT_FOUND` | Session closed/missing | Create new session |
| `OPERATION_TIMEOUT` | Network too slow | Increase timeouts |
| `DATA_CHANNEL_ERROR` | DC not ready | Wait for connection |
| `SYNC_ERROR` | A/V sync failed | Enable RTCP |
