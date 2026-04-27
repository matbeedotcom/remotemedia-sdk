# DAG-Native Provenance Validation Test Suite

**Date:** 2026-04-27
**Status:** Proposed (validation plan, no production code yet)
**Scope:** Decide whether to build a DAG-native per-edge / per-path performance instrumentation system on top of the existing `PerfAggregator`.

---

## Context

We've shipped slice 1 of a `PerfAggregator`-based per-node performance HUD: dispatch-site latency histograms emitted on a `__perf__` tap channel, with a sticky frontend table. It works for "node X in→out p50/p99" but **cannot answer "where did the time go between nodes"** — i.e. per-edge transit cost, ingest-to-egress total, or how the LLM↔TTS chain broke down on a specific turn.

The proposed evolution: a **DAG-native provenance trace** stamped at ingest into `RuntimeData.metadata`, appended at every dispatch, and read back by `PerfAggregator` to produce per-edge and per-path histograms keyed off the manifest's `connections`. Coordinators (`ConversationCoordinator`, `SpeculativeVADCoordinator`) become *just nodes*; turn semantics fall out of path latency, not the perf system.

**Why a test suite first.** Audit results show real risk: only `RuntimeData::Audio` and `ControlMessage` carry a `metadata: Option<Value>` field. `Text`, `Json`, `Binary`, `Video`, `Tensor`, `Numpy`, `File` cannot natively carry provenance — the LLM token stream (Text), VAD events (Json), and any Python-stringified payload would silently lose the trace. Beyond that, ~11 hot-path Audio nodes clobber `metadata: None` on output even though they could forward it. We need **executable evidence** that the design works for its target cases before paying the implementation cost.

This plan is a **validation test suite**, not the feature itself. We write tests that prove or disprove the mechanism at five hard-gate phases. Outcome is a binary go / caveated-go / no-go decision plus a numbered burn-down list of follow-ups.

User-selected scope (2026-04-27):
- **Hard-gates only**: Phases A, B, E, G(1, 5), H. Defer C (clobbering audit), D (synth fallback), F (Text/Json correlator), I (overhead bench) until hard gates pass.
- **Test layout**: `crates/core/tests/perf_provenance/` directory, one file per phase.
- **Patching debt**: Phase D before Phase C — test the runtime rescue first; only fall back to patching individual nodes if D fails.

Phase F's Text/Json fate is parked but explicitly called out as the gating question for any LLM→TTS edge measurement.

---

## Mechanism under test

```rust
// Stamped at ingest into RuntimeData::Audio.metadata or
// ControlMessage.metadata. Hop entry appended on every dispatch.
{
  "__provenance": {
    "trace_id":     "<uuid-v7>",
    "birth_ts_us":  <wall-clock at first ingest>,
    "hops": [
      { "node": "audio_input", "in_ts_us": ..., "out_ts_us": ..., "seq": 0 },
      { "node": "vad",         "in_ts_us": ..., "out_ts_us": ..., "seq": 0 },
      ...
    ]
  }
}
```

- **Stamp site**: `SessionRouter::route_input` at `crates/core/src/transport/session_router.rs:1118` — already mutates `arrival_ts_us`; symmetric stamping site.
- **Hop append site**: the per-input output callback `cb` built at `session_router.rs:991–1015` — already records latency to `PerfAggregator`; same closure mutates outgoing `RuntimeData.metadata`.
- **Read site**: in the same closure, used to derive `(prev_node → main_node_id)` edge latency.

The validation tests treat the mechanism as the system-under-test. They do **not** require shipping the production module — helpers live inside the test crate during validation.

---

## Test layout

```
crates/core/tests/perf_provenance/
  mod.rs                                 # shared helpers
  phase_a_single_hop.rs                  # hard gate
  phase_b_audio_chain.rs                 # hard gate
  phase_e_derivation.rs                  # hard gate
  phase_g_python_ipc.rs                  # hard gate (1, 5 only — pure-Rust round-trip)
  phase_h_aggregator_integration.rs      # hard gate
```

Add `mod perf_provenance;` to a top-level integration entry following the pattern at `crates/core/tests/integration/test_speculative_vad.rs`.

### Shared helpers (`mod.rs`)

The validation module exposes:

