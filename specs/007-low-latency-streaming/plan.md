# Implementation Plan: Low-Latency Real-Time Streaming Pipeline

**Branch**: `007-low-latency-streaming` | **Date**: 2025-11-10 | **Spec**: [spec.md](spec.md)

## Summary

This feature implements comprehensive low-latency optimizations for real-time audio streaming pipelines, targeting sub-250ms P99 end-to-end latency at 100 concurrent sessions. Core approach includes: (1) speculative VAD forwarding with retroactive cancellation via control messages, (2) automatic batching wrapper for non-parallelizable nodes like TTS, (3) executor-level node capability detection and queue management, (4) streaming-capable resampling, and (5) comprehensive per-node latency metrics (P50/P95/P99). The system extends existing runtime-core architecture with new node types (SpeculativeVADGate, BufferedProcessor), executor enhancements (NodeCapabilities registry, bounded queues with policies), and control message propagation across all execution contexts (local Rust, multiprocess Python IPC via iceoryx2, remote transports via gRPC/WebRTC/HTTP).

## Technical Context

**Language/Version**: Rust 1.87 (workspace edition 2021)
**Primary Dependencies**:
- Async runtime: tokio 1.35 (rt-multi-thread, sync, time)
- Audio processing: rubato 0.15 (resampling), ort 2.0 (Silero VAD), rustfft 6.2
- IPC: iceoryx2 0.7.0 (zero-copy multiprocess communication)
- Metrics: prometheus 0.13, tracing 0.1
- Serialization: serde 1.0, bincode 1.3
- Python FFI: pyo3 0.26 (for Python node integration)
- Transport: tonic 0.14 (gRPC), webrtc 0.14.0, axum 0.7 (HTTP/SSE)

**Storage**: In-memory ring buffers (lock-free if possible, Mutex/RwLock fallback), no persistent storage
**Testing**: cargo test (unit), cargo test --test (integration), criterion 0.5 (benchmarks)
**Target Platform**: Linux server (primary), Windows (development), potential WASM (future)
**Project Type**: Rust workspace with runtime-core crate (single project structure in runtime-core/)
**Performance Goals**: P99 latency <250ms end-to-end, 100 concurrent sessions, 30% throughput increase on batching, <1% metrics overhead
**Constraints**:
- Zero-copy audio transfer where possible (via iceoryx2 for IPC)
- <10ms control message propagation (P95)
- <5% false positive speculation rate
- Memory stable under 1-hour load (no leaks)
**Scale/Scope**:
- 5 new Rust modules (~2000 LOC)
- 14 functional requirements
- 4 execution contexts (local Rust, multiprocess Python, gRPC, WebRTC/HTTP)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Note**: No project constitution file found at `.specify/memory/constitution.md` (file contains only template placeholders). Proceeding with standard Rust best practices:

- ✅ **Library-First**: All new components (SpeculativeVADGate, BufferedProcessor, etc.) will be implemented as reusable modules within runtime-core
- ✅ **Test-First**: Unit tests for all new data structures, integration tests for control message propagation across all contexts
- ✅ **Zero-Copy**: Maintain existing zero-copy audio transfer via iceoryx2, ensure control messages don't break this
- ✅ **Observable**: Add comprehensive metrics (P50/P95/P99 histograms, queue depths, speculation rates) via prometheus
- ✅ **Backward Compatible**: New features are opt-in via manifest configuration, existing pipelines continue to work unchanged

**No violations to justify** - This feature extends the existing architecture without breaking changes.

## Project Structure

### Documentation (this feature)

```text
specs/007-low-latency-streaming/
├── plan.md              # This file (/speckit.plan output)
├── research.md          # Phase 0: Ring buffer implementations, histogram libraries, merge strategies
├── data-model.md        # Phase 1: SpeculativeSegment, ControlMessage, NodeCapabilities, BufferingPolicy, LatencyMetrics
├── quickstart.md        # Phase 1: Minimal example pipeline with speculative VAD and auto-batching
├── contracts/           # Phase 1: Control message serialization format, IPC wire protocol extensions
└── tasks.md             # Phase 2: NOT created by /speckit.plan (created by /speckit.tasks)
```

### Source Code (repository root)

