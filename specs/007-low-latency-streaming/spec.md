# Feature Specification: Low-Latency Real-Time Streaming Pipeline

**Feature Branch**: `007-low-latency-streaming`
**Created**: 2025-11-10
**Status**: Draft
**Input**: User description: "Architectural improvements for low-latency real-time streaming with speculative VAD, auto-buffering, and control message propagation"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Real-Time Voice Interaction with Minimal Perceived Latency (Priority: P1)

As an end user of a real-time voice application (e.g., voice assistant, live translation), I want the system to respond to my speech with minimal delay so that the interaction feels natural and responsive, similar to human conversation.

**Why this priority**: This is the primary value proposition of the entire optimization effort. Without achieving sub-250ms P99 latency, all other improvements are meaningless. This directly impacts user satisfaction and application viability for real-time use cases.

**Independent Test**: Can be fully tested by streaming audio through a complete pipeline (VAD → ASR → processing → TTS → playback) and measuring end-to-end latency under various load conditions. Delivers immediate value by making the application usable for real-time scenarios.

**Acceptance Scenarios**:

1. **Given** a user speaks a short phrase (2-3 seconds), **When** the speech ends and silence is detected, **Then** the system begins processing within 50ms and provides initial response within 200ms (P99 < 250ms)
2. **Given** a continuous stream of speech with natural pauses, **When** the VAD detects speech boundaries, **Then** processing begins speculatively without waiting for full silence confirmation
3. **Given** 100 concurrent streaming sessions, **When** users are actively speaking, **Then** 99% of sessions experience end-to-end latency below 250ms

---

### User Story 2 - Efficient Batch Processing for Non-Parallelizable Operations (Priority: P2)

As a system operator running TTS or other compute-intensive synthesis operations, I want the pipeline to automatically batch multiple text inputs when the service is busy so that I can maximize throughput without sacrificing responsiveness for individual requests.

**Why this priority**: After achieving basic low latency, this optimization prevents queue buildup during high load and improves cost efficiency. It's a force multiplier that makes P1 sustainable under real-world load.

**Independent Test**: Can be tested independently by sending rapid bursts of text to TTS nodes and measuring: (a) batch sizes formed, (b) queue depths, (c) individual request latency vs. throughput. Delivers value by reducing infrastructure costs and improving scalability.

**Acceptance Scenarios**:

1. **Given** a TTS node is processing a request and 3 new text inputs arrive, **When** the current processing completes, **Then** the 3 inputs are merged into a single batch and processed together (if within 150ms window)
2. **Given** a single text input arrives with no queue, **When** no other inputs arrive within 75ms, **Then** the input is processed immediately without waiting for a batch
3. **Given** a node is configured as non-parallelizable, **When** the pipeline graph is initialized, **Then** the executor automatically wraps it with buffering logic

---

### User Story 3 - Graceful Handling of Speculative Processing Corrections (Priority: P2)

As a system managing speculative audio forwarding, I want to retroactively cancel or adjust processing when VAD decisions are refined so that downstream nodes don't waste resources on false-positive speech segments.

**Why this priority**: This prevents wasted compute on non-speech audio and improves accuracy. It's essential for the speculative forwarding strategy (P1) to be viable in production without causing quality degradation.

**Independent Test**: Can be tested by injecting audio with ambiguous speech boundaries and verifying: (a) speculative segments are forwarded immediately, (b) cancellation messages are generated when VAD refines decision, (c) downstream nodes honor cancellation. Delivers value by reducing false positives and wasted processing.

**Acceptance Scenarios**:

1. **Given** audio is speculatively forwarded as potential speech, **When** VAD later determines it was noise/silence, **Then** a cancellation control message is emitted with segment ID and timestamp range
2. **Given** a downstream node receives a cancellation message, **When** it's still processing the segment, **Then** it terminates processing and discards partial results
3. **Given** a cancellation message is emitted, **When** propagating through IPC or remote transport, **Then** the message arrives at all downstream nodes within 10ms

---

### User Story 4 - Streaming-Optimized Audio Resampling (Priority: P3)

As a pipeline processing variable-sized audio chunks in real-time, I want the resampler to accept arbitrary input sizes and emit output incrementally so that fixed chunk boundaries don't introduce artificial buffering delays.

