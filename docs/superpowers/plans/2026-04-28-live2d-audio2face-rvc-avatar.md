# Live2D + Audio2Face + RVC Avatar Pipeline — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add five components to the SDK so a manifest can produce a livestream-grade animated Live2D avatar video track synchronized to a speech-to-speech conversation, alongside (not in place of) the existing audio path. Specifically: emoji-tag extraction from text, realtime voice conversion, Audio2Face blendshape inference, and a 30 fps Live2D renderer that consumes the blendshape stream + emotion stream + audio playback clock.

**Architecture (post-validation pivot):** Per the [phase-1 validation report](../specs/2026-04-28-live2d-renderer-backend-validation.md), the spec's §5 split Rust+Python renderer backend is not viable — `live2d-py` exposes none of the `csmGetDrawable*` post-deformer mesh accessors. The validation-report recommendation is adopted: ship the §11(1) **native Rust + wgpu + Live2DCubismCore** renderer as the first backend. The four other components (`EmotionExtractorNode`, `RVCNode`, `LipSyncNode` trait, `Audio2FaceLipSyncNode`) are unchanged from the design spec, and the manifest-level `Live2DRenderNode` interface is unchanged.

**Tech Stack:** Rust 1.87+, `ort` (existing — for Audio2Face + RVC ONNX inference), `wgpu` (existing — for renderer), `Live2DCubismCore` (external download, license-gated, linked via build.rs), `bindgen`, `tokio`, `serde`. No new runtime dependencies in core's hot path.

**Spec:** [`docs/superpowers/specs/2026-04-27-live2d-audio2face-rvc-avatar-design.md`](../specs/2026-04-27-live2d-audio2face-rvc-avatar-design.md)
**Validation report:** [`docs/superpowers/specs/2026-04-28-live2d-renderer-backend-validation.md`](../specs/2026-04-28-live2d-renderer-backend-validation.md)

---

## Build order (rationale)

Five components, four phases. Build smallest-blast-radius first; ship the integration test path early so downstream work has something to wire into:

| Phase | Lands | Why now |
|-------|-------|---------|
| M0 | `EmotionExtractorNode` (pure Rust, zero models) | Smallest scope, fully spec-stable, unblocks any integration test that wants emoji-tagged text. |
| M1 | `audio.out.clock` tap on `AudioSender` (one transport hook) | Single-file change; no new node. Lets us write the renderer's input arbitration tests against a real clock stream before the renderer exists. |
| M2 | `LipSyncNode` trait + `Audio2FaceLipSyncNode` impl | Where the spec's "synthetic emotion keywords drive avatar via WebRTC" integration test lands — exercised end-to-end without needing the renderer. |
| M3 | `RVCNode` | Independent of avatar output. Self-contained tests; ships in parallel with M2 if labor permits. |
| M4 | `Live2DRenderNode` (native wgpu + CubismCore) | Heaviest. Depends on Cubism Core acquisition. Final integration test wires M0 → kokoro_tts → M2 → M4 → WebRTC video out. |

Each milestone ends with a green `cargo test` (or, for milestones gated on external models, a green `cargo build` and a green `--ignored` gate) and a commit. M2 and M3 may be parallelized once M1 lands.

---

## File Structure

| Path | Responsibility |
|------|----------------|
| **M0 — EmotionExtractorNode** | |
| `crates/core/src/nodes/emotion_extractor.rs` | New node + factory. Streaming text→(text, json) with multi-output. Mirrors `silero_vad.rs` pattern. |
| `crates/core/src/nodes/mod.rs` | **Modify** — add `pub mod emotion_extractor;`. |
| `crates/core/src/nodes/registration_macros.rs` (or registry site) | **Modify** — register `EmotionExtractorNode` factory. |
| `crates/core/Cargo.toml` | **Modify** — add `avatar-emotion` feature gating the node. |
| `crates/core/tests/emotion_extractor_test.rs` | Integration test: synthetic LLM text stream → assert two output edges receive correctly partitioned data. |
| **M1 — audio.out.clock tap** | |
| `crates/transports/webrtc/src/media/audio_sender.rs` | **Modify** — thread `SessionControl` into `AudioSender::new`; emit `publish_tap("audio", Some("clock"), …)` after each dequeued frame. |
| `crates/transports/webrtc/src/peer/server_peer.rs` | **Modify** — pass `SessionControl` when constructing `AudioSender`. |
| `crates/transports/webrtc/tests/audio_clock_tap_test.rs` | New test: synthetic audio in → subscribe to `audio.out.clock` → assert pts_ms monotonicity, frame correlation, silence (ring drained) → no publishes. |
| **M2 — LipSyncNode + Audio2FaceLipSyncNode** | |
| `crates/core/src/nodes/lip_sync/mod.rs` | New module. `LipSyncNode` trait/port (`audio → blendshapes_json`). |
| `crates/core/src/nodes/lip_sync/audio2face/mod.rs` | `Audio2FaceLipSyncNode` impl + factory. |
| `crates/core/src/nodes/lip_sync/audio2face/inference.rs` | ONNX session + chunked inference, port of `external/handcrafted-persona-engine/.../Audio2Face/Audio2FaceInference.cs`. |
| `crates/core/src/nodes/lip_sync/audio2face/solver.rs` | PGD + BVLS solver port (see `external/handcrafted-persona-engine/.../Audio2Face/{Pgd,Bvls}BlendshapeSolver.cs`). |
| `crates/core/src/nodes/lip_sync/audio2face/smoothing.rs` | `ParamSmoother` port. |
| `crates/core/src/nodes/lip_sync/audio2face/data.rs` | `BlendshapeFrame` (52-vector + pts_ms + turn_id) wire type. |
| `crates/core/src/nodes/lip_sync/blendshape_json.rs` | `RuntimeData::Json` envelope shape: `{kind:"blendshapes", arkit_52:[f32;52], pts_ms, turn_id?}`. |
| `crates/core/Cargo.toml` | **Modify** — add `avatar-audio2face` feature; add `ort` workspace dep with feature-gated init. |
| `crates/core/tests/audio2face_lipsync_test.rs` | Unit: 52-vector range, pts_ms monotonicity, barge clears buffer. Integration (gated): synthetic audio → blendshape stream rate matches `output_framerate`. |
| `crates/core/tests/avatar_synthetic_emotion_e2e.rs` | **Synthetic-keyword WebRTC e2e** (per user authorization): drive `[EMOTION:🤩] ... [EMOTION:😊]` text stream + synthetic audio through `EmotionExtractor → kokoro_tts → Audio2FaceLipSync` and assert two emotion Json events + a continuous blendshape stream emerge on the right manifest edges. No renderer required at this stage. |
| `crates/core/tests/fixtures/audio2face/README.md` | Env-var contract for tier-2 real ONNX model (`AUDIO2FACE_TEST_ONNX`). |
| `crates/core/tests/fixtures/audio2face/sine_16k.wav` | Tier-1 synthetic audio fixture (1 s, 16 kHz, sine sweep). |
| **M3 — RVCNode** | |
| `crates/core/src/nodes/rvc/mod.rs` | New `RVCNode` + factory (Audio→Audio). |
| `crates/core/src/nodes/rvc/inference.rs` | ONNX inference + chunked I/O. |
| `crates/core/src/nodes/rvc/f0.rs` | F0 estimator (CREPE / RMVPE) — initial impl wraps a single ONNX session, design space for swapping. |
| `crates/core/src/nodes/rvc/index.rs` | Optional retrieval-index loader. |
| `crates/core/Cargo.toml` | **Modify** — add `avatar-rvc` feature. |
| `crates/core/tests/rvc_test.rs` | Unit: chunk boundary continuity, output rate matches config. Integration (gated): A→B sine sweep through RVC; assert RMS levels and pitch shift bounds. |
| `crates/core/tests/fixtures/rvc/README.md` | Env-var contract: `RVC_TEST_MODEL_ONNX`, `RVC_TEST_INDEX`, `RVC_TEST_F0_MODEL`. |
| **M4 — Live2DRenderNode (native wgpu + CubismCore)** | |
| `crates/cubism-core-sys/Cargo.toml` | New `-sys` crate; build.rs links `Live2DCubismCore.{a,lib}` from `LIVE2D_CUBISM_CORE_DIR`. |
| `crates/cubism-core-sys/build.rs` | Reads env var, links static lib, runs bindgen against `Live2DCubismCore.h`. |
| `crates/cubism-core-sys/wrapper.h` | bindgen entry point. |
| `crates/cubism-core-sys/src/lib.rs` | `include!(concat!(env!("OUT_DIR"), "/bindings.rs"))`. |
| `crates/cubism-core-sys/CUBISM_SDK.md` | License-acquisition + build-env docs. |
| `crates/cubism-core/Cargo.toml` | Safe wrapper. |
| `crates/cubism-core/src/lib.rs` | `Moc`, `Model`, `DrawableView` (post-deformer mesh accessors). |
| `crates/cubism-core/src/drawable.rs` | `csmGetDrawableVertexPositions` etc. wrapped as zero-copy slices. |
| `crates/cubism-core/src/parameters.rs` | Parameter / part API. |
| `crates/cubism-core/src/physics.rs` | Physics / motion / expression evaluation. |
| `crates/core/src/nodes/live2d_render/mod.rs` | `Live2DRenderNode` + factory. |
| `crates/core/src/nodes/live2d_render/state.rs` | Input arbitration: blendshape ring, emotion state, idle/blink scheduler, audio-clock tracker. |
| `crates/core/src/nodes/live2d_render/arkit_to_live2d.rs` | ARKit-52 → VBridger param mapper. Default mapping baked in; YAML override per spec §3.5. |
| `crates/core/src/nodes/live2d_render/emotion_map.rs` | Emoji → (expression_id, motion_group) lookup; defaults from `external/handcrafted-persona-engine/Live2D.md`. |
| `crates/core/src/nodes/live2d_render/wgpu_backend/mod.rs` | wgpu render-to-texture backend, drawable graph traversal, mask passes, blend modes, RGBA→Rgb24/Yuv420p readback. |
| `crates/core/src/nodes/live2d_render/wgpu_backend/shaders/` | WGSL shaders (basic, mask pre-pass, blend modes Normal/Additive/Multiply). |
| `crates/core/src/nodes/live2d_render/backend_trait.rs` | `trait Live2DBackend { fn render_frame(...) -> RgbFrame; }` — keeps door open for native iOS / RealityKit follow-ups. |
| `crates/core/Cargo.toml` | **Modify** — add `avatar-render` feature; gate the `cubism-core` dep behind it. |
| `crates/core/tests/live2d_render_state_test.rs` | Unit: input arbitration with `MockBackend`. Recorded `render_frame` calls drive blendshape sampling, emotion expiration, idle/blink, barge ring-clear assertions. |
| `crates/core/tests/live2d_render_wgpu_test.rs` | wgpu render-to-texture against a synthetic `ModelFrame`; pixel-count + alpha sanity check. Headless. |
| `crates/core/tests/avatar_full_pipeline_e2e.rs` | **Final WebRTC e2e** (gated on Cubism SDK + Live2D model env vars): synthetic emotion + synthetic audio → blendshapes → `Live2DRenderNode` → WebRTC video track. Pixel non-zero sanity, frame rate ≈ 30 fps, ≥ 1 emission per emotion event. |
| `crates/core/tests/fixtures/live2d/README.md` | Env-var contract: `LIVE2D_TEST_MODEL_PATH` (path to `.model3.json`). Cubism Core acquisition referenced in `crates/cubism-core-sys/CUBISM_SDK.md`. |
| **Examples + workspace** | |
| `Cargo.toml` | **Modify** — add `crates/cubism-core-sys`, `crates/cubism-core` to workspace members; add to `[workspace.dependencies]`. |
| `crates/core/examples/avatar_smoke.rs` | End-to-end smoke example matching spec §4.1 manifest. |

