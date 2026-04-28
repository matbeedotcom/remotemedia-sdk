# External `StreamingNode` Subprocess Pattern

**Date:** 2026-04-28
**Status:** Quick spec (drafted in response to user request during liquid-audio implementation)
**Owner:** Mathieu Gosbee
**Related:** `2026-04-28-llama-cpp-liquid-audio-design.md` (the concrete first instance), spec 033 "loadable-node-libraries" (the in-process dylib variant — different mechanism, same goal)

## Summary

Generalize the multiprocess pattern being built for the liquid-audio runner so that **any** external Rust crate implementing `StreamingNode` can be plugged into a manifest pipeline as a subprocess-isolated node — without the runtime needing source-level knowledge of that crate. This provides the out-of-process counterpart to spec 033's in-process loadable-dylibs path: same goal (third-party / experimental / ABI-fragile nodes), different isolation model (process boundary instead of dylib boundary).

## Goals

- A third-party crate can publish a binary like `my-fancy-node-runner` and a manifest can reference it by `node_type: "MyFancyNode"` plus a binary-discovery hint (env var, `$PATH`, or absolute path).
- The runtime spawns the binary on first use, communicates with it over the existing iceoryx2 zero-copy IPC, and tears it down on session end — using **the same** `SpawnTarget::Binary` + `LiquidRunnerRegistry`-style machinery being built for liquid-audio.
- The external crate writes idiomatic Rust: implement `StreamingNode`, slap on a `#[node_main]` macro, get a runnable subprocess binary for free.
- `RuntimeData` flows in and out unchanged; capability resolution works the same as for in-process nodes.
- Crash isolation: if an external node panics or segfaults, only its subprocess dies; the host runtime emits a per-chunk error and (configurably) respawns.

## Non-Goals

- ABI stability across rustc versions. External crates compile against the same workspace `remotemedia-core` version they target — no `abi_stable` or C ABI shim. Rebuild on rust upgrades. (For ABI-stable plugins, see spec 033.)
- Cross-language nodes. Python is already covered by the existing multiprocess Python executor.
- Sandboxing / seccomp / containers. Out of scope for v1; the subprocess inherits the parent's privileges.
- Hot reload. Restart the pipeline to pick up a new node binary.

## Context

### What we're already building (liquid-audio)

The liquid-audio integration is producing exactly the infrastructure this generalization needs:

| Component | Liquid-audio specific? | Reusable? |
|-----------|------------------------|-----------|
| `SpawnTarget::Python | Binary` enum + `build_spawn_command` helper | No | ✅ Already generic (M0.1, M0.2, M0.3) |
| `LiquidRunnerRegistry` (per-session, `Weak` refs, lock-ordered spawn) | Named "Liquid" but logic is generic | ✅ Rename to `ExternalRunnerRegistry` |
| `LiquidAudioRunner` (process lifecycle + IPC client + per-node mpsc demux) | Named "Liquid" but logic is generic | ✅ Rename to `ExternalNodeRunner` |
| Runner discovery (env → sibling-of-current-exe → `$PATH`) | Hardcodes `remotemedia-liquid-audio-runner` | ✅ Parameterize the binary name |
| iceoryx2 sibling protocol (`{session_id}_liquid_{runner_id}_input/output`) | Channel naming uses "liquid" | ✅ Parameterize the protocol tag |
| IPC frame codec (`LiquidIpcCommand` / `LiquidIpcEvent`) | Discriminators for AudioChunk, TextUtterance | ⚠️ Replace with `RuntimeData`-typed frames |
| `LlamaCppLiquidASR/TTSNode` pipeline-side glue | Specific to LFM2Audio | ❌ Replaced by generic `ExternalSubprocessNode` |
| `liquid-audio-runner` binary | Specific to LFM2Audio FFI | ❌ Replaced by `#[node_main]` macro |

Roughly 70% of the liquid-audio work directly contributes to this generalization.

### Why this is worth doing