**Why this priority**: This is an optimization on top of P1-P2 that further reduces latency. While beneficial, the system can achieve acceptable latency with smaller fixed chunks (Phase 1 quick wins), making this a refinement rather than a blocker.

**Independent Test**: Can be tested independently by sending variable-sized audio chunks (64-2048 samples) through the resampler and measuring: (a) output availability latency, (b) continuity/quality of resampled audio. Delivers incremental latency improvement (10-30ms reduction).

**Acceptance Scenarios**:

1. **Given** an audio chunk of 320 samples arrives, **When** resampling 48kHz→16kHz, **Then** output samples are available within 5ms without waiting for a full 512-sample buffer
2. **Given** consecutive variable-sized chunks, **When** resampling, **Then** output maintains continuity with no artifacts or discontinuities
3. **Given** a streaming resampler in use, **When** comparing to fixed-chunk resampler, **Then** P95 latency improves by at least 15ms

---

### User Story 5 - Observable Latency Metrics for Performance Tuning (Priority: P3)

As a developer or operator, I want detailed per-node latency metrics (P50/P95/P99) and queue depth visibility so that I can identify bottlenecks and tune performance for my specific use case.

**Why this priority**: This is essential for ongoing optimization but doesn't directly deliver user-facing latency improvements. It enables continuous improvement and debugging but isn't required for initial deployment.

**Independent Test**: Can be tested by running a pipeline under load and querying metrics endpoints/logs to verify: (a) all nodes report timing histograms, (b) queue depths are tracked, (c) speculation acceptance rates are recorded. Delivers operational visibility without changing core behavior.

**Acceptance Scenarios**:

1. **Given** a pipeline is processing audio, **When** querying metrics, **Then** each node reports P50, P95, P99 latency over 1-minute, 5-minute, and 15-minute windows
2. **Given** a buffering wrapper is active, **When** metrics are collected, **Then** they include: queue depth, batch sizes formed, merge strategy applied
3. **Given** speculative VAD is running, **When** metrics are collected, **Then** they include: speculation rate, cancellation rate, false positive percentage

---

### Edge Cases

- **Concurrent cancellation and output**: What happens when a cancellation message arrives while a node is emitting the final output for that segment? System must handle race conditions gracefully (discard output if cancellation wins, or mark output as "potentially cancelled" if output wins).

- **Batch timeout vs. max wait**: How does the system balance `min_batch_size` and `max_wait_ms` when inputs trickle in slowly? Must process immediately when `max_wait_ms` expires, even if `min_batch_size` not reached.

- **Cascading cancellations**: If a speculative segment is cancelled, how does the system ensure all downstream nodes (including remote nodes via gRPC/WebRTC) receive the cancellation? Must implement reliable cancellation propagation with delivery confirmation.

- **Ring buffer overflow**: What happens when the VAD ring buffer (100-200ms lookback) overflows during extremely long utterances? System must either: (a) expand buffer dynamically, or (b) commit oldest speculative segments as confirmed.

- **Non-serializable control messages over IPC**: How are control messages (which may contain complex metadata) serialized for iceoryx2 IPC? Must define a stable binary format that Python can deserialize.

- **Backpressure propagation**: When a downstream node's bounded queue is full, how does backpressure propagate to upstream nodes? Must implement flow control that doesn't deadlock or drop data silently.

- **Quality degradation under deadline pressure**: When soft deadlines are approaching, how does the system decide which quality knobs to adjust (batch more, reduce resampling quality, skip optional nodes)? Must have a policy framework for quality vs. latency tradeoffs.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST forward audio chunks speculatively through a VAD gate node, emitting outputs immediately without waiting for final VAD decision

- **FR-002**: System MUST maintain a per-session ring buffer (100-200ms lookback, 25-50ms lookahead) to support retroactive VAD decision refinement

- **FR-003**: System MUST emit standardized control messages (type: cancel_speculation, segment_id, from_timestamp, to_timestamp) when VAD retroactively determines a speculative segment was not speech

- **FR-004**: System MUST propagate control messages through all execution contexts: local Rust nodes, multiprocess Python IPC (iceoryx2), and remote transports (gRPC, WebRTC, HTTP)

