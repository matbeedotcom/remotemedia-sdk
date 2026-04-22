# HAL Real-Time Assessment: Phase 0 + Phase 1 Verdict

**Subject**: Do Phases 0 and 1 make the remotemedia-sdk pipeline runner viable inside a Core Audio HAL AudioServer plugin IO callback?

**TL;DR**: Phase 0+1 get you most of the way to *soft* real-time, but a HAL IO callback needs *hard* real-time, and three gaps remain. The hearing-aid product should ship on the direct-inline path (call `SyncStreamingNode::process()` straight from `WriteMix`) and migrate to the executor-bridged path post-MVP, once Phase 2/3 lands.

---

## What Phase 0 + Phase 1 buys us

| Problem | Before | After Phase 1 |
|---|---|---|
| Unbounded channels → memory growth under load | All data-plane channels | All bounded + backpressure |
| Locks held across `.await` | VAD coordinator, chunker, text collector | Collect-then-fire pattern |
| Per-frame `tokio::sync::Mutex` | Yes | parking_lot where sync |
| Per-frame `RwLock<HashMap>` drift metrics | Yes | DashMap, wait-free |
| Per-frame `Vec` alloc in resampler | Yes | Pre-sized + scratch reuse |
| Hot-path `tracing::info!` noise | Yes | `trace!` (compiles out) |
| Observability for RT regressions | None | `LatencyProbe` + `RtProbeSet` |

These are correctness and tail-latency fixes. They're **necessary**. They are **not sufficient** for a HAL IO callback.

---

## What's still blocking HAL inline use

The three disqualifiers from the earlier research all still hold:

1. **Tokio is still in the IO path.** Even `parking_lot::Mutex` is fine, but `SessionRouter::process_input` is `async` and runs on a tokio worker. A HAL IO callback cannot be `async` and cannot call `block_on` (would deadlock the audio thread). Phase 1 made the async path non-blocking and bounded; it didn't make it synchronous.

2. **`bounded_channel + .await` is still `.await`.** `parks-the-producer` is a tokio scheduler point. If the HAL thread calls `send().await`, it yields back to tokio. If it calls `try_send` and the channel is full, we drop audio. Neither is acceptable inline.

3. **`RuntimeData::Audio { samples: Vec<f32> }` still clones.** Even with `AudioBufferPool`, sending audio into the pipeline from an RT thread means either `Vec::from(pool_buf)` (allocation) or moving ownership (pool buffer escapes the RT scope). Phase 1's deferral list explicitly calls out "thread `PooledAudioBuf` through `RuntimeData::Audio`".

---

## What's on the "Phase 2+" list that is actually *gating*

Three items on the remaining-work list are not optimization — they are the mechanism that unlocks HAL viability:

- **Pinned-thread data-plane executor + SPSC rings (rtrb) — Phase 3**. This is the handoff mechanism that lets a HAL callback talk to the pipeline without going through tokio's scheduler.
- **Threading `PooledAudioBuf` through `RuntimeData::Audio`**. Required for zero-alloc RT ↔ async handoff. Without it, every frame crossing the boundary is still a heap allocation.
- **RT-priority IPC threads (`thread_priority` + `core_affinity`, feature-gated `realtime`)**. Needed so the worker thread is not preempted by unrelated CPU load on neighboring cores. Without CPU isolation, tail-latency SLOs cannot hold on a busy host.

Without these three, the current architecture still requires the **worker-thread-with-SPSC-ring** pattern described in the previous research document. Phase 0+1 make that worker thread more reliable (bounded channels, lower lock contention, proper metrics) but do not collapse the RT ↔ async boundary.

---

## Verdict for the hearing-aid port

- **Direct-inline path (Phase 2 of the port): unchanged.** Call `SyncStreamingNode::process()` directly from `DoIOOperation`. Do not touch `PipelineExecutor`. Phase 0+1 of the SDK neither help nor hurt this path. This is the path we ship on.

- **Worker + SPSC path (post-MVP): meaningfully de-risked by Phase 1.** Bounded channels mean the worker won't silently drop frames under pressure. Locks no longer held across `.await` points mean no unbounded serialization. `LatencyProbe` means we can actually see regressions when they happen. The pattern was speculative before Phase 1; it is production-viable now.

- **Inline `PipelineExecutor` in HAL callback: still a no.** Needs Phase 3 (pinned-thread executor + SPSC) *and* the `RuntimeData::Audio` Arc/Pool variant. Neither has landed. If that path becomes a product requirement, those two items are the remaining critical work.

---

## Recommended SDK additions for HAL viability

In order of leverage:

### 1. `RuntimeData::Audio` sample storage enum

```rust
pub enum AudioSamples {
    Vec(Vec<f32>),
    Arc(Arc<[f32]>),
    Pooled(PooledAudioBuf),
}
```

Unlocks zero-copy and zero-allocation handoff from an RT thread. Current `Vec<f32>` forces a copy or a move-out-of-RT-scope.

### 2. Public synchronous dispatch helper

```rust
pub fn process_sync(
    node: &dyn SyncStreamingNode,
    data: RuntimeData,
) -> Result<RuntimeData, Error>;
```

Skips the async wrapper when the caller guarantees sync nodes only. Most hearing-aid DSP (WDRC, CROS, HRTF) qualifies. This is the lightest-weight legitimate public entry point into the node layer.

### 3. `remotemedia-rt-bridge` crate

The RT ↔ async pump itself — pinned worker thread, `rtrb` SPSC rings in/out, feature-gated `realtime` build that applies `thread_priority` and `core_affinity`. Ship this as the **documented** way for any RT audio host (HAL plugin, AU plugin, JACK client) to use the SDK, instead of forcing each embedder to rewrite the bridge.

Target embedder API: ~40 lines of glue to push audio into the bridge from an IO callback and pull processed results back. Same mental model as using cpal today, but with full pipeline semantics on the other side.

### 4. `// REAL-TIME UNSAFE` doc comments

On `SessionRouter::process_input` and `PipelineExecutor::send_input`, pointing to the bridge crate. Documents the boundary so future embedders don't re-discover it the hard way.

---

## Summary

| Path | Status after Phase 0+1 | Gates remaining |
|------|---|---|
| Direct inline (node `process()` in IO callback) | Viable today | None — this is the Phase 2 plan for the hearing aid |
| Worker + SPSC bridge | De-risked, production-viable for soft RT | Needs a canonical bridge crate to avoid embedder-side reimplementation |
| `PipelineExecutor` inline in IO callback | Still not viable | Needs Phase 3 SPSC executor + `AudioSamples` Arc/Pool variant |

Phase 0+1 deliver a large correctness and tail-latency win for any async-side consumer of the SDK. They are an enabling prerequisite for HAL integration, not the finish line. Phase 2/3, with the three additions above, are what close the remaining gap.