---

## Milestone Map

| M | Scope | Gate |
|---|-------|------|
| M0 | `EmotionExtractorNode` + tests | ✅ shipped — see [M0 actuals](#m0-actuals-2026-04-28) |
| M1 | `audio.out.clock` tap on `AudioSender` | ✅ shipped — see [M1 actuals](#m1-actuals-2026-04-28) |
| M2 | `LipSyncNode` trait + `Audio2FaceLipSyncNode` + synthetic-emotion e2e | 🟡 partial — interface + synthetic stand-in + e2e shipped; real Audio2Face ONNX port deferred. See [M2 actuals](#m2-actuals-2026-04-28-partial). |
| M3 | `RVCNode` | `cargo test --features avatar-rvc` green; tier-2 tests pass on env-var-bearing host |
| M4 | `Live2DRenderNode` (native wgpu + CubismCore) | `cargo build --features avatar-render` green w/ `LIVE2D_CUBISM_CORE_DIR` set; full pipeline e2e green when both Cubism SDK + Live2D model env vars present |

Each milestone ends with one or more commits. Milestones M2 and M3 may be parallelized once M1 lands. M4 is sequential (depends on M2 for blendshape input).

---

## Fixture / test-double strategy (read before starting M2 or M3)

Same two-tier pattern as the [liquid-audio plan](2026-04-28-llama-cpp-liquid-audio.md):

- **Tier 1 — Synthetic fixtures.**
  - **Text streams** (M0): inline string literals with `[EMOTION:🤩]` etc. tags. No on-disk fixture needed.
  - **Audio** (M2/M3): a 1-second 16 kHz mono sine sweep at `crates/core/tests/fixtures/audio2face/sine_16k.wav` (committed, < 100 KB). Sufficient for "ONNX session loads and runs without panicking" and for shape-conformance checks. Not sufficient for verifying *correct* blendshape outputs.
  - **`MockBackend`** (M4): records `render_frame` calls for input-arbitration assertions without a wgpu device. Gated `#[cfg(any(test, feature = "test-support"))]`.

- **Tier 2 — Real models.** Tests that need actual inference results read paths from env vars and `#[ignore]` cleanly when missing:
  - `AUDIO2FACE_TEST_ONNX` — path to `audio2face.onnx` (the persona-engine's local model; users acquire from persona-engine bootstrapper and commit nothing).
  - `RVC_TEST_MODEL_ONNX`, `RVC_TEST_INDEX`, `RVC_TEST_F0_MODEL` — RVC stack.
  - `LIVE2D_TEST_MODEL_PATH` — `.model3.json` for the renderer e2e (any rigged Live2D model with VBridger params; persona-engine's bundled Aria works once acquired).
  - `LIVE2D_CUBISM_CORE_DIR` — only needed for `cargo build --features avatar-render`; users accept Live2D's license at <https://www.live2d.com/sdk/download/native/> and set this in their build environment. The `-sys` crate's build.rs reads it.

A single helper macro `tests::skip_if_no_real_avatar_models!()` (defined in M2.0) bails out cleanly if env vars are unset.

CI sets these env vars only on machines that have the models cached (best-effort, like the liquid plan); the synthetic-emotion e2e (M2) runs on every CI host because it doesn't need real models for the *control plane* assertions — only that Audio2Face produces *some* 52-vector per audio chunk, which is shape-conformance, not output-correctness.

The fixture creation itself is **Task M2.0** below so the audio fixture is available before the inference work starts.

---

## M0 — `EmotionExtractorNode`

Pure Rust, zero models, zero new deps. Smallest blast radius; ships first; unlocks the synthetic-emotion test fixtures that M2 depends on. Mirrors the existing `silero_vad` multi-output pattern (per [crates/core/src/nodes/silero_vad.rs:297](../../../crates/core/src/nodes/silero_vad.rs#L297)).

### Task M0.1: Failing test for tag extraction on a single channel

**Files:**
- Create: `crates/core/tests/emotion_extractor_test.rs`

- [ ] **Step 1: Write integration tests covering the spec's §3.1 invariants**

```rust
// crates/core/tests/emotion_extractor_test.rs
use remotemedia_core::data::{RuntimeData, text_channel::tag_text_str};
use remotemedia_core::nodes::emotion_extractor::EmotionExtractorNode;

#[tokio::test]
async fn extracts_tag_emits_text_minus_tag_plus_json() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = node.process_one(RuntimeData::Text(
        tag_text_str("Hi there [EMOTION:🤩] yes!", "tts").into()
    )).await.unwrap();
    assert_eq!(outputs.len(), 2, "expect Text + Json");

    match &outputs[0] {
        RuntimeData::Text(s) => assert_eq!(strip_channel(s), "Hi there  yes!"),
        _ => panic!("expected Text first"),
    }
    match &outputs[1] {
        RuntimeData::Json(v) => {
            assert_eq!(v["kind"], "emotion");
            assert_eq!(v["emoji"], "🤩");
            assert!(v["source_offset_chars"].as_u64().unwrap() <= "Hi there ".len() as u64);
            assert!(v.get("ts_ms").is_some());
        }
        _ => panic!("expected Json second"),
    }
}

#[tokio::test]
async fn no_tag_emits_only_text_no_json() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = node.process_one(RuntimeData::Text(
        tag_text_str("plain text", "tts").into()
    )).await.unwrap();
    assert_eq!(outputs.len(), 1);
    matches!(&outputs[0], RuntimeData::Text(_));
}

#[tokio::test]
async fn multiple_tags_emit_multiple_jsons_in_order() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = node.process_one(RuntimeData::Text(
        tag_text_str("[EMOTION:😊] hello [EMOTION:🤩]!", "tts").into()
    )).await.unwrap();
    assert_eq!(outputs.len(), 3, "Text + 2x Json");
    let off0 = outputs[1].as_json().unwrap()["source_offset_chars"].as_u64().unwrap();
    let off1 = outputs[2].as_json().unwrap()["source_offset_chars"].as_u64().unwrap();
    assert!(off0 < off1, "Json frames must be in source order");
}

#[tokio::test]
async fn channel_is_preserved_on_text_output() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = node.process_one(RuntimeData::Text(
        tag_text_str("[EMOTION:😊] x", "ui").into()
    )).await.unwrap();
    let s = outputs[0].as_text().unwrap();
    let (channel, _) = remotemedia_core::data::text_channel::split_text_str(&s);
    assert_eq!(channel, "ui", "channel tag must round-trip");
}

#[tokio::test]
async fn alias_substitution_applied_before_emit() {
    let mut node = EmotionExtractorNode::with_default_pattern();
    node.set_alias("happy", "😊");
    let outputs = node.process_one(RuntimeData::Text(
        tag_text_str("[EMOTION:happy] hi", "tts").into()
    )).await.unwrap();
    let json = outputs[1].as_json().unwrap();
    assert_eq!(json["emoji"], "😊");
    assert_eq!(json["alias"], "happy");
}

#[tokio::test]
async fn turn_id_forwarded_when_metadata_carries_it() {
    let node = EmotionExtractorNode::with_default_pattern();
    let mut frame = RuntimeData::Text(tag_text_str("[EMOTION:🤩]", "tts").into());
    frame.set_metadata("turn_id", serde_json::json!(42));
    let outputs = node.process_one(frame).await.unwrap();
    assert_eq!(outputs[1].as_json().unwrap()["turn_id"], 42);
}

#[tokio::test]
async fn turn_id_omitted_when_metadata_absent() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = node.process_one(RuntimeData::Text(
        tag_text_str("[EMOTION:🤩]", "tts").into()
    )).await.unwrap();
    assert!(outputs[1].as_json().unwrap().get("turn_id").is_none());
}

#[tokio::test]
async fn malformed_regex_at_construction_returns_error() {
    let res = EmotionExtractorNode::with_pattern("[unbalanced");
    assert!(res.is_err());
}
```

- [ ] **Step 2: Run test, verify it fails**

```bash
cargo test -p remotemedia-core --features avatar-emotion --test emotion_extractor_test
```
Expected: `cannot find type EmotionExtractorNode`.

### Task M0.2: Implement `EmotionExtractorNode`

**Files:**
- Create: `crates/core/src/nodes/emotion_extractor.rs`
- Modify: `crates/core/src/nodes/mod.rs`
- Modify: `crates/core/Cargo.toml` (add `avatar-emotion` feature; gate the `regex` dep if not already present)

- [ ] **Step 1: Add the feature flag**

```toml
# crates/core/Cargo.toml
[features]
# ... existing entries ...
avatar-emotion = ["dep:regex"]
```

- [ ] **Step 2: Implement the node**

Mirror `silero_vad`'s `AsyncStreamingNode` pattern. Use `#[node(...)]` derive with `multi_output = true`, `accepts = "text"`, `produces = "text,json"`, `capabilities = "passthrough"`. Internal:

```rust
// crates/core/src/nodes/emotion_extractor.rs
use crate::data::{RuntimeData, text_channel::{split_text_str, tag_text_str}};
use crate::nodes::streaming_node::AsyncStreamingNode;
use regex::Regex;
use serde_json::json;
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(remotemedia_core_derive::Node)]
#[node(
    node_type = "EmotionExtractor",
    accepts = "text",
    produces = "text,json",
    multi_output,
    capabilities = "passthrough"
)]
pub struct EmotionExtractorNode {
    pattern: Regex,
    aliases: HashMap<String, String>,
}

impl EmotionExtractorNode {
    pub fn with_default_pattern() -> Self { /* unwrap default */ }
    pub fn with_pattern(p: &str) -> Result<Self, regex::Error> { /* ... */ }
    pub fn set_alias(&mut self, alias: impl Into<String>, emoji: impl Into<String>) { /* ... */ }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for EmotionExtractorNode {
    fn node_type(&self) -> &str { "EmotionExtractor" }

    async fn process_streaming(
        &self,
        data: RuntimeData,
        callback: &mut dyn FnMut(RuntimeData) -> crate::Result<()>,
    ) -> crate::Result<usize> {
        let RuntimeData::Text(raw) = data else { return Ok(0); };
        let (channel, body) = split_text_str(&raw);
        let turn_id = data.metadata("turn_id").cloned();

        let mut emitted = 0usize;
        let mut last_end = 0usize;
        let mut stripped = String::with_capacity(body.len());
        let ts_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;

        for cap in self.pattern.captures_iter(body) {
            let m = cap.get(0).unwrap();
            stripped.push_str(&body[last_end..m.start()]);
            let raw_match = cap.get(1).map(|c| c.as_str()).unwrap_or("");
            let (emoji, alias) = self.resolve_alias(raw_match);
            let mut json_obj = json!({
                "kind": "emotion",
                "emoji": emoji,
                "source_offset_chars": m.start() as u64,
                "ts_ms": ts_ms,
            });
            if let Some(a) = alias { json_obj["alias"] = json!(a); }
            if let Some(tid) = &turn_id { json_obj["turn_id"] = tid.clone(); }

            // Emit Json AFTER the corresponding text segment, in source order.
            // The Text frame is emitted once at the end (single concatenated frame).
            // The Json frames are queued and emitted after the Text frame.
            // (Deviation from naïve interleaving — matches spec §3.1 "Output frame 1 / Output frame 2".)
            // Decide whether to emit one Text frame or split per-segment in M0.3.
            last_end = m.end();
            // accumulate Json into a vec, emit after the loop
        }
        stripped.push_str(&body[last_end..]);

        callback(RuntimeData::Text(tag_text_str(&stripped, channel).into()))?;
        emitted += 1;
        // emit accumulated Json frames in source order

        Ok(emitted)
    }
}
```

- [ ] **Step 3: Decide single-Text-frame emission shape**

Spec §3.1 says: "Output frame 1: Text — original text on the original channel, with all tags removed. Always emitted." That's one Text frame per input (not per-tag-segment). Implement that. Json frames follow in source order.

- [ ] **Step 4: Run test, verify all pass**

```bash
cargo test -p remotemedia-core --features avatar-emotion --test emotion_extractor_test
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(emotion): EmotionExtractorNode multi-output (text+json)"
```

### Task M0.3: Register the factory + capability declaration

**Files:**
- Modify: `crates/core/src/nodes/mod.rs`
- Modify: registration site (per [`crates/core/src/nodes/registration_macros.rs`](../../../crates/core/src/nodes/registration_macros.rs))

- [ ] **Step 1: Failing test that asserts the registry knows the node**

```rust
#[test]
#[cfg(feature = "avatar-emotion")]
fn registry_resolves_emotion_extractor() {
    let registry = remotemedia_core::nodes::registry();
    assert!(registry.has("EmotionExtractor"));
}
```

- [ ] **Step 2-4:** Add the registration via the inventory pattern (mirror what `silero_vad` does). Verify the test passes.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(emotion): register EmotionExtractor factory"
```

### Task M0.4: Multi-output schema declaration (capability resolver wiring)

**Files:**
- Same node file.

- [ ] **Step 1: Failing test** that the resolver-side `produces([Text, Json])` schema is exposed. (Pattern is the same as `silero_vad`'s `Audio + Json` declaration; reference the silero_vad factory for the exact macro syntax.)
- [ ] **Step 2:** Run, verify failing if not already covered by the `#[node]` macro.
- [ ] **Step 3:** Add the declaration; resolver tests will catch downstream-edge mistakes for free.
- [ ] **Step 4-5:** Verify + commit.

---

## M0 actuals (2026-04-28)

Shipped. Three deviations from the plan above worth noting before M1:

1. **Hand-rolled `AsyncStreamingNode` trait impl, not the `#[node(...)]` derive.** The macro generates a fixed `new(config)` that returns `Self`, not `Result<Self, _>`. Spec test `malformed_regex_at_construction_returns_error` requires a fallible constructor, so the node mirrors `silero_vad`'s manual pattern instead. The `#[node]` macro is still the right shape for downstream avatar nodes that don't need a fallible ctor.

2. **`turn_id` forwarding deferred.** `RuntimeData::Text` is a tuple variant `Text(String)` with no `metadata` field — only `Audio`, `Video`, `Image`, `Tensor`, `ControlMessage`, `File` carry metadata. Spec §3.1's "forward turn_id from upstream metadata" requires either (a) upstream sending a Json envelope with both text and turn_id, or (b) `Text` gaining a metadata field. Documented in the node's docstring; revisit when a real `display_text`-shaped upstream lands.

3. **Registry wiring goes through `CoreNodesProvider::register`, not a direct `inventory::submit!` per node.** That's the in-tree convention (see [`crates/core/src/nodes/core_provider.rs`](../../../crates/core/src/nodes/core_provider.rs#L78-L86) for adjacent registrations). The plan's "M0.3 inventory pattern" wording is updated to match.

**Tests landed (all green):**
- 10 integration tests in [`crates/core/tests/emotion_extractor_test.rs`](../../../crates/core/tests/emotion_extractor_test.rs) covering: tag stripping, no-tag passthrough, multi-tag source order, channel preservation, alias substitution, unknown-alias fallthrough, malformed-regex construction error, non-Text passthrough, registry resolution, factory rejecting bad pattern with actionable error.
- 3 inline unit tests in [`crates/core/src/nodes/emotion_extractor.rs`](../../../crates/core/src/nodes/emotion_extractor.rs).

**Build matrix verified:**
- `cargo build -p remotemedia-core` (default features) — clean.
- `cargo test -p remotemedia-core --features avatar-emotion --test emotion_extractor_test` — 10/10 green.
- `cargo test -p remotemedia-core --features avatar-emotion --lib emotion_extractor` — 3/3 green.
- `cargo build -p remotemedia-core --no-default-features` fails with 9 errors, all pre-existing (multiprocess feature gating); none reference avatar code.

---

## M1 — `audio.out.clock` tap on `AudioSender`

The renderer needs to know "what `pts_ms` is the listener currently hearing?". Spec §3.6 calls this out as the **only** transport-side change. We add it now (before the renderer exists) so the renderer's input-arbitration code can be tested against a real clock stream.

Reference: [`crates/transports/webrtc/src/media/audio_sender.rs:253`](../../../crates/transports/webrtc/src/media/audio_sender.rs#L253) (`AudioSender::new` does not currently hold a `SessionControl`); [`crates/core/src/transport/session_control.rs:225`](../../../crates/core/src/transport/session_control.rs#L225) (`publish_tap` signature).

### Task M1.1: Failing test that asserts `audio.out.clock` publishes per dequeued frame

**Files:**
- Create: `crates/transports/webrtc/tests/audio_clock_tap_test.rs`

- [ ] **Step 1: Write the test**

```rust
// crates/transports/webrtc/tests/audio_clock_tap_test.rs
#[tokio::test]
async fn audio_sender_publishes_clock_tap_per_dequeued_frame() {
    let (ctrl, mut tap_rx) = test_session_control_with_tap("audio", Some("clock"));
    let track = test_audio_track();
    let sender = AudioSender::new(track.clone(), 1024)
        .with_session_control(ctrl.clone());

    // Push 3 frames into the ring; dequeue them.
    for i in 0..3 {
        sender.push_frame(synthetic_frame(i, 20_ms_at_48k())).await.unwrap();
    }

    // Drain transmission thread to commit.
    sender.flush_for_test().await;

    let mut pts_seen = vec![];
    while let Ok(rd) = tokio::time::timeout(Duration::from_millis(50), tap_rx.recv()).await {
        let v = rd.unwrap().as_json().unwrap().clone();
        assert_eq!(v["kind"], "audio_clock");
        pts_seen.push(v["pts_ms"].as_u64().unwrap());
    }
    assert_eq!(pts_seen.len(), 3, "one publish per dequeued frame");
    assert!(pts_seen.windows(2).all(|w| w[0] < w[1]), "pts_ms monotonic");
}

#[tokio::test]
async fn no_publishes_when_ring_buffer_empty() {
    // Spec §3.6: "When the audio ring buffer is empty, no publishes happen — that is the signal."
    let (ctrl, mut tap_rx) = test_session_control_with_tap("audio", Some("clock"));
    let sender = AudioSender::new(test_audio_track(), 1024).with_session_control(ctrl);
    sender.flush_for_test().await;
    let res = tokio::time::timeout(Duration::from_millis(100), tap_rx.recv()).await;
    assert!(res.is_err(), "no publish when no frame dequeued");
}

#[tokio::test]
async fn no_session_control_means_no_publish_no_panic() {
    // SessionControl is optional; absence must not break audio output.
    let sender = AudioSender::new(test_audio_track(), 1024);
    for i in 0..3 {
        sender.push_frame(synthetic_frame(i, 20_ms_at_48k())).await.unwrap();
    }
    sender.flush_for_test().await;
    // Test passes if no panic.
}
```

- [ ] **Step 2: Run, verify failing**

Expected: `with_session_control` not found, or no tap publishes seen.

### Task M1.2: Thread `SessionControl` into `AudioSender`

**Files:**
- Modify: `crates/transports/webrtc/src/media/audio_sender.rs`
- Modify: `crates/transports/webrtc/src/peer/server_peer.rs` (constructor caller)

- [ ] **Step 1:** Add `control: Option<Arc<SessionControl>>` field. Add `with_session_control(self, ctrl) -> Self` builder. Default to `None`.
- [ ] **Step 2:** In the transmission thread (around `transmission_thread` at [`audio_sender.rs:299`](../../../crates/transports/webrtc/src/media/audio_sender.rs#L299)), after each frame is committed for transmission, compute `frame_pts_ms` from accumulated sample count / sample_rate, and call:

```rust
if let Some(ctrl) = self.control.as_ref() {
    let _ = ctrl.publish_tap(
        "audio",
        Some("clock"),
        RuntimeData::Json(serde_json::json!({
            "kind": "audio_clock",
            "pts_ms": frame_pts_ms,
            "stream_id": self.stream_id,
        })),
    );
}
```

- [ ] **Step 3:** Update [`server_peer.rs:103`](../../../crates/transports/webrtc/src/peer/server_peer.rs#L103) (where `TrackRegistry` is created) to pass the session's `SessionControl` when constructing `AudioSender`.
- [ ] **Step 4:** Run test, verify passing. Run full webrtc test suite to confirm no regression: `cargo test -p remotemedia-webrtc-transport`.
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(webrtc): publish audio.out.clock tap per dequeued frame"
```

### Task M1.3: Decide on heartbeat (deferred per spec §3.6, decided here)

Spec §3.6 leaves an optional 500 ms heartbeat with `pts_ms = null` to disambiguate "transport stalled" vs "intentional silence". Risk-callout in spec §10 recommends shipping it.

- [ ] **Step 1:** Add a config knob `heartbeat_interval_ms: Option<u64>` (default `Some(500)`).
- [ ] **Step 2:** Failing test that asserts when no audio frames flow for `heartbeat_interval_ms`, the tap publishes `{"kind":"audio_clock", "pts_ms": null}`.
- [ ] **Step 3:** Implement.
- [ ] **Step 4-5:** Verify + commit `feat(webrtc): audio.out.clock heartbeat`.

---

## M1 actuals (2026-04-28)

Shipped. Four deviations from the plan above:

1. **Late-bind setter, not constructor change.** `AudioSender::new(track, capacity)` is unchanged; existing callers (`AudioTrack::new` → `connection.rs::add_audio_track`) keep working untouched. New surface: `ClockTap` struct + `AudioSender::set_clock_tap(tap)` / `clear_clock_tap()`. Spec §3.6 explicitly calls this out as an option ("via `set_control_handle`"); choosing it avoids a constructor cascade through `AudioTrack` which doesn't currently hold a `SessionControl`.

2. **Caller wiring deferred to M4.** Plan §M1.2 step 3 (update `server_peer.rs` to thread `SessionControl` through) requires `AudioTrack`/`connection.rs` to gain a `SessionControl` reference, which neither currently has. That refactor belongs with the manifest config knob `audio_clock_node_id` from spec §3.5 — it's properly the renderer node's bootstrap concern in M4. M1 ships only the AudioSender-level mechanism; the integration call site lands when there's a renderer to consume it.

3. **`pts_ms` derived from cumulative `frame.duration`, not RTP timestamp / sample rate.** The transmission thread already gets `frame.duration` per dequeued frame; summing it as `cum_played_ms` gives a wall-of-played-audio clock without plumbing sample_rate. This also matches spec §3.6 wording ("what `pts_ms` is the listener currently hearing") more precisely than RTP-timestamp-derived ms.

4. **Heartbeat (spec §3.6 optional + §10 risk) deferred to M4.** No consumer exists yet; the renderer's input-arbitration loop in M4 is what tells us whether `pts_ms = null` heartbeats are needed (vs the renderer just timing-out the audio clock locally). Documented in audio_sender.rs to revisit when M4 lands.

**API additions** (all in [`crates/transports/webrtc/src/media/audio_sender.rs`](../../../crates/transports/webrtc/src/media/audio_sender.rs)):
- `pub struct ClockTap { control: Arc<SessionControl>, node_id: String, stream_id: Option<String> }`
- `AudioSender::set_clock_tap(&self, tap: ClockTap)` / `clear_clock_tap(&self)`
- `clock_tap: Arc<parking_lot::RwLock<Option<ClockTap>>>` field, read by transmission thread on hot path (uncontended atomic).

**Tests landed (4/4 green):** [`crates/transports/webrtc/tests/audio_clock_tap_test.rs`](../../../crates/transports/webrtc/tests/audio_clock_tap_test.rs)
- one publish per dequeued frame, monotonic `pts_ms`, envelope shape locked
- no publishes when ring buffer is empty (spec §3.6 silence signal)
- no clock tap configured = no panic, no publish (opt-in semantics)
- `pts_ms` continues advancing after `flush_buffer()` (it's a wall clock, not a per-utterance reset; renderer's stale-pts ring eviction handles barge per §6.3)

**Build matrix verified:**
- `cargo test -p remotemedia-webrtc --test audio_clock_tap_test` — 4/4 green.
- `cargo test -p remotemedia-webrtc --lib audio_sender` (existing ring-buffer tests) — 3/3 green, no regression.
- `cargo build -p remotemedia-webrtc` — clean.

---

## M2 — `LipSyncNode` trait + `Audio2FaceLipSyncNode`

This is the milestone where the user-authorized **synthetic-emotion WebRTC integration test** lands. The trait keeps the door open for follow-up phoneme-driven impls; the `Audio2FaceLipSyncNode` is the only impl shipped.

Source for the C# port: [`external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/TTS/Synthesis/LipSync/Audio2Face/`](../../../external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/TTS/Synthesis/LipSync/Audio2Face/) — `Audio2FaceInference.cs`, `PgdBlendshapeSolver.cs`, `BvlsBlendshapeSolver.cs`, `ParamSmoother.cs`, `ARKitToLive2DMapper.cs` (the mapper lives in the renderer per spec §3.4, *not* in this node).

### Task M2.0: Tier-1 audio fixture + `skip_if_no_real_avatar_models!()` helper

**Files:**
- Create: `crates/core/tests/fixtures/audio2face/sine_16k.wav` (committed binary, < 100 KB)
- Create: `crates/core/tests/fixtures/audio2face/README.md`
- Create: `crates/core/tests/avatar_test_support.rs` (helper module shared by M2/M3/M4)

- [ ] **Step 1:** Generate the sine sweep with `sox` (one-shot): `sox -n -r 16000 -c 1 sine_16k.wav synth 1 sine 220-880`. Commit.
- [ ] **Step 2:** Write the README enumerating the env-var contract: `AUDIO2FACE_TEST_ONNX`, `RVC_TEST_*`, `LIVE2D_TEST_MODEL_PATH`, `LIVE2D_CUBISM_CORE_DIR`.
- [ ] **Step 3:** Write the macro:

```rust
// crates/core/tests/avatar_test_support.rs
#[macro_export]
macro_rules! skip_if_no_real_avatar_models {
    ($($var:literal),+) => {
        $(
            if std::env::var($var).ok().filter(|v| !v.is_empty()).is_none() {
                eprintln!("[skip] {} not set; skipping (set it to enable real-model test)", $var);
                return;
            }
        )+
    };
}
```

- [ ] **Step 4: Commit**

```bash
git commit -m "test(avatar): tier-1 audio fixture + env-var test helper"
```

### Task M2.1: `LipSyncNode` trait + blendshape Json envelope

**Files:**
- Create: `crates/core/src/nodes/lip_sync/mod.rs`
- Create: `crates/core/src/nodes/lip_sync/blendshape_json.rs`

- [ ] **Step 1: Failing test** that the envelope round-trips through `RuntimeData::Json` with the spec §3.3 shape.

```rust
#[test]
fn blendshape_envelope_shape() {
    let frame = BlendshapeFrame::new([0.0; 52], 12345, Some(7));
    let json = frame.to_json();
    assert_eq!(json["kind"], "blendshapes");
    assert_eq!(json["arkit_52"].as_array().unwrap().len(), 52);
    assert_eq!(json["pts_ms"], 12345);
    assert_eq!(json["turn_id"], 7);
    let back = BlendshapeFrame::from_json(&json).unwrap();
    assert_eq!(back, frame);
}
```

- [ ] **Step 2-5:** Implement `BlendshapeFrame` + `LipSyncNode` trait (`AsyncStreamingNode + audio→json`). Commit `feat(lipsync): trait + blendshape envelope`.

### Task M2.2: ONNX inference scaffolding (port of `Audio2FaceInference.cs`)

**Files:**
- Create: `crates/core/src/nodes/lip_sync/audio2face/inference.rs`
- Create: `crates/core/src/nodes/lip_sync/audio2face/mod.rs`
- Modify: `crates/core/Cargo.toml` (add `avatar-audio2face` feature; transitively pull `ort`)

- [ ] **Step 1:** Add the feature:

```toml
[features]
avatar-audio2face = ["dep:ort", "dep:hound", "avatar-emotion"]  # depends on M0
```

- [ ] **Step 2: Failing test** that loads the synthetic sine fixture, runs inference (gated on `AUDIO2FACE_TEST_ONNX`), and asserts the output is a 52-vector with values in `[-1.0, 2.0]` (per persona-engine's observed range) per inferred frame.

```rust
#[tokio::test]
async fn audio2face_inference_shape_matches_spec() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_ONNX");
    let inf = Audio2FaceInference::load(&std::env::var("AUDIO2FACE_TEST_ONNX").unwrap()).unwrap();
    let samples = read_sine_16k_fixture();
    let frames = inf.infer_chunk(&samples).unwrap();
    assert!(!frames.is_empty());
    for f in &frames {
        assert_eq!(f.len(), 52);
        assert!(f.iter().all(|v| (-2.0..=3.0).contains(v)),
                "value out of expected predicted range");
    }
}
```

- [ ] **Step 3:** Port `Audio2FaceInference.cs`'s session creation + chunk inference to Rust + `ort`. Use the `silero_vad` pattern at [`crates/core/src/nodes/silero_vad.rs:213`](../../../crates/core/src/nodes/silero_vad.rs#L213) for `Session::builder`. Document any C# → Rust deviation in code comments (e.g. tensor naming differences).
- [ ] **Step 4-5:** Verify + commit `feat(audio2face): ONNX inference port`.

### Task M2.3: Solver port (PGD + BVLS)

**Files:**
- Create: `crates/core/src/nodes/lip_sync/audio2face/solver.rs`

- [ ] **Step 1: Failing tests** for both solvers against a known input → expected output (capture from C# reference run; commit as fixture under `tests/fixtures/audio2face/solver_cases.json`).

```rust
#[test]
fn pgd_solver_matches_reference_case() {
    let cases = read_solver_cases();
    for case in cases.iter().filter(|c| c.solver == "pgd") {
        let out = solver::pgd::solve(&case.predictions, &case.bounds, &case.config);
        assert_close(&out, &case.expected, 1e-4);
    }
}
#[test]
fn bvls_solver_matches_reference_case() { /* same shape */ }
```

- [ ] **Step 2-5:** Port the C# `PgdBlendshapeSolver.cs` (12 KB) and `BvlsBlendshapeSolver.cs` (9 KB) to Rust. Both share `IBlendshapeSolver`. Match the C# math line-for-line where practical. Commit each separately (`feat(audio2face): PGD solver`, `feat(audio2face): BVLS solver`).

### Task M2.4: `ParamSmoother` port

**Files:**
- Create: `crates/core/src/nodes/lip_sync/audio2face/smoothing.rs`

- [ ] **Step 1-5:** Port `ParamSmoother.cs` with TDD. Single `feat(audio2face): smoother` commit.

### Task M2.5: `Audio2FaceLipSyncNode` end-to-end

**Files:**
- Create: `crates/core/src/nodes/lip_sync/audio2face/mod.rs` (wire inference + solver + smoother into a streaming node)

- [ ] **Step 1: Failing tests covering spec §3.4 invariants**

```rust
#[tokio::test]
async fn audio2face_emits_blendshapes_at_configured_framerate() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_ONNX");
    let node = Audio2FaceLipSyncNode::new(Audio2FaceConfig {
        model_path: env_path("AUDIO2FACE_TEST_ONNX"),
        solver: SolverChoice::Pgd,
        output_framerate: 60,
        smoothing_alpha: 0.5,
        ..Default::default()
    }).await.unwrap();
    let mut frames = vec![];
    let collected = collect_outputs(&node, sine_16k_chunks(1.0 /* sec */)).await;
    for out in &collected {
        if let RuntimeData::Json(v) = out { frames.push(v.clone()); }
    }
    // 60 fps × 1 s = 60 ± 5 frames
    assert!((55..=65).contains(&frames.len()));
}

#[tokio::test]
async fn pts_ms_monotonic_and_matches_audio_timestamps() { /* ... */ }

#[tokio::test]
async fn turn_id_propagated_when_audio_metadata_carries_it() { /* ... */ }

#[tokio::test]
async fn barge_in_clears_internal_buffer_and_stops_emit() {
    // Spec §6.3: implements in.barge_in aux port.
    let node = Audio2FaceLipSyncNode::new(default_test_config()).await.unwrap();
    push_audio(&node, sine_16k_chunks(0.5)).await;
    let mid = collect_so_far(&node);
    let initial = mid.len();
    node.handle_aux("barge_in", RuntimeData::Text("barge".into())).await.unwrap();
    push_audio(&node, sine_16k_chunks(0.5)).await;
    // After barge: previously-buffered audio should not produce new frames.
    let after = collect_so_far(&node);
    assert!(after.len() < initial * 2,
            "barge must drop pending output, not double through");
}
```

- [ ] **Step 2-5:** Implement; ensure the spec §3.4 behaviour invariants hold (especially that the node *emits ahead of audio playback*). Commit.

### Task M2.6: Coordinator `barge_in_targets` extension test

**Files:**
- Modify: `crates/core/src/nodes/conversation_coordinator.rs` (no code change; just verify the existing `dispatch_barge` loop iterates the configured target list — see [`conversation_coordinator.rs:293-306`](../../../crates/core/src/nodes/conversation_coordinator.rs#L293))
- Test: `crates/core/tests/coordinator_avatar_barge_test.rs` (new)

- [ ] **Step 1: Failing test** that builds a manifest with `barge_in_targets: ["llm", "audio", "audio2face_lipsync"]` and asserts that on barge, the audio2face node receives an `in.barge_in` publication. Use a `MockBargeReceiver` for the assert.
- [ ] **Step 2-5:** No coordinator code changes expected (it already iterates the list); the test just locks the contract. Commit `test(coordinator): barge propagates to avatar lipsync target`.

### Task M2.7: **Synthetic-emotion WebRTC integration test** (user authorization)

**Files:**
- Create: `crates/core/tests/avatar_synthetic_emotion_e2e.rs`

- [ ] **Step 1:** Build a manifest:
  ```
  text_source(static [EMOTION:🤩] hi [EMOTION:😊] bye)
    → emotion_extractor
        ├─(text)→ kokoro_tts → audio_track (webrtc out)
        │                        └─→ audio2face_lipsync ─→ blendshape_sink
        └─(json)→                                          emotion_sink
  ```

  Source the text from a static fixture (no LLM). Use kokoro_tts in real mode if `KOKORO_TTS_AVAILABLE=1` else swap a synthetic-text-to-synthetic-audio shim node.

- [ ] **Step 2: Assert:**
  - Two emotion Json events arrive at `emotion_sink` with `emoji ∈ {🤩, 😊}` in source order.
  - Continuous blendshape Json stream arrives at `blendshape_sink` with monotonic `pts_ms`.
  - WebRTC audio track receives audio frames (RMS > 0).
  - The text reaching kokoro_tts has the tags stripped (no `[EMOTION:` substring).

- [ ] **Step 3:** Implementation: assemble the manifest in code; spawn the WebRTC server-peer in test mode (per [`crates/transports/webrtc/tests/multi_track_e2e_test.rs`](../../../crates/transports/webrtc/tests/multi_track_e2e_test.rs) for the layout); wire the sinks as in-process collector nodes.

- [ ] **Step 4:** Run, verify pass. Commit:

```bash
git commit -m "test(avatar): synthetic-emotion WebRTC e2e through EmotionExtractor + Audio2Face"
```

---

## M2 actuals (2026-04-28, partial)

This pass shipped the **interface + integration plumbing** half of M2. The real Audio2Face ONNX port (M2.2 inference + M2.3 PGD/BVLS solvers + M2.4 smoother) is deferred to a follow-up turn — it's a multi-day port of ~33 KB of C# math from `external/handcrafted-persona-engine/.../LipSync/Audio2Face/` and gates on `AUDIO2FACE_TEST_ONNX` (the persona-engine bootstrapper-downloaded model) which we don't have on disk yet anyway.

**Why split it.** Landing the contract first means the synthetic-emotion e2e (M2.7) — the user-authorized milestone showpiece — actually runs in CI without the license-walled ONNX model. When the real Audio2Face arrives it slots into the same trait, same envelope, same manifest edge, and the existing assertions stand.

**Shipped:**

- **M2.0** — fixture infrastructure
  - `avatar-lipsync = ["avatar-emotion"]` feature flag in [`crates/core/Cargo.toml`](../../../crates/core/Cargo.toml)
  - `skip_if_no_real_avatar_models!()` macro + `sine_sweep_16k_mono()` helper in [`crates/core/tests/avatar_test_support.rs`](../../../crates/core/tests/avatar_test_support.rs) — pure-Rust sine generator, no `sox` dep
- **M2.1** — `LipSyncNode` trait + `BlendshapeFrame` envelope in [`crates/core/src/nodes/lip_sync/`](../../../crates/core/src/nodes/lip_sync/)
  - Canonical 52-element ARKit blendshape name array
  - Json round-trip with `kind: "blendshapes"`, `pts_ms`, optional `turn_id`
  - Trait declares `required_sample_rate()` + `required_channels()` for capability resolver wiring
- **M2.5-lite** — `SyntheticLipSyncNode` deterministic stand-in
  - RMS-driven jaw + smile activation, all other 49 ARKit slots = 0
  - Per-chunk emit with cumulative-ms `pts_ms`
  - `reset_clock()` for barge handling (M2.6 follow-up will wire it through `in.barge_in` aux port)
  - Useful for tests AND as a manifest fallback when `AUDIO2FACE_TEST_ONNX` isn't set
- **M2.7** — synthetic-emotion e2e in [`crates/core/tests/avatar_synthetic_emotion_e2e.rs`](../../../crates/core/tests/avatar_synthetic_emotion_e2e.rs)
  - Wires `EmotionExtractorNode → SyntheticLipSyncNode` with sine-sweep audio chunks
  - Asserts emotion event count + emoji + source order, blendshape stream monotonicity, jaw-movement-with-audio, tag stripping for the (synthetic) TTS path

**Deferred (next M2 turn):**

- **M2.2** — `Audio2FaceInference` Rust port (ONNX session + chunk inference) of [`external/.../Audio2FaceInference.cs`](../../../external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/TTS/Synthesis/LipSync/Audio2Face/Audio2FaceInference.cs)
- **M2.3** — ✅ `PgdBlendshapeSolver` + `BvlsBlendshapeSolver` Rust ports — landed in M2 actuals pass 2 below
- **M2.4** — ✅ smoother — landed in M2 actuals pass 2 below
- **M2.5** — `Audio2FaceLipSyncNode` end-to-end wrapping the above; gated on `avatar-audio2face = ["avatar-lipsync", "dep:ort"]`
- **M2.6** — coordinator `barge_in_targets` propagation test (covers both Audio2Face and Synthetic via `in.barge_in` aux port)

**Tests landed (15 total, all green):**
- 10 inline blendshape envelope tests in [`blendshape.rs`](../../../crates/core/src/nodes/lip_sync/blendshape.rs) (5 tests) and synthetic node tests in [`synthetic.rs`](../../../crates/core/src/nodes/lip_sync/synthetic.rs) (6 tests)
- 2 integration tests in [`avatar_synthetic_emotion_e2e.rs`](../../../crates/core/tests/avatar_synthetic_emotion_e2e.rs)
- All 10 prior `emotion_extractor_test` integration tests still green

**Build matrix verified:**
- `cargo build -p remotemedia-core` (default features) — clean.
- `cargo test -p remotemedia-core --features avatar-emotion,avatar-lipsync --lib lip_sync` — 13/13 green.
- `cargo test -p remotemedia-core --features avatar-emotion,avatar-lipsync --test avatar_synthetic_emotion_e2e` — 2/2 green.
- `cargo test -p remotemedia-core --features avatar-emotion,avatar-lipsync --test emotion_extractor_test` — 10/10 green (M0 still passes).

---

## M2 actuals (2026-04-28, pass 2 — solver math)

This pass ports the **math half** of the real Audio2Face stack:
`SolverMath` utilities, the `BlendshapeSolver` trait, both
`PgdBlendshapeSolver` and `BvlsBlendshapeSolver`, `ResponseCurves`,
plus the spec's `smoothing_alpha` knob as `ArkitSmoother`. All
self-contained — no `ort` crate, no model files, no NPZ readers.
Pure linear algebra, testable with synthetic small problems.

**Why split this way (revised after reading the C# tree).** The
persona-engine's `Audio2FaceInference.cs` (300 lines) has the only
hard dependency on the audio2face.onnx model + the Cubism Box-Muller
RNG + the GRU recurrent state. The solver math underneath it is
pure compute that runs on hand-crafted matrices. Landing the math
first means: (1) end-to-end ONNX testing has a target to plug into,
(2) the math is correctness-verified independently of ML model
quality, (3) when the audio2face.onnx model becomes available, only
the inference glue + the NPZ loader remain to be written.

**Shipped:**

- **M2.3** — `crates/core/src/nodes/lip_sync/audio2face/`
  - [`solver_math.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/solver_math.rs) — bounding-box diagonal, `Dᵀ`, `DᵀD`, regularization application; named multipliers `L2_MULTIPLIER`, `L1_MULTIPLIER`, `TEMPORAL_MULTIPLIER`
  - [`solver_trait.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/solver_trait.rs) — `BlendshapeSolver` trait (`solve` / `reset_temporal` / `save_temporal` / `restore_temporal`)
  - [`pgd_solver.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/pgd_solver.rs) — projected-gradient with LU-warm-started initial guess + 50-iter power-iteration step size
  - [`bvls_solver.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/bvls_solver.rs) — active-set BVLS with in-place Cholesky sub-solver
  - [`response_curves.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/response_curves.rs) — `ease_in` + `center_weighted` Hermite splines
- **M2.4** — [`crates/core/src/nodes/lip_sync/arkit_smoother.rs`](../../../crates/core/src/nodes/lip_sync/arkit_smoother.rs) — uniform EMA on the 52-vector for the spec's `smoothing_alpha` knob (per-axis tuning lives in the renderer's `ParamSmoother` per spec §3.4)

**Still deferred (final M2 pass):**

- **M2.2** — `Audio2FaceInference` Rust port: `ort`-based ONNX session, `IoBinding` setup, GRU state mgmt, deterministic Box-Muller noise generation. Gates on `audio2face.onnx` (persona-engine bootstrapper download).
- NPZ/NPY reader for `bs_skin.npz` + `model_data.npz` (delta matrix, frontal mask, neutral pose).
- **M2.5** — `Audio2FaceLipSyncNode` coordinator wiring inference + solver + smoother into a streaming node behind `avatar-audio2face` feature.
- **M2.6** — coordinator `barge_in_targets` propagation test against the real node.

**Tests landed (45 new, all green; 70 cumulative across the avatar code):**

| File | Tests | Coverage |
|---|---|---|
| `solver_math.rs` | 9 | bounding-box edge cases, transpose, DᵀD, regularization formula |
| `pgd_solver.rs` | 9 | K=1 / K=2 cases, box-clipping, temporal smoothing pull, save/restore round-trip, LU primitives, power iteration |
| `bvls_solver.rs` | 8 | same shape as PGD plus Cholesky-rejects-non-SPD, two-blendshape competition split |
| `response_curves.rs` | 10 | endpoints, monotonicity, clamping, degenerate spans |
| `arkit_smoother.rs` | 7 | first-frame passthrough, alpha=0 passthrough, alpha=1 hold, save/restore, decay |
| All prior lip_sync tests | 13 | unchanged, still green |
| Synthetic-emotion e2e | 2 | unchanged, still green |
| EmotionExtractor | 10 | unchanged, still green |
| `audio_clock_tap` | 4 | unchanged, still green |

**Build matrix verified:**
- `cargo test -p remotemedia-core --features avatar-lipsync --lib lip_sync` — 58/58 green.
- `cargo test -p remotemedia-core --features avatar-emotion,avatar-lipsync --test avatar_synthetic_emotion_e2e --test emotion_extractor_test` — 12/12 green.
- `cargo build -p remotemedia-core` (default features) — clean.

---

## M3 — `RVCNode`

Independent of avatar output. Self-contained tests; ships in parallel with M2 if labor permits. Inputs/outputs are all audio; no manifest entanglement with the renderer.

### Task M3.1: Failing tests

**Files:**
- Create: `crates/core/tests/rvc_test.rs`
- Create: `crates/core/tests/fixtures/rvc/README.md`

- [ ] **Step 1: Tests**

```rust
#[tokio::test]
async fn rvc_passthrough_does_not_explode() {
    skip_if_no_real_avatar_models!("RVC_TEST_MODEL_ONNX", "RVC_TEST_F0_MODEL");
    let node = RVCNode::new(default_rvc_config()).await.unwrap();
    let out = drive_with_sine_sweep(&node, /*seconds*/ 1.0).await;
    let rms = compute_rms(&out);
    assert!(rms > 0.001, "RVC output silent");
    assert!(rms < 1.0, "RVC output clipping");
}

#[tokio::test]
async fn rvc_chunk_boundaries_no_clicks() {
    skip_if_no_real_avatar_models!("RVC_TEST_MODEL_ONNX", "RVC_TEST_F0_MODEL");
    // Drive with small chunks vs one large chunk; concatenated outputs should differ
    // by less than X dB at chunk boundaries.
    let node = RVCNode::new(default_rvc_config()).await.unwrap();
    let small = drive_chunked(&node, &sine_sweep(1.0), 20 /*ms*/).await;
    let large = drive_chunked(&node, &sine_sweep(1.0), 1000 /*ms*/).await;
    assert_no_click_at_boundaries(&small, /*chunk_ms*/ 20);
    assert_similar_rms(&small, &large);
}

#[tokio::test]
async fn rvc_output_rate_matches_config() {
    // Resolver-side: assert the node declares its output sample_rate matches config.
    let caps = RVCNode::declared_capabilities(&serde_json::json!({"sample_rate_out": 40000}));
    assert_eq!(caps.audio_out().unwrap().sample_rate, ConstraintValue::Exact(40000));
}
```

- [ ] **Step 2:** Run, verify failing.

### Task M3.2: Inference + F0 pipeline

**Files:**
- Create: `crates/core/src/nodes/rvc/{mod,inference,f0,index}.rs`
- Modify: `crates/core/Cargo.toml` (add `avatar-rvc` feature)

- [ ] **Step 1:** Add feature: `avatar-rvc = ["dep:ort"]`.
- [ ] **Step 2:** Port the persona-engine RVC pipeline shape (see persona-engine's `TTS/Synthesis/Rvc/` if present; otherwise reference any open-source RVC ONNX wrapper). The factor that matters: load the model, run F0 estimator (CREPE or RMVPE per config), feed audio + F0 through main session, return resampled output.
- [ ] **Step 3-5:** Verify + commit `feat(rvc): ONNX inference pipeline`.

### Task M3.3: Capability declaration + resolver wiring

**Files:**
- Same node module.

- [ ] **Step 1: Failing test** that resolver inserts a resampler when `RVCNode(sample_rate_out=40000) → Audio2FaceLipSyncNode(sample_rate_in=16000)` are wired.

```rust
#[test]
fn resolver_inserts_resampler_between_rvc_and_audio2face() {
    let manifest = parse_manifest(r#"
nodes:
  - id: rvc
    node_type: RVC
    params: { sample_rate_out: 40000, ... }
  - id: lip
    node_type: Audio2FaceLipSync
connections:
  - { from: rvc, to: lip }
"#);
    let resolved = resolve(&manifest).unwrap();
    let inserted = resolved.find_inserted_node_between("rvc", "lip");
    assert_eq!(inserted.node_type, "AudioResample");
}
```

- [ ] **Step 2-5:** Should pass without code change once the `Configured` capability declaration is in place. Commit `test(rvc): resolver auto-inserts resampler against audio2face`.

---

## M4 — `Live2DRenderNode` (native wgpu + CubismCore)

The heaviest milestone. Adopts the validation report's recommendation: drop the Python model-state layer, link `Live2DCubismCore` directly, render in wgpu.

### Task M4.0: Cubism Core acquisition + `cubism-core-sys` scaffold

**Files:**
- Create: `crates/cubism-core-sys/{Cargo.toml,build.rs,wrapper.h,src/lib.rs,CUBISM_SDK.md}`
- Modify: `Cargo.toml` (workspace members + dependencies)

- [ ] **Step 1: Document acquisition** in `CUBISM_SDK.md`:
  - User downloads "Cubism SDK for Native" from <https://www.live2d.com/sdk/download/native/> (license-gated, manual).
  - User unpacks to a directory of their choice.
  - Sets `LIVE2D_CUBISM_CORE_DIR=/path/to/CubismSdkForNative-5.x` in their build environment.
  - The build.rs reads the env var, links the static lib for the host platform, runs bindgen.

- [ ] **Step 2: Failing build** without env var — `cargo build -p cubism-core-sys` should fail with a clear actionable error.

- [ ] **Step 3: Implement `build.rs`**

```rust
// crates/cubism-core-sys/build.rs
fn main() {
    let dir = std::env::var("LIVE2D_CUBISM_CORE_DIR")
        .expect("set LIVE2D_CUBISM_CORE_DIR — see crates/cubism-core-sys/CUBISM_SDK.md");
    let dir = std::path::PathBuf::from(dir);

    println!("cargo:rerun-if-env-changed=LIVE2D_CUBISM_CORE_DIR");
    println!("cargo:rerun-if-changed=wrapper.h");

    // Per-platform static lib path under Core/lib/<platform>/
    let lib_dir = dir.join("Core/lib").join(host_lib_subdir());
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static={}", host_lib_name()); // e.g. "Live2DCubismCore"

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}/Core/include", dir.display()))
        .allowlist_function("csm.*")
        .allowlist_type("csm.*")
        .generate()
        .expect("bindgen");

    bindings
        .write_to_file(std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .unwrap();
}
fn host_lib_subdir() -> &'static str {
    if cfg!(target_os = "macos") { "macos" }
    else if cfg!(target_os = "linux") { "linux/x86_64" }
    else if cfg!(target_os = "windows") { "windows/x86_64" }
    else { panic!("unsupported host") }
}
fn host_lib_name() -> &'static str {
    "Live2DCubismCore" // see Core/lib/<platform>/ for actual filename
}
```

- [ ] **Step 4:** `wrapper.h` includes `<Live2DCubismCore.h>`.
- [ ] **Step 5: Verify** with `LIVE2D_CUBISM_CORE_DIR` set: `cargo build -p cubism-core-sys` succeeds.
- [ ] **Step 6: Commit** `feat: cubism-core-sys (Live2DCubismCore bindings)`.

### Task M4.1: `cubism-core` safe wrapper — `Moc`, `Model`, post-deformer mesh accessors

**Files:**
- Create: `crates/cubism-core/{Cargo.toml,src/lib.rs,src/drawable.rs,src/parameters.rs,src/physics.rs}`

- [ ] **Step 1: Failing test** that loads a Live2D `.moc3` file and reads drawable count, vertex positions, render order, opacities — *without any GL / wgpu*.

```rust
#[test]
fn cubism_core_loads_moc3_and_exposes_drawable_data() {
    let path = std::env::var("LIVE2D_TEST_MODEL_PATH")
        .expect("set LIVE2D_TEST_MODEL_PATH to path/to/foo.moc3");
    let moc = Moc::load_from_file(&path).unwrap();
    let mut model = Model::from_moc(&moc);
    model.update();
    let drawables = model.drawables();
    assert!(drawables.len() > 0);
    let d0 = drawables.get(0).unwrap();
    assert!(!d0.vertex_positions().is_empty());
    assert!(d0.opacity() >= 0.0);
}
```

- [ ] **Step 2:** Run, verify failing.
- [ ] **Step 3:** Implement: `Moc::load_from_file`, `Model::from_moc`, `Model::update(delta_seconds)`, `Drawable::{vertex_positions, opacity, render_order, vertex_uvs, indices, dynamic_flags}`. Map the `csmGetDrawable*` pointers to Rust slices keyed by lifetime of `&Model`.
- [ ] **Step 4-5:** Verify + commit `feat(cubism-core): safe wrapper with post-deformer mesh accessors`.

### Task M4.2: Cubism `.model3.json` loader

**Files:**
- Add: `crates/cubism-core/src/model_json.rs`

- [ ] **Step 1-5:** Port enough of `.model3.json` parsing to load textures, motions, expressions, physics, and PoseGroups. Reference: persona-engine's C# loader at [`external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/Live2D/`](../../../external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/Live2D/) (just for the JSON shape). Commit.

### Task M4.3: `MockBackend` + input-arbitration tests

**Files:**
- Create: `crates/core/src/nodes/live2d_render/state.rs`
- Create: `crates/core/src/nodes/live2d_render/backend_trait.rs`
- Create: `crates/core/src/nodes/live2d_render/test_support.rs` (gated `#[cfg(any(test, feature = "test-support"))]`)
- Create: `crates/core/tests/live2d_render_state_test.rs`

- [ ] **Step 1:** Define `trait Live2DBackend` with `render_frame(&mut self, vbridger_params, expression_id, motion_id) -> RgbFrame`.

- [ ] **Step 2: Failing tests** for the spec §6.1 input arbitration loop:

```rust
#[tokio::test]
async fn renderer_samples_blendshape_keyframe_at_audio_clock_pts() {
    let mut state = Live2DRenderState::new(default_state_config());
    state.push_blendshape(BlendshapeFrame::new([0.5; 52], 100, None));
    state.push_blendshape(BlendshapeFrame::new([1.0; 52], 200, None));
    state.update_audio_clock(150);
    let pose = state.compute_pose();
    // Linear interp at midpoint.
    assert_close(pose.mouth_value("ParamJawOpen"), 0.75, 1e-3);
}

#[tokio::test]
async fn renderer_evicts_stale_blendshape_frames_after_200ms() {
    // Spec §6.1: evict pts_ms < audio_clock_ms - 200
    let mut state = Live2DRenderState::new(default_state_config());
    state.push_blendshape(BlendshapeFrame::new([0.5; 52], 100, None));
    state.update_audio_clock(500);
    state.push_blendshape(BlendshapeFrame::new([1.0; 52], 510, None));
    let ring = state.blendshape_ring_for_test();
    assert_eq!(ring.len(), 1);
    assert_eq!(ring[0].pts_ms, 510);
}

#[tokio::test]
async fn renderer_interpolates_to_neutral_when_audio_clock_quiet() {
    let mut state = Live2DRenderState::new(default_state_config());
    state.push_blendshape(BlendshapeFrame::new([1.0; 52], 100, None));
    state.update_audio_clock(110);
    state.tick_no_clock_for(/*ms*/ 200); // simulate 200 ms with no clock publish
    let pose = state.compute_pose();
    assert_close(pose.mouth_value("ParamJawOpen"), 0.0, 1e-2,
                 "must interpolate toward neutral");
}

#[tokio::test]
async fn emotion_event_drives_expression_and_motion() {
    let mut state = Live2DRenderState::new(default_state_config());
    state.push_emotion("🤩");
    let pose = state.compute_pose();
    assert_eq!(pose.expression_id, "excited_star");
    assert_eq!(pose.motion_group, "Excited");
}

#[tokio::test]
async fn emotion_expires_after_hold_seconds_back_to_neutral() {
    let mut state = Live2DRenderState::new(StateConfig {
        expression_hold_seconds: 1.0, ..default_state_config()
    });
    state.push_emotion("🤩");
    state.tick_wall(Duration::from_millis(1100));
    let pose = state.compute_pose();
    assert_eq!(pose.expression_id, "neutral");
}

#[tokio::test]
async fn barge_in_clears_blendshape_ring_but_NOT_emotion() {
    // Spec §6.3: emotion is NOT cleared by barge.
    let mut state = Live2DRenderState::new(default_state_config());
    state.push_blendshape(BlendshapeFrame::new([1.0; 52], 100, None));
    state.push_emotion("🤩");
    state.handle_barge();
    assert!(state.blendshape_ring_for_test().is_empty());
    assert_eq!(state.compute_pose().expression_id, "excited_star");
}

#[tokio::test]
async fn idle_blink_fires_when_no_emotion_active() {
    let mut state = Live2DRenderState::new(StateConfig {
        blink_interval_min_ms: 100, blink_interval_max_ms: 100, ..default_state_config()
    });
    state.tick_wall(Duration::from_millis(150));
    let pose = state.compute_pose();
    assert!(pose.eye_open < 1.0, "blink in progress");
}
```

- [ ] **Step 3-4:** Implement `Live2DRenderState` and the `MockBackend`. The state machine is the bulk of spec §6.1; the wgpu rendering is layered on top in M4.4.
- [ ] **Step 5: Commit** `feat(live2d-render): input arbitration state machine`.

### Task M4.4: wgpu backend — drawable graph + ordered draw

**Files:**
- Create: `crates/core/src/nodes/live2d_render/wgpu_backend/mod.rs`
- Create: `crates/core/src/nodes/live2d_render/wgpu_backend/shaders/{normal,additive,multiply,mask}.wgsl`
- Create: `crates/core/tests/live2d_render_wgpu_test.rs`

> **Spec §10 risk callout**: This is the largest implementation-time risk. Mask passes (clip + inverted), three blend modes (Normal/Additive/Multiply), premultiplied-alpha quirks, drawable cull flags, ordered draw — all must match Cubism's reference renderer's *visual* output bit-for-bit-ish, not just "looks like Live2D." Reference: Cubism's open-source `CubismRenderer_OpenGLES2`, `CubismRenderer_D3D11`, `CubismRenderer_Metal`. Cross-reference the structure of `CubismRenderer_OpenGLES2::DoDrawModel` for the pre-pass + ordered-draw shape.

- [ ] **Step 1: Failing test** with a tiny synthetic `Model` (3 drawables, no masks): assert wgpu render-to-texture produces non-zero pixels.

```rust
#[tokio::test]
async fn wgpu_backend_renders_synthetic_model_to_nonzero_pixels() {
    let backend = WgpuBackend::new_headless(1280, 720, PixelFormat::Rgb24).await.unwrap();
    let frame = backend.render_synthetic_test_model(SYNTHETIC_MODEL_3_DRAWABLES);
    let nonzero = frame.iter().filter(|&&b| b != 0).count();
    assert!(nonzero > 100, "expected non-trivial pixel coverage");
}
```

- [ ] **Step 2-3:** Implement headless wgpu device init, render-to-texture pipeline, RGBA → RGB24/YUV420p readback. Per-drawable: bind texture, apply blend mode, draw indexed.
- [ ] **Step 4:** Implement mask pre-pass.
- [ ] **Step 5:** Verify + commit per logical chunk: `feat(live2d-render): wgpu device init`, `feat(live2d-render): ordered drawable draw`, `feat(live2d-render): mask pre-pass`, `feat(live2d-render): blend modes`.

### Task M4.5: `Live2DRenderNode` — wire state + wgpu + emit Video

**Files:**
- Create: `crates/core/src/nodes/live2d_render/mod.rs`

- [ ] **Step 1: Failing test** that the node, given a `MockBackend`, emits one `RuntimeData::Video` frame per render tick at the configured framerate, with the configured `stream_id`.

```rust
#[tokio::test]
async fn live2d_render_emits_video_at_30fps_with_stream_id() {
    let node = Live2DRenderNode::new_with_backend(MockBackend::new(), default_render_config()).await.unwrap();
    let outputs = drive_for_seconds(&node, 1.0).await;
    let video_frames: Vec<_> = outputs.iter().filter_map(|o| o.as_video()).collect();
    assert!((28..=32).contains(&video_frames.len()));
    assert!(video_frames.iter().all(|f| f.stream_id.as_deref() == Some("avatar")));
}

#[tokio::test]
async fn render_continues_when_audio_clock_is_quiet() {
    // Renderer is decoupled — the test asserts no input pressure stalls a tick.
    let node = Live2DRenderNode::new_with_backend(MockBackend::new(), default_render_config()).await.unwrap();
    let outputs = drive_for_seconds_no_audio_clock(&node, 1.0).await;
    let video_frames: Vec<_> = outputs.iter().filter_map(|o| o.as_video()).collect();
    assert!(video_frames.len() >= 28);
}
```

- [ ] **Step 2-5:** Implement, verify, commit. Wire the `audio_clock_node_id` config to subscribe via `ControlAddress::node_out("audio").with_port("clock")`.

### Task M4.6: Full WebRTC pipeline e2e (gated)

**Files:**
- Create: `crates/core/tests/avatar_full_pipeline_e2e.rs`

- [ ] **Step 1: Failing test** matching spec §4.1, gated on all four env vars:

```rust
#[tokio::test]
async fn full_avatar_pipeline_emits_video_track_with_emotion_and_lipsync() {
    skip_if_no_real_avatar_models!(
        "AUDIO2FACE_TEST_ONNX",
        "LIVE2D_TEST_MODEL_PATH",
        "LIVE2D_CUBISM_CORE_DIR"
    );
    // Manifest:
    //   text_source([EMOTION:🤩]hi[EMOTION:😊]bye)
    //     → emotion_extractor
    //         ├─(text)→ kokoro_tts → audio_track
    //         │                       └─→ audio2face_lipsync ─┐
    //         └─(json) ──────────────────────────────────────────┤
    //                                                           ├→ live2d_render → video_track
    //   audio_track.AudioSender ──tap──→ audio.out.clock ────────┘
    let session = build_avatar_test_session().await;
    let video_frames = collect_video_frames_for_seconds(&session, 2.0).await;
    assert!(video_frames.len() >= 50, "≥ 50 frames in 2 s");
    assert!(video_frames.iter().all(|f| f.stream_id.as_deref() == Some("avatar")));
    let nonzero = video_frames[20].pixels.iter().filter(|&&b| b != 0).count();
    assert!(nonzero > 1000, "non-trivial pixels mid-stream");
}
```

- [ ] **Step 2-5:** Implement the test harness, verify, commit `test(avatar): full pipeline e2e (gated on Cubism + ONNX + Live2D model env vars)`.

### Task M4.7: Spec doc updates per validation-report knock-ons

**Files:**
- Modify: `docs/superpowers/specs/2026-04-27-live2d-audio2face-rvc-avatar-design.md`

- [ ] Apply the four knock-on changes the validation report enumerated:
  1. Drop §5.2 IPC schema and §5.4 Python-layer responsibilities; replace §5 with the native-wgpu+CubismCore description.
  2. Update §3.5 backend list.
  3. Update §10 risks (resolve `live2d-py` gate; promote wgpu-Cubism-semantics risk).
  4. Drop §11(1) follow-up.
- [ ] Commit `docs(avatar): align spec with native-wgpu renderer pivot`.

---

## M5 — Smoke example + docs

### Task M5.1: `avatar_smoke` example

**Files:**
- Create: `crates/core/examples/avatar_smoke.rs`

- [ ] **Step 1-5:** Mirror an existing smoke example structure. Read four env vars (audio2face, rvc, live2d, cubism), bail with friendly message if absent, otherwise run the §4.1 manifest for 5 s and emit an MP4 of the resulting video track. Commit.

### Task M5.2: Per-component README files

- [ ] `crates/cubism-core-sys/README.md` — license + acquisition.
- [ ] `crates/cubism-core/README.md` — quick start.
- [ ] One section in `docs/` covering the avatar manifest config end-to-end.
- [ ] Commit each.

### Task M5.3: CI matrix as concrete jobs

**Files:**
- Modify: `.github/workflows/<existing>.yml` or new `avatar.yml`

- [ ] Jobs:
  - `cargo build` (no features) — Linux, macOS — verifies feature gates leave hot path untouched.
  - `cargo build --features avatar-emotion` — Linux, macOS.
  - `cargo build --features avatar-audio2face` — Linux, macOS.
  - `cargo build --features avatar-rvc` — Linux, macOS.
  - `cargo build --features avatar-render` — Linux, macOS — **gated** on `LIVE2D_CUBISM_CORE_DIR`; document as manual-trigger if no cached SDK.
  - `cargo test --features avatar-emotion,avatar-audio2face` — synthetic-emotion e2e (M2.7) — runs everywhere; tier-2 portions skip via `skip_if_no_real_avatar_models!`.
  - `cargo test --features avatar-render` — full e2e — gated on all model env vars.
  - `cargo test -p remotemedia-webrtc-transport audio_clock_tap` — Linux.
- [ ] Commit `ci: avatar pipeline jobs`.

---

## Verification gates

Before declaring "done":

- [ ] `cargo build` (no features) — succeeds, no avatar pieces present in the hot path.
- [ ] `cargo build --features avatar-emotion,avatar-audio2face,avatar-rvc` — all three audio-side features compile together on Linux + macOS without Cubism Core present.
- [ ] `cargo build --features avatar-render` — succeeds with `LIVE2D_CUBISM_CORE_DIR` set on macOS + Linux.
- [ ] `cargo test --features avatar-emotion` — green; M0 fully self-contained.
- [ ] `cargo test -p remotemedia-webrtc-transport --test audio_clock_tap_test` — green.
- [ ] `cargo test --test avatar_synthetic_emotion_e2e --features avatar-audio2face` — green on hosts without real models (skips inference assertions cleanly).
- [ ] `cargo test --test avatar_synthetic_emotion_e2e --features avatar-audio2face --ignored` — green on hosts with `AUDIO2FACE_TEST_ONNX` set.
- [ ] `cargo test --test avatar_full_pipeline_e2e --features avatar-render --ignored` — green on hosts with all four env vars.
- [ ] `cargo run --example avatar_smoke --features avatar-render` — produces a 5-second MP4 with non-zero pixel content and audio.
- [ ] Existing webrtc tests still green after M1.
- [ ] Existing Python multiprocess tests still green throughout (we did not touch that path).

---

## Commit & PR hygiene

- One commit per task step that produces working code.
- Semantic commit prefixes: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`.
- Do not bundle unrelated changes.
- Cubism Core is **not** vendored — it stays a build-environment dependency (license-gated). This is by design; do not attempt to commit any portion of `Live2DCubismCore.{a,lib}` or its headers.
- The `external/handcrafted-persona-engine/` clone is gitignored already and does not need to be committed.

## Out-of-scope reminders (per spec §2 + validation report)

- No Spout/NDI/RTMP video sinks (renderer emits `RuntimeData::Video` only).
- No phoneme-driven lip-sync (interface accommodates it; impl is a follow-up).
- No music separation / background-music ducking.
- No automatic model fetching; users supply Audio2Face ONNX + RVC + Live2D model on disk.
- No subtitle / UI overlay rendering.
- No avatar runtime control plane (live emotion poke / motion override) beyond `barge_in`.
- No Python model-state layer; no iceoryx2 mesh IPC (per validation report).
- No Cubism Core redistribution.

## Follow-ups unlocked by this plan (not in scope)

1. `phoneme-lipsync` — `PhonemeLipSyncNode` consuming Kokoro's `TimedPhoneme[]` metadata for tighter sync on TTS-only paths.
2. `spout-video-sink` — Windows-only sink so OBS picks up the avatar locally without WebRTC round-trip.
3. `avatar-control-plane` — runtime emotion override / expression freeze / motion poke aux ports on the renderer.
4. iOS / RealityKit `Live2DBackend` impl behind the trait shipped here.