- **Plugin ecosystem.** Third-party labs (custom CV models, niche TTS, experimental encoders) can ship a node without forking `remotemedia-core` or PRing into our crate.
- **ABI-fragile dependencies.** Crates pinning specific `tch`, `candle`, `onnxruntime`, or `llama.cpp` versions can isolate themselves. No more "candle 0.3 wants tokenizers 0.15 but core wants 0.16".
- **Crash containment.** A flaky CUDA kernel or buggy ONNX op in one node doesn't take down the whole runtime.
- **GPU sharding.** Pin different external runners to different GPUs via env vars at spawn time — no source changes.

## Decisions (proposed for quick review)

| # | Decision |
|---|----------|
| D1 | The external node author implements `StreamingNode` against the public `remotemedia-core` API and adds `remotemedia_node!(MyFancyNode)` to their `main.rs`. The macro emits a `tokio` main that runs the iceoryx2 IPC loop. |
| D2 | The IPC frame is `RuntimeData`-typed (reuses the existing `data_transfer.rs` format with all 8 `DataType` discriminators), wrapped in an envelope carrying `corr_id`, `node_id`, and frame kind (Input / Output / Control / Capabilities / Error / Shutdown). |
| D3 | Manifest reference: `node_type: "MyFancyNode"` plus a top-level `external_runners` map at the manifest root (or a global runtime config) that resolves `MyFancyNode` to a binary path / env var / `$PATH` lookup. |
| D4 | Capabilities are negotiated at runner spawn via a `Capabilities` event the runner emits before `READY` (mirrors the liquid-audio `RunnerCapabilities` pattern). The runner reports both potential ranges and (after init) actual values. |
| D5 | One subprocess per `(node_type, config_hash)` per session, refcounted by the registry. Two manifest nodes of the same type with the same config share one subprocess (matches liquid-audio's per-session-shared-runner pattern). |

## Architecture

### External crate side (third-party author writes this)

```rust
// crates/my-fancy-node/src/main.rs
use remotemedia_core::nodes::StreamingNode;
use remotemedia_core::data::RuntimeData;
use remotemedia_node_runner::remotemedia_node;     // new: thin trait + macro crate

pub struct MyFancyNode { /* ... */ }

#[async_trait::async_trait]
impl StreamingNode for MyFancyNode {
    fn node_type(&self) -> &str { "MyFancyNode" }
    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> { /* ... */ }
    /* ... rest of trait ... */
}

remotemedia_node!(MyFancyNode);
```

The `remotemedia_node!` macro expands to (sketch):

```rust
fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let runner = remotemedia_node_runner::Runner::<MyFancyNode>::from_env().await?;
        runner.run().await
    })
}
```

`Runner::run()` does:
1. Read CLI args / env for `--session-id`, `--runner-id`, `--node-type`, `--config-json`.
2. Construct `MyFancyNode` from config.
3. Open iceoryx2 services `{session_id}_ext_{runner_id}_input/output` and `_control`.
4. Call `node.initialize(&InitializeContext::default()).await?`.
5. Emit `Capabilities` event with `node.media_capabilities(...)` (or potential ranges).
6. Emit `READY` on the control channel.
7. Loop: receive `Input { corr_id, data }` → call `node.process_async(data)` → emit `Output { corr_id, data }`.
8. On `Shutdown`: drop the node (which releases models / GPU memory) and exit.

### Host runtime side

The pipeline-side node — call it `ExternalSubprocessNode` — is a single generic node type. The manifest reference looks like:

```yaml
nodes:
  - id: fancy-1
    node_type: ExternalSubprocessNode  # internal type
    params:
      external_node_type: "MyFancyNode"  # what the external runner advertises
      config:                            # opaque to the runtime; passed to the runner
        threshold: 0.5
        gpu: 0

external_runners:
  MyFancyNode:
    discovery: env_or_sibling_or_path  # default
    binary_name: "my-fancy-node-runner"
    env_var:    "MY_FANCY_NODE_RUNNER" # optional override
```

`ExternalSubprocessNode` resolves the binary via the existing discovery logic (parameterized: `binary_name` + `env_var`), looks up the runner in the per-session `ExternalRunnerRegistry`, attaches a per-node `mpsc::Receiver`, and forwards `process_async` calls as `Input` frames over IPC.

### IPC envelope

Generic, `RuntimeData`-typed:

```text
+-----+------+--------+-----+------+------------------+
| ver | kind | corr_id| nid | n_id | RuntimeData blob |
| u8  | u8   | u64_le | u16 | utf8 | (existing format)|
+-----+------+--------+-----+------+------------------+
```

`kind`:
- `0x01` Input (host → runner): payload is a `RuntimeData::to_bytes()` blob.
- `0x02` Control (host → runner): ControlMessage variant.
- `0x03` Shutdown (host → runner): empty.
- `0x80` Capabilities (runner → host): potential + actual capabilities (serde_json blob, one-time).
- `0x81` Output (runner → host): payload is `RuntimeData::to_bytes()`.
- `0x82` ProgressEvent (runner → host): for `InitializeContext::emit_progress` mirrored over IPC.
- `0x83` Error (runner → host): structured error string.

Crucially, the payload reuses the existing `RuntimeData` binary format from `data_transfer.rs` — no new serialization. The envelope is the only new bit.

### Lifecycle reuse from liquid-audio

Direct rename of liquid-audio components into a `crates/external-node-runtime` (host side) + `crates/remotemedia-node-runner` (external-author side) split:

- `LiquidRunnerRegistry` → `ExternalRunnerRegistry<R: ExternalRunnerHandle>` (generic over runner type so liquid-audio and any external use the same impl).
- `LiquidAudioRunner` → `ExternalNodeRunner` (still process-per-config; the iceoryx2 demux task and per-node `mpsc` channels are unchanged).
- `LlamaCppLiquidASRNode` becomes a thin wrapper around `ExternalSubprocessNode` that hardcodes `external_node_type: "LlamaCppLiquidASR"` and the GGUF-paths config translation. Same for TTS. Liquid-audio is then *one user of* the generic mechanism, not a separate code path.
- `liquid-audio-runner` `main.rs` shrinks to: define the `LiquidASR` and `LiquidTTS` `StreamingNode` impls, then `remotemedia_node!(LiquidASR); remotemedia_node!(LiquidTTS);` (or one binary serving both via a node-type dispatch the macro can also emit).

## Crate layout (delta vs. liquid-audio plan)

```
crates/
├── external-node-runtime/         # NEW (host side)
│   └── src/
│       ├── registry.rs            # ExternalRunnerRegistry<H>  (was LiquidRunnerRegistry)
│       ├── runner.rs              # ExternalNodeRunner          (was LiquidAudioRunner)
│       ├── discovery.rs           # parameterized binary lookup (was liquid-specific)
│       ├── node.rs                # ExternalSubprocessNode (the manifest-facing pipeline node)
│       ├── ipc.rs                 # generic envelope codec
│       └── factory.rs             # registers ExternalSubprocessNode with the inventory
│
├── remotemedia-node-runner/       # NEW (external-author side, public crate)
│   └── src/
│       ├── lib.rs                 # remotemedia_node! macro, Runner<N> generic
│       └── runtime.rs             # iceoryx2 main loop, READY handshake, Capabilities emit
│
├── liquid-audio-runner/           # SHRINKS — becomes a thin user of remotemedia-node-runner
└── core/src/nodes/llama_cpp/liquid_audio/
                                   # SHRINKS — ASR/TTS nodes become wrappers over
                                   # ExternalSubprocessNode with a fixed external_node_type
```

The original liquid-audio plan's M5 (registry + runner client) becomes a build-out of `external-node-runtime` instead, and the M3/M4 work (FFI + runner binary) stays liquid-specific but uses `remotemedia-node-runner`'s macro.

## Risks

### Risk 1 — `StreamingNode` is not currently `Send + Sync + 'static + dyn-compatible-across-process`

The trait is `Send + Sync` but uses associated `async_trait` methods that desugar to type-erased futures. Sending the trait *across a process boundary* doesn't apply (we don't); we only need the in-process side to hold a `Box<dyn StreamingNode>`. **Mitigation:** none needed; the trait is fine as-is for this use.

