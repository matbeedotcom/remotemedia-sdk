# Research: Low-Latency Real-Time Streaming Pipeline

**Feature**: 007-low-latency-streaming | **Date**: 2025-11-10

## Research Questions

This document consolidates technical research needed to make informed implementation decisions for the low-latency streaming optimizations.

---

## 1. Lock-Free Ring Buffer Implementation

**Question**: What's the best approach for implementing a thread-safe ring buffer for speculative audio segments (100-200ms lookback)?

**Decision**: Use `crossbeam_queue::ArrayQueue` for fixed-size ring buffer

**Rationale**:
- `crossbeam_queue::ArrayQueue` is a lock-free, bounded MPMC queue well-suited for audio segments
- Fixed capacity (e.g., 200ms @ 48kHz = ~9600 samples = ~10 segments of 20ms each)
- Lock-free operations for `push()` (overwrite oldest) and `pop()`/`get(index)` for lookback
- No dynamic allocation during runtime (predictable latency)
- Battle-tested in production Rust codebases

**Alternatives Considered**:
- **`std::collections::VecDeque` with `Mutex`**: Simpler but requires locking, adds ~50-100ns overhead per operation
- **Custom lock-free circular buffer**: More control but higher complexity, ~500 LOC vs. 50 LOC, not worth maintenance burden
- **`ringbuf` crate**: Designed for audio but single-producer/single-consumer only, doesn't support multiple nodes reading
- **`crossbeam_deque::Worker/Stealer`**: Designed for work-stealing, not FIFO ring buffer semantics

**Implementation Notes**:
- Add `crossbeam = "0.8"` to workspace dependencies
- Wrap `ArrayQueue<SpeculativeSegment>` in custom `RingBuffer` struct with helper methods:
  - `push_overwrite()`: Push new segment, overwrite oldest if full
  - `get_range(start_ts, end_ts)`: Retrieve segments for cancellation
  - `clear_before(timestamp)`: Remove committed segments

---

## 2. Histogram Library for P50/P95/P99 Metrics

**Question**: Which library provides efficient histogram tracking for per-node latency metrics with minimal overhead?

**Decision**: Use `hdrhistogram` crate (v7.5+)

**Rationale**:
- HDR Histogram provides accurate percentile tracking (P50/P95/P99) with bounded memory
- ~0.5-1μs recording overhead per sample (<<1% of node processing time)
- Configurable precision (significant figures) and value range
- Widely used in performance-critical systems (Cassandra, HBase, etc.)
- Built-in support for time-windowed histograms (1min/5min/15min via rotating histograms)

**Alternatives Considered**:
- **`prometheus` built-in histograms**: Bucket-based, less accurate for percentiles, but already in dependencies
- **`metrics` crate with `metrics-exporter-prometheus`**: More flexible but adds abstraction layer
- **Manual quantile tracking with `quantiles` crate**: Simpler but requires more manual management

**Implementation Notes**:
- Add `hdrhistogram = "7.5"` to workspace dependencies
- Create `LatencyHistogram` wrapper struct:
  - Three rotating `Histogram<u64>` instances (1min, 5min, 15min windows)
  - Record in microseconds: `histogram.record(latency_us)?`
  - Query percentiles: `histogram.value_at_quantile(0.99)`
- Integration with existing `prometheus` metrics via custom collector

---

## 3. Merge Strategies for BufferedProcessor

**Question**: What are the best practices for merging batched inputs (text concatenation, audio concatenation) with minimal latency?

**Decision**: Implement enum-based strategy pattern with four variants

**Rationale**:
- Different node types require different merge semantics:
  - **ConcatenateText**: Join strings with space/newline for TTS input
  - **ConcatenateAudio**: Append audio samples in time order (ensure continuity)
  - **KeepSeparate**: Process each input individually (default for parallelizable nodes)
  - **Custom**: User-provided merge function for specialized cases

**Implementation**:

```rust
pub enum MergeStrategy {
    ConcatenateText { separator: String },  // " " or "\n"
    ConcatenateAudio {
        ensure_continuity: bool,  // Check sample rate/channels match
        max_gap_ms: u64,           // Max silence gap before separate segment
    },
    KeepSeparate,
    Custom(Arc<dyn Fn(Vec<RuntimeData>) -> RuntimeData + Send + Sync>),
}
```

