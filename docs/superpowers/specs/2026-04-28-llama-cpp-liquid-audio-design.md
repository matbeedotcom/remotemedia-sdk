# LFM2Audio (Liquid Audio) Integration via llama.cpp PR #18641 — Multiprocess

**Date:** 2026-04-28
**Status:** Draft (revised after spec review #1; pending review #2 + user approval)
**Owner:** Mathieu Gosbee
**Supersedes:** v1 of this document (in-process FFI link with `objcopy` symbol rename) — withdrawn after review identified three blockers (C++ re-mangling infeasible, header-only inline state collisions, ggml CUDA backend Meyers-singleton conflicts).

## Summary

Add llama.cpp-backed LFM2Audio (Liquid AI's audio-in / audio-out extension to
llama.cpp, currently in [ggml-org/llama.cpp PR #18641](https://github.com/ggml-org/llama.cpp/pull/18641))
as Rust pipeline nodes that run their inference in a **dedicated subprocess**
linked against a vendored, SHA-pinned copy of the PR. The existing in-process
`llama-cpp-4 = "0.2.13"` integration stays untouched. The two stacks coexist
by living in **different processes**, which sidesteps the C++ symbol
collisions, header-inline state sharing, and ggml backend-registry conflicts
that killed the in-process plan.

This is the same architectural pattern this codebase already uses for Python
nodes via `multiprocess_executor.rs` and iceoryx2 zero-copy IPC. We are
applying that pattern to a Rust child process instead of a Python child.

## Goals

- In-pipeline `LlamaCppLiquidASRNode` (audio → text) and `LlamaCppLiquidTTSNode`
  (text → audio) nodes usable from manifests, indistinguishable in shape from
  any other node in the runtime.
- Zero impact on existing `llama-cpp` / `llama-cpp-cuda` features; both stacks
  can be enabled simultaneously because they never link into the same binary.
- Pin the PR at a known-good commit SHA, vendored as plain files. Reproducible
  builds, no submodule churn.
- Reuse iceoryx2 zero-copy IPC. No new IPC infrastructure.

## Non-Goals

- In-process linking of stock llama.cpp + LFM2Audio fork. Withdrawn — see
  v1 review for technical reasons.
- Replacing the existing Python-backed `LFM2AudioNode` (transformers / MLX).
  This new node is a third backend, not a substitute.
- Streaming TTS partials at sub-utterance granularity (deferred).
- WASM build of the liquid runner.
- Auto-downloading GGUFs.

## Context

### What's already in the repo

- `llama-cpp-4 = "0.2.13"` integration lives in `crates/core/src/nodes/llama_cpp/`
  (generation, embedding, activation, steer). Untouched by this design.
- A Python-backed `LFM2AudioNode` and `LFM2AudioMlxNode` already exist (used by
  `crates/transports/webrtc/examples/lfm2_audio_webrtc_server.rs`). Those are
  transformers-based, not llama.cpp-based. This design adds a third backend
  (llama.cpp + GGUF) for users who want native quantized inference without a
  Python runtime.
- `crates/core/src/python/multiprocess/multiprocess_executor.rs` provides the
  full subprocess + iceoryx2 IPC machinery: `spawn_ipc_thread`,
  `IpcCommand::{SendData, RegisterOutputCallback, Shutdown}`,
  per-session `GLOBAL_SESSIONS` registry, dedicated OS-thread iceoryx2 pub/sub,
  `process_manager.rs` for lifecycle, `health_monitor.rs` for liveness. We
  reuse all of it.
- The IPC payload binary format is in
  `crates/core/src/python/multiprocess/data_transfer.rs`:
  `type | session_len | session_id | timestamp | payload_len | payload`.
  Audio is `f32` little-endian; text is UTF-8.

### What PR #18641 ships

A standalone server (`llama-liquid-audio-server`), a CLI
(`llama-liquid-audio-cli`), and the new mmproj/vocoder/speaker-tokenizer
machinery on top of llama.cpp. Inference requires four GGUF files:

- `LFM2.5-Audio-1.5B-Q8_0.gguf` — main model
- `mmproj-LFM2.5-Audio-1.5B-Q8_0.gguf` — audio encoder (input)
- `vocoder-LFM2.5-Audio-1.5B-Q8_0.gguf` — TTS decoder
- `tokenizer-LFM2.5-Audio-1.5B-Q8_0.gguf` — speaker tokenizer

The PR is open and has not landed; rebases / force-pushes are expected.

## Decisions (revised)

| #   | Decision                                                                                |
|-----|-----------------------------------------------------------------------------------------|
| D1' | One subprocess per liquid model (handles both ASR and TTS, sharing GGUFs internally).   |
| D2' | We ship our own thin Rust runner binary, `remotemedia-liquid-audio-runner`. Not the PR's HTTP server. |
| D3' | Reuse the existing iceoryx2 IPC pattern from `multiprocess_executor.rs`. No HTTP/gRPC.  |
| D4' | Pin the PR at a specific commit SHA, vendored as plain files (unchanged from v1).       |
| D5' | Two pipeline-side nodes (`LlamaCppLiquidASRNode`, `LlamaCppLiquidTTSNode`) sharing one subprocess via the existing per-session registry pattern. |

## Architecture

### Process topology

```
┌──────────────────────────────────────────────────────────────────────┐
│ Main runtime process (remotemedia-core, gRPC server, etc.)            │
│   - Stock llama-cpp-4 0.2.13 linked here (existing nodes)             │
│   - LlamaCppLiquidASRNode  ─┐                                         │
│   - LlamaCppLiquidTTSNode  ─┤  thin IPC clients (no llama.cpp linkage)│
│                              │                                         │
│                              ▼                                         │
│                  spawn_ipc_thread (existing)                           │
│                              │                                         │
└──────────────────────────────┼─────────────────────────────────────────┘
                               │ iceoryx2 zero-copy
                               │ (pub/sub on dedicated OS thread)
                               ▼
┌──────────────────────────────────────────────────────────────────────┐
│ remotemedia-liquid-audio-runner (one per model, per session)          │
│                                                                       │
│   - Links llama-cpp-liquid → llama-cpp-liquid-sys (PR-fork llama.cpp) │
│   - Loads model + mmproj + vocoder + speaker tokenizer ONCE           │
│   - Reads commands over iceoryx2 input channel                        │
│   - Routes each command to the ASR or TTS path internally             │
│   - Emits results over iceoryx2 output channel                        │
│                                                                       │
│  Commands (in):  AudioChunk(bytes, source_node_id)                    │
│                  TextUtterance(text, source_node_id)                  │
│                  Shutdown                                             │
│  Events (out):   TextResult(node_id, text, partial: bool)             │
│                  AudioResult(node_id, f32_pcm, sample_rate)           │
│                  Error(node_id, message)                              │
└──────────────────────────────────────────────────────────────────────┘
```

The `source_node_id` field lets the runner demultiplex which pipeline-side
node (ASR or TTS) sent the command, so a single subprocess can serve both
without ambiguity.

### Crate layout

```
crates/
├── llama-cpp-liquid-sys/          # NEW — vendored fork at PR #18641 SHA
│   ├── llama.cpp/                 #   vendored source (plain files, immutable)
│   ├── llama.cpp.SHA              #   one-line SHA pin (verified by build.rs)
│   ├── build.rs                   #   cmake-rs → static archives → bindgen
│   │                              #   NO symbol renaming (only this binary links it)
│   ├── src/lib.rs                 #   raw FFI bindings
│   └── Cargo.toml                 #   features: cuda / metal / vulkan
│
├── llama-cpp-liquid/              # NEW — safe Rust wrapper over the -sys crate
│   └── src/
│       ├── model.rs               #   LiquidModel
│       ├── context.rs             #   LiquidContext, LiquidDecodeContext
│       ├── audio.rs               #   mmproj / vocoder / speaker tokenizer
│       ├── ipc.rs                 #   IPC command/event types (serde, shared with runner)
│       └── lib.rs
│
├── liquid-audio-runner/           # NEW — the subprocess binary
│   ├── src/main.rs                #   load GGUFs, run iceoryx2 loop, dispatch ASR/TTS
│   └── Cargo.toml                 #   bin = "remotemedia-liquid-audio-runner"
│
└── core/
    ├── Cargo.toml                 #   adds optional dep on llama-cpp-liquid (for ipc types only)
    │                              #   adds runtime dep search for liquid-audio-runner binary
    └── src/nodes/llama_cpp/
        ├── mod.rs                 #   gates liquid_audio behind feature
        └── liquid_audio/          # NEW
            ├── mod.rs
            ├── runner.rs          #   LiquidAudioRunner: process lifecycle + IPC client
            ├── registry.rs        #   LiquidRunnerRegistry (Weak refs, see §"Sharing")
            ├── asr.rs             #   LlamaCppLiquidASRNode + factory
            ├── tts.rs             #   LlamaCppLiquidTTSNode + factory
            ├── config.rs          #   LiquidAudioConfig / ASR / TTS configs
            └── factory.rs         #   LiquidAudioNodesProvider (inventory)
```

**Key invariants:**

- The PR fork's symbols only ever live inside the `liquid-audio-runner` binary.
  The main `remotemedia-core` binary never links them. Symbol-collision
  problems are eliminated by physical separation, not by name mangling.
- `llama-cpp-liquid` is consumed by both the runner (for inference) and by
  `core` (for the IPC type definitions only — `LiquidIpcCommand`, `LiquidIpcEvent`).
  The `core` consumer enables only a `types` cargo feature that compiles the
  serde structs, not the FFI bindings.
- The runner binary is shipped as a build artifact alongside `remotemedia-core`.
  At runtime the node looks it up via `$REMOTEMEDIA_LIQUID_AUDIO_RUNNER` env
  var or `which`, falling back to a sibling-of-current-exe search. Same
  pattern as how the existing Python multiprocess runner is found.

### Subprocess lifecycle

Reuses `crates/core/src/python/multiprocess/process_manager.rs` pattern,
generalized to spawn an arbitrary binary instead of `python -m runner`:

1. Pipeline construction: `LlamaCppLiquidASRNode::initialize()` (or TTS's)
   asks `LiquidRunnerRegistry::get_or_spawn(&LiquidAudioConfig)` for an
   `Arc<LiquidAudioRunner>`.
2. Registry key: `(model_path, mmproj_path, vocoder_path, speaker_path,
   backend_tag)`. If a live entry exists with that key, return its `Arc`.
3. If no live entry: spawn `remotemedia-liquid-audio-runner --session-id X
   --model … --mmproj … --vocoder … --speaker …`, set
   `PR_SET_PDEATHSIG` (Linux) so the child dies if we die (the codebase
   already does this for Python children — see project memory entry on
   orphan process fix), spawn IPC thread for it, register output callback,
   wait for `READY` signal on stdout (control channel only — data flows
   over iceoryx2). Insert `Weak` into registry.
4. Return the `Arc<LiquidAudioRunner>`. ASR and TTS nodes both receive the
   same `Arc` if their configs match.
5. On node drop: the `Arc` count decrements. When it hits zero (no nodes
   reference this runner), the registry's `Weak` upgrade fails on next
   lookup, and a future request for the same config respawns. The runner
   itself dies because we explicitly send `Shutdown` and join its IPC thread
   in `LiquidAudioRunner::drop`.

### IPC payload format

Reuse the existing binary format from
`crates/core/src/python/multiprocess/data_transfer.rs`. We add two new type
discriminator bytes:

- `0x10` = `LiquidIpcCommand::AudioChunk { source_node_id, samples }`
- `0x11` = `LiquidIpcCommand::TextUtterance { source_node_id, text }`
- `0x12` = `LiquidIpcCommand::Shutdown`
- `0x20` = `LiquidIpcEvent::TextResult { node_id, text, partial }`
- `0x21` = `LiquidIpcEvent::AudioResult { node_id, samples, sample_rate }`
- `0x22` = `LiquidIpcEvent::Error { node_id, message }`

The serializer/deserializer lives in `llama-cpp-liquid::ipc` and is shared
between the runner and the pipeline-side node code, ensuring on-wire
compatibility by construction. No serde/JSON in the hot path — fixed-layout
binary, zero-copy where possible.

### Pipeline-side node design

#### `LiquidRunnerRegistry`

```rust
pub struct LiquidRunnerRegistry {
    inner: RwLock<HashMap<RunnerKey, Weak<LiquidAudioRunner>>>,
    spawn_locks: Mutex<HashMap<RunnerKey, Arc<Mutex<()>>>>,
}

impl LiquidRunnerRegistry {
    pub fn get_or_spawn(&self, cfg: &LiquidAudioConfig) -> Result<Arc<LiquidAudioRunner>>;
}
```

- `Weak` references prevent leaking the runner across pipeline restarts.
- `spawn_locks` is the per-key thundering-herd guard: if two nodes call
  `get_or_spawn` with the same key concurrently after the previous runner
  died, only one spawns; the other waits on the per-key mutex and then
  observes the live `Arc`.
- One global registry (`OnceLock<LiquidRunnerRegistry>`), per-process.

#### `LiquidAudioRunner`

```rust
pub struct LiquidAudioRunner {
    process: ManagedChild,
    command_tx: mpsc::Sender<LiquidIpcCommand>,
    output_rx: Mutex<broadcast::Receiver<LiquidIpcEvent>>, // per-node subscription
    health: Arc<HealthMonitor>,
}

impl LiquidAudioRunner {
    pub fn send_audio(&self, source_node_id: &str, samples: &[f32]) -> Result<()>;
    pub fn send_text(&self, source_node_id: &str, text: &str) -> Result<()>;
    pub fn subscribe(&self, node_id: &str) -> broadcast::Receiver<LiquidIpcEvent>;
}
```

A `broadcast` channel from the IPC thread fans events out to all
subscribers; each node filters events by `node_id` to receive only its own
results. This matches how multi-node-per-process Python runners work today.

#### `LlamaCppLiquidASRNode`

```rust
#[node(
    node_type = "LlamaCppLiquidASR",
    capabilities = "runtime_discovered",
    input_caps  = "audio(sample_rate=16000, channels=1, format=F32)",
    output_caps = "text"
)]
pub struct LlamaCppLiquidASRNode {
    runner: Arc<LiquidAudioRunner>,
    rx: broadcast::Receiver<LiquidIpcEvent>,
    cfg: LiquidAudioASRConfig,
    node_id: String,
}
```

Capability behavior is `RuntimeDiscovered` (corrected from v1's `Configured`,
per spec-review #9): the node reports a *potential* range
(`audio(sample_rate=8000..48000)`) at construction, and the *actual* value
(read from mmproj GGUF metadata after the runner has loaded it) on
post-init `resolve_capabilities()`.

Per-chunk flow: `RuntimeData::Audio` in → `runner.send_audio(self.node_id, &samples)`
→ await `LiquidIpcEvent::TextResult { node_id == self.node_id }` from `rx` →
emit `RuntimeData::Text`.

#### `LlamaCppLiquidTTSNode`

```rust
#[node(
    node_type = "LlamaCppLiquidTTS",
    capabilities = "runtime_discovered",
    input_caps  = "text",
    output_caps = "audio"  // sample_rate resolved at runtime from vocoder GGUF
)]
pub struct LlamaCppLiquidTTSNode {
    runner: Arc<LiquidAudioRunner>,
    rx: broadcast::Receiver<LiquidIpcEvent>,
    cfg: LiquidAudioTTSConfig,
    node_id: String,
}
```

Per-input flow: `RuntimeData::Text` in →
`runner.send_text(self.node_id, &text)` → await `AudioResult` → emit
`RuntimeData::Audio` (whole utterance, not streamed within an utterance —
see Risk 3).

#### Configs

```rust
pub struct LiquidAudioConfig {
    pub model_path: PathBuf,
    pub mmproj_path: PathBuf,
    pub vocoder_path: PathBuf,
    pub speaker_tokenizer_path: PathBuf,
    pub backend: LlamaBackendConfig,   // reuses existing enum (CPU / CUDA)
    pub n_ctx: u32,
    pub n_threads: u32,
}

pub struct LiquidAudioASRConfig {
    pub backend: LiquidAudioConfig,    // drives the registry key
    pub max_tokens: u32,
    pub temperature: f32,
    pub system_prompt: Option<String>,
}

pub struct LiquidAudioTTSConfig {
    pub backend: LiquidAudioConfig,
    pub speaker_id: Option<String>,
    pub speed: f32,
    pub temperature: f32,
}
```

Both ASR/TTS configs embed the same `LiquidAudioConfig` so the registry
deduplicates on the backend portion alone — pipelines that wire
`LiquidASR → … → LiquidTTS` with matching paths share one subprocess.

### Cargo features

```toml
# crates/core/Cargo.toml

[dependencies]
llama-cpp-4         = { workspace = true, optional = true }
llama-cpp-sys-4     = { workspace = true, optional = true }
llama-cpp-liquid    = { workspace = true, optional = true, default-features = false, features = ["types"] }

[features]
# Existing — unchanged
llama-cpp           = ["dep:llama-cpp-4", "dep:llama-cpp-sys-4", ...]
llama-cpp-cuda      = ["llama-cpp", "llama-cpp-4/cuda", "llama-cpp-sys-4/cuda"]

# New
llama-cpp-liquid-audio       = ["dep:llama-cpp-liquid"]   # ipc types only in core
llama-cpp-liquid-audio-cuda  = ["llama-cpp-liquid-audio"] # CUDA flag is on the *runner* build, not core

# Convenience
llama-cpp-all  = ["llama-cpp", "llama-cpp-liquid-audio"]
```

Note: `llama-cpp-liquid-audio-cuda` for `core` only flips a flag that the
runner build script checks. Core itself never links CUDA on the liquid path.
The runner has its own feature gate (`liquid-audio-runner/cuda`) and is
built as a separate cargo target; if both `llama-cpp-cuda` and
`liquid-audio-runner/cuda` are enabled, two separate CUDA-using binaries
exist (the main runtime and the runner) — they share the same CUDA driver
without conflict because each has its own ggml backend registry inside its
own process.

## Risks

### Risk 1 — Subprocess cold start latency

Loading four GGUFs and initializing CUDA inside the runner takes seconds to
tens of seconds depending on model size and disk speed. Pipeline construction
will block on the runner's `READY` signal.

**Mitigation:** the registry caches runners across pipeline restarts (within a
process lifetime). The first pipeline pays the cost; subsequent pipelines
with the same config attach to the existing runner instantly. For the very
first pipeline, surface progress via the existing pipeline-construction
progress events (the runtime already reports node-init progress to clients).

### Risk 2 — IPC throughput for audio

Audio at 16 kHz mono f32 is ~64 KB/s — trivial for iceoryx2. TTS output at
24 kHz mono f32 is ~96 KB/s. iceoryx2's zero-copy shared-memory transport
handles MB/s easily. Latency overhead per chunk is single-digit microseconds
(measured on existing Python multiprocess audio paths in this repo).

**Mitigation:** none needed. Specifically NOT a concern.

### Risk 3 — Streaming TTS

Vocoders typically need full utterances or coherent chunks for clean audio.
Token-by-token streaming through the vocoder may produce boundary clicks.

**Mitigation:** v1 finalizes audio per-utterance — text in → audio out, no
intra-utterance streaming. Streaming TTS deferred until the PR's vocoder
demonstrates clean streaming behavior in upstream tests.

### Risk 4 — Runner crash / OOM

If the runner segfaults (model loading bug, CUDA OOM, PR rebase regression),
the IPC thread observes channel closure. The pipeline-side node returns
`Error::ProcessError` for in-flight requests. The `LiquidAudioRunner`'s
`Drop` cleans up; next call to `get_or_spawn` respawns.

**Mitigation:** reuse `health_monitor.rs`. Surface crashes as structured
errors per chunk, not panics. Document that the *first* chunk after restart
will pay the cold-start cost again.

### Risk 5 — PR rebase churn

PR #18641 force-pushes will break the SHA pin. Bumping requires:
1. Update `crates/llama-cpp-liquid-sys/llama.cpp.SHA`.
2. Replace the vendored tree contents.
3. Rerun `bindgen` (build.rs handles this automatically).
4. Adjust `llama-cpp-liquid` wrappers if the PR changed function signatures.

**Mitigation:** acceptable cost. Document the bump procedure in
`crates/llama-cpp-liquid-sys/README.md`. The scope of churn is contained
because nothing outside that crate's family is affected.

### Risk 6 — GGUF availability

LFM2.5-Audio GGUFs (model + mmproj + vocoder + speaker tokenizer) must be
downloadable. The PR references `LiquidAI/LFM2.5-Audio-1.5B-GGUF`.

**Mitigation:** document the four file paths in node config docs; do not ship
a downloader in this iteration. The existing Python `LFM2AudioNode` uses
`hf_repo` parameter; the llama.cpp nodes take filesystem paths.

## Open Questions (deferred, not blockers)

- **Q1** — Does the PR expose stable C entry points for the audio decode loop
  that we can call from the runner? If yes, the runner is thin glue. If no,
  we wrap the PR's internal C++ inference loop directly. Read during
  implementation; does not affect the design.
- **Q2** — Should the runner support hot-swapping speakers (different
  `--tts-speaker-file` per request) or is it fixed at runner spawn? v1 fixes
  it at spawn (matches the PR's CLI). Hot-swap is a v2 feature.
- **Q3** — Should we emit progress events while the runner is loading? Useful
  UX but not required for correctness. Defer to implementation time.

## Migration & Rollout

1. Land `llama-cpp-liquid-sys` (vendored fork, no rename pipeline). Verify it
   builds standalone with CUDA on Linux + macOS.
2. Land `llama-cpp-liquid` safe wrapper. Unit-test against the canary GGUFs.
3. Land `liquid-audio-runner` binary. Test `READY → AudioChunk → TextResult`
   round-trip with a recorded audio fixture.
4. Land `core/src/nodes/llama_cpp/liquid_audio/` behind the
   `llama-cpp-liquid-audio` feature. No effect on existing users.
5. Add an example (`crates/core/examples/liquid_audio_smoke.rs`) modeled on
   `crates/core/examples/llama_cpp_chat_smoke.rs`. The existing
   `lfm2_audio_webrtc_server.rs` example becomes a candidate for a third
   backend choice (`LFM2_AUDIO_BACKEND=llamacpp`) once nodes land.
6. Document GGUF paths in `docs/`.

## Success Criteria

- `cargo build --release -p liquid-audio-runner --features cuda` succeeds on
  Linux with the SHA-pinned PR tree.
- `cargo test -p remotemedia-core --features llama-cpp,llama-cpp-liquid-audio`
  passes on Linux + macOS, including a smoke test that round-trips audio
  through ASR and TTS via the runner.
- A pipeline manifest `MicInput → LiquidASR → LlamaGeneration → LiquidTTS →
  SpeakerOutput` constructs, validates capabilities, runs end-to-end, and
  produces audible TTS output.
- No regression in `--features llama-cpp` or `--features llama-cpp-cuda`
  builds. No symbol collisions are even possible: the two stacks live in
  different processes.
- Killing the liquid runner from outside (`kill -9`) results in a clean
  per-chunk error and a successful respawn on the next request.

## References

- [ggml-org/llama.cpp PR #18641 (LFM2Audio)](https://github.com/ggml-org/llama.cpp/pull/18641)
- [LiquidAI/LFM2.5-Audio-1.5B-GGUF](https://huggingface.co/LiquidAI/LFM2.5-Audio-1.5B-GGUF)
- [eugenehp/llama-cpp-rs (current `llama-cpp-4` source)](https://github.com/eugenehp/llama-cpp-rs)
- Existing multiprocess infra: `crates/core/src/python/multiprocess/multiprocess_executor.rs`
- Existing IPC binary format: `crates/core/src/python/multiprocess/data_transfer.rs`
- Existing prior art (Python-backed LFM2): `crates/transports/webrtc/examples/lfm2_audio_webrtc_server.rs`
- Withdrawn v1 (in-process FFI link with symbol rename): see git history at commit `491632e`