### Risk 2 — Macro complexity hides initialization errors

The `remotemedia_node!(NodeType)` macro generates `main`, which means panics during config parsing or initialization may not produce useful diagnostics. **Mitigation:** the macro emits a `tracing-subscriber` init at the top of `main` that respects `RUST_LOG`. Document this in the macro's doc.

### Risk 3 — Manifest schema needs a new top-level `external_runners` field

This is a public-API addition to the manifest. Existing manifests stay valid (the field is optional). **Mitigation:** treat as a v1.x manifest extension. Document in `docs/manifest.md`.

### Risk 4 — RuntimeData IPC throughput for large payloads

A 1080p RGB frame is ~6 MB; iceoryx2 sample-size bounds need configuration. Liquid-audio sets a 64 KiB default; image/video nodes need much larger. **Mitigation:** `external_runners.<name>.max_ipc_frame_bytes` config knob (default per-data-type, overridable). Cite an upper bound in docs (~256 MB iceoryx2 hard limit on shared memory regions).

### Risk 5 — Subprocess spawn cost per session

Cold start of a new subprocess for every session is expensive. **Mitigation:** the registry is per-session today (per liquid-audio's decision); we may want a process-pool / cross-session sharing mode for stateless nodes. **Defer to a v2 follow-up** unless a concrete user appears who needs it.

## Open Questions

- **Q1.** Should the runner protocol support multi-input / multi-output nodes (`process_multi_async`, streaming callback)? The envelope can carry an `input_name` field; the runner's `Input` handler routes by name. **Recommendation:** add `input_name: Option<String>` to the envelope from day one; cost is one extra `u8 + utf8` per frame.
- **Q2.** How does `InitializeContext::emit_progress` cross the process boundary? **Proposal:** the runner emits `ProgressEvent` IPC frames during `initialize()`; the host-side `ExternalSubprocessNode::initialize()` forwards them through its own `InitializeContext`. Same wire format as the existing in-process pattern.
- **Q3.** Versioning: how does the host detect a runner built against an incompatible `remotemedia-core`? **Proposal:** the `Capabilities` event includes a `core_api_version: u32` field; host fails fast on mismatch with a clear message.

## Migration & Rollout

1. **Land liquid-audio as planned** (current implementation work). The infrastructure built there *is* the generic infrastructure with liquid-specific names.
2. **Refactor pass (post-liquid-audio M7):** rename `LiquidRunnerRegistry` → `ExternalRunnerRegistry<H>`, lift `LiquidAudioRunner` → `ExternalNodeRunner<H>`, parameterize discovery. Liquid-audio nodes become users of the generic types, not separate code paths.
3. **New crates:** `external-node-runtime` (host side, factor-out from liquid-audio refactor) and `remotemedia-node-runner` (external-author side, the macro + Runner generic).
4. **Manifest extension:** add `external_runners` top-level map; resolver consults it during pipeline construction.
5. **Sample external node** (`crates/examples/external-noop-node/`): a no-op `StreamingNode` that echoes its input; lives in the workspace as a worked example and a CI smoke test.
6. **Docs:** "Writing an External StreamingNode" guide showing the macro usage end-to-end.

## Success Criteria

- A third-party crate (not in the workspace) can `cargo new my-node-runner`, depend on `remotemedia-core` + `remotemedia-node-runner`, implement `StreamingNode`, run `cargo install --path .`, and reference `MyNode` from a manifest — all without touching `remotemedia-core` source.
- The liquid-audio `LlamaCppLiquidASRNode` after refactor is < 100 lines: a thin wrapper that constructs an `ExternalSubprocessNode` with `external_node_type: "LlamaCppLiquidASR"`.
- The example `external-noop-node` round-trips `RuntimeData::Audio` through a real subprocess in a CI smoke test.
- The host runtime regression suite shows zero behavior change for in-process nodes after the refactor.

## Decision-points to confirm with user before turning this into a plan

- D1 (macro vs. trait-only): is `remotemedia_node!(MyType)` the right ergonomics, or do you want a runtime function `serve(MyType)` the author calls themselves? Macro is one line; function is more discoverable.
- D3 (manifest shape): top-level `external_runners` map, or per-node `params.binary_path` field? The former is DRY when one binary serves multiple node-types; the latter is simpler.
- D5 (per-session runner sharing): keep liquid-audio's per-session decision, or open the door to cross-session sharing for stateless nodes?
- Whether to roll this in **before** liquid-audio M5/M6 (pay the abstraction cost upfront, build liquid-audio nodes on top) or **after** (ship liquid-audio first, refactor next). Strongly recommend **after** — the abstractions become much clearer once one concrete user has shaken out the requirements.