**Alternatives Considered**:
- **Trait-based strategy**: More flexible but requires Box<dyn Trait>, adds allocation overhead
- **Hard-coded per node type**: Less flexible, doesn't support user customization

**Best Practices** (from audio processing literature):
- For text: Preserve sentence boundaries, max 5 sentences per batch to avoid TTS quality degradation
- For audio: Ensure sample rate consistency, detect >50ms gaps as separate segments
- For batching timeout: Prefer `max_wait_ms` over `min_batch_size` to avoid indefinite waits

---

## 4. Control Message Serialization Format

**Question**: How should control messages be serialized for iceoryx2 IPC to support both Rust and Python?

**Decision**: Extend existing RuntimeData binary format with new `ControlMessage` variant

**Rationale**:
- Existing `RuntimeData::to_bytes()` format is simple and efficient (type byte + length-prefixed fields)
- Add new data type variant: `DataType::ControlMessage = 5`
- Reuse session_id and timestamp fields for correlation
- Payload encodes control-specific fields in JSON for flexibility

**Format**:
```
type (1 byte) = 5 (ControlMessage)
| session_len (2 bytes)
| session_id (variable)
| timestamp (8 bytes)
| payload_len (4 bytes)
| payload (JSON-encoded ControlMessage struct)
```

**Payload JSON**:
```json
{
  "message_type": "cancel_speculation",
  "target_segment_id": "uuid",
  "from_timestamp": 123456,
  "to_timestamp": 123789,
  "metadata": { /* extensible */ }
}
```

**Alternatives Considered**:
- **Separate control channel**: Cleaner separation but requires dual-channel management, complexity not justified
- **Protocol Buffers (protobuf)**: More structured but adds dependency, overkill for simple messages
- **bincode**: Faster than JSON but Python deserialization requires rust-python bindings, less flexible

---

## 5. Streaming Resampler Implementation

**Question**: How can we modify rubato's `FftFixedIn` to accept variable-sized chunks without buffering delays?

**Decision**: Wrap `FftFixedIn` with stateful adapter that accumulates/fragments chunks

**Rationale**:
- `rubato::FftFixedIn` requires exact input chunk sizes (e.g., 512 samples)
- Create `StreamingResampler` wrapper that:
  1. Accumulates input samples in internal buffer until chunk_size reached
  2. Calls `process()` on inner resampler
  3. Emits output immediately, stores remainder in buffer
  4. On small inputs (<chunk_size), holds until next input or timeout (5-10ms)

**Alternatives Considered**:
- **`rubato::FftFixedInOut`**: Requires both input and output to be fixed, even more restrictive
- **`rubato::SincFixedIn`**: Better quality but slower, not suitable for real-time (<1ms processing time required)
- **Fork rubato and modify**: Too much maintenance burden, upstream unlikely to accept streaming API

**Implementation Notes**:
- Add `StreamingResampler` struct with:
  - `inner: FftFixedIn<f32>`
  - `input_buffer: Vec<f32>` (max size = 2x chunk_size)
  - `output_buffer: Vec<f32>` (max size = 2x chunk_size)
- For P3 priority, initial implementation can use smaller fixed chunks (256 samples @ 48kHz = 5.3ms) as quick win
- Streaming adapter can be added later for additional 10-15ms latency improvement

---

## 6. Backpressure Propagation Strategy

**Question**: How should backpressure be handled when a downstream node's bounded queue is full?

**Decision**: Implement per-node configurable overflow policy via `OverflowPolicy` enum

**Rationale**:
- Different nodes have different backpressure requirements:
  - **Audio streaming**: Drop oldest to maintain real-time (prefer recent data)
  - **Text processing**: Block (backpressure) to avoid losing user input
  - **TTS batch queue**: Merge on overflow to maximize throughput

**Implementation**:

```rust
pub enum OverflowPolicy {
    DropOldest,       // Pop oldest, push new (audio default)
    DropNewest,       // Reject new input, keep queue intact
    Block,            // Async wait until space available (apply backpressure)
    MergeOnOverflow,  // Apply merge strategy to reduce queue depth
}
```

