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
  Discovery is **new infrastructure** for this design (the existing Python
  runner is found via `python -m remotemedia.core.multiprocessing.runner`,
  not via filesystem search). See §"Runner discovery" below.

### Spawn / IPC infrastructure refactor (prerequisite)

The existing `process_manager.rs` is structurally Python-specific: it
hardcodes `Command::new(&spawn_config.python_executable)` with `-m
remotemedia.core.multiprocessing.runner`, owns a `MultiprocessConfig` that
carries `python_executable`/`python_path`/`python_env`, auto-resolves a
uv-managed venv per session, expects the child to read params from stdin as
JSON, and expects the child to emit `b"READY"` over an iceoryx2 control
channel. None of this is appropriate for a static Rust binary.

Therefore, **before** any liquid-audio code lands, factor out the spawn target:

```rust
pub enum SpawnTarget {
    Python {
        executable: PathBuf,
        env: PythonEnvConfig,
        node_module: String,         // "remotemedia.core.multiprocessing.runner"
        params: Value,                // sent on stdin
    },
    Binary {
        executable: PathBuf,          // resolved liquid runner path
        argv: Vec<String>,            // model paths, etc.
        env: HashMap<String, String>,
    },
}
```

`ProcessManager::spawn_node()` matches on `SpawnTarget`. The Python branch
keeps the venv resolver, params-stdin protocol, and dependency-merging code
exactly as today (no behavior change for existing nodes). The Binary branch
spawns directly, passes config via argv, and shares the same READY-signal /
iceoryx2 channel-naming / health-monitor / `PR_SET_PDEATHSIG` machinery — all
of which is already factored cleanly enough to reuse.

The shared parts that **do** legitimately reuse without change:
`spawn_ipc_thread`, `IpcCommand` enum, `register_output_callback`,
`HealthMonitor`, the iceoryx2 channel registry. The runner-side READY
protocol is also reused: write `b"READY"` to the control channel after init.

This refactor is small but non-zero — call it ~1-2 days. Listed in
§"Migration & Rollout" step 0.

### Runner discovery

The pipeline-side node resolves the `remotemedia-liquid-audio-runner` binary
path on first `get_or_spawn` call. Precedence:

1. `$REMOTEMEDIA_LIQUID_AUDIO_RUNNER` env var, if set and pointing to an
   executable file.
2. Sibling of `std::env::current_exe()` — i.e. `<dir-of-current-exe>/remotemedia-liquid-audio-runner`.
3. `$PATH` lookup via `which::which("remotemedia-liquid-audio-runner")`.
4. If none resolve: `Err(LiquidAudioError::RunnerNotFound)` with a clear
   message naming all three locations searched and pointing the user at the
   install-step doc.

Distribution: the runner is built as part of the workspace (`cargo build -p
liquid-audio-runner --release`) and `cargo install --path
crates/liquid-audio-runner` puts it on `$PATH`. Documented in
`crates/liquid-audio-runner/README.md`.

### Subprocess lifecycle

Reuses the refactored `process_manager.rs` (see prerequisite above) with
`SpawnTarget::Binary`:

1. Pipeline construction: `LlamaCppLiquidASRNode::initialize()` (or TTS's)
   asks `SessionState::liquid_runners().get_or_spawn(&LiquidAudioConfig)`
   for an `Arc<LiquidAudioRunner>`. **Sharing scope: per-session, not
   process-global.** The registry lives inside `SessionState`, not as a
   `OnceLock`. Cross-session sharing was considered and rejected: it would
   require dropping `session_id` from iceoryx2 channel names (departing
   from the existing `{session_id}_{node_id}_input` convention), would
   complicate `terminate_session` (which currently tears down all
   per-session IPC), and the cold-start mitigation (Risk 1) is mostly
   about *intra-session* sharing of one runner across ASR + TTS, not
   cross-session caching.
2. Registry key: `(model_path, mmproj_path, vocoder_path, speaker_path,
   backend_tag)`.
3. **Lock ordering** (corrects v2's earlier ambiguity):
   ```text
   lock outer spawn_locks map briefly → fetch-or-insert Arc<Mutex<()>> for key → drop outer
       ↓
   acquire per-key Arc<Mutex<()>>
       ↓
   Weak::upgrade on inner[key]
       ↓
   if Some(arc): return arc
   if None:      spawn → register Weak in inner → return arc
   ```
   The outer map mutex is held only long enough to look up or create the
   per-key arc and is released before the per-key mutex is taken.
   Deadlock-free because the outer is never re-acquired while holding the
   per-key. The per-key mutex serializes *both* the upgrade attempt and the
   spawn, eliminating the TOCTOU race where two nodes drop and respawn
   between each other's checks.
4. Spawn path: build `SpawnTarget::Binary` with the resolved runner path
   (see "Runner discovery"); call `process_manager.spawn_node(target,
   session_id, node_id="liquid_audio_runner_<key_hash>")`. The IPC thread,
   health monitor, channel naming, and `PR_SET_PDEATHSIG` (Linux) are all
   inherited from the refactored manager.
5. Wait for `READY` signal on the iceoryx2 control channel (existing
   protocol, byte string `b"READY"`). The runner emits `READY` *only after*
   it has loaded all four GGUFs and emitted its `RunnerCapabilities` event
   (see §"IPC protocol" below) — so by the time `initialize()` returns, the
   pipeline-side node has the actual sample rates available for capability
   resolution Phase 2.
6. On node drop: the `Arc` count decrements. When it hits zero, the
   per-session registry's `Weak::upgrade` fails on next lookup. The runner
   itself dies via `Drop` on `LiquidAudioRunner`, which sends `Shutdown`
   and joins the IPC thread.
7. On parent crash (Linux): `PR_SET_PDEATHSIG` kills the runner. On
   parent crash (macOS): no equivalent system call — see §"Risk 4" for the
   heartbeat-based fallback.

### IPC protocol

**Sibling protocol, not the existing `RuntimeData` format.** The existing
`data_transfer.rs` format uses a `DataType` u8 discriminator at offset 0
that is exhausted (values `1..=8` for Audio/Text/Image/etc.) and rejects
any other value. Adding new top-level discriminators (`0x10`, `0x20`) would
not survive `RuntimeData::from_bytes`. The liquid-audio IPC therefore runs
on **separate iceoryx2 services** with a different on-wire schema.

iceoryx2 service-name convention (parallel to the existing
`{session_id}_{node_id}_input`):

- `{session_id}_liquid_{runner_id}_input`  — pipeline-side → runner
- `{session_id}_liquid_{runner_id}_output` — runner → pipeline-side

where `runner_id` is a hash of the registry key (so multiple liquid runners
in the same session don't collide).

Frame layout (every frame on either channel):

```text
+------+------+--------+------+------+--------------+
| ver  | kind | corr_id| nid_l| n_id | payload      |
| u8=1 | u8   | u64    | u16  | utf-8| ...          |
+------+------+--------+------+------+--------------+
```

- `ver = 1` (protocol version; bump for breaking changes)
- `kind`: command/event discriminator
- `corr_id` (u64, little-endian): correlation id; the runner echoes the
  command's `corr_id` on responses so async pipelines can match results
  back to inputs. The runner processes commands per-`node_id` in FIFO
  order, so within a single pipeline node, results arrive in the order
  the inputs were sent. `corr_id` matters for pipelines that interleave
  multiple in-flight requests against the same node — e.g. a future
  streaming-partials extension where a single utterance produces multiple
  `TextResult` events that must be associated with the originating
  `AudioChunk`.
- `nid_l` (u16): length of `n_id` in bytes
- `n_id`: pipeline-side node id (UTF-8, no null terminator), tells the
  runner-client router which `mpsc::Receiver` to dispatch to
- `payload`: kind-specific, length-prefixed where needed

Discriminator values (sibling protocol; never appear in `RuntimeData::from_bytes`):

| `kind` | direction         | meaning                                                                |
|--------|-------------------|------------------------------------------------------------------------|
| `0x01` | client → runner   | `AudioChunk` (payload: `len:u32 \| f32_samples_le`)                    |
| `0x02` | client → runner   | `TextUtterance` (payload: `len:u32 \| utf8_bytes`)                     |
| `0x03` | client → runner   | `Shutdown` (payload: empty)                                            |
| `0x80` | runner → client   | `RunnerCapabilities` (payload: `asr_rate:u32 \| tts_rate:u32 \| n_ch:u16 \| asr_format:u8 \| tts_format:u8`) |
| `0x81` | runner → client   | `TextResult` (payload: `partial:u8 \| len:u32 \| utf8_bytes`)          |
| `0x82` | runner → client   | `AudioResult` (payload: `sample_rate:u32 \| len:u32 \| f32_samples_le`) |
| `0x83` | runner → client   | `Error` (payload: `len:u32 \| utf8_bytes`)                             |

`RunnerCapabilities` is emitted by the runner exactly once, immediately
before the `b"READY"` control byte. This wires Phase-2 capability
resolution (see §"Capability resolution wiring" below).

The encode/decode lives in `llama-cpp-liquid::ipc` and is shared between
the runner and the pipeline-side node code, ensuring on-wire compatibility
by construction. No serde/JSON in the hot path — fixed-layout binary,
zero-copy where possible. Frames are bounded by an iceoryx2 sample-size
config that's set per service to fit the largest expected `AudioResult`
(default: 64 KiB, configurable per `LiquidAudioConfig`).

### Capability resolution wiring

Two-phase, runs against the existing `CapabilityResolver`:

1. **Phase 1 (forward pass, at construction).** Both ASR and TTS nodes
   declare `CapabilityBehavior::RuntimeDiscovered` and return *potential*
   ranges from `potential_capabilities()`:
   - ASR: `audio(sample_rate=8000..48000, channels=1, format=F32)` →
     `text`
   - TTS: `text` → `audio(sample_rate=16000..48000, channels=1, format=F32)`
2. **`node.initialize()`** calls `LiquidRunnerRegistry::get_or_spawn`,
   which blocks until the runner emits `RunnerCapabilities` and `READY`.
   The pipeline-side node caches the actual `(asr_rate, tts_rate)` from
   `RunnerCapabilities` into its own state. `initialize()` then returns.
3. **Phase 2 (re-validation).** The framework calls
   `actual_capabilities()` on each `RuntimeDiscovered` node. The node
   returns the cached values from step 2. The resolver re-validates
   downstream connections (e.g. SpeakerOutput's input rate against the
   TTS's actual output rate) and fails fast with a clear error if
   downstream nodes can't accept the discovered values.

When a second node attaches to an already-running runner (cache hit), the
runner client *also* synthesizes `RunnerCapabilities` from cached state and
returns it synchronously to the second `get_or_spawn` caller. So the
contract — "after `initialize()` returns, `actual_capabilities()` is ready"
— holds regardless of cache hit/miss.

### Pipeline-side node design

#### `LiquidRunnerRegistry` (per-session)

```rust
pub struct LiquidRunnerRegistry {
    inner: RwLock<HashMap<RunnerKey, Weak<LiquidAudioRunner>>>,
    spawn_locks: Mutex<HashMap<RunnerKey, Arc<Mutex<()>>>>,
}

impl LiquidRunnerRegistry {
    pub async fn get_or_spawn(&self, cfg: &LiquidAudioConfig)
        -> Result<Arc<LiquidAudioRunner>>;
}
```

- Lives **inside `SessionState`**, not as a process-global. One registry per
  session. Cross-session sharing was rejected — see §"Subprocess lifecycle"
  step 1.
- `Weak` references prevent leaking the runner across pipeline restarts
  *within* the session.
- `spawn_locks` is the per-key thundering-herd guard. Lock ordering is
  documented in §"Subprocess lifecycle" step 3.
- `SessionState::Drop` (or `terminate_session`) drops the registry; any
  `Arc<LiquidAudioRunner>` still held drops to zero, the runner gets
  `Shutdown`, and the IPC thread joins.

#### `LiquidAudioRunner`

```rust
pub struct LiquidAudioRunner {
    process: ManagedChild,
    command_tx: mpsc::Sender<LiquidIpcCommand>,
    capabilities: RunnerCapabilities,                    // cached from READY
    subscribers: Mutex<HashMap<String, mpsc::Sender<LiquidIpcEvent>>>,
    health: Arc<HealthMonitor>,
}

impl LiquidAudioRunner {
    pub async fn send_audio(&self, src: &str, samples: &[f32]) -> Result<u64>;
    pub async fn send_text(&self, src: &str, text: &str) -> Result<u64>;

    /// Returns the per-node mpsc receiver. Drop on node shutdown to
    /// remove the subscription.
    pub fn subscribe(&self, node_id: &str) -> mpsc::Receiver<LiquidIpcEvent>;

    pub fn capabilities(&self) -> &RunnerCapabilities;
}
```

**Per-node `mpsc::Receiver`, not `broadcast`.** The runner-client owns a
single iceoryx2-output-draining task that demultiplexes events by `node_id`
and forwards each to the right per-node `mpsc::Sender`. This matches how
the existing Python multiprocess executor handles fan-out (one `mpsc` per
registered output callback) and avoids `broadcast`'s lag-and-drop semantics
that would silently drop ASR results when TTS or downstream nodes briefly
stall.

Backpressure: the per-node `mpsc` capacity is bounded (default 32, tunable
via `LiquidAudioConfig`). If a slow consumer fills the channel, the
demuxer task awaits and the iceoryx2 subscriber's internal queue absorbs
the burst; if the iceoryx2 queue saturates, the runner observes pub-side
backpressure (via `Publisher::loan` failure) and pauses its inference loop
until drained. This is the correct flow-control story; `broadcast` would
have lied about it.

#### `LlamaCppLiquidASRNode`

```rust
#[node(
    node_type = "LlamaCppLiquidASR",
    capabilities = "runtime_discovered"
)]
pub struct LlamaCppLiquidASRNode {
    runner: Arc<LiquidAudioRunner>,
    rx: mpsc::Receiver<LiquidIpcEvent>,
    cfg: LiquidAudioASRConfig,
    node_id: String,
}
```

`potential_capabilities()`: `audio(sample_rate=8000..48000, channels=1,
format=F32) → text`.

`actual_capabilities()`: returns the cached value from
`runner.capabilities().asr_rate` (populated during `initialize()` from
the runner's `RunnerCapabilities` event — see §"Capability resolution
wiring").

Per-chunk flow: `RuntimeData::Audio` in →
`runner.send_audio(self.node_id, &samples)` → await
`LiquidIpcEvent::TextResult { node_id == self.node_id }` from `rx` →
emit `RuntimeData::Text`.

#### `LlamaCppLiquidTTSNode`

```rust
#[node(
    node_type = "LlamaCppLiquidTTS",
    capabilities = "runtime_discovered"
)]
pub struct LlamaCppLiquidTTSNode {
    runner: Arc<LiquidAudioRunner>,
    rx: mpsc::Receiver<LiquidIpcEvent>,
    cfg: LiquidAudioTTSConfig,
    node_id: String,
}
```

`potential_capabilities()`: `text → audio(sample_rate=16000..48000,
channels=1, format=F32)`.

`actual_capabilities()`: returns the cached value from
`runner.capabilities().tts_rate`.

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
handles MB/s easily on its own benchmarks.

**Verification status:** the project has `crates/core/benches/docker_ipc_benchmark.rs`
and `crates/core/benches/docker_vs_multiprocess.rs`, which measure full
multiprocess pipeline latency including Python overhead. Neither isolates
"raw iceoryx2 chunk overhead" specifically. Numbers should be confirmed
during the smoke-test phase by adding a microbenchmark that round-trips an
empty `AudioChunk` command through the runner with a no-op handler.

**Mitigation:** verify with a microbenchmark before claiming no concern.
If overhead exceeds ~1 ms per chunk on target hardware, revisit chunk-size
defaults.

### Risk 3 — Streaming TTS

Vocoders typically need full utterances or coherent chunks for clean audio.
Token-by-token streaming through the vocoder may produce boundary clicks.

**Mitigation:** v1 finalizes audio per-utterance — text in → audio out, no
intra-utterance streaming. Streaming TTS deferred until the PR's vocoder
demonstrates clean streaming behavior in upstream tests.

### Risk 4 — Runner crash / OOM / parent-death on macOS

If the runner segfaults (model loading bug, CUDA OOM, PR rebase regression),
the IPC thread observes channel closure. The pipeline-side node returns
`Error::ProcessError` for in-flight requests. The `LiquidAudioRunner`'s
`Drop` cleans up; next call to `get_or_spawn` respawns.

On Linux, the parent's `PR_SET_PDEATHSIG(SIGTERM)` ensures the runner dies
if the runtime crashes. On macOS, no equivalent system call exists — this
matches the existing Python multiprocess executor's behavior (also
Linux-only). Without intervention, a runtime crash on macOS will orphan the
runner with the GGUFs resident in memory.

**Mitigation:** reuse `health_monitor.rs` for liveness; surface crashes as
structured errors per chunk, not panics; document the cold-start cost
after restart. **For macOS specifically**, the runner emits a heartbeat
ping on the iceoryx2 control channel every 5 s; if it does not see a pong
from the parent within 30 s, it self-shutdowns. Cost: ~10 lines in the
runner main loop. Consistent with parent-death on Linux without requiring
a system call macOS doesn't provide.

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

### Risk 7 — iceoryx2 macOS coverage

The pivot to multiprocess implicitly assumes iceoryx2 works on macOS. The
existing Python multiprocess integration is Linux-tested; macOS coverage of
iceoryx2 itself depends on upstream support and is not gated by this repo's
CI today. If upstream macOS support regresses, the liquid-audio path
regresses with it.

**Mitigation:** add a macOS smoke-test job to the CI matrix (see §"CI
matrix" below) that exercises the existing Python multiprocess path *and*
the new liquid-audio path. If iceoryx2 macOS proves fragile during
implementation, demote macOS to "best-effort, untested" in the README and
file an upstream-tracking issue.

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

0. **Refactor `process_manager.rs` to support `SpawnTarget::{Python,Binary}`.**
   No behavioral change for existing Python nodes; lays the groundwork for
   the runner. ~1-2 days. (See §"Spawn / IPC infrastructure refactor".)
1. Land `llama-cpp-liquid-sys` (vendored fork, no rename pipeline). Verify it
   builds standalone with CUDA on Linux and (best-effort) macOS.
2. Land `llama-cpp-liquid` safe wrapper. Unit-test against the canary GGUFs.
3. Land `liquid-audio-runner` binary. Test
   `READY → RunnerCapabilities → AudioChunk → TextResult` round-trip with a
   recorded audio fixture, and `TextUtterance → AudioResult` round-trip.
4. Land `core/src/nodes/llama_cpp/liquid_audio/` behind the
   `llama-cpp-liquid-audio` feature, including the runner-discovery logic
   (§"Runner discovery"). No effect on existing users.
5. Add a microbenchmark for IPC chunk overhead (verifies Risk 2's claim).
6. Add an example (`crates/core/examples/liquid_audio_smoke.rs`) modeled on
   `crates/core/examples/llama_cpp_chat_smoke.rs`. The existing
   `lfm2_audio_webrtc_server.rs` example becomes a candidate for a third
   backend choice (`LFM2_AUDIO_BACKEND=llamacpp`) once nodes land.
7. Document GGUF paths and the install procedure for the runner binary
   (`cargo install --path crates/liquid-audio-runner`) in `docs/`.

## CI matrix

| Job                                                             | Platform       | Purpose                                                                |
|-----------------------------------------------------------------|----------------|------------------------------------------------------------------------|
| `cargo build` (no features)                                      | Linux, macOS   | No liquid pieces present; baseline                                     |
| `cargo build --features llama-cpp`                                | Linux, macOS   | Stock llama.cpp only                                                   |
| `cargo build --features llama-cpp-liquid-audio`                   | Linux, macOS   | Liquid types in core; runner builds separately                         |
| `cargo build -p liquid-audio-runner --release --features cuda`    | Linux          | Runner with CUDA                                                       |
| `cargo build --features llama-cpp-all`                            | Linux, macOS   | Both stacks enabled (in different binaries)                            |
| `cargo test --features llama-cpp,llama-cpp-liquid-audio` smoke    | Linux, macOS   | Round-trips ASR + TTS through the runner with mocked GGUFs             |
| `cargo test` runner-respawn-after-kill                            | Linux          | `kill -9` the runner; next chunk produces clean error + respawn        |
| Microbenchmark: IPC chunk overhead                                | Linux          | Verifies Risk 2's single-digit-µs claim                                |
| `cargo test` macOS-multiprocess-smoke                             | macOS          | Verifies iceoryx2 macOS still works (gates Risk 7)                     |

## Success Criteria

- `cargo build --release -p liquid-audio-runner --features cuda` succeeds on
  Linux with the SHA-pinned PR tree.
- `cargo test -p remotemedia-core --features llama-cpp,llama-cpp-liquid-audio`
  passes on Linux + macOS, including a smoke test that round-trips audio
  through ASR and TTS via the runner.
- A pipeline manifest `MicInput → LiquidASR → LlamaGeneration → LiquidTTS →
  SpeakerOutput` constructs, runs Phase-2 capability resolution against
  values discovered from the runner's `RunnerCapabilities` event, runs
  end-to-end, and produces audible TTS output.
- No regression in `--features llama-cpp` or `--features llama-cpp-cuda`
  builds. No symbol collisions are even possible: the two stacks live in
  different processes.
- The Python multiprocess executor's tests still pass after the
  `SpawnTarget` refactor (no behavioral change for existing nodes).
- Killing the liquid runner from outside (`kill -9`) results in a clean
  per-chunk error and a successful respawn on the next request.
- IPC chunk-overhead microbenchmark reports < 100 µs on commodity hardware.

## Non-Goals (restated)

- **Windows.** iceoryx2's Windows transport is gated behind a non-default
  feature flag and is not exercised by the existing Python multiprocess
  integration. Windows support for the liquid runner is filed as a
  follow-up.
- **In-process linking** of stock + liquid llama.cpp. See "Supersedes" at
  the top.
- **Streaming TTS partials** (intra-utterance). See Risk 3.
- **Auto-downloading GGUFs.** See Risk 6.

## References

- [ggml-org/llama.cpp PR #18641 (LFM2Audio)](https://github.com/ggml-org/llama.cpp/pull/18641)
- [LiquidAI/LFM2.5-Audio-1.5B-GGUF](https://huggingface.co/LiquidAI/LFM2.5-Audio-1.5B-GGUF)
- [eugenehp/llama-cpp-rs (current `llama-cpp-4` source)](https://github.com/eugenehp/llama-cpp-rs)
- Existing multiprocess infra: `crates/core/src/python/multiprocess/multiprocess_executor.rs`
- Existing IPC binary format: `crates/core/src/python/multiprocess/data_transfer.rs`
- Existing prior art (Python-backed LFM2): `crates/transports/webrtc/examples/lfm2_audio_webrtc_server.rs`
- Withdrawn v1 (in-process FFI link with symbol rename): see git history at commit `491632e`
