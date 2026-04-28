# LFM2Audio (Liquid Audio) Multiprocess Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add llama.cpp-backed LFM2Audio ASR/TTS pipeline nodes that run their inference in a dedicated subprocess linked against a SHA-pinned copy of [ggml-org/llama.cpp PR #18641](https://github.com/ggml-org/llama.cpp/pull/18641), while keeping the existing in-process `llama-cpp-4 = 0.2.13` integration untouched.

**Architecture:** A new `remotemedia-liquid-audio-runner` binary loads four LFM2Audio GGUFs and serves ASR/TTS requests over iceoryx2 (sibling protocol on `{session_id}_liquid_{runner_id}_input/output` channels). Two pipeline-side nodes (`LlamaCppLiquidASRNode`, `LlamaCppLiquidTTSNode`) share one runner per session via a `Weak`-ref registry inside `SessionState`. `RuntimeDiscovered` capabilities resolve from a `RunnerCapabilities` event the runner emits before `READY`.

**Tech Stack:** Rust 1.87+, llama.cpp (PR fork, vendored), bindgen, cmake-rs, iceoryx2 0.8 (existing), tokio, serde (types only). No new dependencies in core's runtime path.

**Spec:** `docs/superpowers/specs/2026-04-28-llama-cpp-liquid-audio-design.md`

---

## File Structure

| Path | Responsibility |
|------|----------------|
| `crates/llama-cpp-liquid-sys/Cargo.toml` | New `-sys` crate manifest, features `cuda`/`metal`/`vulkan` |
| `crates/llama-cpp-liquid-sys/llama.cpp.SHA` | One-line SHA pin of PR #18641 commit |
| `crates/llama-cpp-liquid-sys/llama.cpp/` | Vendored llama.cpp PR tree (plain files, not submodule) |
| `crates/llama-cpp-liquid-sys/build.rs` | cmake-rs invocation + bindgen against fork headers |
| `crates/llama-cpp-liquid-sys/wrapper.h` | bindgen entry-point header |
| `crates/llama-cpp-liquid-sys/src/lib.rs` | `include!(concat!(env!("OUT_DIR"), "/bindings.rs"))` |
| `crates/llama-cpp-liquid/Cargo.toml` | Safe-wrapper crate manifest, `types` and full features |
| `crates/llama-cpp-liquid/src/lib.rs` | Re-exports |
| `crates/llama-cpp-liquid/src/ipc.rs` | `LiquidIpcCommand` / `LiquidIpcEvent` enums + frame codec (no_std-friendly, used by both runner and core) |
| `crates/llama-cpp-liquid/src/model.rs` | `LiquidModel` (only when full features enabled) |
| `crates/llama-cpp-liquid/src/context.rs` | `LiquidContext`, `LiquidDecodeContext` |
| `crates/llama-cpp-liquid/src/audio.rs` | mmproj / vocoder / speaker-tokenizer helpers |
| `crates/liquid-audio-runner/Cargo.toml` | `[[bin]] name = "remotemedia-liquid-audio-runner"` |
| `crates/liquid-audio-runner/src/main.rs` | CLI parsing, GGUF loads, iceoryx2 main loop, READY emission |
| `crates/liquid-audio-runner/src/asr.rs` | Audio→text inference path |
| `crates/liquid-audio-runner/src/tts.rs` | Text→audio inference path |
| `crates/liquid-audio-runner/src/heartbeat.rs` | macOS parent-death fallback |
| `crates/core/src/python/multiprocess/process_manager.rs:170-340` | **Modify** — extract `SpawnTarget::{Python, Binary}` |
| `crates/core/src/nodes/llama_cpp/liquid_audio/mod.rs` | Module entry point |
| `crates/core/src/nodes/llama_cpp/liquid_audio/config.rs` | `LiquidAudioConfig` / ASR / TTS configs |
| `crates/core/src/nodes/llama_cpp/liquid_audio/runner.rs` | `LiquidAudioRunner` (process lifecycle + IPC client + per-node mpsc demux) |
| `crates/core/src/nodes/llama_cpp/liquid_audio/registry.rs` | `LiquidRunnerRegistry` (per-session, `Weak` refs, lock-ordered spawn) |
| `crates/core/src/nodes/llama_cpp/liquid_audio/discovery.rs` | Runner-binary path resolution |
| `crates/core/src/nodes/llama_cpp/liquid_audio/asr.rs` | `LlamaCppLiquidASRNode` + factory |
| `crates/core/src/nodes/llama_cpp/liquid_audio/tts.rs` | `LlamaCppLiquidTTSNode` + factory |
| `crates/core/src/nodes/llama_cpp/liquid_audio/factory.rs` | `LiquidAudioNodesProvider` (inventory) |
| `crates/core/src/nodes/llama_cpp/liquid_audio/test_support.rs` | `MockRunner` (used by M5/M6 tests; behind `#[cfg(test)]` or `#[cfg(feature = "test-support")]`) |
| `crates/libs/pipeline-runner/src/session.rs:67` | **Modify** — add `liquid_runners: LiquidRunnerRegistry` field to `PipelineSession`; tear down in `Drop` |
| `crates/core/tests/fixtures/liquid_audio/` | Tiny stub GGUFs (or download script) used by M3.1 / M4.1 acceptance tests |
| `crates/core/src/nodes/llama_cpp/mod.rs:75-82` | **Modify** — add `liquid_audio` module gate |
| `crates/core/Cargo.toml:55-180` | **Modify** — add `llama-cpp-liquid-audio` feature + dep |
| `Cargo.toml:149-152` | **Modify** — add liquid sibling crates to workspace |
| `crates/core/examples/liquid_audio_smoke.rs` | End-to-end smoke example |
| `crates/core/benches/liquid_audio_ipc.rs` | IPC chunk-overhead microbenchmark |

---

## Milestone Map

| M | Scope | Gate |
|---|-------|------|
| M0 | `SpawnTarget` refactor in `process_manager.rs` | Existing Python tests pass; no behavior change |
| M1 | `llama-cpp-liquid` IPC types + frame codec (pure-Rust, no FFI) | Codec round-trip tests pass |
| M2 | `llama-cpp-liquid-sys` vendored + builds standalone | `cargo build -p llama-cpp-liquid-sys` succeeds; bindgen emits expected symbols |
| M3 | `llama-cpp-liquid` safe wrapper | Loads a real GGUF in a unit test |
| M4 | `liquid-audio-runner` binary | `READY → Capabilities → AudioChunk → TextResult` round-trip with fixture |
| M5 | `core` registry + runner client | Unit tests cover lock ordering, `Weak` refs, demux |
| M6 | `LlamaCppLiquidASRNode` + `LlamaCppLiquidTTSNode` | Integration test wires both nodes through one runner |
| M7 | Example, benchmark, docs | `cargo run --example liquid_audio_smoke` works end-to-end |

Each milestone ends with a green `cargo test` (or, for M2, a green `cargo build`) and a commit. Milestones may be parallelized once M0-M2 land.

## Fixture / test-double strategy (read before starting M3 or M4)

Several tasks (M3.1 happy-path GGUF load, M3.2 family, M4.1+ runner tests) need *something* to run inference against. **Two-tier strategy:**