```text
runtime-core/src/
├── nodes/
│   ├── speculative_vad_gate.rs       # NEW: Speculative forwarding with ring buffer
│   ├── buffered_processor.rs         # NEW: Auto-batching wrapper with merge strategies
│   ├── audio/
│   │   └── resample_streaming.rs     # MODIFY: Streaming-capable resampling (accept arbitrary chunk sizes)
│   ├── text_collector.rs             # MODIFY: Optional batch window configuration
│   ├── remote_pipeline.rs            # MODIFY: Forward control messages to remote transports
│   ├── streaming_node.rs             # MODIFY: Add process_control_message() method
│   └── streaming_registry.rs         # MODIFY: Register new nodes, auto-wrap non-parallelizable
│
├── executor/
│   ├── node_capabilities.rs          # NEW: Capability registry (parallelizable, batch_aware, avg_ms)
│   ├── mod.rs                         # MODIFY: Per-node bounded queues, auto-wrap detection, deadline hints
│   ├── scheduler.rs                   # MODIFY: Integrate with NodeCapabilities for scheduling
│   └── metrics.rs                     # MODIFY: Add histogram support (P50/P95/P99), queue depth tracking
│
├── data/
│   ├── control_message.rs            # NEW: Standardized control message types (cancel_speculation, etc.)
│   ├── runtime_data.rs               # MODIFY: Extend with control message variant (or separate channel?)
│   └── ring_buffer.rs                # NEW: Lock-free ring buffer for speculative segments
│
├── python/multiprocess/
│   ├── data_transfer.rs              # MODIFY: Serialize/deserialize control messages for iceoryx2
│   └── multiprocess_executor.rs      # MODIFY: Forward control messages to Python via IPC
│
└── transports/
    ├── remotemedia-grpc/             # MODIFY: Propagate control messages via gRPC streaming
    ├── remotemedia-webrtc/           # MODIFY: Propagate control messages via data channel
    └── remotemedia-http/             # MODIFY: Propagate control messages via SSE

tests/
├── integration/
│   ├── test_speculative_vad.rs       # NEW: Test speculative forwarding + cancellation
│   ├── test_buffered_processor.rs    # NEW: Test auto-batching under various loads
│   ├── test_control_messages.rs      # NEW: Test propagation across all contexts
│   └── test_streaming_resampler.rs   # NEW: Test variable-sized chunk handling
│
├── unit/
│   ├── test_ring_buffer.rs           # NEW: Test ring buffer correctness and concurrency
│   ├── test_node_capabilities.rs     # NEW: Test capability detection and registration
│   └── test_merge_strategies.rs      # NEW: Test text/audio concatenation strategies
│
└── benchmarks/
    ├── bench_latency.rs              # NEW: End-to-end latency measurement (P50/P95/P99)
    └── bench_buffering.rs            # NEW: Throughput with/without batching

python-client/remotemedia/
└── nodes/
    └── node.py                       # MODIFY: Honor cancellation control messages, terminate processing
```

**Structure Decision**: Extends existing runtime-core single-project structure. All new components are modules within runtime-core/src/. Transport modifications live in their respective workspace crates (remotemedia-grpc, remotemedia-webrtc, remotemedia-http). Python client changes are minimal (add control message handling to node.py).

## Complexity Tracking

No violations to justify. This feature adds new capabilities without breaking existing architecture or adding unnecessary abstraction layers.

---

## Phase 0: Research (COMPLETED)

✅ All research questions resolved in [research.md](research.md)

**Key Decisions**:
1. Lock-free ring buffer: `crossbeam::ArrayQueue`
2. Histogram library: `hdrhistogram` crate
3. Merge strategies: Enum-based pattern (ConcatenateText, ConcatenateAudio, KeepSeparate, Custom)
4. Control message format: Extend RuntimeData binary format with JSON payload
5. Streaming resampler: Stateful wrapper around `FftFixedIn`
6. Backpressure: Configurable `OverflowPolicy` enum
7. Deadline hints: Optional field in RuntimeData
8. P99 testing: Criterion + custom load harness

**New Dependencies**:
- `crossbeam = "0.8"` (lock-free data structures)
- `hdrhistogram = "7.5"` (P50/P95/P99 metrics)

---

## Phase 1: Design & Contracts (COMPLETED)

✅ Data model defined in [data-model.md](data-model.md)
✅ Contracts specified in [contracts/control_message_format.md](contracts/control_message_format.md)
✅ Quickstart guide created in [quickstart.md](quickstart.md)
✅ Agent context updated (CLAUDE.md)