- `provenance::stamp(data: &mut RuntimeData) -> Result<(), NoMetadataCarrier>` — initializes `__provenance` on Audio / ControlMessage; returns `Err` for non-carrier variants.
- `provenance::append_hop(data: &mut RuntimeData, node: &str, in_ts_us: u64, out_ts_us: u64, seq: u32)`.
- `provenance::read(data: &RuntimeData) -> Option<ProvenanceHeader>`.
- `MockForwardingAudioNode` — recording mock that copies `metadata` verbatim and optionally calls `append_hop`. Modeled on `MockSpeculativeVADGate` at `crates/core/tests/integration/test_speculative_vad.rs:17–131`.
- `build_chain_manifest(node_ids: &[&str]) -> Manifest` — adapts the existing `create_test_manifest` helper at `session_router.rs:1452`.

These helpers are **not** copied into production code at this stage. If hard gates pass, they become the v1 of `crates/core/src/transport/provenance.rs`.

---

## Phase A — Provenance survives single-hop dispatch

**Goal.** Prove the carrier mechanism works for the variants that have a `metadata` slot. If this fails, the design is dead — needs a `RuntimeData` wrapper rewrite.

**File.** `crates/core/tests/perf_provenance/phase_a_single_hop.rs`

**Tests.**
1. `provenance_stamp_audio_at_ingest` — call `stamp` on `RuntimeData::Audio { metadata: None, .. }`. Assert `metadata == Some(Object { __provenance: { trace_id, birth_ts_us, hops: [] } })`.
2. `provenance_appends_hop_on_audio_dispatch` — `MockForwardingAudioNode` round-trip; assert `read(out).hops.len() == 1`, hop name and timestamps.
3. `provenance_stamp_control_message_at_ingest` — same as (1) for `RuntimeData::ControlMessage`. Helper must promote `Value::Null` to `Object` to host the key.
4. `provenance_appends_hop_on_control_dispatch` — same as (2) for ControlMessage.
5. `provenance_text_variant_returns_no_carrier` — assert `stamp(text)` returns `Err(NoMetadataCarrier)`.
6. `provenance_dark_variants_table` — table-driven over `Json`, `Binary`, `Video`, `Tensor`, `Numpy`, `File`. Same assertion as (5).
7. `birth_ts_and_arrival_ts_are_independent_fields` — stamp on a packet that already has `arrival_ts_us`; assert `birth_ts_us == arrival_ts_us` at ingest moment but they diverge at any downstream node.

**Pass criterion.** All 7 green. Failure on (5)/(6) is *expected* — that's the documented limitation, not a regression.

**Cost.** ~180 LOC including the helpers in `mod.rs` (helpers reused by every later phase, so this is amortized).

---

## Phase B — Provenance survives a full Audio-only chain

**Goal.** Prove transitive end-to-end carry through a real `SessionRouter` over a real manifest. If A passes but B fails, the bug is in the dispatch closure's mutation timing — fix before continuing.

**File.** `crates/core/tests/perf_provenance/phase_b_audio_chain.rs`

**Tests.**
1. `provenance_traverses_5_node_audio_chain_via_real_router` — manifest `A → B → C → D → E` of `MockForwardingAudioNode`. Inject one stamped Audio packet, drain `output_rx`, assert `read(received).hops.len() == 5` with names in order.
2. `provenance_birth_ts_preserved_across_chain` — `birth_ts_us` at sink equals value stamped at ingest.
3. `provenance_trace_id_stable_across_chain` — `trace_id` identical at every hop.
4. `provenance_in_ts_monotone_nondecreasing` — `hops[i].in_ts_us <= hops[i+1].in_ts_us` and `hops[i].out_ts_us <= hops[i+1].in_ts_us`.
5. `provenance_forks_cleanly_on_fan_out` — manifest `A → B, A → C`. Assert sinks B and C each see `hops == [A_hop, B_hop_or_C_hop]` with no cross-contamination. Validates the dispatch site appends the hop *before* the fan-out clone in `session_router.rs:935–942`.
6. `provenance_fan_in_keeps_per_input_lineage` — manifest `A → C, B → C`. Two stamped inputs (different `trace_id`s) into A and B. Assert C's outputs preserve each input's own `trace_id` chain — fan-in does not merge by default. (Future merge policy is a Phase C consideration.)
7. `n_outputs_per_input_each_get_independent_provenance` — node that emits 3 outputs from 1 input. Assert each output carries an independent provenance with the same `trace_id` and `birth_ts_us` but distinct `seq` values (0, 1, 2).

**Pass criterion.** All 7 green. (1)–(4) are baseline; (5)–(7) prove the design handles the DAG topologies we actually use.

