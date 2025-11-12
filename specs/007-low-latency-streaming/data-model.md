# Data Model: Low-Latency Real-Time Streaming Pipeline

**Feature**: 007-low-latency-streaming | **Date**: 2025-11-10

## Overview

This document defines the core data structures for low-latency streaming optimizations. All structures are designed for Rust implementation with serialization support for IPC and network transport.

---

## Core Entities

### 1. SpeculativeSegment

**Purpose**: Represents an audio segment forwarded speculatively before final VAD decision.

**Structure**:
```rust
pub struct SpeculativeSegment {
    /// Unique identifier for this segment
    pub segment_id: Uuid,

    /// Start timestamp (microseconds since session start)
    pub start_timestamp: u64,

    /// End timestamp (microseconds since session start)
    pub end_timestamp: u64,

    /// Current status
    pub status: SegmentStatus,

    /// Reference to audio data in ring buffer (index range)
    pub buffer_range: (usize, usize),

    /// Session ID this segment belongs to
    pub session_id: String,
}

pub enum SegmentStatus {
    /// Speculatively forwarded, awaiting VAD confirmation
    Speculative,

    /// VAD confirmed as speech, safe to process
    Confirmed,

    /// VAD retroactively cancelled (was noise/silence)
    Cancelled { reason: String },
}
```

**Validation Rules**:
- `segment_id` must be unique within session
- `start_timestamp` < `end_timestamp`
- `end_timestamp - start_timestamp` should be 10-50ms (typical VAD window)
- `buffer_range.1 > buffer_range.0`

**State Transitions**:
```
Speculative -> Confirmed  (VAD confirms speech)
Speculative -> Cancelled  (VAD refines decision, was noise)
Confirmed -> [final]      (no further transitions)
Cancelled -> [final]      (no further transitions)
```

**Relationships**:
- Belongs to one `RingBuffer` instance per session
- Referenced by `ControlMessage` for cancellation
- Tracked in `LatencyMetrics` for speculation acceptance rate

---

### 2. ControlMessage

**Purpose**: Standardized message for pipeline control flow (cancellation, batching hints, deadlines).

