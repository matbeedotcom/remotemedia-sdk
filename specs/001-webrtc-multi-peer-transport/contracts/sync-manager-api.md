# Sync Manager API Contract

**Version:** 1.0.0
**Feature:** WebRTC Multi-Peer Transport
**Status:** Specification
**Created:** 2025-11-07
**Last Updated:** 2025-11-07

## Overview

This contract defines the public API for `SyncManager`, which handles audio/video synchronization for individual peer connections. It manages RTP timestamp tracking, jitter buffering, clock drift estimation, and lip-sync alignment.

**Critical**: WebRTC does NOT automatically synchronize audio/video or handle multi-peer clock drift. The SyncManager explicitly handles these via RTP timestamps, RTCP Sender Reports, and adaptive jitter buffers.

---

## Architecture

```
Per-Peer Synchronization Pipeline:

Incoming RTP Stream (audio/video)
     ↓
RTP Timestamp Extraction
     ↓
├─→ JitterBuffer (reorder, handle packet loss)
│    ↓
│   Adaptive Delay (50-100ms)
│    ↓
└─→ Clock Drift Estimator (monitor sender vs receiver clocks)
     ↓
   NTP/RTP Mapping (from RTCP Sender Report)
     ↓
   Wall-Clock Conversion
     ↓
   Audio/Video Lip-Sync Alignment
     ↓
Output to Pipeline (synchronized frame with absolute timestamp)
```

---

## Core API Methods

### 1. Lifecycle Methods

#### `new(peer_id: &str, config: SyncConfig) -> Result<Self>`

**Purpose**: Create a new SyncManager instance for a peer

**Parameters**:
- `peer_id: &str` - Associated peer identifier
- `config: SyncConfig` - Synchronization configuration

**Return Type**: `Result<SyncManager, Error>`

**Example**:
```rust
use remotemedia_webrtc::sync::{ SyncManager, SyncConfig };

let config = SyncConfig {
    audio_clock_rate: 48_000,     // Opus: always 48kHz
    video_clock_rate: 90_000,     // RTP: 90kHz
    jitter_buffer_size_ms: 50,    // Target: 50-100ms
    max_jitter_buffer_ms: 200,    // Hard limit
    enable_clock_drift_correction: true,
    drift_correction_threshold_ppm: 100, // ±100 ppm = ±0.01%
    rtcp_interval_ms: 5000,       // RTCP every 5 seconds
};

let sync_manager = SyncManager::new("peer-alice-123", config)?;
```

**Config Structure**:
```rust
pub struct SyncConfig {
    /// Audio RTP clock rate (Hz) - Opus always 48kHz
    pub audio_clock_rate: u32,

    /// Video RTP clock rate (Hz) - Standard 90kHz
    pub video_clock_rate: u32,

    /// Target jitter buffer duration (milliseconds)
    /// Typical: 50-100ms (trade-off: lower = less latency, higher = more reordering)
    pub jitter_buffer_size_ms: u32,

    /// Maximum jitter buffer size (hard limit)
    /// If exceeded: discard oldest frames
    pub max_jitter_buffer_ms: u32,

    /// Enable automatic clock drift correction
    /// If true: apply sample rate adjustment when drift > threshold
    pub enable_clock_drift_correction: bool,

    /// Drift threshold in parts per million (ppm)
    /// Example: 100 ppm = ±0.01% deviation = ~1 second drift per 3 hours
    pub drift_correction_threshold_ppm: i32,

    /// RTCP Sender Report interval (milliseconds)
    /// Sync updates from remote peer (should be ~5 seconds)
    pub rtcp_interval_ms: u32,
}

// Validation rules
impl SyncConfig {
    pub fn validate(&self) -> Result<()> {
        assert!(self.audio_clock_rate == 48_000, "Opus requires 48kHz");
        assert!(self.video_clock_rate == 90_000, "RTP video uses 90kHz");
        assert!(self.jitter_buffer_size_ms >= 50, "Min 50ms buffer");
        assert!(self.jitter_buffer_size_ms <= 200, "Max 200ms buffer");
        assert!(self.max_jitter_buffer_ms <= 500, "Hard max 500ms");
        Ok(())
    }
}
```

**Error Conditions**:
- `InvalidConfig`: Config values out of valid range
- `PeerNotFound`: Peer ID is empty

**Preconditions**:
- Peer must be connected

**Postconditions**:
- SyncManager initialized and ready
- RTP clocks not yet synchronized

**Thread Safety**: Instance should be owned by PeerConnection (not shared)

---

#### `reset(&mut self) -> Result<()>`

**Purpose**: Reset synchronization state (e.g., after pause/resume)