**Cost.** ~250 LOC.

**Critical files referenced.**
- `crates/core/src/transport/session_router.rs:935–942` (fan-out clone site).
- `crates/core/src/transport/session_router.rs:991–1015` (output callback `cb`).
- `crates/core/src/transport/session_router.rs:1452` (`create_test_manifest` helper).

---

## Phase E — Edge / path latency derivation (pure data)

**Goal.** Prove the consumer side: given a chain of hops, the deriver computes the histograms `PerfAggregator` will publish. Pure-data, no router, no async.

**File.** `crates/core/tests/perf_provenance/phase_e_derivation.rs`

**Tests.**
1. `edge_latency_computed_from_consecutive_hops` — synthetic provenance `[(A,in=0,out=10), (B,in=12,out=30), (C,in=32,out=40)]`. Assert two interpretations are accessible: transit (`B.in − A.out = 2`) and elapsed-incl-processing (`B.out − A.out = 20`). Pick one as default, assert default is documented.
2. `path_latency_equals_last_out_minus_birth_ts` — `paths["A→C"].latency_us == hops.last().out_ts_us − birth_ts_us`.
3. `derivation_handles_single_hop_no_edges` — `hops.len() == 1` ⇒ `edges.is_empty()`.
4. `derivation_handles_zero_hops_empty_maps` — empty pipeline returns empty maps, no panic.
5. `derivation_per_edge_histogram_aggregates_multiple_traces` — feed 100 synthetic provenances; assert `edges["A→B"].p50_us` matches the median of the input distribution within HDR precision (3 sig figs).
6. `derivation_skips_or_marks_synth_boundaries` — provenance with a `synth: true` marker on a hop. Pick a policy (skip or mark `dark: true`); assert.

**Pass criterion.** All 6 green. Failures are pure-logic and cheap to fix; failure here just means the algorithm needs another iteration.

**Cost.** ~200 LOC.

---

## Phase G(1, 5) — Audio + ControlMessage round-trip through IPC serializer

**Goal.** Confirm `data_transfer.rs` actually round-trips a JSON object containing `__provenance` byte-exact through the serialization layer. The audit asserted Audio's metadata field exists; this test verifies the wire format doesn't truncate or reject the provenance payload.

