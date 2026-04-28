# LFM2Audio (Liquid Audio) Integration via a Parallel `llama-cpp-4` Stack

**Date:** 2026-04-28
**Status:** Draft (pending spec review + user approval)
**Owner:** Mathieu Gosbee

## Summary

Add LFM2Audio (Liquid AI's audio-in / audio-out extension to llama.cpp, currently
in [ggml-org/llama.cpp PR #18641](https://github.com/ggml-org/llama.cpp/pull/18641))
as in-process Rust nodes, while keeping the existing stock-llama.cpp integration
(`llama-cpp-4 = "0.2.13"`) intact. Both stacks must be linkable into the same
binary at the same time so a single pipeline can use stock llama.cpp text
generation **and** LFM2Audio ASR/TTS without IPC.

This document specifies a fork of `llama-cpp-sys-4` that vendors the PR branch at
a pinned commit, prefixes its symbols at build time via `objcopy --redefine-syms`
to avoid collisions with stock `libllama` / `libggml`, exposes a parallel safe
wrapper crate (`llama-cpp-liquid`), and adds two new streaming nodes
(`LlamaCppLiquidASRNode`, `LlamaCppLiquidTTSNode`) backed by a shared
`LiquidAudioContext`.

## Goals

- Enable an in-process Rust path to LFM2Audio ASR (audio → text) and TTS
  (text → audio) using the GGUFs published by Liquid AI.
- Allow stock `llama-cpp-4` and the liquid fork to be linked into the **same**
  binary simultaneously without symbol clashes.
- Keep the existing `llama-cpp` / `llama-cpp-cuda` features and node surface
  unchanged for current users — adoption is opt-in via a new feature flag.
- Pin the fork to a known-good commit SHA (vendored, not a submodule) so builds
  are reproducible.

## Non-Goals

- Streaming TTS partials at sub-utterance granularity (deferred; see Risk 5).
- Vulkan / Metal builds of the liquid stack in v1 (CUDA + CPU only).
- Auto-downloading GGUFs.
- Windows support in v1 (filed as a follow-up; `objcopy` rename pipeline on
  COFF requires `llvm-objcopy` and CUDA toolchain interactions are different).
- WASM builds of the liquid stack (existing WASM target is unaffected).
- Replacing the stock `llama-cpp-4` integration. The two stacks are siblings.

## Context

The repo currently integrates llama.cpp through `llama-cpp-4 = "0.2.13"` (the
eugenehp fork at github.com/eugenehp/llama-cpp-rs), which vendors llama.cpp as
plain files inside the `-sys` crate and links it statically via `build.rs`.
Existing nodes live in `crates/core/src/nodes/llama_cpp/` (generation, embedding,
activation, steer) and are gated by the `llama-cpp` cargo feature.

PR 18641 adds LFM2Audio support as a **standalone server**
(`llama-liquid-audio-server`) plus a CLI (`llama-liquid-audio-cli`). It bundles
audio-input via a multimodal projector (`mmproj-LFM2.5-Audio-1.5B-Q8_0.gguf`)
and audio-output via a vocoder (`vocoder-LFM2.5-Audio-1.5B-Q8_0.gguf`) plus a
speaker tokenizer (`tokenizer-LFM2.5-Audio-1.5B-Q8_0.gguf`). The PR is open and
has not landed; rebases / force-pushes are expected.

The user explicitly chose the in-process FFI path over a server-shelling path
because they want the integration to "work similarly to how llama.cpp
integration is expected" — i.e. as a Rust node loaded into the runtime, not as a
managed subprocess.

## Decisions (locked in brainstorming)

| # | Decision                                                                                  |
|---|-------------------------------------------------------------------------------------------|
| 1 | FFI integration in-process. No HTTP server shelling.                                      |
| 2 | Both flavors must be linkable into the same binary.                                       |
| 3 | Track the PR by pinning a specific commit SHA, vendored as plain files.                   |
| 4 | Resolve symbol clashes via `objcopy --redefine-syms` post-build (not source-level patch). |
| 5 | Two specialized nodes (`LiquidASR` + `LiquidTTS`) sharing a `LiquidAudioContext`.         |

## Architecture

### Crate Layout

Two parallel `-sys` crates feeding two parallel high-level crates, both
consumable from `remotemedia-core` at the same time:

```
crates/
├── llama-cpp-liquid-sys/          # NEW — vendored fork at PR 18641 SHA
│   ├── llama.cpp/                 #   vendored source (plain files)
│   ├── llama.cpp.SHA              #   one-line SHA pin (verified by build.rs)
│   ├── build.rs                   #   cmake -> archives -> objcopy rename -> bindgen
│   ├── src/lib.rs                 #   raw FFI bindings to renamed symbols
│   └── Cargo.toml                 #   features mirror llama-cpp-sys-4: cuda/metal/vulkan
│
├── llama-cpp-liquid/              # NEW — safe Rust wrapper, mirrors llama-cpp-4 API
│   └── src/
│       ├── model.rs               #   LiquidModel (parallel to LlamaModel)
│       ├── context.rs             #   LiquidContext / LiquidDecodeContext
│       ├── audio.rs               #   audio-specific entry points (mmproj, vocoder, speaker)
│       └── lib.rs
│
└── core/
    ├── Cargo.toml                 #   adds optional dep on llama-cpp-liquid
    └── src/nodes/llama_cpp/
        ├── mod.rs                 #   gates liquid_audio behind feature
        └── liquid_audio/          # NEW
            ├── mod.rs
            ├── context.rs         #   LiquidAudioContext + registry
            ├── asr.rs             #   LlamaCppLiquidASRNode + factory
            ├── tts.rs             #   LlamaCppLiquidTTSNode + factory
            ├── config.rs          #   LiquidAudioConfig / ASR / TTS configs
            └── factory.rs         #   LiquidAudioNodesProvider
```

**Key invariants:**

- `llama-cpp-liquid-sys` produces only `liquid_llama_*` / `liquid_ggml_*` /
  `liquid_gguf_*` / `liquid_common_*` / `liquid_mtmd_*` etc. symbols. No
  overlap with stock `llama-cpp-sys-4`.
- `llama-cpp-liquid` does **not** depend on `llama-cpp-4` or `llama-cpp-sys-4`.
  They are siblings, never a chain.
- `core` can enable `llama-cpp` and `llama-cpp-liquid-audio` features
  simultaneously; both static libs link in.
- The PR is pinned in `llama-cpp-liquid-sys/llama.cpp.SHA` (a one-line file)
  and verified at the start of `build.rs`. Mismatch fails the build.

### Data Flow (per-session)

```
                ┌──────────────────────────────────────────────────────────┐
                │  Session                                                  │
                │                                                            │
   audio chunks │    ┌───────────────────┐    text     ┌───────────────┐    │
  ─────────────┼───▶│ LlamaCppLiquidASR │────────────▶│ ... (LLM, etc) │   │
                │    └────────┬──────────┘              └───────┬───────┘    │
                │             │                                  │            │
                │             ▼                                  ▼            │
                │        ┌─────────────────────────────────────────────┐      │
                │        │  LiquidAudioContext (Arc, refcounted)        │     │
                │        │   - LiquidModel  (LFM2.5-Audio GGUF)         │     │
                │        │   - LiquidMmproj (audio encoder)             │     │
                │        │   - LiquidVocoder (audio decoder)            │     │
                │        │   - LiquidSpeakerTokenizer                   │     │
                │        │   - LiquidBackend (CPU / CUDA selection)     │     │
                │        └─────────────────────────────────────────────┘      │
                │             ▲                                  ▲            │
                │             │                                  │            │
                │    ┌────────┴──────────┐    audio              │            │
   audio out   │   │ LlamaCppLiquidTTS │◀─────────── text ──────┘            │
  ◀────────────┼───└───────────────────┘                                      │
                └────────────────────────────────────────────────────────────┘
```

A `LiquidContextRegistry` keyed by
`(model_path, mmproj_path, vocoder_path, speaker_path, backend_id)` ensures the
GGUFs load once per session even when multiple nodes reference them. This is
the same pattern as the existing `LlamaCppNodesProvider` for plain text models.

## Symbol-Rename Pipeline (`llama-cpp-liquid-sys/build.rs`)

This is the load-bearing piece of the design. Five steps, executed on every
build (with caching keyed on archive content hashes):

### Step 1 — Build the fork's static libs unmodified

Drive the vendored `llama.cpp/` tree through `cmake-rs` with
`-DBUILD_SHARED_LIBS=OFF`, mirroring `llama-cpp-sys-4`'s flags (CUDA / Vulkan /
Metal pass through from cargo features on this crate). Outputs (in
`OUT_DIR/build/`):

- `libllama.a`
- `libggml.a`, `libggml-base.a`, optional `libggml-cuda.a`
- `libcommon.a`
- Any liquid-audio-specific archives the PR introduces (`libmtmd.a`, etc.)

### Step 2 — Discover symbols to rename

Run `nm --defined-only --extern-only` over each archive. Collect every symbol
matching the rename set:

- C symbols: `^(llama_|ggml_|gguf_|common_|llava_|mtmd_|liquid_audio_)`
- C++ namespaced symbols: demangle with `c++filt`, match by namespace
  (`llama::`, `ggml::`, etc.), regenerate the mangled form via `llvm-cxxfilt`
  reverse-mangling (or hand-rolled Itanium namespace mangling — both work; the
  C-symbol path is simple, the C++ path is the risky bit).

**Filter out** symbols whose demangled form is in `std::`, `__cxa_*`, or
`__gnu_*` — those resolve to libstdc++/libc and must NOT be renamed.

Emit `OUT_DIR/prefix.txt` as `<original> liquid_<original>` lines (one per
symbol). Cache by `(archive_content_hash, fork_SHA)` so incremental builds
skip rediscovery.

### Step 3 — Rewrite archives in place

For each `.a`:

```bash
objcopy --redefine-syms=$OUT_DIR/prefix.txt input.a output.a
```

Then replace the original. After this step, every public symbol in the liquid
archives is prefixed.

### Step 4 — Rewrite headers for bindgen

Copy `llama.cpp/include/*.h` (and any LFM2Audio-specific headers) to
`OUT_DIR/include_renamed/`. Run a deterministic regex pass replacing the same
identifier set with the `liquid_` prefix. **Do not** modify the original
on-disk headers — this preserves the pinned SHA's tree exactly. Run `bindgen`
against the renamed headers → `OUT_DIR/bindings.rs`. Rust callers see
`liquid_llama_model_load_from_file`, `liquid_ggml_init`, etc.

### Step 5 — Linker directives

```rust
println!("cargo:rustc-link-search=native={OUT_DIR}/build");
println!("cargo:rustc-link-lib=static=llama");      // archive name unchanged
println!("cargo:rustc-link-lib=static=ggml");
println!("cargo:rustc-link-lib=static=ggml-base");
println!("cargo:rustc-link-lib=static=common");
// (cuda variant adds ggml-cuda)
println!("cargo:rustc-link-lib=dylib=stdc++");      // shared with stock stack
```

Archive filenames are unchanged; only the *symbols inside* are renamed.
`stdc++` (or `c++` on macOS) is shared between both stacks intentionally — both
forks were built against the same C++ standard library and we did not rename
C++ runtime symbols.

### Build-time canary

Gated on `cfg(all(feature = "llama-cpp", feature = "llama-cpp-liquid-audio"))`,
a small integration test loads a tiny GGUF through each stack independently in
the same process and asserts both succeed. This catches:

- Rename-list regressions when the PR rebases.
- New symbol prefixes the regex didn't anticipate (e.g. `lfm2_*`).
- C++ inline symbols that escaped the rename pass (see Risk 2).

The `nm`-based prefix generator additionally compares unrenamed symbols against
stock `llama-cpp-sys-4`'s symbol set; any overlap fails the build with a clear
message naming the offending symbol.

### Platform notes

| Platform | Tool       | Status     |
|----------|------------|------------|
| Linux    | binutils `objcopy` | Supported in v1 |
| macOS    | `llvm-objcopy` (Homebrew `llvm`) | Supported in v1; build fails with clear message if missing |
| Windows  | `llvm-objcopy` on COFF | **Out of scope for v1** |

## Node Design

### `LiquidAudioContext` (shared backend)

```rust
pub struct LiquidAudioContext {
    model: Arc<LiquidModel>,                        // LFM2.5-Audio-1.5B-Q8_0.gguf
    mmproj: Arc<LiquidMmproj>,                      // audio encoder
    vocoder: Arc<LiquidVocoder>,                    // TTS decoder
    speaker_tokenizer: Arc<LiquidSpeakerTokenizer>, // speaker embedding lookup
    backend: Arc<LiquidBackend>,
}

impl LiquidAudioContext {
    pub fn from_config(cfg: &LiquidAudioConfig) -> Result<Arc<Self>>;
    pub fn new_decode_context(&self) -> Result<LiquidDecodeContext>; // per-stream
}
```

Refcounted across nodes within a session. First node to initialize loads the
GGUFs; subsequent nodes with matching config get an `Arc` clone via the
registry. Drop releases when the last node goes away.

### `LlamaCppLiquidASRNode` (audio → text)

```rust
#[node(
    node_type = "LlamaCppLiquidASR",
    capabilities = "configured",
    input_caps  = "audio(sample_rate=16000, channels=1, format=F32)",
    output_caps = "text"
)]
pub struct LlamaCppLiquidASRNode {
    ctx: Arc<LiquidAudioContext>,
    decode: Mutex<LiquidDecodeContext>,
    cfg: LiquidAudioASRConfig,
}
```

**Per-chunk flow:**

1. Accumulate audio until `chunk_ms` worth (consumer-side `audio_buffer_accumulator`
   upstream is fine; node tolerates short reads).
2. Encode through `mmproj` to audio embeddings.
3. Append embeddings to KV cache.
4. Decode tokens until EOS or `max_tokens`.
5. Emit `RuntimeData::Text` per finalized utterance. Streaming partials emit as
   separate `RuntimeData::Text` items with `partial: true` metadata, matching
   the existing `WhisperNode` convention.

Capability behavior is `Configured` (not `Static`) because the input rate
depends on which `mmproj` GGUF is loaded — read at load time from GGUF metadata
during `media_capabilities()`.

### `LlamaCppLiquidTTSNode` (text → audio)

```rust
#[node(
    node_type = "LlamaCppLiquidTTS",
    capabilities = "configured",
    input_caps  = "text",
    output_caps = "audio(sample_rate=24000, channels=1, format=F32)" // vocoder-determined
)]
pub struct LlamaCppLiquidTTSNode {
    ctx: Arc<LiquidAudioContext>,
    decode: Mutex<LiquidDecodeContext>,
    cfg: LiquidAudioTTSConfig,
}
```

**Per-input flow:**

1. Tokenize text + speaker prompt.
2. Decode through the LLM to produce audio codebook tokens.
3. Run vocoder over those tokens to produce f32 PCM.
4. Emit `RuntimeData::Audio`.

Output sample rate is read from vocoder GGUF metadata at load time, surfaced as
`Configured` capability. Streaming partial audio is **not** emitted in v1 — see
Risk 5.

### Config

```rust
pub struct LiquidAudioConfig {
    pub model_path: PathBuf,
    pub mmproj_path: PathBuf,
    pub vocoder_path: PathBuf,
    pub speaker_tokenizer_path: PathBuf,
    pub backend: LlamaBackendConfig,   // reuses existing struct
    pub n_ctx: u32,
    pub n_threads: u32,
}

pub struct LiquidAudioASRConfig {
    pub backend: LiquidAudioConfig,    // drives the registry key
    pub chunk_ms: u32,
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
`LiquidASR → … → LiquidTTS` with matching paths share one loaded model in
memory.

### Factory

```rust
pub struct LiquidAudioNodesProvider;

impl NodesProvider for LiquidAudioNodesProvider {
    fn register(&self, registry: &mut NodeRegistry) {
        registry.register("LlamaCppLiquidASR", Arc::new(LlamaCppLiquidASRNodeFactory));
        registry.register("LlamaCppLiquidTTS", Arc::new(LlamaCppLiquidTTSNodeFactory));
    }
}
```

`#[inventory::submit]`-collected behind the `llama-cpp-liquid-audio` feature,
matching how the existing `LlamaCppNodesProvider` is registered.

## Cargo Features & Build Matrix

### Workspace `Cargo.toml`

```toml
[workspace.dependencies]
llama-cpp-4         = "0.2.13"           # unchanged
llama-cpp-sys-4     = "0.2"              # unchanged
llama-cpp-liquid    = { path = "crates/llama-cpp-liquid" }
llama-cpp-liquid-sys = { path = "crates/llama-cpp-liquid-sys" }
```

No `[patch.crates-io]` — the liquid crates are siblings, never substitutes.

### `crates/core/Cargo.toml`

```toml
[dependencies]
llama-cpp-4         = { workspace = true, optional = true }
llama-cpp-sys-4     = { workspace = true, optional = true }
llama-cpp-liquid    = { workspace = true, optional = true }

[features]
# Existing
llama-cpp           = ["dep:llama-cpp-4", "dep:llama-cpp-sys-4",
                       "dep:encoding_rs", "dep:minijinja", "dep:minijinja-contrib"]
llama-cpp-cuda      = ["llama-cpp", "llama-cpp-4/cuda", "llama-cpp-sys-4/cuda"]

# New
llama-cpp-liquid-audio       = ["dep:llama-cpp-liquid"]
llama-cpp-liquid-audio-cuda  = ["llama-cpp-liquid-audio", "llama-cpp-liquid/cuda"]

# Convenience
llama-cpp-all        = ["llama-cpp", "llama-cpp-liquid-audio"]
llama-cpp-all-cuda   = ["llama-cpp-cuda", "llama-cpp-liquid-audio-cuda"]
```

The two stacks have independent CUDA toggles because users may want CPU
upstream + GPU liquid (or vice versa) during evaluation.

### `crates/llama-cpp-liquid-sys/Cargo.toml`

```toml
[features]
default = []
cuda    = []
metal   = []
vulkan  = []
```

Each feature drives the cmake flags inside this crate's vendored llama.cpp
tree, independent of the upstream `-sys` crate.

### CI Matrix

| Feature flags                                              | Expected outcome                                                    |
|------------------------------------------------------------|---------------------------------------------------------------------|
| (none)                                                     | Builds; neither stack present                                       |
| `--features llama-cpp`                                      | Stock llama.cpp only                                                |
| `--features llama-cpp-liquid-audio`                         | Liquid fork only                                                    |
| `--features llama-cpp,llama-cpp-liquid-audio`               | **Both linked**; no symbol collisions (canary test runs)            |
| `--features llama-cpp-cuda,llama-cpp-liquid-audio-cuda`     | Both linked with CUDA; single CUDA runtime shared                   |
| `--features llama-cpp-all`                                  | Same as both-linked                                                 |

The both-linked configurations are the only ones that actually exercise the
rename pipeline. CI must include at least one both-linked job per supported
platform (Linux + macOS for v1).

### CUDA Runtime Sharing (constraint)

`libcudart` and `libcublas` are **not** prefixed. Both stacks call into the
same CUDA runtime — this is correct and intentional, but it means CUDA versions
must match between the upstream-pinned llama.cpp and the PR-pinned llama.cpp.
If PR 18641 ever bumps to a CUDA version that upstream `llama-cpp-sys-4`
0.2.13 doesn't support, the both-linked build breaks. Mitigation: pin the
upstream `llama-cpp-sys-4` to whatever version targets the same CUDA major as
the PR. Both currently target CUDA 12.x, so this is not an issue today.

## Risks

Ordered by severity.

### Risk 1 — C++ name mangling vs. objcopy (highest)

`objcopy --redefine-syms` operates on symbol-table strings verbatim. For
mangled C++ names like `_ZN5llama5model10load_fileEPKc`, a pattern matching
`llama_*` doesn't apply because the mangled prefix is `_ZN5llama5model...`,
not `llama_`. The rename pass needs a separate path for mangled symbols:

1. Demangle each candidate via `c++filt` / `llvm-cxxfilt`.
2. Match by namespace (`llama::`, `ggml::`, …) in source-space.
3. Regenerate the mangled form by either calling `llvm-cxxfilt --reverse` or
   hand-rolling Itanium namespace mangling (the namespace tag mangling is
   well-defined: `_ZN5llama...` becomes `_ZN12liquid_llama...`).

**Mitigation:** every renamed symbol round-trips through demangle → rename →
re-mangle → verify. Any failure aborts the build with the offending symbol
named. The build-time canary additionally proves both stacks load in one
process.

**Worst case:** a multi-day debugging session if a non-trivial fraction of
mangled symbols don't round-trip cleanly.

### Risk 2 — Inline functions / templates duplicated across both libs

C++ entities defined in headers (`inline`, `template`, `constexpr`) get emitted
into both archives' translation units with identical mangled names that the
rename pipeline cannot rewrite (they are weak symbols, the linker picks one).

- For pure-function inlines: harmless, both versions are identical or close
  enough.
- For inlines containing a `static` local: the two stacks would silently share
  state. **This is the dangerous case.**

**Mitigation:** audit upstream-vs-fork header diffs at the pinned SHA. Flag
divergent inline definitions for either rename or extraction into the
fork-side cpp file. PR 18641 is mostly new C++ files plus mmproj/vocoder
additions, so the risk is moderate, not high.

### Risk 3 — PR rebase churn

Every force-push on PR 18641 moves the SHA, may shift the symbol set, and may
require regenerating the rename list. **Acceptable cost.** The build-time
canary catches regressions; expect a maintenance commit per PR rebase.

### Risk 4 — GGUF availability

LFM2.5-Audio GGUFs (model + mmproj + vocoder + speaker tokenizer) must be
downloadable. The PR references `LiquidAI/LFM2.5-Audio-1.5B-GGUF`. **Decision:**
document the four file paths in the node config docs; do not ship a downloader
in this iteration.

### Risk 5 — Streaming TTS boundary artifacts

Vocoders generally need full utterances (or coherent chunks) to produce smooth
audio — token-by-token streaming may produce clicking artifacts at chunk
boundaries. **Decision for v1:** finalize audio per-utterance (text-in →
audio-out, no streaming partials). Streaming TTS can come later if the vocoder
supports it cleanly.

## Open Questions (deferred, not blockers)

- **Q1.** Does the PR's `llama-liquid-audio-server` expose a stable C entry
  point for the audio decode loop, or only an internal C++ implementation
  invoked from the HTTP handler? Determines how thin the `llama-cpp-liquid`
  wrapper can be. **Resolution:** read the PR diff during implementation; does
  not change the design.
- **Q2.** Speaker file format and whether speakers are baked into the GGUF or
  loaded separately. The Makefile snippet
  (`--tts-speaker-file LFM2.5-Audio-1.5B-GGUF/tokenizer-LFM2.5-Audio-1.5B-Q8_0.gguf`)
  suggests speaker tokenizer is a separate GGUF. The `LiquidSpeakerTokenizer`
  API may need revision once we read the PR.
- **Q3.** Windows support timeline. **Filed as a follow-up spec.**

## Migration & Rollout

1. Land `llama-cpp-liquid-sys` and `llama-cpp-liquid` as new crates with no
   `core` integration. Verify both-linked CI.
2. Land `core/src/nodes/llama_cpp/liquid_audio/` behind the
   `llama-cpp-liquid-audio` feature. No effect on existing users.
3. Add an example (`crates/core/examples/liquid_audio_smoke.rs`) modeled on
   the existing `llama_cpp_chat_smoke.rs`.
4. Document GGUF paths and the four-file requirement in `docs/`.

## Success Criteria

- `cargo test -p remotemedia-core --features llama-cpp,llama-cpp-liquid-audio`
  passes on Linux + macOS, including the both-linked canary test.
- A pipeline manifest with `MicInput → LiquidASR → LlamaGeneration → LiquidTTS
  → SpeakerOutput` constructs, validates capabilities, and produces audible
  output end-to-end on a test machine with the GGUFs present.
- No regression in any existing `--features llama-cpp` or `--features
  llama-cpp-cuda` build.
- Symbol-collision regressions are caught at build time, not at runtime.

## References

- [ggml-org/llama.cpp PR #18641 (LFM2Audio)](https://github.com/ggml-org/llama.cpp/pull/18641)
- [LiquidAI/LFM2.5-Audio-1.5B-GGUF](https://huggingface.co/LiquidAI/LFM2.5-Audio-1.5B-GGUF)
- [eugenehp/llama-cpp-rs (current `llama-cpp-4` source)](https://github.com/eugenehp/llama-cpp-rs)
- Existing integration: `crates/core/src/nodes/llama_cpp/`
- Existing example: `crates/core/examples/llama_cpp_chat_smoke.rs`
