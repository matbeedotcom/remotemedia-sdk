# WebRTC Transport Performance Tuning Guide

This guide provides strategies for optimizing WebRTC transport performance for different use cases.

## Performance Targets

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Audio latency (95th %ile) | <50ms | End-to-end timestamp |
| Video latency (95th %ile) | <100ms | Frame capture to display |
| Connection setup | <2s | Signaling start to media |
| CPU usage (720p 30fps) | <30% single core | System profiler |
| Memory per peer | <100MB | RSS measurement |
| Throughput | 30fps video + 1000 audio/sec | Frame counter |

---

## Quick Optimization: Configuration Presets

Choose the right preset for your use case:

```rust
use remotemedia_webrtc::WebRtcTransportConfig;

// Real-time communication (voice/video calls)
let config = WebRtcTransportConfig::low_latency_preset("ws://signaling.example.com");

// Broadcasting/recording (quality over latency)
let config = WebRtcTransportConfig::high_quality_preset("ws://signaling.example.com");

// Mobile/unstable networks
let config = WebRtcTransportConfig::mobile_network_preset("ws://signaling.example.com")
    .with_turn_servers(turn_servers);
```

---

## Latency Optimization

### Audio Latency Breakdown

Typical audio path latency:

```
Capture (5-20ms) → Encode (5-10ms) → Network (10-50ms) → Jitter Buffer (50-100ms) → Decode (5-10ms) → Playback (5-20ms)
                                                              ↑
                                                    Main optimization target
```

**Total typical range: 80-210ms**

### Reducing Audio Latency

1. **Minimize jitter buffer** (biggest impact):
```rust
let config = WebRtcTransportConfig {
    jitter_buffer_size_ms: 50, // Minimum stable value
    ..Default::default()
};
```

2. **Use Opus with low complexity**:
```rust
// Opus encoder settings (internal to transport)
// Lower complexity = faster encoding
// Complexity 4 vs 8 saves ~3ms per frame
```

3. **Faster RTCP feedback**:
```rust
let config = WebRtcTransportConfig {
    rtcp_interval_ms: 2000, // More frequent than default 5000ms
    ..Default::default()
};
```

### Video Latency Breakdown

```
Capture (8-33ms) → Encode (20-50ms) → Network (10-50ms) → Jitter Buffer (50-100ms) → Decode (10-30ms) → Render (8-16ms)
                       ↑                                        ↑
                 Codec choice                           Buffer size
```

**Total typical range: 106-279ms**

### Reducing Video Latency

1. **Choose faster codec**:
```rust
let config = WebRtcTransportConfig {
    video_codec: VideoCodec::VP8,  // ~20ms encode
    // vs VP9: ~35ms encode
    // vs H264: ~25ms encode (with hardware)
    ..Default::default()
};
```

2. **Lower resolution/framerate**:
```rust
let config = WebRtcTransportConfig {
    options: ConfigOptions {
        max_video_resolution: VideoResolution::P480, // vs P720/P1080
        video_framerate_fps: 24, // vs 30/60
        ..Default::default()
    },
    ..Default::default()
};
```

3. **Reduce bitrate** (faster encoding):
```rust
let config = WebRtcTransportConfig {
    options: ConfigOptions {
        target_bitrate_kbps: 1000, // vs 2000-4000
        ..Default::default()
    },
    ..Default::default()
};
```

---

## CPU Optimization

### Profiling CPU Usage

```bash
# Using perf (Linux)
perf record -g target/release/webrtc-app
perf report

# Using flamegraph
cargo install flamegraph
cargo flamegraph --release --bin webrtc-app

# Using Instruments (macOS)
xcrun instruments -t "Time Profiler" target/release/webrtc-app
```

### CPU Hotspots

Typical CPU usage breakdown (720p 30fps):

| Component | % CPU | Optimization |
|-----------|-------|--------------|
| Video encoding | 40-60% | Use VP8, lower resolution |
| Video decoding | 15-25% | Hardware decode if available |
| Audio codec | 5-10% | Lower Opus complexity |
| Jitter buffer | 2-5% | Already optimized (O(log n)) |
| Signaling | <1% | Minimal overhead |
| Sync manager | <1% | Minimal overhead |

### Reducing CPU Usage

1. **Use VP8 instead of VP9**:
   - VP8: ~15% CPU for 720p encode
   - VP9: ~25% CPU for 720p encode (but better compression)

2. **Reduce resolution**:
   - 1080p: ~30% CPU
   - 720p: ~15% CPU
   - 480p: ~8% CPU

3. **Reduce framerate**:
   - 60fps: 2x CPU of 30fps
   - 30fps: baseline
   - 15fps: 0.5x CPU