**Deadlock Prevention**:
- Bounded queues use `tokio::sync::mpsc::channel(capacity)` which supports `try_send()` (non-blocking) and `send().await` (backpressure)
- For `Block` policy: Propagate backpressure upstream via Tokio's async `.await` (natural flow control)
- For `DropOldest`/`DropNewest`: Use `try_send()` and handle `TrySendError::Full` immediately
- Maximum queue depth: 100 items (configurable per node), prevents unbounded memory growth

---

## 7. Deadline Hints Propagation

**Question**: How should soft deadline hints be propagated through the pipeline to enable adaptive quality degradation?

**Decision**: Extend `RuntimeData` with optional `deadline_hint_us` field (non-breaking change)

**Rationale**:
- Soft deadlines are advisory, not enforced (nodes can ignore if not supported)
- Add optional field to RuntimeData:
  ```rust
  pub struct RuntimeData {
      // ... existing fields ...
      pub deadline_hint_us: Option<u64>,  // Microseconds from now
  }
  ```
- Nodes check deadline proximity and adjust behavior:
  - If `deadline_hint_us.unwrap_or(u64::MAX) - now < 50ms`: Increase batching, reduce quality
  - Example: TTS node increases `max_batch_size` from 3 to 5, or reduces audio quality from 24kHz to 16kHz

**Alternatives Considered**:
- **Separate deadline channel**: Adds complexity, deadlines are per-data-item not global
- **Context propagation via tokio-tracing spans**: More overhead, not designed for real-time scheduling

**Implementation Notes**:
- Initial implementation (Phase 1) adds field but doesn't enforce policy
- Phase 3 (advanced runtime) adds policy framework based on node type and load

---

## 8. Testing Strategy for P99 Latency

**Question**: How do we reliably test P99 latency <250ms under load?

**Decision**: Use `criterion` benchmarks + custom load test harness with `hdrhistogram`

**Rationale**:
- **Unit tests**: Fast feedback, test individual components (ring buffer, merge strategies)
- **Integration tests**: Test control message propagation across contexts
- **Criterion benchmarks**: Measure per-component latency (P50/P95/P99) in isolation
- **Load test**: Custom harness that spawns 100 concurrent sessions, streams audio, measures end-to-end latency

**Load Test Implementation**:
```rust
// tests/load/test_latency_p99.rs
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_p99_latency_100_sessions() {
    let mut handles = vec![];
    let histogram = Arc::new(Mutex::new(Histogram::<u64>::new(3).unwrap()));

    for i in 0..100 {
        let hist = histogram.clone();
        handles.push(tokio::spawn(async move {
            // Stream audio, measure latency per chunk
            let start = Instant::now();
            pipeline.process(audio_chunk).await?;
            let latency_us = start.elapsed().as_micros() as u64;
            hist.lock().unwrap().record(latency_us)?;
        }));
    }

    // Wait for all sessions, assert P99 < 250ms
    let p99 = histogram.lock().unwrap().value_at_quantile(0.99);
    assert!(p99 < 250_000, "P99 latency {}μs exceeds 250ms", p99);
}
```

---

## Summary of Technical Decisions

| Decision Area | Choice | Key Dependency/Pattern |
|---------------|--------|------------------------|
| Ring Buffer | `crossbeam::ArrayQueue` | Lock-free MPMC queue |
| Histograms | `hdrhistogram` crate | HDR Histogram with rotating windows |
| Merge Strategies | Enum-based strategy pattern | ConcatenateText, ConcatenateAudio, KeepSeparate, Custom |
| Control Message Format | Extend RuntimeData binary format | Type byte = 5, JSON payload for flexibility |
| Streaming Resampler | Stateful wrapper around `FftFixedIn` | Internal buffering, emit on threshold/timeout |
| Backpressure | Configurable `OverflowPolicy` enum | DropOldest, DropNewest, Block, MergeOnOverflow |
| Deadline Hints | Optional field in RuntimeData | Non-breaking addition, advisory only |
| P99 Testing | Criterion + custom load harness | `hdrhistogram` for accurate percentiles |

**All NEEDS CLARIFICATION items resolved** - Ready to proceed to Phase 1 (Design & Contracts).