**Artifacts Created**:
- **5 core entities**: SpeculativeSegment, ControlMessage, NodeCapabilities, BufferingPolicy, LatencyMetrics
- **Control message wire format**: Binary (IPC), Protobuf (gRPC), JSON (WebRTC/HTTP)
- **3 example pipelines**: Speculative VAD, Auto-batching TTS, Combined optimized
- **Validation rules**: All entities have clear constraints and state transitions

---

## Phase 2: Tasks Generation (NOT YET STARTED)

⏸️ **Command to proceed**: `/speckit.tasks`

This will generate [tasks.md](tasks.md) with:
- Dependency-ordered implementation tasks
- Test coverage requirements
- Acceptance criteria per task
- Estimated effort per task

---

## Constitution Re-Check (Post-Design)

Re-evaluating gates after Phase 1 design:

- ✅ **Library-First**: All components are reusable modules in runtime-core (no duplication)
- ✅ **Test-First**: Test strategy defined (unit, integration, benchmarks)
- ✅ **Zero-Copy**: Control messages use separate channel (no impact on audio zero-copy)
- ✅ **Observable**: Comprehensive metrics via hdrhistogram + prometheus
- ✅ **Backward Compatible**: All new features are opt-in via manifest config

**No new violations introduced** - Ready for implementation.

---

## Implementation Readiness Checklist

- [x] Specification complete and validated ([spec.md](spec.md))
- [x] Technical research complete ([research.md](research.md))
- [x] Data model defined ([data-model.md](data-model.md))
- [x] Contracts specified ([contracts/](contracts/))
- [x] Quickstart guide written ([quickstart.md](quickstart.md))
- [x] Agent context updated (CLAUDE.md)
- [x] Constitution gates passed
- [ ] Tasks generated (run `/speckit.tasks`)
- [ ] Implementation started

**Status**: ✅ Ready for task generation (Phase 2)

---

## Next Steps

1. Run `/speckit.tasks` to generate implementation tasks
2. Review tasks.md for dependency order and effort estimates
3. Begin implementation following TDD approach (tests first)
4. Track progress via task checklist
5. Run integration tests and benchmarks to validate performance targets

---

## Success Criteria Validation Plan

| Criterion | How to Measure | Target | Test Location |
|-----------|---------------|--------|---------------|
| SC-001: P99 latency <250ms | Load test with 100 concurrent sessions | <250ms | `tests/load/test_latency_p99.rs` |
| SC-002: 50ms latency reduction | A/B test speculative vs. non-speculative | >50ms | `tests/benchmarks/bench_latency.rs` |
| SC-003: 30% throughput increase | TTS load test with/without batching | >30% | `tests/benchmarks/bench_buffering.rs` |
| SC-004: <10ms control propagation | Integration test across all contexts | <10ms P95 | `tests/integration/test_control_messages.rs` |
| SC-005: 15ms resampling reduction | Benchmark streaming vs. fixed-chunk | >15ms P95 | `tests/integration/test_streaming_resampler.rs` |
| SC-006: <5% false positive rate | VAD speculation accuracy test | <5% | `tests/integration/test_speculative_vad.rs` |
| SC-007: Stable 1-hour load | Memory profiling, leak detection | No leaks | `tests/load/test_stability.rs` |
| SC-008: <1% metrics overhead | Benchmark with/without metrics | <1% | `tests/benchmarks/bench_metrics_overhead.rs` |
| SC-009: Tunable via manifest | Configuration test (no code changes) | 3 iterations | Manual validation |

---

## Risk Mitigation

| Risk | Mitigation | Status |
|------|-----------|--------|
| Ring buffer overflow in long utterances | Dynamic expansion or commit oldest segments | ✅ Designed (clear_before) |
| Control message lost in remote transport | Reliable channels (gRPC stream, WebRTC reliable) | ✅ Specified in contracts |
| Race condition: cancellation vs. output | Timestamp-based ordering + idempotency | ✅ Handled in data model |
| Over-batching increases latency | `max_wait_ms` timeout enforced | ✅ Implemented in BufferingPolicy |
| Lock contention in metrics collection | Lock-free atomics + hdrhistogram | ✅ Chosen in research |
| Python deserialization overhead | Simple binary format + JSON | ✅ <10μs overhead estimated |

All risks have documented mitigation strategies.