- **Tier 1 — Synthetic fixtures.** A stub GGUF (`crates/core/tests/fixtures/liquid_audio/tiny.gguf`) generated by a one-shot Python helper using `gguf-py` to emit a 0-parameter model with valid metadata. Sufficient for "load + close" tests and for runner READY-handshake testing without actual inference. Add to repo (size < 1 MB).
- **Tier 2 — Real GGUFs.** Tests that need actual inference results (full ASR/TTS round-trip) read paths from env vars (`LIQUID_TEST_MODEL_GGUF`, `LIQUID_TEST_MMPROJ_GGUF`, etc.) and *skip via `#[ignore]`* if any path is missing or the file is absent. Document the env-var contract once in `crates/core/tests/fixtures/liquid_audio/README.md`; reuse it across M3 and M4 tests. CI sets these env vars only on machines that have the GGUFs cached (best-effort), and the `kill -9` test runs against Tier 1 because it doesn't need a working ASR/TTS — only a runner that emits READY.

Add a single helper `tests::skip_if_no_real_gguf!()` macro that bails out cleanly if env vars are unset, so individual tests don't repeat the env-var-checking boilerplate.

The fixture creation itself is **Task M2.0** (added below) so it's available before M3.

---

## M0 — `SpawnTarget` refactor

The existing `process_manager.rs` hardcodes `python -m remotemedia.core.multiprocessing.runner`. We extract a `SpawnTarget` enum so the same lifecycle code can spawn a Rust binary. **No behavior change for existing Python nodes.**

### Task M0.1: Add `SpawnTarget` enum (failing test first)