- **FR-005**: System MUST automatically detect non-parallelizable nodes (TTS, certain synthesis operations) and wrap them with buffering logic at executor initialization

- **FR-006**: System MUST implement a BufferedProcessor wrapper that: (a) queues inputs when inner node is busy, (b) merges inputs based on configurable strategy (ConcatenateText, ConcatenateAudio, KeepSeparate, Custom), (c) applies configurable policies (min_batch_size, max_wait_ms, max_buffer_size)

- **FR-007**: System MUST maintain a NodeCapabilities registry that tracks per-node metadata: parallelizable (bool), batch_aware (bool), avg_processing_ms (measured heuristic)

- **FR-008**: System MUST implement per-node bounded input queues with configurable overflow policies: drop-oldest, drop-newest, merge-on-overflow, block (backpressure)

- **FR-009**: System MUST support streaming-capable resampling that: (a) accepts arbitrary input chunk sizes, (b) maintains internal state for continuity, (c) emits output incrementally as available

- **FR-010**: System MUST expose per-node latency metrics: P50, P95, P99 histograms over 1min/5min/15min windows, queue depth, batch sizes, speculation acceptance rate

- **FR-011**: System MUST allow TextCollector to optionally batch outputs over a configurable window (e.g., 100-150ms) before emitting to downstream TTS nodes

- **FR-012**: System MUST propagate soft deadline hints per processing stage, allowing nodes to adaptively adjust quality/batching behavior when approaching deadlines

- **FR-013**: Python multiprocess nodes MUST honor cancellation control messages by terminating in-progress processing and discarding partial results for the specified segment

- **FR-014**: System MUST provide manifest-level configuration for: VAD thresholds (min_speech_ms, min_silence_ms, pad_ms), chunk sizes (input, VAD, resample), resampling quality level, batching policies per node

### Key Entities

- **SpeculativeSegment**: Represents an audio segment forwarded before final VAD decision. Attributes: segment_id (UUID), start_timestamp, end_timestamp, status (speculative|confirmed|cancelled), audio_data_ref (ring buffer slice)

- **ControlMessage**: Standardized message for pipeline control flow. Attributes: message_type (cancel_speculation|batch_hint|deadline_warning), target_segment_id, metadata (JSON payload for extensibility)

- **NodeCapabilities**: Metadata describing a node's execution characteristics. Attributes: node_type (string), parallelizable (bool), batch_aware (bool), avg_processing_ms (float, updated via exponential moving average)

- **BufferingPolicy**: Configuration for auto-buffering wrapper. Attributes: min_batch_size (int, default 2-5), max_wait_ms (int, default 75-150), max_buffer_size (int, memory limit), merge_strategy (enum)

- **LatencyMetrics**: Per-node performance metrics. Attributes: node_id, p50_ms, p95_ms, p99_ms, queue_depth_current, queue_depth_max, batch_size_avg, speculation_acceptance_rate

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: End-to-end latency (speech end to response start) MUST be below 250ms for P99 under load of 100 concurrent sessions

- **SC-002**: VAD speculative forwarding MUST reduce perceived latency by at least 50ms compared to non-speculative baseline (measured via A/B test)

- **SC-003**: Auto-buffering on TTS nodes MUST increase throughput by at least 30% under high load (>50 requests/sec) while maintaining per-request P95 latency under 300ms

- **SC-004**: Control message propagation (local, IPC, remote) MUST complete within 10ms from emission to delivery at all downstream nodes (P95)

- **SC-005**: Streaming resampler (when enabled) MUST reduce resampling latency by at least 15ms at P95 compared to fixed-chunk baseline

- **SC-006**: False positive speculation rate (segments speculatively forwarded but later cancelled) MUST be below 5% of total segments processed

- **SC-007**: System MUST maintain stable performance (latency, throughput) under sustained load for at least 1 hour with no memory leaks or degradation

- **SC-008**: Per-node metrics MUST be queryable with less than 5ms overhead per query, and metric collection MUST add less than 1% overhead to node processing time

- **SC-009**: Operator can tune VAD thresholds and batching policies via manifest changes without code modifications, achieving target latency profile within 3 iterations