4. **Limit concurrent peers**:
```rust
let config = WebRtcTransportConfig {
    max_peers: 5, // Each peer adds ~15-20% CPU at 720p
    ..Default::default()
};
```

5. **Use native Rust nodes** (vs Python):
```yaml
# In pipeline manifest
nodes:
  - id: "processor"
    executor: "native"  # vs "multiprocess" (Python)
```

---

## Memory Optimization

### Memory Breakdown

Per-peer memory usage:

| Component | Memory | Notes |
|-----------|--------|-------|
| Jitter buffer (audio) | 1-5 MB | ~100ms of audio |
| Jitter buffer (video) | 10-30 MB | ~100ms of 720p frames |
| Codec state | 5-10 MB | Encoder + decoder |
| RTP buffers | 2-5 MB | Packet queues |
| Sync manager | <1 MB | Timestamps + estimates |
| **Total per peer** | ~20-50 MB | |

### Reducing Memory Usage

1. **Smaller jitter buffers**:
```rust
let config = WebRtcTransportConfig {
    jitter_buffer_size_ms: 50, // vs 100-200ms
    ..Default::default()
};
```

2. **Lower resolution**:
   - 1080p frame: ~3 MB (I420)
   - 720p frame: ~1.4 MB
   - 480p frame: ~0.5 MB

3. **Proper cleanup**:
```rust
// Always close sessions when done
session.close().await?;
transport.disconnect_peer("peer-id").await?;

// Shutdown transport cleanly
transport.shutdown().await?;
```

4. **Monitor memory**:
```rust
// Check peer count periodically
let peer_count = transport.list_peers().await?.len();
if peer_count > expected_max {
    warn!("Too many peers: {}", peer_count);
}
```

---

## Network Optimization

### Bandwidth Requirements

| Quality | Video Bitrate | Audio Bitrate | Total |
|---------|---------------|---------------|-------|
| 1080p 30fps | 3-5 Mbps | 64 kbps | ~4 Mbps |
| 720p 30fps | 1.5-2.5 Mbps | 64 kbps | ~2 Mbps |
| 480p 30fps | 0.5-1 Mbps | 32 kbps | ~0.8 Mbps |
| Audio only | - | 32-64 kbps | ~50 kbps |

### Adaptive Bitrate

Enable adaptive bitrate for variable networks:

```rust
let config = WebRtcTransportConfig {
    options: ConfigOptions {
        adaptive_bitrate_enabled: true,
        target_bitrate_kbps: 2000, // Starting point
        ..Default::default()
    },
    ..Default::default()
};

// The transport automatically adjusts based on:
// - Packet loss rate (>5% triggers reduction)
// - RTT increases
// - RTCP receiver reports
```

### Reducing Bandwidth

1. **Lower bitrate ceiling**:
```rust
options: ConfigOptions {
    target_bitrate_kbps: 800, // For mobile/constrained networks
    ..Default::default()
}
```

2. **Audio-only mode** (when video unnecessary):
```rust
// Don't add video track to peer connection
// Only stream audio
```

3. **Unreliable data channel** (for non-critical data):
```rust
let config = WebRtcTransportConfig {
    data_channel_mode: DataChannelMode::Unreliable,
    ..Default::default()
};
```

---

## Multi-Peer Scaling

### Mesh Topology Limits

| Peers | Connections | Encoding | Bandwidth (720p) |
|-------|-------------|----------|------------------|
| 2 | 1 | 1x | 2 Mbps |
| 4 | 6 | 3x | 6 Mbps |
| 6 | 15 | 5x | 10 Mbps |
| 10 | 45 | 9x | 18 Mbps |

**Recommendation**: Limit to 5-6 peers for mesh topology.

### Optimizing Multi-Peer

1. **Limit peers based on resources**:
```rust
let config = WebRtcTransportConfig {
    max_peers: 5, // Reasonable for most hardware
    ..Default::default()
};
```

2. **Use selective routing** (broadcast to subset):
```rust
// Instead of broadcast to all
transport.broadcast(&data).await?;

// Send to specific peers
for peer_id in target_peers {
    transport.send_to_peer(&peer_id, &data).await?;
}
```

3. **Quality tiers for different peers**:
```rust
// High-quality for speaker
let speaker_data = high_quality_encode(&frame);
transport.send_to_peer("speaker", &speaker_data).await?;

// Lower quality for listeners
let listener_data = low_quality_encode(&frame);
for listener in listeners {
    transport.send_to_peer(&listener, &listener_data).await?;
}
```

---

## Jitter Buffer Tuning

### Size Guidelines