This phase intentionally **excludes** the real Python multiprocess test (originally G #2) and the buffer-overflow stress (G #3) — those are deferred until hard gates pass.

**File.** `crates/core/tests/perf_provenance/phase_g_python_ipc.rs`

**Tests.**
1. `audio_provenance_roundtrips_through_data_transfer_serialization` — pure-Rust:

   ```rust
   let prov = synthetic_provenance_with_5_hops();
   let payload = IpcPayload::audio(&samples, 16000, 1, "session", Some(&prov));
   let decoded = payload.audio()?;
   assert_eq!(decoded.metadata, Some(prov));
   ```
2. `control_message_provenance_roundtrips` — `IpcPayload::control_message(...)` includes a metadata `Value` slot; confirm `__provenance` survives.

**Pass criterion.** Both green. Failure means the audit lied about the metadata field surviving IPC and we need to either extend the wire format or document multiprocess Python nodes as a permanent provenance dark zone for Audio.

**Cost.** ~120 LOC.

**Critical files referenced.**
- `crates/core/src/python/multiprocess/data_transfer.rs:33` (`IpcPayload::audio` metadata serialization).
- `crates/core/src/python/multiprocess/data_transfer.rs:203` (`IpcPayload::control_message`).
- `crates/core/src/python/multiprocess/data_transfer.rs:72` (text serialization, no metadata slot — referenced for documentation only, not under test in this phase).

---

## Phase H — PerfAggregator integration

**Goal.** Wire the provenance reader into a `PerfAggregator`-shaped API and prove per-edge / per-path percentile fields populate correctly. Pure aggregator, synthetic input.

**File.** `crates/core/tests/perf_provenance/phase_h_aggregator_integration.rs`

**Tests.**
1. `aggregator_records_edge_latency_from_provenance_hops` — add `record_provenance(prov: &ProvenanceHeader)` to a test wrapper around `PerfAggregator`. Feed 100 synthetic A→B→C provenances. Assert snapshot has `edges["A→B"]`, `edges["B→C"]` with correct p50/p95/p99/max.
2. `aggregator_records_path_latency_from_provenance` — same input; assert `paths["A→C"]` populated.
3. `aggregator_per_node_stats_match_existing_record_input_record_output` — provenance-driven node stats equal the stats produced by direct `record_input`/`record_output` calls fed equivalent latencies. Cross-checks observation equivalence with the existing aggregator API at `crates/core/src/transport/perf_aggregator.rs:127`.
4. `aggregator_handles_partial_provenance_gracefully` — feed a malformed provenance (missing `out_ts_us` on a hop); assert no panic and a `provenance_malformed_total` counter increments.
5. `aggregator_window_resets_edges_and_paths` — call `flush_snapshot` twice; second snapshot has empty edge/path stats. Mirrors `enabled_aggregator_records_and_resets` at `perf_aggregator.rs:282`.

**Pass criterion.** All 5 green. Failure means the existing aggregator's window/reset semantics don't compose with edge/path stats — needs a redesign of the snapshot schema in `crates/core/src/data/perf.rs`.

**Cost.** ~300 LOC, mostly setup. The `record_provenance` helper is ~50 LOC.

**Critical files referenced.**
- `crates/core/src/transport/perf_aggregator.rs:127` (`PerfAggregator` API).
- `crates/core/src/transport/perf_aggregator.rs:282` (`enabled_aggregator_records_and_resets` test rig pattern).
- `crates/core/src/data/perf.rs` (snapshot schema — extension target).

---

## Verification (end-to-end)

Run the suite locally once the phases are implemented:

```bash
# Hard-gate phases as a single test target
cargo test -p remotemedia-core --test perf_provenance \
    --no-fail-fast 2>&1 | tee /tmp/perf_provenance.log

# Or run individual phase modules
cargo test -p remotemedia-core --test perf_provenance phase_a
cargo test -p remotemedia-core --test perf_provenance phase_b
cargo test -p remotemedia-core --test perf_provenance phase_e
cargo test -p remotemedia-core --test perf_provenance phase_g
cargo test -p remotemedia-core --test perf_provenance phase_h
```

Expected log shape on a passing run: 30+ test cases green across 5 files, no `ignored`, no panics. Phase G(2) is intentionally absent (real Python process test deferred). Phase A tests 5–6 confirm Text/Json/Binary/etc. dark zones — they do *not* fail the build; they document the limitation.

---

## GO / NO-GO rubric for hard gates

| Phase | Hard gate? | Effect of failure |
|---|---|---|
| A   (1–4, 7) | **YES** | Carrier mechanism doesn't work end-to-end → redesign as `RuntimeData` wrapper struct. Design dead. |
| A   (5–6)    | NO       | Documentation; failure here means a `metadata` field appeared on a variant we thought lacked one (good news). |
| B   (all)    | **YES** | Carry-through is buggy → fix before continuing. Implementation-level, not design-level. |
| E   (all)    | **YES** | Deriver is broken → unusable data. Cheap to fix, but blocks Phase H. |
| G   (1, 5)   | **YES** | Multiprocess Audio loses provenance at IPC boundary → extend wire format or accept multiprocess dark zone. |
| H   (all)    | **YES** | No aggregator integration → feature has no consumer. |

**Full GO** = all hard-gate tests green. Greenlight Phase D (synth fallback) next; that decides whether we patch 11 nodes or absorb the debt in the runtime.

**Caveated GO** = A, B, E, H green; G(1) red. Ship as single-process-only; document multiprocess Audio as a dark zone until wire format extends.

**NO-GO** = A red OR B red OR E red OR H red. Mechanism doesn't work for its target case.

---

## Out of scope for this validation pass

These tests exist in the master plan but are deferred per the user's "hard-gates only" choice:

- **Phase C** — clobbering-node audit (11 red-by-design tests tracking patching debt).
- **Phase D** — runtime synth-fallback test (decides whether the 11 nodes need patching at all). **Run immediately after hard gates pass** if any hard gate goes green; that's the gating question for the patching strategy.
- **Phase F** — Text/Json/Binary dark-zone confirmation + correlator-based out-of-band rescue. Required only if LLM→TTS edge latency must be measurable.
- **Phase G(2, 3, 4)** — real Python multiprocess round-trip, 1000-hop overflow, text-IPC gap.
- **Phase I** — Criterion benchmark (sub-µs claim validation, default-on vs opt-in decision).

---

## Audit findings (for resumption context)

These were established by the exploration phase and should not need re-discovery:

**RuntimeData metadata-bearing variants:**
- `RuntimeData::Audio` at `crates/core/src/lib.rs:142` — has `metadata: Option<Value>`.
- `RuntimeData::ControlMessage` at `crates/core/src/lib.rs:233` — has `metadata: Value`.

**RuntimeData variants without a metadata slot (provenance dark zones):**
- `Text`, `Json`, `Binary`, `Video`, `Tensor`, `Numpy`, `File`.

**Hot-path Audio nodes that clobber `metadata: None` even when they could forward it (Phase C / D candidates):**
- `audio_buffer_accumulator.rs:428` (`flush_buffer`).
- `audio_chunker.rs:186`.
- `audio_resample_streaming.rs` (lines 91, 169, 381, 415, 453).
- `speculative_vad_coordinator.rs:263` (the speculative fast path — most critical).
- `speculative_audio_commit.rs:196` (`build_committed_audio`).
- `speculative_vad_gate.rs` (lines 231, 381, 423, 452).
- `audio_channel_splitter.rs:336, 361` (preserves timestamp_us but drops metadata).
- `batch_aware_node.rs:80`.
- `sync_av.rs:64`.
- `audio_level.rs:87`.
- `speaker_diarization.rs:242`.

**Python multiprocess IPC (`crates/core/src/python/multiprocess/data_transfer.rs`):**
- `IpcPayload::audio` (line 33) — serializes `metadata` correctly. Audio survives Python round-trip ✅.
- `IpcPayload::text` (line 72) — no metadata slot. Text cannot carry provenance through Python ❌.
- `IpcPayload::control_message` (line 203) — has metadata Value slot. ControlMessage survives ✅.

**Existing test scaffolding to reuse:**
- `crates/core/src/transport/session_router.rs:1448–1650` — `create_test_manifest(nodes, connections)` helper plus `test_session_router_*` tests covering linear/fan-out/fan-in/diamond DAGs.
- `crates/core/tests/integration/test_speculative_vad.rs:17–131` — `MockSpeculativeVADGate`: AsyncStreamingNode that records outputs to `Arc<Mutex<Vec<RuntimeData>>>`. Template for `MockForwardingAudioNode`.
- `crates/core/src/transport/perf_aggregator.rs:282–348` — in-process aggregator test rig: `record_input`/`record_output`/`flush_snapshot` with percentile assertions.
- `crates/core/tests/transport_integration_test.rs` — `PipelineExecutor::create_session(manifest) → StreamSession` flow.

---

## Critical files (consolidated)

Production code referenced by the tests, **never modified during validation**:

- `crates/core/src/lib.rs:140` — `RuntimeData` enum; metadata-bearing variants at lib.rs:142 (Audio) and lib.rs:233 (ControlMessage).
- `crates/core/src/transport/session_router.rs:1118` — `route_input`, ingest stamping site.
- `crates/core/src/transport/session_router.rs:991–1015` — per-input output callback `cb`, hop-append site.
- `crates/core/src/transport/session_router.rs:935–942` — fan-out clone site (must append hop *before* clone).
- `crates/core/src/transport/session_router.rs:1452` — `create_test_manifest` helper to reuse.
- `crates/core/src/transport/perf_aggregator.rs:127` — aggregator API.
- `crates/core/src/transport/perf_aggregator.rs:282` — existing test rig pattern.
- `crates/core/src/python/multiprocess/data_transfer.rs:33` — `IpcPayload::audio` metadata path.
- `crates/core/src/python/multiprocess/data_transfer.rs:203` — `IpcPayload::control_message`.
- `crates/core/src/data/perf.rs` — snapshot schema; Phase H extends with `edges` and `paths`.
- `crates/core/tests/integration/test_speculative_vad.rs:17–131` — `MockSpeculativeVADGate` template for `MockForwardingAudioNode`.

Test-only code created during validation:

- `crates/core/tests/perf_provenance/mod.rs` — helpers + `MockForwardingAudioNode`.
- `crates/core/tests/perf_provenance/phase_a_single_hop.rs`.
- `crates/core/tests/perf_provenance/phase_b_audio_chain.rs`.
- `crates/core/tests/perf_provenance/phase_e_derivation.rs`.
- `crates/core/tests/perf_provenance/phase_g_python_ipc.rs`.
- `crates/core/tests/perf_provenance/phase_h_aggregator_integration.rs`.

Total estimated cost: ~1050 LOC of test code + helpers, no production-code changes during this validation pass.