**Files:**
- Modify: `crates/core/src/python/multiprocess/process_manager.rs` (around line 170)
- Test: `crates/core/src/python/multiprocess/process_manager.rs` (inline `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write failing test for `SpawnTarget::Python` default**

```rust
#[cfg(test)]
mod spawn_target_tests {
    use super::*;

    #[test]
    fn spawn_target_python_default_matches_legacy_config() {
        let target = SpawnTarget::default();
        match target {
            SpawnTarget::Python { ref executable, .. } => {
                assert_eq!(executable, &std::path::PathBuf::from("python"));
            }
            _ => panic!("default should be Python"),
        }
    }
}
```

- [ ] **Step 2: Run test, verify it fails**

```bash
cargo test -p remotemedia-core --lib spawn_target -- --nocapture
```
Expected: `error[E0412]: cannot find type 'SpawnTarget'`

- [ ] **Step 3: Add the enum next to `SpawnConfig`**

```rust
#[derive(Debug, Clone)]
pub enum SpawnTarget {
    Python {
        executable: std::path::PathBuf,
        python_path: Vec<std::path::PathBuf>,
        register_modules: Vec<String>,
        node_module: String, // "remotemedia.core.multiprocessing.runner"
    },
    Binary {
        executable: std::path::PathBuf,
        argv: Vec<String>,
    },
}

impl Default for SpawnTarget {
    fn default() -> Self {
        SpawnTarget::Python {
            executable: std::path::PathBuf::from("python"),
            python_path: Vec::new(),
            register_modules: Vec::new(),
            node_module: "remotemedia.core.multiprocessing.runner".to_string(),
        }
    }
}
```

- [ ] **Step 4: Run test, verify it passes**

```bash
cargo test -p remotemedia-core --lib spawn_target
```
Expected: 1 passed

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/python/multiprocess/process_manager.rs
git commit -m "refactor: add SpawnTarget enum (no behavior change)"
```

### Task M0.2: Route `spawn_node` through `SpawnTarget`

**Files:**
- Modify: `crates/core/src/python/multiprocess/process_manager.rs:280-380` (`spawn_node` method)

- [ ] **Step 1: Add a regression test that pins the existing Python spawn behavior**

```rust
#[tokio::test]
async fn spawn_node_python_uses_module_invocation() {
    // Don't actually spawn; intercept Command construction.
    // Use a helper that returns the prepared Command without spawning.
    let mgr = ProcessManager::new(MultiprocessConfig::default());
    let cmd = mgr.build_spawn_command(
        &SpawnTarget::default(),
        "TestNode",
        "node-1",
        "session-1",
        &serde_json::json!({}),
    );
    let program = cmd.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(program, "python");
    assert!(args.contains(&"-m".to_string()));
    assert!(args.contains(&"remotemedia.core.multiprocessing.runner".to_string()));
    assert!(args.contains(&"--node-type".to_string()));
}
```

This forces extraction of a `build_spawn_command` helper; the helper is what we'll match `SpawnTarget` inside.

- [ ] **Step 2: Run test, verify it fails**

Expected: `error[E0599]: no method named 'build_spawn_command'`.

- [ ] **Step 3: Extract `build_spawn_command(&self, target, node_type, node_id, session_id, params) -> Command`**

Move the Command construction out of `spawn_node` into this helper. Match on `&SpawnTarget`:
- `Python { executable, python_path, register_modules, node_module }` → produce the existing `python -m <module> --node-type ...` invocation.
- `Binary { executable, argv }` → `Command::new(executable); cmd.args(argv);`.

`spawn_node` retains all the Stdio/pre_exec/process_group plumbing exactly as today; only the program-and-argv selection is delegated.

- [ ] **Step 4: Run regression test plus existing multiprocess tests**

```bash
cargo test -p remotemedia-core --lib --features multiprocess
cargo test -p remotemedia-core --test '*multiprocess*' --features multiprocess
```
Expected: all passing.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/python/multiprocess/process_manager.rs
git commit -m "refactor: route ProcessManager::spawn_node through SpawnTarget"
```

### Task M0.3: Add `SpawnTarget::Binary` smoke test

**Files:**
- Test: `crates/core/tests/integration/test_spawn_target_binary.rs` (new)

- [ ] **Step 1: Write a test that spawns `/bin/sh -c 'echo READY'` via `SpawnTarget::Binary`**

```rust
#[tokio::test]
#[cfg(unix)]
async fn spawn_target_binary_runs_arbitrary_command() {
    let mgr = ProcessManager::new(MultiprocessConfig::default());
    let target = SpawnTarget::Binary {
        executable: std::path::PathBuf::from("/bin/sh"),
        argv: vec!["-c".into(), "echo READY".into()],
    };
    let handle = mgr
        .spawn_node_with_target(target, "Echo", "echo-1", "session-1", &serde_json::json!({}))
        .await
        .expect("spawn");
    let stdout = handle.wait_with_stdout().await.expect("wait");
    assert!(stdout.contains("READY"));
}
```

- [ ] **Step 2: Run test, verify it fails**

Expected: `no method named 'spawn_node_with_target'`.

- [ ] **Step 3: Add `spawn_node_with_target` taking a `SpawnTarget`. Keep `spawn_node` as a Python-target convenience wrapper.**

- [ ] **Step 4: Run test, verify it passes on Linux**

```bash
cargo test -p remotemedia-core --test integration -- spawn_target_binary
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/python/multiprocess/process_manager.rs crates/core/tests/integration/test_spawn_target_binary.rs
git commit -m "feat: ProcessManager::spawn_node_with_target for arbitrary binaries"
```

---

## M1 — `llama-cpp-liquid` IPC types + codec (pure Rust)

Build the wire-format codec **before** any FFI work. It has no llama.cpp dependency, so we can land and test it in isolation. Both the runner and `core` consume it.

### Task M1.1: Scaffold `llama-cpp-liquid` crate with `types` feature only

**Files:**
- Create: `crates/llama-cpp-liquid/Cargo.toml`
- Create: `crates/llama-cpp-liquid/src/lib.rs`
- Modify: `Cargo.toml:149-152` (workspace `[workspace.dependencies]`)

- [ ] **Step 1: Add to workspace `Cargo.toml`**

```toml
[workspace.dependencies]
# ... existing entries ...
llama-cpp-liquid     = { path = "crates/llama-cpp-liquid", default-features = false }
llama-cpp-liquid-sys = { path = "crates/llama-cpp-liquid-sys", default-features = false }
```

Also add to `[workspace] members`.

- [ ] **Step 2: Create `crates/llama-cpp-liquid/Cargo.toml`**

```toml
[package]
name = "llama-cpp-liquid"
version = "0.1.0"
edition = "2021"
rust-version.workspace = true
description = "Safe wrapper for llama.cpp PR #18641 (LFM2Audio). IPC types are usable without FFI."
publish = false

[features]
default = ["full"]
# Just the IPC types and codec — no FFI, no llama.cpp linkage.
types = []
# Full safe wrapper, requires llama-cpp-liquid-sys.
full  = ["dep:llama-cpp-liquid-sys", "types"]
cuda  = ["full", "llama-cpp-liquid-sys/cuda"]

[dependencies]
llama-cpp-liquid-sys = { workspace = true, optional = true }
thiserror = { workspace = true }
tracing   = { workspace = true }
```

- [ ] **Step 3: Create `crates/llama-cpp-liquid/src/lib.rs`**

```rust
//! llama.cpp PR #18641 (LFM2Audio) Rust bindings.
//!
//! Two flavors:
//! - `types` (default-off in core consumers): IPC enums + codec, pure Rust.
//! - `full` (runner-side): adds the safe wrapper over llama-cpp-liquid-sys.

pub mod ipc;

#[cfg(feature = "full")]
pub mod model;
#[cfg(feature = "full")]
pub mod context;
#[cfg(feature = "full")]
pub mod audio;
```

- [ ] **Step 4: Verify it compiles with `types`-only**

```bash
cargo build -p llama-cpp-liquid --no-default-features --features types
```
Expected: succeeds (the `ipc` module doesn't exist yet, so this will fail with a missing-module error — that is the intended failing test for the next task).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/llama-cpp-liquid/Cargo.toml crates/llama-cpp-liquid/src/lib.rs
git commit -m "feat: scaffold llama-cpp-liquid crate"
```

### Task M1.2: IPC enum + codec round-trip

**Files:**
- Create: `crates/llama-cpp-liquid/src/ipc.rs`
- Test: same file (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the codec round-trip test first**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_chunk_round_trips() {
        let cmd = LiquidIpcCommand::AudioChunk {
            corr_id: 0xDEADBEEF,
            node_id: "asr-1".to_string(),
            samples: vec![0.0, 0.5, -0.5, 1.0],
        };
        let bytes = cmd.encode();
        let decoded = LiquidIpcCommand::decode(&bytes).expect("decode");
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn capabilities_event_round_trips() {
        let evt = LiquidIpcEvent::RunnerCapabilities {
            corr_id: 0,
            asr_rate: 16000,
            tts_rate: 24000,
            n_channels: 1,
            asr_format: SampleFormat::F32,
            tts_format: SampleFormat::F32,
        };
        let bytes = evt.encode();
        let decoded = LiquidIpcEvent::decode(&bytes).expect("decode");
        assert_eq!(decoded, evt);
    }

    #[test]
    fn version_byte_mismatch_is_rejected() {
        let mut bytes = vec![0u8; 16];
        bytes[0] = 99; // wrong version
        assert!(LiquidIpcCommand::decode(&bytes).is_err());
    }

    #[test]
    fn unknown_kind_is_rejected() {
        let bytes = vec![1u8, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ver=1, kind=0xFF
        assert!(LiquidIpcCommand::decode(&bytes).is_err());
    }
}
```

- [ ] **Step 2: Run test, verify it fails**

```bash
cargo test -p llama-cpp-liquid --no-default-features --features types
```
Expected: compile errors (types not defined).

- [ ] **Step 3: Implement the codec per spec §"IPC protocol"**

Frame layout: `ver:u8(=1) | kind:u8 | corr_id:u64_le | nid_len:u16_le | nid_utf8 | payload`.

```rust
use thiserror::Error;

pub const PROTO_VERSION: u8 = 1;

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("frame too short")]
    Truncated,
    #[error("unsupported protocol version: {0}")]
    BadVersion(u8),
    #[error("unknown kind: 0x{0:02x}")]
    UnknownKind(u8),
    #[error("invalid utf-8 in node_id")]
    BadNodeId,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat { F32 = 0, I16 = 1 }

#[derive(Debug, Clone, PartialEq)]
pub enum LiquidIpcCommand {
    AudioChunk { corr_id: u64, node_id: String, samples: Vec<f32> },
    TextUtterance { corr_id: u64, node_id: String, text: String },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiquidIpcEvent {
    RunnerCapabilities {
        corr_id: u64,
        asr_rate: u32, tts_rate: u32, n_channels: u16,
        asr_format: SampleFormat, tts_format: SampleFormat,
    },
    TextResult { corr_id: u64, node_id: String, partial: bool, text: String },
    AudioResult { corr_id: u64, node_id: String, sample_rate: u32, samples: Vec<f32> },
    Error { corr_id: u64, node_id: String, message: String },
}

impl LiquidIpcCommand {
    pub fn encode(&self) -> Vec<u8> { /* ... */ }
    pub fn decode(buf: &[u8]) -> Result<Self, CodecError> { /* ... */ }
}
// (analogous impl for LiquidIpcEvent)
```

Discriminator constants: `0x01 AudioChunk`, `0x02 TextUtterance`, `0x03 Shutdown`, `0x80 RunnerCapabilities`, `0x81 TextResult`, `0x82 AudioResult`, `0x83 Error`.

- [ ] **Step 4: Run test, verify all pass**

```bash
cargo test -p llama-cpp-liquid --no-default-features --features types
```
Expected: 4 passed.

- [ ] **Step 4b: Add tests for protocol-version byte position and corr_id correlation**

```rust
#[test]
fn version_byte_lives_at_offset_zero() {
    let cmd = LiquidIpcCommand::Shutdown;
    let bytes = cmd.encode();
    assert_eq!(bytes[0], PROTO_VERSION);
}

#[test]
fn corr_id_round_trips_at_offset_two() {
    let cmd = LiquidIpcCommand::AudioChunk {
        corr_id: 0x0102030405060708,
        node_id: "n".into(),
        samples: vec![],
    };
    let bytes = cmd.encode();
    // ver(1) + kind(1) + corr_id_le(8) starting at offset 2
    assert_eq!(&bytes[2..10], &0x0102030405060708u64.to_le_bytes());
}

#[test]
fn corr_id_correlation_two_overlapping_chunks() {
    // Simulate runner echoing TextResults with matching corr_ids.
    let chunk_a = LiquidIpcCommand::AudioChunk { corr_id: 1, node_id: "asr".into(), samples: vec![0.0] };
    let chunk_b = LiquidIpcCommand::AudioChunk { corr_id: 2, node_id: "asr".into(), samples: vec![0.0] };
    // Encode + decode (the encoder/decoder is the codec under test;
    // a fuller test in M5.3 verifies the demux behavior under real IPC).
    for cmd in [chunk_a.clone(), chunk_b.clone()] {
        let decoded = LiquidIpcCommand::decode(&cmd.encode()).unwrap();
        match (decoded, cmd) {
            (LiquidIpcCommand::AudioChunk { corr_id: a, .. },
             LiquidIpcCommand::AudioChunk { corr_id: b, .. }) => assert_eq!(a, b),
            _ => panic!(),
        }
    }
}
```

Locks the wire-format invariants the spec calls out.

- [ ] **Step 5: Add proptest for arbitrary frames (optional, recommended)**

```rust
proptest! {
    #[test]
    fn audio_chunk_roundtrip_arbitrary(
        corr_id in any::<u64>(),
        node_id in "[a-z0-9_-]{1,64}",
        samples in proptest::collection::vec(any::<f32>(), 0..1024),
    ) {
        let cmd = LiquidIpcCommand::AudioChunk { corr_id, node_id: node_id.clone(), samples: samples.clone() };
        let decoded = LiquidIpcCommand::decode(&cmd.encode()).unwrap();
        prop_assert_eq!(decoded, LiquidIpcCommand::AudioChunk { corr_id, node_id, samples });
    }
}
```

- [ ] **Step 6: Commit**

```bash
git add crates/llama-cpp-liquid/src/ipc.rs
git commit -m "feat: liquid-audio IPC codec with version + correlation ids"
```

---

## M2 — `llama-cpp-liquid-sys` (vendored fork, no rename pipeline)

This crate compiles the PR-pinned llama.cpp tree and runs bindgen. **Not** linked into anything except the runner — no symbol-rename pipeline.

### Task M2.0: Generate the tiny synthetic GGUF fixture

**Files:**
- Create: `crates/core/tests/fixtures/liquid_audio/gen_tiny_gguf.py` (one-shot helper)
- Create: `crates/core/tests/fixtures/liquid_audio/tiny.gguf` (committed binary, < 1 MB)
- Create: `crates/core/tests/fixtures/liquid_audio/README.md` (env-var contract)

- [ ] **Step 1:** Write `gen_tiny_gguf.py` using `gguf` Python package — emit a 0-parameter model with `general.architecture=lfm2` metadata and the four header sections LFM2Audio expects (model, mmproj, vocoder, speaker — separate files if necessary). Goal: load-only validity, no inference.
- [ ] **Step 2:** Run the helper, commit the resulting `.gguf` files. Document size budget (each < 1 MB; total < 4 MB). If size budget is exceeded, switch to "fixture is downloaded by a script, not committed."
- [ ] **Step 3:** Write `README.md` enumerating the env-var contract: `LIQUID_TEST_MODEL_GGUF`, `LIQUID_TEST_MMPROJ_GGUF`, `LIQUID_TEST_VOCODER_GGUF`, `LIQUID_TEST_SPEAKER_GGUF`. Document the `skip_if_no_real_gguf!()` macro pattern (defined in M5.0).
- [ ] **Step 4:** Commit.

```bash
git add crates/core/tests/fixtures/liquid_audio/
git commit -m "test: add synthetic LFM2Audio GGUF fixtures + env-var contract"
```

### Task M2.1: Scaffold `-sys` crate

**Files:**
- Create: `crates/llama-cpp-liquid-sys/Cargo.toml`
- Create: `crates/llama-cpp-liquid-sys/src/lib.rs`
- Create: `crates/llama-cpp-liquid-sys/wrapper.h`
- Create: `crates/llama-cpp-liquid-sys/llama.cpp.SHA` (placeholder)

- [ ] **Step 1: Pick the PR commit SHA**

Inspect <https://github.com/ggml-org/llama.cpp/pull/18641> commits, pick the latest tip SHA, write to `llama.cpp.SHA`:

```
# crates/llama-cpp-liquid-sys/llama.cpp.SHA
<40-char-sha-here>
```

- [ ] **Step 2: Vendor the tree**

```bash
mkdir -p crates/llama-cpp-liquid-sys/llama.cpp
git clone --depth 1 --branch <pr-branch> https://github.com/<pr-author>/llama.cpp /tmp/llama-pr
( cd /tmp/llama-pr && git checkout <SHA> )
rsync -a --exclude='.git/' /tmp/llama-pr/ crates/llama-cpp-liquid-sys/llama.cpp/
```

Verify `git status` shows the vendored tree as new files. Add `crates/llama-cpp-liquid-sys/llama.cpp/.git*` to `.gitignore` if anything snuck in.

- [ ] **Step 3: Write Cargo.toml**

```toml
[package]
name = "llama-cpp-liquid-sys"
version = "0.1.0"
edition = "2021"
rust-version.workspace = true
build = "build.rs"
links = "llama_liquid"
description = "Low-level bindings for llama.cpp PR #18641 (LFM2Audio)."
publish = false

[features]
default = []
cuda    = []
metal   = []
vulkan  = []

[build-dependencies]
bindgen = "0.69"
cmake   = "0.1"
```

- [ ] **Step 4: Write `wrapper.h`**

```c
#include "llama.h"
#include "ggml.h"
#include "common/common.h"
/* PR-specific headers as confirmed during impl: */
#include "mtmd.h"      /* multimodal projector */
#include "vocoder.h"   /* if exists; otherwise drop */
```

(Confirm exact header names by inspecting the vendored tree.)

- [ ] **Step 5: Write `src/lib.rs`**

```rust
#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
```

- [ ] **Step 6: Commit (placeholder build.rs)**

Stub `build.rs` to just verify the SHA file:

```rust
fn main() {
    let sha = std::fs::read_to_string("llama.cpp.SHA").expect("llama.cpp.SHA missing");
    let sha = sha.trim();
    assert_eq!(sha.len(), 40, "llama.cpp.SHA must be a full git SHA");
    println!("cargo:rerun-if-changed=llama.cpp.SHA");
    panic!("real build.rs not implemented yet (M2.2)");
}
```

```bash
git add crates/llama-cpp-liquid-sys/
git commit -m "feat: scaffold llama-cpp-liquid-sys (PR #18641 vendored)"
```

### Task M2.2: Real `build.rs` with cmake + bindgen

**Files:**
- Modify: `crates/llama-cpp-liquid-sys/build.rs`

- [ ] **Step 1: Write a build-acceptance shell test**

`crates/llama-cpp-liquid-sys/tests/build_smoke.rs`:

```rust
#[test]
fn bindgen_emits_expected_symbols() {
    // The lib.rs `include!` will fail to compile if bindings.rs is absent
    // or doesn't expose llama_model_load_from_file. Just reference one symbol.
    let _ = unsafe { llama_cpp_liquid_sys::llama_model_default_params };
}
```

- [ ] **Step 2: Run, verify failing**

```bash
cargo test -p llama-cpp-liquid-sys
```
Expected: build.rs panics ("real build.rs not implemented yet").

- [ ] **Step 3: Implement `build.rs`**

```rust
use std::env;
use std::path::PathBuf;

fn main() {
    let sha = std::fs::read_to_string("llama.cpp.SHA").expect("llama.cpp.SHA");
    let sha = sha.trim();
    assert_eq!(sha.len(), 40, "llama.cpp.SHA must be a 40-char SHA");
    println!("cargo:rerun-if-changed=llama.cpp.SHA");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=build.rs");

    let llama_src = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("llama.cpp");

    let mut cfg = cmake::Config::new(&llama_src);
    cfg.define("BUILD_SHARED_LIBS", "OFF")
       .define("LLAMA_BUILD_TESTS", "OFF")
       .define("LLAMA_BUILD_EXAMPLES", "OFF")
       .define("LLAMA_BUILD_SERVER", "OFF");

    if cfg!(feature = "cuda")   { cfg.define("GGML_CUDA",   "ON"); }
    if cfg!(feature = "metal")  { cfg.define("GGML_METAL",  "ON"); }
    if cfg!(feature = "vulkan") { cfg.define("GGML_VULKAN", "ON"); }

    let dst = cfg.build();
    let lib_dir = dst.join("build");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-search=native={}/src", lib_dir.display());
    println!("cargo:rustc-link-search=native={}/ggml/src", lib_dir.display());
    println!("cargo:rustc-link-lib=static=llama");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=common");

    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=dylib=c++");
        println!("cargo:rustc-link-lib=framework=Accelerate");
    } else {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}/include", llama_src.display()))
        .clang_arg(format!("-I{}/ggml/include", llama_src.display()))
        .clang_arg(format!("-I{}/common", llama_src.display()))
        .allowlist_function("llama_.*|ggml_.*|gguf_.*|mtmd_.*|common_.*")
        .allowlist_type("llama_.*|ggml_.*|gguf_.*|mtmd_.*")
        .generate()
        .expect("bindgen");

    bindings
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .expect("write bindings.rs");
}
```

Adjust archive names + include dirs once you observe the actual cmake output layout.

- [ ] **Step 4: Run, verify build succeeds**

```bash
cargo build -p llama-cpp-liquid-sys
cargo test  -p llama-cpp-liquid-sys
```
Expected: PASS. First build is slow (cmake compiles llama.cpp); subsequent are cached.

- [ ] **Step 5: Repeat with CUDA on a CUDA box**

```bash
cargo build -p llama-cpp-liquid-sys --features cuda
```
Expected: PASS on Linux + CUDA 12.x.

- [ ] **Step 6: Commit**

```bash
git add crates/llama-cpp-liquid-sys/build.rs crates/llama-cpp-liquid-sys/tests/build_smoke.rs
git commit -m "feat: build.rs for llama-cpp-liquid-sys (cmake + bindgen)"
```

---

## M3 — `llama-cpp-liquid` safe wrapper

Wraps the FFI surface enough to load GGUFs and run audio I/O. Mirrors `llama-cpp-4`'s patterns where convenient, but is a standalone API (no shared types).

> **Stop and verify before each task in this milestone:** PR #18641 may rebase between when this plan was written and when you implement. Before committing to specific FFI signatures, run `nm OUT_DIR/build/libllama.a | grep -E '<expected_symbol>'` to confirm the symbols below exist and have the assumed shape. If any pinned signature in this milestone disagrees with the bindings, **stop**, re-read the PR diff, and update the plan tasks before proceeding. The names below (`llama_model_load_from_file`, etc.) are *as of upstream master ~late 2025* and are best-guesses for the PR.

### Task M3.1: `LiquidModel::load_from_file` happy path

**Files:**
- Create: `crates/llama-cpp-liquid/src/model.rs`
- Test: same file

- [ ] **Step 1: Write a test that loads a tiny stub GGUF**

```rust
#[cfg(all(test, feature = "full"))]
mod tests {
    use super::*;

    #[test]
    fn load_tiny_gguf() {
        let path = std::env::var("LIQUID_TEST_MODEL_GGUF").expect("set LIQUID_TEST_MODEL_GGUF");
        let model = LiquidModel::load_from_file(&path, &LiquidModelParams::default()).unwrap();
        assert!(model.n_params() > 0);
    }
}
```

- [ ] **Step 2: Run, verify failing**

```bash
LIQUID_TEST_MODEL_GGUF=/path/to/tiny.gguf cargo test -p llama-cpp-liquid --features full
```
Expected: compile errors.

- [ ] **Step 3: Implement minimal wrapper**

```rust
use llama_cpp_liquid_sys as ffi;
use std::ffi::CString;
use std::path::Path;

pub struct LiquidModel {
    raw: *mut ffi::llama_model,
}
unsafe impl Send for LiquidModel {}
unsafe impl Sync for LiquidModel {}

#[derive(Debug, Clone, Default)]
pub struct LiquidModelParams {
    pub n_gpu_layers: i32,
    pub use_mmap: bool,
}

impl LiquidModel {
    pub fn load_from_file(path: impl AsRef<Path>, p: &LiquidModelParams) -> Result<Self, LiquidError> {
        let cpath = CString::new(path.as_ref().to_string_lossy().as_bytes()).map_err(|_| LiquidError::BadPath)?;
        let mut params = unsafe { ffi::llama_model_default_params() };
        params.n_gpu_layers = p.n_gpu_layers;
        params.use_mmap = p.use_mmap;
        let raw = unsafe { ffi::llama_model_load_from_file(cpath.as_ptr(), params) };
        if raw.is_null() { return Err(LiquidError::LoadFailed); }
        Ok(Self { raw })
    }
    pub fn n_params(&self) -> u64 { unsafe { ffi::llama_model_n_params(self.raw) } }
}

impl Drop for LiquidModel {
    fn drop(&mut self) { unsafe { ffi::llama_model_free(self.raw); } }
}
```

- [ ] **Step 4: Verify test passes against a real GGUF**

- [ ] **Step 5: Commit**

```bash
git add crates/llama-cpp-liquid/src/model.rs
git commit -m "feat: LiquidModel safe wrapper (load_from_file, drop)"
```

### Task M3.2: `LiquidContext` + `LiquidDecodeContext`

**Files:**
- Create: `crates/llama-cpp-liquid/src/context.rs`

- [ ] **Step 1:** Failing test: load a `LiquidModel`, create a `LiquidContext` with `n_ctx=512`, derive a `LiquidDecodeContext`. Assert `decode.n_ctx() == 512`. Use `skip_if_no_real_gguf!()` macro.
- [ ] **Step 2:** Run, verify failing.
- [ ] **Step 3:** Implement `LiquidContext::from_model(&LiquidModel, &LiquidContextParams)` and `LiquidDecodeContext` (per-stream KV cache wrapper), using `llama_new_context_with_model` / `llama_decode` family from bindings.
- [ ] **Step 4:** Run, verify pass.
- [ ] **Step 5:** Commit `feat: LiquidContext + LiquidDecodeContext`.

### Task M3.3: `LiquidMmproj` (audio encoder)

**Files:**
- Create: `crates/llama-cpp-liquid/src/audio.rs` (mmproj section)

- [ ] **Step 1:** Failing test: load a stub mmproj GGUF; assert encoder reports an input rate (`encoder.expected_sample_rate()`).
- [ ] **Step 2:** Run, verify failing.
- [ ] **Step 3:** Implement `LiquidMmproj::load(path)` and `encode(samples) -> Vec<f32>` using PR's mtmd entry points. **Pause at this task** to read PR #18641's `mtmd.h` and confirm the actual entry-point names; the names `mtmd_init`, `mtmd_encode_audio` are assumed.
- [ ] **Step 4:** Run, verify pass.
- [ ] **Step 5:** Commit `feat: LiquidMmproj load + encode`.

### Task M3.4: `LiquidVocoder` (TTS decoder)

**Files:**
- Modify: `crates/llama-cpp-liquid/src/audio.rs` (vocoder section)

- [ ] **Step 1:** Failing test: load stub vocoder GGUF; assert `vocoder.output_sample_rate() > 0`.
- [ ] **Step 2-5:** TDD impl + commit `feat: LiquidVocoder load + synth`.

### Task M3.5: `LiquidSpeakerTokenizer`

**Files:**
- Modify: `crates/llama-cpp-liquid/src/audio.rs` (speaker section)

- [ ] **Step 1:** Failing test: load stub speaker GGUF; tokenize a fixed speaker ID; assert non-empty output.
- [ ] **Step 2-5:** TDD impl + commit `feat: LiquidSpeakerTokenizer`.

---

## M4 — `liquid-audio-runner` binary

The subprocess. Reads commands from iceoryx2 input, dispatches to ASR or TTS, writes events to iceoryx2 output. Emits `RunnerCapabilities` then `READY` on the control channel.

### Task M4.1: Scaffold the binary crate, READY emission, no-op IPC loop

**Files:**
- Create: `crates/liquid-audio-runner/Cargo.toml`
- Create: `crates/liquid-audio-runner/src/main.rs`

- [ ] **Step 1: Cargo.toml**

```toml
[package]
name = "liquid-audio-runner"
version = "0.1.0"
edition = "2021"
publish = false

[[bin]]
name = "remotemedia-liquid-audio-runner"
path = "src/main.rs"

[features]
default = []
cuda = ["llama-cpp-liquid/cuda"]

[dependencies]
llama-cpp-liquid = { workspace = true, features = ["full"] }
iceoryx2 = { workspace = true }
clap     = { version = "4", features = ["derive"] }
tokio    = { workspace = true, features = ["macros", "rt-multi-thread", "signal"] }
tracing  = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow   = { workspace = true }
```

- [ ] **Step 2: Failing acceptance test (capabilities precede READY, channel-naming pinned)**

`crates/liquid-audio-runner/tests/ready_handshake.rs`:

```rust
#[tokio::test]
async fn runner_emits_capabilities_then_ready() {
    let session_id = "test-session";
    let runner_id  = "test-runner";

    // Pin the channel-naming convention from spec §"IPC protocol".
    // setup_iceoryx_pair must construct services named:
    //   {session_id}_liquid_{runner_id}_input
    //   {session_id}_liquid_{runner_id}_output
    let (tx, rx, ctrl_rx) = setup_iceoryx_pair(session_id, runner_id);
    assert_eq!(tx.service_name(), format!("{session_id}_liquid_{runner_id}_input"));
    assert_eq!(rx.service_name(), format!("{session_id}_liquid_{runner_id}_output"));

    let mut child = spawn_runner_for_test(session_id, runner_id, tier1_fixture_paths()).await;

    // Order must be: RunnerCapabilities, then READY on control channel.
    let evt = recv_event(&rx, Duration::from_secs(60)).await.expect("event");
    matches!(evt, LiquidIpcEvent::RunnerCapabilities { .. });
    let ready = wait_for_control_byte(&ctrl_rx, b"READY", Duration::from_secs(5))
        .await.expect("READY after capabilities");
    assert!(ready);

    child.kill().await.unwrap();
}
```

Locks both the spec's "capabilities before READY" ordering invariant and the iceoryx2 service-name convention.

- [ ] **Step 3: Implement enough of `main.rs` to load, emit `RunnerCapabilities` + `READY`, then idle on input**

- [ ] **Step 4: Run, verify PASS**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat: liquid-audio-runner READY handshake"
```

### Task M4.2: Shutdown command → clean exit

**Files:**
- Modify: `crates/liquid-audio-runner/src/main.rs`

- [ ] **Step 1:** Failing test that sends `LiquidIpcCommand::Shutdown`, asserts the runner exits with status 0 within 2 s.
- [ ] **Step 2-5:** TDD impl + commit.

### Task M4.3: `AudioChunk` → ASR → `TextResult`

**Files:**
- Create: `crates/liquid-audio-runner/src/asr.rs`
- Modify: `crates/liquid-audio-runner/src/main.rs` (dispatch table)

- [ ] **Step 1:** Failing acceptance test using **real** GGUFs (gated by `skip_if_no_real_gguf!`): send a fixed audio fixture, expect a non-empty `TextResult` with a matching `corr_id`.
- [ ] **Step 2:** Run, verify failing.
- [ ] **Step 3:** Implement `asr::process(ctx, mmproj, samples) -> String`, wire dispatch.
- [ ] **Step 4:** Run, verify pass.
- [ ] **Step 5:** Commit `feat: liquid-audio-runner ASR path`.

### Task M4.4: `TextUtterance` → TTS → `AudioResult`

**Files:**
- Create: `crates/liquid-audio-runner/src/tts.rs`

- [ ] **Step 1-5:** Same TDD pattern; assert the returned audio's `sample_rate` matches the vocoder's reported rate (Phase-2 capability invariant). Commit `feat: liquid-audio-runner TTS path`.

### Task M4.5: macOS heartbeat fallback

**Files:**
- Create: `crates/liquid-audio-runner/src/heartbeat.rs`
- Modify: `crates/liquid-audio-runner/src/main.rs`

- [ ] **Step 1:** Failing test (gated `#[cfg(target_os = "macos")]` or run universally for broader coverage): start runner, simulate parent silence for 30+ s, assert runner self-exits with status code 1.
- [ ] **Step 2:** Run, verify failing.
- [ ] **Step 3:** Implement: on macOS spawn a tokio task that sends a heartbeat ping over the control channel every 5 s; if no pong from parent within 30 s, exit cleanly. Parent-side pong is sent by the runner-client's IPC thread on each iteration. On Linux this task is a no-op (PR_SET_PDEATHSIG handles it).
- [ ] **Step 4:** Run, verify pass.
- [ ] **Step 5:** Commit `feat: liquid-audio-runner macOS heartbeat fallback`.

---

## M5 — Core registry + runner client

Pipeline-side glue. No llama.cpp linkage in this code — it only uses `llama-cpp-liquid` with the `types` feature.

### Task M5.0: `MockRunner` test double

**Files:**
- Create: `crates/core/src/nodes/llama_cpp/liquid_audio/test_support.rs`

- [ ] **Step 1:** Define `MockRunner` implementing the same trait as `LiquidAudioRunner` but driven by user-supplied closures. Provide a `MockRunner::spawn_count() -> usize` for the M5.2 race-test, scripted-response helpers (`expect_audio_chunk_then_send_text`), and a controllable `RunnerCapabilities`.
- [ ] **Step 2:** Mark `pub(crate)` and gate the module behind `#[cfg(any(test, feature = "test-support"))]`.
- [ ] **Step 3:** Add a smoke test that exercises `MockRunner` itself (sanity-check the mock).
- [ ] **Step 4:** Commit `test: MockRunner test double for liquid audio`.

### Task M5.1: Discovery

**Files:**
- Create: `crates/core/src/nodes/llama_cpp/liquid_audio/discovery.rs`
- Test: same file

- [ ] **Step 1: Failing tests** for the three precedence rules (env var, sibling-of-current-exe, `$PATH`).
- [ ] **Step 2: Run, verify failing.**
- [ ] **Step 3: Implement `resolve_runner_path() -> Result<PathBuf, LiquidAudioError>`** using `which::which`, `std::env::current_exe`, `std::env::var`.
- [ ] **Step 4: Verify pass.**
- [ ] **Step 5: Commit.**

### Task M5.2: `LiquidRunnerRegistry` lock-ordered spawn

**Files:**
- Create: `crates/core/src/nodes/llama_cpp/liquid_audio/registry.rs`

- [ ] **Step 1: Failing tests covering both the happy path and the Weak::upgrade race**

```rust
#[tokio::test]
async fn concurrent_get_or_spawn_yields_single_runner() {
    let reg = LiquidRunnerRegistry::<MockRunner>::new();
    let cfg = test_cfg();
    let (a, b) = tokio::join!(reg.get_or_spawn(&cfg), reg.get_or_spawn(&cfg));
    let a = a.unwrap(); let b = b.unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(MockRunner::spawn_count(), 1);
}

#[tokio::test]
async fn weak_upgrade_race_does_not_double_spawn() {
    // Spec §"Subprocess lifecycle" step 3: per-key mutex must serialize the
    // upgrade attempt and the spawn so two callers arriving after the
    // previous runner died still produce exactly one new spawn.
    let reg = LiquidRunnerRegistry::<MockRunner>::new();
    let cfg = test_cfg();

    let arc1 = reg.get_or_spawn(&cfg).await.unwrap();
    drop(arc1);                          // refcount → 0; Weak should fail to upgrade
    MockRunner::reset_spawn_count();

    // Two callers arrive after the drop; expect exactly ONE re-spawn.
    let (a, b) = tokio::join!(reg.get_or_spawn(&cfg), reg.get_or_spawn(&cfg));
    assert!(Arc::ptr_eq(&a.unwrap(), &b.unwrap()));
    assert_eq!(MockRunner::spawn_count(), 1, "TOCTOU race spawned twice");
}

#[tokio::test]
async fn lock_ordering_does_not_deadlock_under_load() {
    let reg = LiquidRunnerRegistry::<MockRunner>::new();
    let cfgs: Vec<_> = (0..16).map(|i| test_cfg_with_id(i)).collect();
    let futs: Vec<_> = cfgs.iter().map(|c| reg.get_or_spawn(c)).collect();
    let _ = tokio::time::timeout(Duration::from_secs(5), futures::future::join_all(futs))
        .await
        .expect("deadlock");
}
```

- [ ] **Step 2-5:** TDD impl + commit.

### Task M5.3: `LiquidAudioRunner` with per-node mpsc demux

- [ ] **Step 1: Failing test** that two simulated subscribers (`asr-1` + `tts-1`) receive only their own events from a stream of mixed events.
- [ ] **Step 2-5:** TDD impl. The demuxer is a single tokio task reading from iceoryx2 output, decoding `LiquidIpcEvent`, looking up the per-node `mpsc::Sender` from the `subscribers` map, and forwarding.

### Task M5.4: `PipelineSession` integration + termination

**Files:**
- Modify: `crates/libs/pipeline-runner/src/session.rs:67` (add `liquid_runners: LiquidRunnerRegistry<LiquidAudioRunner>` field, behind `#[cfg(feature = "llama-cpp-liquid-audio")]`).

- [ ] **Step 1: Failing test** that dropping a `PipelineSession` triggers `Shutdown` to all live runners and joins their IPC threads within 2 seconds.
- [ ] **Step 2:** Run, verify failing.
- [ ] **Step 3:** Add the field and implement `Drop` / `terminate` to walk `liquid_runners.inner` and send `Shutdown`. Use `MockRunner` in the test, gated on the feature.
- [ ] **Step 4-5:** Verify pass + commit.

### Task M5.5: `kill -9` runner respawn

**Files:**
- Test: `crates/core/tests/integration/test_liquid_runner_respawn.rs` (new)

- [ ] **Step 1: Failing integration test** using the **real** `liquid-audio-runner` binary plus Tier-1 stub GGUFs:

```rust
#[tokio::test]
#[cfg(all(unix, feature = "llama-cpp-liquid-audio"))]
async fn liquid_runner_respawns_after_kill_9() {
    let session = PipelineSession::new_for_test();
    let cfg = LiquidAudioConfig::for_tier1_fixtures();
    let runner1 = session.liquid_runners().get_or_spawn(&cfg).await.unwrap();
    let pid1 = runner1.process_pid();

    // Simulate runner crash.
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid1 as i32), nix::sys::signal::SIGKILL).unwrap();

    // Wait for IPC thread to observe channel closure and tear down.
    tokio::time::sleep(Duration::from_millis(500)).await;
    drop(runner1);

    // Next get_or_spawn must succeed with a different PID.
    let runner2 = session.liquid_runners().get_or_spawn(&cfg).await.unwrap();
    assert_ne!(runner2.process_pid(), pid1);

    // And the runner must respond to a Shutdown immediately.
    runner2.shutdown().await.unwrap();
}
```

- [ ] **Step 2:** Run, verify failing.
- [ ] **Step 3:** Implement `process_pid()` accessor; ensure `LiquidAudioRunner::Drop` cleanly handles "child already dead." Verify a fresh spawn happens on the next `get_or_spawn`.
- [ ] **Step 4-5:** Verify pass + commit `test: liquid runner respawns after kill -9`.

---

## M6 — Pipeline nodes

### Task M6.1: `LlamaCppLiquidASRNode`

- [ ] **Step 1: Failing tests** covering three behaviors:
  1. Wire a `MockRunner` returning a fixed `TextResult` for any `AudioChunk`; verify the node emits `RuntimeData::Text`.
  2. `initialize()` returns only after `MockRunner::set_capabilities(asr_rate=16000)` has fired; assert `actual_capabilities()` returns 16000 afterward.
  3. Phase-2 capability re-validation: build a graph `LiquidASR → DownstreamNode(requires text)` and assert resolver succeeds; build `LiquidTTS(actual_rate=24000) → AudioSink(requires sample_rate=8000)` and assert resolver returns the spec's "downstream cannot accept discovered rate" error fast.
- [ ] **Step 2-5:** Implement `process_streaming`, `initialize` (synchronizes on `RunnerCapabilities`), `potential_capabilities`, `actual_capabilities`. Commit.

### Task M6.2: `LlamaCppLiquidTTSNode`

Mirror of M6.1 for text→audio.

### Task M6.3: Factory + inventory registration

- [ ] **Step 1: Failing test** that `core::nodes::registry()` returns a factory for both `LlamaCppLiquidASR` and `LlamaCppLiquidTTS` when the feature is on.
- [ ] **Step 2-5:** Implement `LiquidAudioNodesProvider`, register via `inventory::submit!`, gate behind `#[cfg(feature = "llama-cpp-liquid-audio")]`.

### Task M6.4: Cargo features wired into `core` + `LiquidAudioConfig` finalized

**Files:**
- Modify: `crates/core/Cargo.toml`
- Modify: `crates/core/src/nodes/llama_cpp/liquid_audio/config.rs`

- [ ] Add `llama-cpp-liquid` optional dependency with `default-features = false, features = ["types"]`.
- [ ] Add `llama-cpp-liquid-audio` and `llama-cpp-liquid-audio-cuda` features per spec.
- [ ] Add `max_ipc_frame_bytes: u32` to `LiquidAudioConfig` (default `64 * 1024`, per spec §"IPC protocol"). Wire it through to runner spawn args (`--max-frame-bytes <N>`) and to the iceoryx2 sample-size config on both ends. Add a unit test asserting the default and a `with_max_ipc_frame_bytes(128 * 1024)` builder method works.
- [ ] Add `mpsc_capacity_per_node: usize` to `LiquidAudioConfig` (default 32, per spec §"`LiquidAudioRunner`" backpressure section). Plumbed into `LiquidAudioRunner::subscribe`.
- [ ] Verify `cargo build --features llama-cpp-liquid-audio` succeeds; commit.
- [ ] Verify `cargo build --features llama-cpp,llama-cpp-liquid-audio` succeeds (both stacks; no symbol clashes because the runner is a separate binary); commit.

---

## M7 — Example, benchmark, docs

### Task M7.1: Smoke example

**Files:**
- Create: `crates/core/examples/liquid_audio_smoke.rs`

- [ ] **Step 1:** Mirror `crates/core/examples/llama_cpp_chat_smoke.rs` structure.
- [ ] **Step 2:** Read GGUF paths from env vars (`LIQUID_MODEL_GGUF`, `LIQUID_MMPROJ_GGUF`, `LIQUID_VOCODER_GGUF`, `LIQUID_SPEAKER_GGUF`). Bail with a friendly message if missing.
- [ ] **Step 3:** Build a minimal `MicInput → LiquidASR → LiquidTTS → AudioFileSink` manifest, run for N seconds against a fixture WAV, write output WAV.
- [ ] **Step 4:** `cargo run --example liquid_audio_smoke` — verify it completes and the output WAV contains audio.
- [ ] **Step 5:** Commit.

### Task M7.2: IPC microbenchmark

**Files:**
- Create: `crates/core/benches/liquid_audio_ipc.rs`

- [ ] **Step 1:** Use a stub runner that echoes commands as no-op events. Measure round-trip latency for `AudioChunk` of 16 KiB.
- [ ] **Step 2:** Run on a Linux box; record results.
- [ ] **Step 3:** If > 100 µs P50, investigate. Commit results to `docs/`.
- [ ] **Step 4:** Commit.

### Task M7.3: Docs

- [ ] **Step 1:** Add `crates/llama-cpp-liquid-sys/README.md` with the SHA-bump procedure.
- [ ] **Step 2:** Add `crates/liquid-audio-runner/README.md` with install instructions (`cargo install --path crates/liquid-audio-runner`).
- [ ] **Step 3:** Add a section to `docs/` describing the LFM2Audio node config (four GGUF paths, feature flags).
- [ ] **Step 4:** Commit.

### Task M7.4: Optional — wire into `lfm2_audio_webrtc_server` example

- [ ] **Step 1:** Add `LFM2_AUDIO_BACKEND=llamacpp` branch alongside the existing `transformers` and `mlx` branches.
- [ ] **Step 2:** Verify the WebRTC example still works for the existing two backends; verify it works for the new one if GGUFs are available.
- [ ] **Step 3:** Commit.

### Task M7.5: CI matrix as concrete jobs

**Files:**
- Modify: `.github/workflows/<existing-workflow>.yml` (or create a new workflow file `liquid-audio.yml` if existing structure makes inserting easier)

- [ ] **Step 1:** Add jobs corresponding to spec §"CI matrix":
  - `cargo build` (no features) — Linux, macOS
  - `cargo build --features llama-cpp` — Linux, macOS
  - `cargo build --features llama-cpp-liquid-audio` — Linux, macOS
  - `cargo build -p liquid-audio-runner --release --features cuda` — Linux + CUDA runner
  - `cargo build --features llama-cpp-all` — Linux, macOS
  - `cargo test --features llama-cpp,llama-cpp-liquid-audio` smoke (Tier-1 fixtures only) — Linux, macOS
  - `cargo test --test test_liquid_runner_respawn` — Linux
  - macOS multiprocess smoke job (gates Risk 7) — macOS
  - IPC microbenchmark non-blocking job — Linux
- [ ] **Step 2:** Run CI on a draft PR; verify all jobs pass. If a job stalls (e.g. CUDA runner unavailable), gate it with `if: ${{ runner.gpu == 'cuda' }}` or document as manual-trigger-only.
- [ ] **Step 3:** Commit.

---

## Verification gates

Before declaring "done":

- [ ] `cargo build` (no features) — succeeds, no liquid pieces present.
- [ ] `cargo build --features llama-cpp` — stock only, unchanged.
- [ ] `cargo build --features llama-cpp-liquid-audio` — types in core, runner builds separately.
- [ ] `cargo build -p liquid-audio-runner --release --features cuda` — Linux CUDA box.
- [ ] `cargo build --features llama-cpp-all` — both stacks, two binaries.
- [ ] `cargo test --features llama-cpp,llama-cpp-liquid-audio` — full suite passes Linux + macOS.
- [ ] `cargo test --test '*spawn_target*'` — `SpawnTarget::Binary` round-trip.
- [ ] Existing Python multiprocess tests still green after M0.
- [ ] Runner-respawn-after-`kill -9` test green.
- [ ] IPC microbench < 100 µs P50.
- [ ] `cargo run --example liquid_audio_smoke` produces audible output WAV.

---

## Commit & PR hygiene

- One commit per task step that produces working code.
- Semantic commit prefixes: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:` (see `/home/acidhax/dev/personal/CLAUDE.md`).
- Do not commit the vendored llama.cpp tree under any `.git*` artifacts — verify with `git status` after vendoring.
- The vendored tree is a large one-time commit; mark with `chore: vendor llama.cpp PR #18641 at <sha>`.
- Do not bundle unrelated changes (lesson from the docs commit `491632e` — left alone per user direction, but don't repeat).

## Out-of-scope reminders

- No streaming TTS partials within an utterance.
- No Windows support.
- No GGUF auto-download.
- No replacement of the existing Python `LFM2AudioNode`.
- No in-process linking of stock + liquid llama.cpp.