| Network Condition | Jitter Buffer | Tradeoff |
|-------------------|---------------|----------|
| LAN / Stable | 50ms | Lowest latency |
| Broadband | 75ms | Good balance |
| WiFi / Variable | 100ms | Handles bursts |
| Mobile / Unstable | 150-200ms | Maximum stability |

### Monitoring Jitter

```rust
// Get jitter statistics from peer
let peers = transport.list_peers().await?;
for peer in peers {
    let metrics = peer.metrics;
    println!("Peer {}: jitter={}ms, loss={}%",
        peer.peer_id, metrics.jitter_ms, metrics.packet_loss_rate);

    // Adjust buffer dynamically
    if metrics.jitter_ms > 50.0 {
        // Consider increasing buffer
    }
}
```

---

## Benchmarking

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench -p remotemedia-webrtc

# Run specific benchmark
cargo bench -p remotemedia-webrtc jitter_buffer

# Generate benchmark report
cargo bench -p remotemedia-webrtc -- --save-baseline main
```

### Key Benchmarks

1. **Jitter buffer insertion**: Target <5ms for 1000 frames
2. **Codec encode/decode**: Target <10ms audio, <30ms video
3. **Sync manager processing**: Target <1ms per frame
4. **Signaling message parsing**: Target <0.1ms per message

### Custom Latency Measurement

```rust
use std::time::Instant;

// Measure send latency
let start = Instant::now();
transport.send_to_peer("peer-id", &data).await?;
let send_latency = start.elapsed();

// Measure round-trip (requires echo from peer)
let start = Instant::now();
session.send_input(data).await?;
let response = session.recv_output().await?;
let rtt = start.elapsed();
```

---

## Configuration Quick Reference

### Low Latency (<100ms target)

```rust
WebRtcTransportConfig {
    jitter_buffer_size_ms: 50,
    rtcp_interval_ms: 2000,
    video_codec: VideoCodec::VP8,
    data_channel_mode: DataChannelMode::Unreliable,
    options: ConfigOptions {
        target_bitrate_kbps: 1500,
        max_video_resolution: VideoResolution::P720,
        video_framerate_fps: 30,
        ice_timeout_secs: 15,
        ..Default::default()
    },
    ..Default::default()
}
```

### High Quality (quality over latency)

```rust
WebRtcTransportConfig {
    jitter_buffer_size_ms: 100,
    rtcp_interval_ms: 5000,
    video_codec: VideoCodec::VP9,
    data_channel_mode: DataChannelMode::Reliable,
    options: ConfigOptions {
        target_bitrate_kbps: 4000,
        max_video_resolution: VideoResolution::P1080,
        video_framerate_fps: 30,
        ..Default::default()
    },
    ..Default::default()
}
```

### Low CPU (<15% target at 720p)

```rust
WebRtcTransportConfig {
    video_codec: VideoCodec::VP8,
    max_peers: 3,
    options: ConfigOptions {
        target_bitrate_kbps: 1000,
        max_video_resolution: VideoResolution::P480,
        video_framerate_fps: 24,
        ..Default::default()
    },
    ..Default::default()
}
```

### Low Bandwidth (<1 Mbps)

```rust
WebRtcTransportConfig {
    video_codec: VideoCodec::VP8,
    options: ConfigOptions {
        adaptive_bitrate_enabled: true,
        target_bitrate_kbps: 500,
        max_video_resolution: VideoResolution::P480,
        video_framerate_fps: 15,
        ..Default::default()
    },
    ..Default::default()
}
```

---

## Monitoring in Production

### Key Metrics to Track

```rust
// Collect metrics periodically
async fn collect_metrics(transport: &WebRtcTransport) {
    let peers = transport.list_peers().await.unwrap_or_default();

    for peer in peers {
        // Connection quality
        metrics::gauge!("webrtc.latency_ms", peer.metrics.latency_ms);
        metrics::gauge!("webrtc.packet_loss", peer.metrics.packet_loss_rate);
        metrics::gauge!("webrtc.jitter_ms", peer.metrics.jitter_ms);

        // Throughput
        metrics::gauge!("webrtc.bitrate_kbps", peer.metrics.video_bitrate_kbps);
        metrics::gauge!("webrtc.framerate", peer.metrics.video_framerate);
    }

    // Peer count
    metrics::gauge!("webrtc.peer_count", peers.len() as f64);
}
```

### Alerting Thresholds

| Metric | Warning | Critical |
|--------|---------|----------|
| Latency | >150ms | >300ms |
| Packet loss | >2% | >5% |
| Jitter | >50ms | >100ms |
| CPU per peer | >25% | >40% |
| Memory per peer | >75MB | >100MB |

---

## Troubleshooting Performance Issues

See [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for:
- High CPU usage diagnosis
- Memory leak detection
- Latency debugging
- Network optimization