**Example**:
```rust
// On connection pause/resume
sync_manager.reset()?;

// Clears:
// - Jitter buffer
// - RTP timestamp tracking
// - Clock drift estimates
// - NTP/RTP mappings
```

**Preconditions**:
- Streaming has stopped or paused

**Postconditions**:
- All buffers cleared
- Clock tracking reset
- Ready for new stream

**Error Conditions**:
- `AlreadyReset`: Called multiple times without streaming

---

### 2. Audio Processing Methods

#### `process_audio_frame(&mut self, frame: AudioFrame) -> Result<SyncedAudioFrame>`

**Purpose**: Process incoming audio frame with synchronization

**Parameters**:
- `frame: AudioFrame` - Incoming RTP audio frame

**Return Type**: `Result<SyncedAudioFrame, Error>`

**Example**:
```rust
use remotemedia_webrtc::sync::{ AudioFrame, SyncedAudioFrame };

let incoming_frame = AudioFrame {
    rtp_timestamp: 12345,      // From RTP header
    rtp_sequence: 100,         // 16-bit sequence number
    samples: Arc::new(vec![...]), // f32 audio samples (960 @ 48kHz = 20ms)
    received_at: Instant::now(),
    payload_size: 960,
};

let synced_frame = sync_manager.process_audio_frame(incoming_frame)?;

println!(
    "Audio frame: {} samples @ {} Hz, wall_clock: {} us",
    synced_frame.sample_count,
    synced_frame.sample_rate,
    synced_frame.wall_clock_timestamp_us
);
```

**Input Structure**:
```rust
pub struct AudioFrame {
    /// RTP timestamp (48kHz reference clock)
    /// Increments by sample count per frame (960 for 20ms @ 48kHz)
    pub rtp_timestamp: u32,

    /// RTP sequence number (16-bit, wraps at 65536)
    /// Used to detect missing packets and reordering
    pub rtp_sequence: u16,

    /// Audio samples (f32, always 48kHz after Opus decode)
    pub samples: Arc<Vec<f32>>,

    /// When frame was received locally
    pub received_at: Instant,

    /// Opus frame size in samples (960, 1920, 2880, etc.)
    pub payload_size: usize,
}
```

**Output Structure**:
```rust
pub struct SyncedAudioFrame {
    /// Synchronized audio samples
    pub samples: Arc<Vec<f32>>,

    /// Sample rate (always 48kHz)
    pub sample_rate: u32,

    /// Wall-clock timestamp (microseconds since Unix epoch)
    /// Used to align with video and other peers
    pub wall_clock_timestamp_us: u64,

    /// RTP timestamp (echoed from input)
    pub rtp_timestamp: u32,

    /// Jitter buffer delay (ms)
    /// How long frame spent in buffer before output
    pub buffer_delay_ms: f64,

    /// Confidence in synchronization (0.0-1.0)
    /// 1.0 = NTP mapping available and recent
    /// 0.5 = Using fallback (local clock)
    pub sync_confidence: f32,

    /// Clock drift estimate (ppm)
    /// Positive = sender clock faster than receiver
    pub clock_drift_ppm: i32,
}
```

**Behavior**:

1. **Packet Loss Detection**: Compares sequence number with expected next
   - If gap detected: logs warning
   - Frames are output in order (later frame won't be output until earlier expected frame arrives or timeout)

2. **Jitter Buffer Management**:
   - Inserts frame in order by RTP timestamp
   - Waits ~50ms before outputting (allows out-of-order packets to arrive)
   - Automatically pops oldest frame when ready

3. **Clock Drift Tracking**:
   - Monitors RTP timestamps vs local arrival times
   - Estimates if sender clock is faster/slower than receiver
   - If drift > threshold: returns in drift_ppm field

4. **NTP/RTP Mapping**:
   - Updates from RTCP Sender Reports (call `update_rtcp()`)
   - Converts RTP timestamp to absolute wall-clock time
   - Without NTP: uses local clock (lower confidence)

**Error Conditions**:
- `JitterBufferOverflow`: Too many frames buffered (>500ms)
- `InvalidTimestamp`: RTP timestamp discontinuity (likely error)
- `SyncNotInitialized`: Called before RTCP sync data available

**Performance**:
- Jitter buffer insertion: O(log n) where n = buffered frames (typically 2-5)
- Output: O(1)
- Clock drift calculation: O(1)

**Example Flow**:
```
Frame 1: rtp_ts=1000, seq=100, received at T0
Frame 2: rtp_ts=1960, seq=101, received at T1 (out of order)
Frame 3: rtp_ts=960, seq=99, received at T2 (very late)

Jitter Buffer Timeline:
T0: Insert Frame 1 (seq 100)
T1: Insert Frame 2 (seq 101)
T2: Insert Frame 3 (seq 99) → reorder to correct position
T0+50ms: Pop Frame 3 (seq 99) → output (wait elapsed)
T0+50ms: Output Frame 1 (seq 100)
T1+50ms: Output Frame 2 (seq 101)

Result: Frames output in correct order despite arrival disorder
```

---

#### `pop_next_audio_frame(&mut self) -> Result<Option<SyncedAudioFrame>>`

**Purpose**: Get next buffered audio frame (if ready)

**Return Type**: `Result<Option<SyncedAudioFrame>, Error>`

**Example**:
```rust
loop {
    if let Some(synced_frame) = sync_manager.pop_next_audio_frame()? {
        println!("Audio ready: {} samples", synced_frame.sample_count);
        // Feed to pipeline
        pipeline.process_audio(&synced_frame.samples).await?;
    } else {
        // No frame ready yet (buffer not filled)
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}
```

**Preconditions**:
- At least one frame in buffer
- Minimum buffer delay elapsed (50ms default)

**Postconditions**:
- Frame removed from jitter buffer
- Ready for downstream processing

**Return Value**:
- `Some(frame)`: Frame is ready and synchronized
- `None`: No frames ready (buffer not yet full)

**Error Conditions**:
- `BufferEmpty`: No frames queued
- `NotReady`: Frame available but buffer delay not elapsed

**Notes**:
- Non-blocking: returns immediately
- Use in loop with small sleep to avoid busy-waiting
- Frames are output in strict order (by RTP sequence)

---

### 3. Video Processing Methods

#### `process_video_frame(&mut self, frame: VideoFrame) -> Result<SyncedVideoFrame>`

**Purpose**: Process incoming video frame with synchronization

**Parameters**:
- `frame: VideoFrame` - Incoming RTP video frame

**Return Type**: `Result<SyncedVideoFrame, Error>`

**Example**:
```rust
use remotemedia_webrtc::sync::{ VideoFrame, SyncedVideoFrame };

let video_frame = VideoFrame {
    rtp_timestamp: 8100,  // 90kHz clock, 90ms apart
    rtp_sequence: 50,
    width: 1280,
    height: 720,
    format: PixelFormat::I420,
    planes: vec![y_plane, u_plane, v_plane],
    received_at: Instant::now(),
    marker_bit: true,  // Last RTP packet of frame
    is_keyframe: false,
};

let synced_frame = sync_manager.process_video_frame(video_frame)?;

println!(
    "Video frame: {}x{} @ {} Hz, wall_clock: {} us",
    synced_frame.width,
    synced_frame.height,
    synced_frame.framerate_estimate,
    synced_frame.wall_clock_timestamp_us
);
```

**Input Structure**:
```rust
pub struct VideoFrame {
    /// RTP timestamp (90kHz reference clock, per RTP spec)
    /// Typically 90000 timestamps per second
    /// E.g., 30fps = 3000 timestamp increment per frame
    pub rtp_timestamp: u32,

    /// RTP sequence number (16-bit)
    pub rtp_sequence: u16,

    /// Frame width in pixels
    pub width: u32,

    /// Frame height in pixels
    pub height: u32,

    /// Pixel format (I420, NV12, RGB, etc.)
    pub format: PixelFormat,

    /// Video plane data (Y, U, V for I420)
    pub planes: Vec<Vec<u8>>,

    /// When received locally
    pub received_at: Instant,

    /// RTP marker bit (frame boundary)
    /// true = last RTP packet of this frame
    pub marker_bit: bool,

    /// Is this a keyframe (I-frame)?
    /// Important for decoder state
    pub is_keyframe: bool,
}
```

**Output Structure**:
```rust
pub struct SyncedVideoFrame {
    /// Video frame data
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub planes: Vec<Vec<u8>>,

    /// Synchronized timestamp (microseconds)
    pub wall_clock_timestamp_us: u64,

    /// RTP timestamp
    pub rtp_timestamp: u32,

    /// Estimated framerate (fps) based on timestamps
    pub framerate_estimate: f32,

    /// Jitter buffer delay (ms)
    pub buffer_delay_ms: f64,

    /// Audio sync offset
    /// If negative: video is behind audio (needs to hold frame longer)
    /// If positive: video is ahead of audio (output immediately)
    pub audio_sync_offset_ms: i32,

    /// Sync confidence
    pub sync_confidence: f32,
}
```

**Behavior**:

1. **Frame Reordering**: Similar to audio (jitter buffer)
2. **Keyframe Detection**: Tracks I-frames for decoder recovery
3. **Framerate Estimation**: Computes fps from RTP timestamps
4. **Audio/Video Alignment**: Computes offset relative to audio timeline

**Error Conditions**:
- `JitterBufferOverflow`: Too many frames buffered
- `InvalidFrameFormat`: Width/height invalid or unsupported
- `InvalidTimestamp`: Discontinuous RTP timestamp

**Performance**:
- Insertion: O(log n)
- Output: O(1) to O(h*w) for format conversion

**Notes**:
- Video frames are larger (typically hundreds of KB to MB)
- Be mindful of memory usage with jitter buffer
- Keyframes should be requested periodically (every 2-5 seconds)

---

#### `pop_next_video_frame(&mut self) -> Result<Option<SyncedVideoFrame>>`

**Purpose**: Get next video frame ready for output

**Return Type**: `Result<Option<SyncedVideoFrame>, Error>`

**Example**:
```rust
if let Some(synced_frame) = sync_manager.pop_next_video_frame()? {
    // Check if audio is still behind
    if synced_frame.audio_sync_offset_ms > 0 {
        // Video ahead of audio - hold frame until audio catches up
        buffer.hold_frame(synced_frame, synced_frame.audio_sync_offset_ms).await;
    } else {
        // Audio caught up - output video now
        pipeline.process_video(&synced_frame).await?;
    }
}
```

**Notes**:
- Same behavior as `pop_next_audio_frame()`
- May return None if buffer not yet ready
- Check `audio_sync_offset_ms` to implement lip-sync

---

### 4. Clock Synchronization Methods

#### `update_rtcp_sender_report(&mut self, rtcp_sr: RtcpSenderReport) -> Result<()>`

**Purpose**: Update RTP/NTP timestamp mapping from RTCP Sender Report

**Parameters**:
- `rtcp_sr: RtcpSenderReport` - Sender Report from remote peer

**Return Type**: `Result<(), Error>`

**Example**:
```rust
use remotemedia_webrtc::sync::RtcpSenderReport;

let rtcp_sr = RtcpSenderReport {
    ntp_timestamp: 3735928559_123456,  // NTP timestamp (64-bit)
    rtp_timestamp: 12345,              // Corresponding RTP timestamp
    packet_count: 5000,                // Total packets sent
    octet_count: 5000000,              // Total octets sent
};

sync_manager.update_rtcp_sender_report(rtcp_sr)?;

// Now clock drift can be calculated and RTP -> wall-clock conversion works
```

**RTCP Sender Report Structure**:
```rust
pub struct RtcpSenderReport {
    /// NTP timestamp (64-bit: 32 seconds + 32 fraction)
    /// Represents absolute wall-clock time from sender
    pub ntp_timestamp: u64,

    /// RTP timestamp at NTP time
    /// Used to map RTP → wall-clock
    pub rtp_timestamp: u32,

    /// Total RTP packets sent by sender
    pub packet_count: u32,

    /// Total RTP octets sent
    pub octet_count: u32,

    /// Optional: sender's local time for debugging
    pub sender_time: Option<SystemTime>,
}

impl RtcpSenderReport {
    /// Convert NTP 64-bit to wall-clock microseconds
    pub fn ntp_to_us(&self) -> u64 {
        let seconds = (self.ntp_timestamp >> 32) as u64;
        let fraction = (self.ntp_timestamp & 0xFFFFFFFF) as u64;

        // Seconds since 1900-01-01
        let epoch_seconds = seconds.saturating_sub(2208988800);
        let fraction_us = (fraction * 1_000_000) >> 32;

        epoch_seconds * 1_000_000 + fraction_us
    }
}
```

**Preconditions**:
- RTP stream already being processed

**Postconditions**:
- NTP/RTP mapping updated
- Clock drift estimator informed
- RTP → wall-clock conversion now available

**Frequency**: Should be called every 5-10 seconds (when RTCP SR received)

**Error Conditions**:
- `InvalidNtpTimestamp`: Timestamp is zero or invalid
- `TimestampDiscontinuity`: Large gap from last SR (possible stream reset)

**Impact**:
- Before first SR: fall back to local clock (confidence = 0.5)
- After SR: use NTP mapping (confidence = 1.0)
- Multiple SRs: enables clock drift estimation

**Example Timeline**:
```
T0: Stream starts, no SR yet
    RTP → wall-clock: use local clock
    Confidence: 0.5

T5s: First RTCP SR received
    NTP mapping: (3735928559_123456, RTP=12345)
    Confidence: 1.0
    Clock drift: collecting samples

T10s: Second RTCP SR received
    NTP mapping updated
    Clock drift estimated: ±50ppm (0.005%)
    Can now adjust for drift

T15s: Clock drift stable
    Estimate: sender clock 50ppm faster
    Correction: speed up audio playback by 0.005%
```

---

#### `estimate_clock_drift(&self) -> Option<ClockDriftEstimate>`

**Purpose**: Get current clock drift estimate between sender and receiver

**Return Type**: `Option<ClockDriftEstimate>`

**Example**:
```rust
if let Some(drift) = sync_manager.estimate_clock_drift() {
    println!(
        "Clock drift: {} ppm ({:.3}%)",
        drift.drift_ppm,
        drift.drift_ppm as f32 / 10000.0
    );

    if drift.drift_ppm > 100 {
        println!("Warning: sender clock running fast");
    }

    // Adaptive bitrate adjustment
    if drift.sample_count > 10 {
        // Enough samples to trust estimate
        adjust_sample_rate(drift.correction_factor)?;
    }
}
```

**Return Value**:
```rust
pub struct ClockDriftEstimate {
    /// Estimated drift in parts per million
    /// Positive = sender clock faster than receiver
    /// Negative = sender clock slower than receiver
    /// Typical range: ±100 to ±1000 ppm
    pub drift_ppm: i32,

    /// Number of observations used for estimate
    /// Requires minimum 10 before trusting
    pub sample_count: usize,

    /// Correction factor to apply to sample rate
    /// 1.0 = no correction
    /// 1.001 = speed up playback by 0.1%
    /// 0.999 = slow down playback by 0.1%
    pub correction_factor: f32,

    /// Confidence (0.0-1.0)
    /// 1.0 = stable estimate with many samples
    /// 0.5 = provisional estimate
    pub confidence: f32,

    /// Recommended action
    pub recommended_action: DriftAction,
}

pub enum DriftAction {
    /// No action needed (drift within tolerance)
    None,

    /// Monitor closely (drift at edge of tolerance)
    Monitor,

    /// Apply sample rate adjustment (drift exceeds threshold)
    Adjust,

    /// Potential error condition (large discontinuity)
    Investigate,
}
```

**Preconditions**:
- At least one RTCP SR received (for accurate drift)
- At least 10 samples collected

**Return Value**:
- `Some(estimate)`: Clock drift calculated
- `None`: Not enough data yet

**Accuracy**:
- After 1 SR: provisional (confidence = 0.5)
- After 10 SRs: stable (confidence = 1.0)
- Typical accuracy: ±10 ppm

**Typical Values**:
- Well-synchronized clocks: ±10 ppm
- Typical consumer hardware: ±50 ppm
- Degraded clocks: ±500 ppm (rare, indicates system issues)

**Use Cases**:
1. **Monitoring**: Log drift estimates for diagnostics
2. **Adaptive Playback**: Adjust sample rate for resilience
3. **Alerting**: Warn if drift > ±100 ppm (indicates hardware issue)
4. **Network Quality**: Include in connection quality metrics

---

#### `apply_clock_drift_correction(&mut self, correction_factor: f32) -> Result<()>`

**Purpose**: Manually apply sample rate adjustment to correct for clock drift

**Parameters**:
- `correction_factor: f32` - Multiplicative adjustment
  - 1.0 = no adjustment
  - 1.001 = speed up 0.1%
  - 0.999 = slow down 0.1%

**Return Type**: `Result<(), Error>`

**Example**:
```rust
// Automatic correction based on estimate
if let Some(drift) = sync_manager.estimate_clock_drift() {
    if drift.recommended_action == DriftAction::Adjust {
        sync_manager.apply_clock_drift_correction(drift.correction_factor)?;
        println!("Applied clock correction: {}", drift.correction_factor);
    }
}

// Manual correction
sync_manager.apply_clock_drift_correction(1.0005)?; // Speed up 0.05%
```

**Constraints**:
- Correction factor must be in range [0.99, 1.01] (±1% max)
- Changes apply to next audio frame processing
- Not abrupt: gradual transition over ~100ms

**Error Conditions**:
- `InvalidCorrectionFactor`: Outside allowed range
- `CorrelationAlreadyApplied`: Correction factor already active

**Impact**:
- Modifies the RTP timestamp to sample rate conversion
- Next frame's wall-clock timestamp adjusted accordingly
- Prevents audio glitches from large sample rate changes

**Notes**:
- Should be applied infrequently (once every 10+ seconds)
- Avoid oscillating corrections (hysteresis needed)
- Correction factor changes take effect immediately on next frame

---

### 5. Synchronization Query Methods

#### `get_sync_state(&self) -> SyncState`

**Purpose**: Get current synchronization state

**Return Type**: `SyncState`

**Example**:
```rust
match sync_manager.get_sync_state() {
    SyncState::Unsynced => {
        println!("Waiting for RTCP Sender Report...");
        // Don't start processing yet
    }
    SyncState::Syncing => {
        println!("Syncing in progress (drift estimation)");
        // Can process but results may not be perfectly synchronized
    }
    SyncState::Synced => {
        println!("Ready for synchronized processing");
        // Full synchronization available
    }
}
```

**State Values**:
```rust
pub enum SyncState {
    /// No sync data available yet
    /// - No RTCP SR received
    /// - No valid NTP/RTP mapping
    Unsynced,

    /// Collecting sync samples
    /// - First RTCP SR received
    /// - Collecting multiple observations for drift estimate
    /// - Takes ~10-50 seconds
    Syncing,

    /// Full synchronization available
    /// - Multiple RTCP SRs received
    /// - Clock drift estimated and stable
    /// - Ready for precision audio/video processing
    Synced,
}
```

**Use Cases**:
1. **Initialization**: Wait for Synced before processing
2. **Monitoring**: Log state transitions
3. **UI**: Show sync indicator to user

---

#### `get_buffer_statistics(&self) -> BufferStats`

**Purpose**: Get jitter buffer statistics for diagnostics

**Return Type**: `BufferStats`

**Example**:
```rust
let stats = sync_manager.get_buffer_statistics();

println!(
    "Jitter Buffer - Capacity: {}%, Late packets: {}, Overruns: {}",
    (stats.current_frames as f32 / stats.max_frames as f32) * 100.0,
    stats.late_packet_count,
    stats.buffer_overrun_count
);

if stats.current_frames > stats.max_frames / 2 {
    println!("Warning: buffer running high");
}
```

**Return Value**:
```rust
pub struct BufferStats {
    /// Current frames in buffer
    pub current_frames: usize,

    /// Maximum frames ever buffered (peak)
    pub peak_frames: usize,

    /// Max capacity before dropping frames
    pub max_frames: usize,

    /// Frames dropped due to overflow
    pub dropped_frames: usize,

    /// Packets that arrived too late for buffer
    pub late_packet_count: u32,

    /// Times buffer overflowed and discarded frames
    pub buffer_overrun_count: u32,

    /// Current buffer delay (milliseconds)
    pub current_delay_ms: f64,

    /// Average delay over last 10 frames
    pub average_delay_ms: f64,

    /// Estimated packet loss rate
    pub estimated_loss_rate: f32,
}
```

**Diagnostics**:
- High `dropped_frames`: Network is very lossy
- High `late_packet_count`: Excessive jitter
- Growing `current_delay_ms`: Possible underflow
- High `buffer_overrun_count`: Buffer size inadequate

---

### 6. Timestamp Conversion Methods

#### `rtp_to_wall_clock(&self, rtp_timestamp: u32) -> Result<u64>`

**Purpose**: Convert RTP timestamp to wall-clock microseconds

**Parameters**:
- `rtp_timestamp: u32` - RTP timestamp value

**Return Type**: `Result<u64, Error>` - Microseconds since Unix epoch

**Example**:
```rust
// For audio (48kHz)
let rtp_ts = 12345;
let wall_clock_us = sync_manager.rtp_to_wall_clock(rtp_ts)?;

println!(
    "RTP {} -> wall-clock {} us",
    rtp_ts,
    wall_clock_us
);

// Calculate seconds
let seconds = wall_clock_us / 1_000_000;
let micros = wall_clock_us % 1_000_000;
println!("Time: {}.{:06} seconds", seconds, micros);

// Compare with other peers/streams for synchronization
for peer_id in peers {
    let peer_wall_time = sync_manager.rtp_to_wall_clock(peer_rtp_ts)?;
    if (wall_clock_us as i64 - peer_wall_time as i64).abs() < 20_000 {
        println!("{} is synchronized with {}", peer_id, current_peer_id);
    }
}
```

**Preconditions**:
- At least one RTCP SR received (for accurate conversion)
- Falls back to local clock if no SR available (lower confidence)

**Return Value**:
- Microseconds since Unix epoch (1970-01-01 00:00:00 UTC)
- Can be directly compared across peers/streams

**Error Conditions**:
- `NoNtpMapping`: No RTCP SR received (uses fallback)
- `InvalidTimestamp`: RTP timestamp is negative/invalid

**Accuracy**:
- With RTCP SR: ±10 microseconds
- Without RTCP SR: ±1 millisecond (local clock dependent)

**Performance**: O(1), negligible overhead

**Formula**:
```
wall_clock_us = ntp_timestamp_us + (rtp_ts - rtp_base) / clock_rate_hz * 1_000_000
```

---

#### `wall_clock_to_rtp(&self, wall_clock_us: u64) -> Result<u32>`

**Purpose**: Reverse conversion (wall-clock to RTP timestamp)

**Parameters**:
- `wall_clock_us: u64` - Wall-clock time in microseconds

**Return Type**: `Result<u32, Error>` - RTP timestamp

**Example**:
```rust
// Get current wall-clock time
let now_us = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_micros() as u64;

// Convert to RTP timestamp for this peer
let rtp_ts = sync_manager.wall_clock_to_rtp(now_us)?;

println!("Current RTP timestamp would be: {}", rtp_ts);
```

**Use Case**: When constructing synthetic frames or metadata

**Error Conditions**:
- `NoNtpMapping`: Cannot perform conversion
- `OutOfRange`: Wall-clock time too far in past/future

---

### 7. Multi-Peer Synchronization Methods

#### `align_with_peer(&self, other_sync: &SyncManager) -> Result<TimestampOffset>`

**Purpose**: Calculate synchronization offset relative to another peer

**Parameters**:
- `other_sync: &SyncManager` - SyncManager from another peer

**Return Type**: `Result<TimestampOffset, Error>`

**Example**:
```rust
// After receiving audio from multiple peers
let offset_ab = sync_manager_a.align_with_peer(&sync_manager_b)?;

println!(
    "Peer B is {} ms {}",
    offset_ab.offset_ms.abs(),
    if offset_ab.offset_ms > 0 { "ahead" } else { "behind" }
);

// Use for multi-peer mixing
if offset_ab.offset_ms < 5 {
    println!("Peers are synchronized within 5ms");
    // Safe to mix audio
} else {
    println!("Peers out of sync - need to buffer");
    // Add extra buffering
}
```

**Return Value**:
```rust
pub struct TimestampOffset {
    /// Offset in milliseconds
    /// Positive: other peer is ahead
    /// Negative: other peer is behind
    pub offset_ms: i32,

    /// Confidence (0.0-1.0)
    pub confidence: f32,

    /// Whether offset is stable
    pub is_stable: bool,
}
```

**Preconditions**:
- Both peers must have sync state >= Syncing
- At least one RTCP SR from each peer

**Use Cases**:
1. **Audio Mixing**: Ensure all audio is synchronized before mixing
2. **Multi-Peer Monitoring**: Detect clock skew between peers
3. **Recording**: Ensure multi-peer audio stays in sync during recording

**Accuracy**: ±10ms typical

---

## Configuration Examples

### Example 1: Standard Configuration (Recommended)

```rust
let config = SyncConfig {
    audio_clock_rate: 48_000,
    video_clock_rate: 90_000,
    jitter_buffer_size_ms: 50,
    max_jitter_buffer_ms: 200,
    enable_clock_drift_correction: true,
    drift_correction_threshold_ppm: 100,
    rtcp_interval_ms: 5000,
};

let sync = SyncManager::new("peer-1", config)?;
```

**Characteristics**:
- Low latency: 50ms buffer
- Tight sync: ±100ppm drift correction
- Handles typical network jitter well
- Suitable for real-time conferencing

### Example 2: High-Latency Network (Poor Connectivity)

```rust
let config = SyncConfig {
    jitter_buffer_size_ms: 100,     // Larger buffer
    max_jitter_buffer_ms: 300,      // More headroom
    drift_correction_threshold_ppm: 500, // Tolerate more drift
    ..Default::default()
};
```

**Characteristics**:
- Higher latency: 100ms buffer (total pipeline delay may be 150-200ms)
- More tolerant of jitter
- Better for lossy networks
- Trade-off: slightly higher latency

### Example 3: Synchronized Recording

```rust
let config = SyncConfig {
    jitter_buffer_size_ms: 100,
    enable_clock_drift_correction: true,
    drift_correction_threshold_ppm: 50, // Stricter
    rtcp_interval_ms: 2000, // More frequent RTCP
};
```

**Characteristics**:
- Ensures all peers stay tightly synchronized
- More frequent RTCP for better tracking
- Suitable for multi-party recording where sync is critical

---

## Error Recovery Patterns

### Pattern 1: Handle RTP Timestamp Discontinuity

```rust
match sync_manager.process_audio_frame(frame) {
    Ok(synced) => {
        // Process normally
    }
    Err(SyncError::InvalidTimestamp) => {
        // Large jump in RTP timestamp - likely stream reset
        println!("Stream reset detected");
        sync_manager.reset()?;

        // Wait for next RTCP SR before processing
        if sync_manager.get_sync_state() == SyncState::Unsynced {
            println!("Waiting for RTCP sync...");
            // Skip frame, wait for SR
        }
    }
    Err(e) => eprintln!("Sync error: {}", e),
}
```

### Pattern 2: Monitor and Alert on Clock Drift

```rust
let mut last_drift_check = Instant::now();

loop {
    // ... process frames ...

    if last_drift_check.elapsed() > Duration::from_secs(10) {
        if let Some(drift) = sync_manager.estimate_clock_drift() {
            match drift.recommended_action {
                DriftAction::Investigate => {
                    eprintln!("ALERT: Large clock drift detected: {} ppm", drift.drift_ppm);
                    // May need to reconnect
                }
                DriftAction::Adjust => {
                    sync_manager.apply_clock_drift_correction(drift.correction_factor)?;
                }
                _ => {}
            }
        }
        last_drift_check = Instant::now();
    }
}
```

### Pattern 3: Handle Buffer Overflow

```rust
let stats = sync_manager.get_buffer_statistics();

if stats.buffer_overrun_count > 10 {
    eprintln!(
        "Buffer overruns detected: {}",
        stats.buffer_overrun_count
    );

    // Option 1: Increase buffer size
    // (Would need to reconfigure and reset)

    // Option 2: Reduce incoming frame rate
    if let Some(frame) = sync_manager.pop_next_audio_frame()? {
        // Skip some frames to drain buffer
        continue; // Don't process every frame
    }

    // Option 3: Adjust network parameters (request lower bitrate)
    peer_connection.set_bitrate_limit(lower_bitrate)?;
}
```

---

## Testing

### Unit Tests

```rust
#[test]
fn test_rtp_timestamp_continuity() {
    let mut sync = SyncManager::new("test-peer", default_config()).unwrap();

    // Frame 1: timestamp=1000
    let frame1 = AudioFrame {
        rtp_timestamp: 1000,
        rtp_sequence: 100,
        ..default()
    };
    sync.process_audio_frame(frame1).unwrap();

    // Frame 2: timestamp=1960 (960 samples = 20ms @ 48kHz)
    let frame2 = AudioFrame {
        rtp_timestamp: 1960,
        rtp_sequence: 101,
        ..default()
    };
    sync.process_audio_frame(frame2).unwrap();

    // Both should process without error
    assert_eq!(sync.get_sync_state(), SyncState::Unsynced);
}

#[test]
fn test_jitter_buffer_reordering() {
    let mut sync = SyncManager::new("test-peer", default_config()).unwrap();

    // Insert out of order
    sync.process_audio_frame(frame_with_ts(2000, 102)).unwrap();
    sync.process_audio_frame(frame_with_ts(1000, 100)).unwrap();
    sync.process_audio_frame(frame_with_ts(1960, 101)).unwrap();

    // Wait for buffer to fill
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Pop should return in order
    let f1 = sync.pop_next_audio_frame().unwrap().unwrap();
    let f2 = sync.pop_next_audio_frame().unwrap().unwrap();

    assert!(f1.rtp_timestamp < f2.rtp_timestamp);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_clock_drift_estimation() {
    let mut sync = SyncManager::new("peer-1", default_config()).unwrap();

    // Simulate RTCP SR arriving every 5 seconds for 1 minute
    for i in 0..12 {
        let sr = RtcpSenderReport {
            ntp_timestamp: base_ntp + i * 5_000_000, // 5 seconds apart
            rtp_timestamp: base_rtp + i * 240_000,    // 240kHz per 5 seconds (48kHz)
            ..default()
        };
        sync.update_rtcp_sender_report(sr).unwrap();

        if let Some(drift) = sync.estimate_clock_drift() {
            println!("Iteration {}: drift = {} ppm", i, drift.drift_ppm);
        }
    }

    // After 12 SRs, should have stable estimate
    let final_drift = sync.estimate_clock_drift().unwrap();
    assert!(final_drift.confidence > 0.9);
}
```

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2025-11-07 | Initial specification with jitter buffer, clock drift estimation, RTP/NTP mapping |

---

## See Also

- [Transport API Contract](./transport-api.md)
- [Signaling Protocol Contract](./signaling-protocol.md)
- [Feature Specification](../spec.md)
- [Data Model](../data-model.md)
- [Research Document](../../transports/remotemedia-webrtc/research.md)

---

## Appendix: RTP Clock Rates

| Codec | Clock Rate | Timestamp per Frame | Frame Duration | Example Frame Size |
|-------|-----------|-------------------|-----------------|-------------------|
| Opus (Audio) | 48 kHz | 960 per 20ms | 20ms typical | 960 samples |
| VP9 (Video) | 90 kHz | 3000 per 33.3ms | 33.3ms (30fps) | Multiple packets |
| H.264 (Video) | 90 kHz | 3000 per 33.3ms | 33.3ms (30fps) | Multiple packets |

---

## Appendix: RTCP Sender Report Format

RTCP Sender Reports arrive approximately every 5 seconds and contain:
1. **NTP Timestamp** (64-bit): Absolute wall-clock time from sender
2. **RTP Timestamp** (32-bit): Corresponding RTP clock value
3. **Packet Count**: Total RTP packets sent
4. **Octet Count**: Total RTP bytes sent

The NTP/RTP pair creates a synchronization point allowing:
- Conversion of RTP timestamps to absolute wall-clock time
- Estimation of clock drift between sender and receiver
- Coordination across multiple streams (audio + video)