**Structure**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlMessage {
    /// Type of control message
    pub message_type: ControlMessageType,

    /// Session ID this message applies to
    pub session_id: String,

    /// Timestamp when message was created (microseconds)
    pub timestamp: u64,

    /// Optional target segment ID (for cancellation)
    pub target_segment_id: Option<Uuid>,

    /// Extensible metadata (JSON-compatible)
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ControlMessageType {
    /// Cancel a speculative segment
    CancelSpeculation {
        from_timestamp: u64,
        to_timestamp: u64,
    },

    /// Hint to batch more aggressively
    BatchHint {
        suggested_batch_size: usize,
    },

    /// Soft deadline approaching
    DeadlineWarning {
        deadline_us: u64,  // Microseconds from now
    },
}
```

**Validation Rules**:
- `session_id` must match current session
- For `CancelSpeculation`: `from_timestamp` < `to_timestamp`
- `timestamp` should be recent (within last 100ms, warn if stale)

**Serialization**:
- Wire format (IPC/network): Follows existing RuntimeData binary format
  ```
  type (1 byte) = 5 (ControlMessage)
  | session_len (2 bytes) | session_id
  | timestamp (8 bytes)
  | payload_len (4 bytes) | JSON payload
  ```
- JSON payload contains full `ControlMessage` struct

**Propagation Contexts**:
- ✅ Local Rust nodes: Direct struct passing via channels
- ✅ Multiprocess Python (iceoryx2): Binary serialization + JSON payload
- ✅ gRPC transport: Protobuf message (new `ControlMessage` proto)
- ✅ WebRTC data channel: JSON over data channel
- ✅ HTTP/SSE: JSON event

---

### 3. NodeCapabilities

**Purpose**: Metadata describing a node's execution characteristics for runtime optimization.

**Structure**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Node type identifier (matches registry)
    pub node_type: String,

    /// Can this node process multiple inputs concurrently?
    pub parallelizable: bool,

    /// Does this node benefit from batched inputs?
    pub batch_aware: bool,

    /// Average processing time (microseconds), updated via EMA
    pub avg_processing_us: f64,

    /// Recommended queue capacity
    pub queue_capacity: usize,

    /// Overflow policy for bounded queue
    pub overflow_policy: OverflowPolicy,

    /// Does this node support control messages?
    pub supports_control_messages: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OverflowPolicy {
    /// Drop oldest item when queue full (real-time audio)
    DropOldest,

    /// Reject new item, keep queue intact
    DropNewest,

    /// Block until space available (backpressure)
    Block,

    /// Apply merge strategy to reduce queue depth
    MergeOnOverflow,
}
```

**Default Capabilities by Node Type**:
| Node Type | Parallelizable | Batch Aware | Queue Capacity | Overflow Policy |
|-----------|----------------|-------------|----------------|-----------------|
| `AudioResample` | true | false | 50 | DropOldest |
| `SileroVAD` | true | false | 50 | DropOldest |
| `SpeculativeVADGate` | true | false | 100 | DropOldest |
| `TTS` (Python) | false | true | 20 | MergeOnOverflow |
| `TextCollector` | true | true | 50 | Block |
| `BufferedProcessor` | false | true | 100 | MergeOnOverflow |

**Registry Management**:
- Capabilities registered at node factory creation
- Runtime updates `avg_processing_us` via exponential moving average (EMA α=0.1)
- User can override via manifest configuration

---

### 4. BufferingPolicy

**Purpose**: Configuration for auto-buffering wrapper behavior.

**Structure**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferingPolicy {
    /// Minimum inputs to batch before processing
    pub min_batch_size: usize,  // Default: 2-5

    /// Maximum wait time before processing (microseconds)
    pub max_wait_us: u64,  // Default: 75000-150000 (75-150ms)

    /// Maximum buffer size (memory limit)
    pub max_buffer_size: usize,  // Default: 100

    /// Strategy for merging buffered inputs
    pub merge_strategy: MergeStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MergeStrategy {
    /// Concatenate text with separator
    ConcatenateText {
        separator: String,  // " " or "\n"
    },

    /// Concatenate audio samples in time order
    ConcatenateAudio {
        ensure_continuity: bool,  // Check sample rate/channels match
        max_gap_ms: u64,           // Max silence gap before separate segment
    },

    /// Keep inputs separate (no merging)
    KeepSeparate,

    /// Custom merge function (not serializable, runtime only)
    Custom,  // Arc<dyn Fn> stored separately
}
```

**Validation Rules**:
- `min_batch_size` >= 1
- `max_wait_us` > 0 (must have timeout to prevent indefinite waits)
- `max_buffer_size` >= `min_batch_size`
- For `ConcatenateAudio`: `max_gap_ms` <= 100ms (avoid long silence in batches)

**Behavior**:
- Inputs accumulate in buffer until:
  - Count reaches `min_batch_size`, OR
  - Timer expires after `max_wait_us`, OR
  - Buffer reaches `max_buffer_size` (immediate flush)
- On flush: Apply `merge_strategy` and submit to inner node

**Default Policies by Node Type**:
```rust
// TTS Node
BufferingPolicy {
    min_batch_size: 3,
    max_wait_us: 100_000,  // 100ms
    max_buffer_size: 50,
    merge_strategy: MergeStrategy::ConcatenateText { separator: " ".into() },
}

// Audio Processing
BufferingPolicy {
    min_batch_size: 1,  // No batching
    max_wait_us: 10_000,  // 10ms
    max_buffer_size: 10,
    merge_strategy: MergeStrategy::KeepSeparate,
}
```

---

### 5. LatencyMetrics

**Purpose**: Per-node performance metrics for monitoring and tuning.

**Structure**:
```rust
pub struct LatencyMetrics {
    /// Node identifier
    pub node_id: String,

    /// Latency histogram (P50/P95/P99)
    histogram_1min: Histogram<u64>,
    histogram_5min: Histogram<u64>,
    histogram_15min: Histogram<u64>,

    /// Current queue depth
    pub queue_depth_current: AtomicUsize,

    /// Maximum queue depth observed
    pub queue_depth_max: AtomicUsize,

    /// Average batch size (for buffering nodes)
    pub batch_size_avg: AtomicU64,  // Fixed-point: value * 100

    /// Speculation acceptance rate (for SpeculativeVADGate)
    pub speculation_acceptance_rate: AtomicU64,  // Percentage * 100

    /// Total inputs processed
    pub total_inputs: AtomicU64,

    /// Timestamp of last reset
    pub last_reset: AtomicU64,
}

impl LatencyMetrics {
    /// Record a latency sample (microseconds)
    pub fn record_latency(&self, latency_us: u64) -> Result<(), String>;

    /// Get percentile from specified window
    pub fn percentile(&self, p: f64, window: Window) -> u64;

    /// Export to Prometheus format
    pub fn to_prometheus(&self) -> String;
}

pub enum Window {
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
}
```

**Metrics Collection**:
- Recording overhead: <1μs per sample (using `hdrhistogram`)
- Histograms rotate every 1min/5min/15min to maintain sliding windows
- Atomic counters for zero-lock reads
- Prometheus export exposes all metrics as gauges/histograms

**Example Prometheus Output**:
```
# HELP node_latency_us Node processing latency in microseconds
# TYPE node_latency_us histogram
node_latency_us{node_id="tts_node",quantile="0.5",window="1min"} 45000
node_latency_us{node_id="tts_node",quantile="0.95",window="1min"} 120000
node_latency_us{node_id="tts_node",quantile="0.99",window="1min"} 200000

# HELP node_queue_depth Current queue depth
# TYPE node_queue_depth gauge
node_queue_depth{node_id="tts_node"} 5

# HELP node_speculation_acceptance_rate Speculation acceptance rate (0-100)
# TYPE node_speculation_acceptance_rate gauge
node_speculation_acceptance_rate{node_id="vad_gate"} 95.3
```

---

## 6. RingBuffer (Internal)

**Purpose**: Lock-free circular buffer for speculative audio segments.

**Structure**:
```rust
pub struct RingBuffer<T> {
    /// Underlying lock-free queue
    queue: ArrayQueue<T>,

    /// Capacity (fixed at creation)
    capacity: usize,

    /// Metrics
    overwrites: AtomicU64,
}

impl RingBuffer<SpeculativeSegment> {
    /// Create with fixed capacity (e.g., 200ms / 20ms chunks = 10 segments)
    pub fn new(capacity: usize) -> Self;

    /// Push segment, overwrite oldest if full
    pub fn push_overwrite(&self, segment: SpeculativeSegment) -> Option<SpeculativeSegment>;

    /// Get segments in timestamp range
    pub fn get_range(&self, from_ts: u64, to_ts: u64) -> Vec<SpeculativeSegment>;

    /// Clear segments before timestamp
    pub fn clear_before(&self, timestamp: u64);
}
```

**Capacity Calculation**:
- Lookback: 200ms, Lookahead: 50ms → Total: 250ms
- Segment size: 20ms (typical VAD hop)
- Capacity: 250ms / 20ms = 12.5 → round to 16 segments (power of 2)

---

## Relationships Diagram

```
Session
  ├─ RingBuffer [1]
  │    └─ SpeculativeSegment [0..N]
  │
  ├─ Executor [1]
  │    ├─ NodeCapabilities [N] (registry)
  │    └─ Node Instances [N]
  │         ├─ BufferingPolicy [0..1] (if BufferedProcessor)
  │         ├─ LatencyMetrics [1]
  │         └─ Input Queue [1] (bounded channel)
  │
  └─ ControlMessage Stream [0..N]
       └─ Propagates to all nodes in pipeline
```

---

## Validation Summary

All entities are:
- ✅ **Serializable**: Derive `Serialize`/`Deserialize` for IPC/network
- ✅ **Thread-safe**: Use `Arc`, `Atomic*`, or lock-free structures where needed
- ✅ **Testable**: Clear validation rules and state transitions
- ✅ **Observable**: Integrated with metrics collection
- ✅ **Extensible**: Metadata fields and enums support future additions

**Ready for Phase 1 contract generation.**
