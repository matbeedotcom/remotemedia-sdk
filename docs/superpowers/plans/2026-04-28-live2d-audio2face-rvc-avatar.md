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
| M2 | `LipSyncNode` trait + `Audio2FaceLipSyncNode` + synthetic-emotion e2e + barge propagation | 🟢 shipped — full M2 set landed across 5 incremental passes (interface+synthetic+e2e, solver math, ONNX inference+bundle loaders, Audio2FaceLipSyncNode coordinator, manifest-level barge propagation via session-control bus). See [M2 actuals](#m2-actuals-2026-04-28-partial) and passes 2–5 below. |
| M3 | `RVCNode` | `cargo test --features avatar-rvc` green; tier-2 tests pass on env-var-bearing host |
| M4 | `Live2DRenderNode` (native wgpu + CubismCore) | 🟡 in progress — M4.0–M4.3 shipped + M4.4 pass 1 (wgpu device + ordered draw + Normal blend + readback; first visible Aria render) shipped; Aria installer landed; M4.4 pass 2 (mask pre-pass + dedicated Additive/Multiplicative pipelines) + M4.5–M4.7 outstanding. See [M4.0 actuals](#m40-actuals-2026-04-28) + [M4.1 actuals](#m41-actuals-2026-04-28) + [M4.2 actuals](#m42-actuals-2026-04-28) + [M4.3 actuals](#m43-actuals-2026-04-28) + [M4.4 actuals (pass 1)](#m44-actuals-2026-04-28-pass-1). |

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

- **M2.2** — ✅ landed in M2 actuals pass 3 below
- NPZ/NPY reader — ✅ landed in M2 actuals pass 3 below
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

## M2 actuals (2026-04-28, pass 3 — ONNX inference + bundle loaders)

This pass ports the **inference + bundle loading half** of Audio2Face.
NPY/NPZ readers, identity + path resolver, blendshape config + data
loaders, and the `Audio2FaceInference` ONNX wrapper. Gated behind a
new `avatar-audio2face` feature flag pulling `ort` + `zip`. Tier-2
integration tests against the actual 738 MiB persona-engine bundle
pass when `AUDIO2FACE_TEST_BUNDLE` points at the unpacked dir; skip
cleanly otherwise.

**Shipped:**

- **M2.2 inference layer** — [`crates/core/src/nodes/lip_sync/audio2face/inference.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/inference.rs)
  - `Audio2FaceInference::load(path, use_gpu)` — ort 2.0.0-rc.10 session + commit_from_file
  - `Audio2FaceInference::infer(audio, identity_index)` → `Audio2FaceOutput { skin_flat, eye_flat, frame_count }`
  - GRU recurrent state carried across calls (`reset_state` / `save_gru_state` / `restore_gru_state` for barge handling)
  - Deterministic Box-Muller noise via embedded SplitMix64 (cross-platform-deterministic; C# uses `System.Random` which isn't)
  - Pre-allocated 64 MB noise tensor (1×3×60×88831), cloned per call (perf knob to revisit later)
- **NPY/NPZ readers** — [`npy.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/npy.rs) + [`npz.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/npz.rs)
  - `<f4` and `<i4` only (matches what the bundle ships)
  - Versions 1 + 2 supported; bad-magic / wrong-dtype / unsupported-version produce actionable errors
- **BlendshapeConfig + BlendshapeData** — [`blendshape_data.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/blendshape_data.rs)
  - Parses `bs_skin_config_<Identity>.json` + assembles the dense `[V*3, K]` delta matrix from the per-blendshape NPYs in `bs_skin_<Identity>.npz` and the `model_data_<Identity>.npz` extras
  - Validates shape consistency (active-pose array length = num_poses, multipliers/offsets match)
- **Identity + bundle resolver** — [`identity.rs`](../../../crates/core/src/nodes/lip_sync/audio2face/identity.rs)
  - `Audio2FaceIdentity { Claire, James, Mark }` enum with `one_hot_index()` and `suffix()`
  - `BundlePaths::new(root, identity)` resolves every per-identity filename in the persona-engine bundle layout

**Tests landed (16 unit + 4 tier-2 integration):**

| File | Tests | Notes |
|---|---|---|
| `npy.rs` | 9 | round-trip f32 1d/2d, i32, wrong dtype, bad magic, unsupported version, parse_header variants |
| `npz.rs` | 3 | open + read entries, missing-entry error, has_entry |
| `identity.rs` | 4 | one-hot indices, default, paths per identity, serde round-trip |
| `blendshape_data.rs` | 3 | parse Claire-shaped config, active-indices match input, mismatched-array rejection |
| `inference.rs` (unit) | 5 | Box-Muller determinism, mean/stddev sanity, odd count, splitmix64 range, output dim constants |
| `audio2face_inference_test.rs` (tier-2) | 4 | parses real Claire config (39 active poses), loads real Claire blendshape data, runs real ONNX inference (3.6s for first call cold), GRU state determinism (different across calls / matches across reset) |

**Cumulative test count across the avatar code: 86 unit + 6 integration = 92 tests, all green.**

**Build matrix verified:**
- `cargo test -p remotemedia-core --features avatar-audio2face --lib lip_sync::audio2face` — 63/63 green.
- `cargo test --features avatar-audio2face --test audio2face_inference_test` (no env var) — 4/4 skipped cleanly in 0.00s.
- `AUDIO2FACE_TEST_BUNDLE=$PWD/models/audio2face cargo test … --test audio2face_inference_test` — 4/4 green; first inference cold ~3.6s on Apple Silicon CPU.
- `cargo build -p remotemedia-core` (default features) — clean.

**Surprises worth noting**:
1. Bundle filename is `network.onnx`, not `audio2face.onnx` — installer + paths updated.
2. Bundle ships **3 identities** (Claire/James/Mark). LipSync node config (M2.5) needs an `identity` knob.
3. Claire's active-pose count is **39**, not 47 as my plan estimate — the model deliberately abstains from predicting eye-look L/R, jaw L/R, mouth L/R, and tongueOut from audio alone. The PGD/BVLS solvers run in 39-D, not 52-D.
4. ort 2.0.0-rc.10 `Tensor::from_array` requires owned `Vec`; the C# `OrtValue::CreateTensorValueFromMemory` zero-copy trick isn't available in the same form. We pay a ~64 MB clone per inference for the noise tensor — flagged as a perf-tuning task once renderer (M4) lands and we can profile against real audio cadence.

**Still deferred (final M2 pass — needs renderer to be relevant):**

- **M2.5** — ✅ landed in M2 actuals pass 4 below
- **M2.6** — coordinator `barge_in_targets` propagation test against the manifest-level `["llm", "audio", "audio2face_lipsync"]` target list (covers reset reaching the node via session-control bus, beyond the in-process `barge()` API exercised by M2.5).

---

## M2 actuals (2026-04-28, pass 4 — Audio2FaceLipSyncNode coordinator)

This pass wires the previous three passes' primitives (inference, blendshape config + data, PGD/BVLS solvers, smoother) into a **streaming `LipSyncNode`** that consumes `RuntimeData::Audio @ 16 kHz` and emits `RuntimeData::Json {kind:"blendshapes", arkit_52, pts_ms}` envelopes per spec §3.4.

**Shipped:**

- **`Audio2FaceLipSyncNode`** — [`crates/core/src/nodes/lip_sync/audio2face_node.rs`](../../../crates/core/src/nodes/lip_sync/audio2face_node.rs)
  - `Audio2FaceLipSyncConfig { bundle_path, identity, solver, use_gpu, smoothing_alpha }` — solver is `pgd` or `bvls` (lowercase serde)
  - `Audio2FaceLipSyncNode::load(config)` — assembles inference + bundle data + boxed `BlendshapeSolver` + `ArkitSmoother` from disk; one-time cost (~3.6 s on Apple Silicon CPU for ort session + bundle parse)
  - `process_streaming` accumulates audio in a buffer and drains 1-second windows: per window runs one inference (30 center frames), and for each frame computes `delta = skin - neutral_skin` → frontal-mask gather → solve → expand to 52-D → multiplier/offset → smooth → emit
  - `pts_ms` math: `cum_window_ms + f * 1000.0 / 30.0` (f64 step, u64 store) — monotonic, sub-1000ms aligned to the *center* of each window
  - **Barge** handling: in-band `RuntimeData::Json {kind:"barge_in"}` envelope OR direct `Self::barge()` call resets inference GRU + solver temporal pull + smoother + audio buffer + cum_window_ms in one shot
  - Non-Audio inputs pass through (mirrors `silero_vad` / `synthetic`)
  - 22.05 kHz / non-16 kHz inputs error with an actionable message pointing at the capability resolver
- **Module wiring** — [`crates/core/src/nodes/lip_sync/mod.rs`](../../../crates/core/src/nodes/lip_sync/mod.rs)
  - Re-exports `Audio2FaceLipSyncConfig`, `Audio2FaceLipSyncNode`, `Audio2FaceSolverChoice` (renamed from `SolverChoice` to avoid collision in user code) under `#[cfg(feature = "avatar-audio2face")]`

**Tests landed (3 unit + 8 tier-2 integration):**

| File | Tests | Notes |
|---|---|---|
| `audio2face_node.rs` (unit) | 3 | solver-choice serde lowercase, config defaults, `process()` rejects non-streaming with the documented error message |
| `audio2face_lipsync_node_test.rs` (tier-2) | 8 | one-second window → exactly 30 frames; pts_ms monotonic + bounded by audio time over 2-window run; chunk accumulation across two half-second sends; non-Audio passthrough; 22.05 kHz error; barge clears state + restarts pts_ms; non-trivial blendshape activity on loud audio; BVLS produces valid output |

**Build + run matrix verified:**
- `cargo build -p remotemedia-core --features avatar-audio2face` — clean.
- `cargo test -p remotemedia-core --features avatar-audio2face --lib lip_sync::audio2face_node` — 3/3 green.
- `AUDIO2FACE_TEST_BUNDLE=$PWD/models/audio2face cargo test … --test audio2face_lipsync_node_test` — 8/8 green in 27.83 s (one bundle load amortized across all tests in the harness; per-window inference is ~50 ms on Apple Silicon CPU).
- `AUDIO2FACE_TEST_BUNDLE=$PWD/models/audio2face cargo test … --test audio2face_inference_test` — 4/4 still green (no regressions in the inference layer).

**Cumulative test count across the avatar code: 89 unit + 14 integration = 103 tests, all green.**

**Design choices worth flagging:**

1. **`parking_lot::Mutex` over `tokio::sync::Mutex`** for inference + solver + smoother + buffer state. None of the operations span an `.await`, so the cheaper non-async lock works and matches `silero_vad`'s pattern shape.
2. **No response curves applied** in the lip-sync node. Per spec §3.4 the renderer's ARKit→VBridger mapper is where per-axis curves belong. The `response_curves` module is public for the renderer (M4) to consume.
3. **No upstream resampling**. The capability resolver (spec 023) is the right place to insert a resampler when an upstream node (e.g. `RVCNode` at 40 kHz) feeds this node's 16 kHz requirement. The M2.5 node enforces `sample_rate == 16_000` at runtime as a hard gate.
4. **Barge envelope is `{kind:"barge_in"}`**. Wire-format-agnostic; the M2.6 control-bus test will assert this reaches the node when the coordinator's `barge_in_targets = ["llm", "audio", "audio2face_lipsync"]` propagation fires.

**Still deferred (final M2 pass):**

- **M2.6** — ✅ landed in M2 actuals pass 5 below

---

## M2 actuals (2026-04-28, pass 5 — M2.6 manifest-level barge propagation)

This pass closes M2 by wiring the **session router → node** half of the barge path, so the coordinator's existing `barge_in_targets` mechanism actually clears `&self`-held state on lip-sync nodes (not just cancels in-flight calls). The runtime change is small and principled — it activates a previously-dormant `process_control_message` trait method that several other nodes already implement.

**Why this needed runtime work, not just node work:**

The router's filter task swallowed `barge_in` envelopes — calling `cancel.notify_waiters()` and then dropping the frame. That works for nodes whose only barge response is in-flight cancellation (e.g. `OpenAIChatNode`'s reqwest stream drop), but the lip-sync nodes need to clear persistent state (audio accumulator, GRU, solver temporal pull, smoother, pts clock) that no future-cancellation can touch. So the router needed to deliver the envelope to the node — and `AsyncNodeWrapper`, which bridges `AsyncStreamingNode → StreamingNode`, was missing the `process_control_message` forward, meaning even nodes that overrode it would never have been called.

**Shipped:**

- **`AsyncNodeWrapper::process_control_message` forward** — [`crates/core/src/nodes/streaming_node.rs`](../../../crates/core/src/nodes/streaming_node.rs#L527)
  - Adds the missing async forward from the unified `StreamingNode` trait to the underlying `AsyncStreamingNode`. Backwards-compatible: nodes that don't override return `Ok(false)` (the default).
- **Session router barge dispatch** — [`crates/core/src/transport/session_router.rs`](../../../crates/core/src/transport/session_router.rs)
  - Filter task: keeps firing `cancel.notify_waiters()` (universal cancellation), and now ALSO forwards the wrapped barge envelope through `filt_tx`.
  - Main task: detects `aux_port_of(&input) == Some(BARGE_IN_PORT)` at the top of its loop and dispatches via `node_ref.process_control_message(input, Some(session_id))` — bypasses perf instrumentation + `process_streaming_async`. Errors logged at `debug!` and ignored (fire-and-forget).
- **Lip-sync `process_control_message` overrides** — both lip-sync impls inspect the envelope's wrapped aux port and clear state on `barge_in`:
  - [`Audio2FaceLipSyncNode::process_control_message`](../../../crates/core/src/nodes/lip_sync/audio2face_node.rs) → calls `self.barge()` (drops GRU, solver temporal, smoother, audio buffer, `cum_window_ms`)
  - [`SyntheticLipSyncNode::process_control_message`](../../../crates/core/src/nodes/lip_sync/synthetic.rs) → calls `self.reset_clock()`
- **`SyntheticLipSyncNodeFactory` registered** — [`crates/core/src/nodes/streaming_registry.rs`](../../../crates/core/src/nodes/streaming_registry.rs) + [`core_provider.rs`](../../../crates/core/src/nodes/core_provider.rs); makes the synthetic stand-in usable in real manifests (not just the unit-test direct-construction path), and gives M2.6's integration test a routable target without the heavy Audio2Face bundle.

**Tests landed (4 unit + 4 integration; +2 over M2.5's 89/14 baseline → 91 unit + 16 integration = 107 tests):**

| File | Tests | Notes |
|---|---|---|
| `synthetic.rs` (unit) | +2 | `process_control_message_barge_in_resets_clock`, `process_control_message_ignores_non_barge` |
| `audio2face_lipsync_node_test.rs` (tier-2) | +2 | `process_control_message_barge_in_clears_state`, `process_control_message_ignores_non_barge` |
| `lipsync_barge_propagation_test.rs` (integration) | +2 | end-to-end through `SessionRouter` + `SessionControl::publish` against `SyntheticLipSyncNode`: `manifest_barge_in_resets_lipsync_pts_ms` + `manifest_barge_in_to_other_node_is_no_op` (pins the routing so a future broadcast bug fails loudly) |

**Build + run matrix verified:**
- `cargo build -p remotemedia-core --features avatar-audio2face` — clean.
- `cargo test -p remotemedia-core --features avatar-audio2face --lib lip_sync` — 88/88 green (up from 86, +2 new synthetic tests).
- `cargo test -p remotemedia-core --features avatar-audio2face --test session_control_integration` — 8/8 green (no regressions in pre-existing control-bus tests; the routing change is non-breaking).
- `cargo test -p remotemedia-core --features avatar-audio2face --lib transport` — 52/52 green.
- `cargo test -p remotemedia-core --features avatar-audio2face --test lipsync_barge_propagation_test` — 2/2 green.
- `AUDIO2FACE_TEST_BUNDLE=$PWD/models/audio2face cargo test … --test audio2face_lipsync_node_test` — 10/10 green (8 from M2.5 + 2 new for M2.6).

(Pre-existing flake noted: `data::ring_buffer::tests::test_concurrent_push_overwrite` is a concurrent-timing test that fails identically on `main` pre-changes; not related to M2.6.)

**Why this is the right shape:**

1. **`process_control_message` was already in the trait, just unwired.** Activating it now cleans up dead code (10+ existing impls), and the dispatch path is explicit at the runtime call site rather than smuggled through the data path.
2. **Universal cancellation still happens.** Existing nodes that rely on future-drop barge handling (e.g. `OpenAIChatNode`) keep working — `cancel.notify_waiters()` still fires before `process_control_message` is invoked.
3. **Non-barge envelopes are unaffected.** The dispatch only fires when `aux_port_of(&input) == Some(BARGE_IN_PORT)`. Other aux-port frames (`context`, etc.) flow through to `process_streaming_async` as before.
4. **No node-side opt-in flag.** Nodes that don't override `process_control_message` return `Ok(false)` (the default) — harmless. The opt-in is implicit: override the method to handle barges.

**M2 done. Cumulative shipped:** EmotionExtractorNode (M0) + audio.out.clock tap (M1) + LipSyncNode trait + BlendshapeFrame + SyntheticLipSyncNode (M2 partial) + Audio2Face solver math + ArkitSmoother (M2.3 + M2.4) + Audio2Face ONNX inference + bundle loaders (M2.2) + Audio2FaceLipSyncNode coordinator (M2.5) + manifest-level barge propagation (M2.6). Avatar audio→blendshape pipeline is now complete; M3 (RVCNode) and M4 (Live2DRenderNode) are the remaining tracks.

---

## M4.0 actuals (2026-04-28)

`cubism-core-sys` — raw FFI bindings to **Live2D Cubism Core** (the proprietary `.moc3` parser + post-deformer mesh evaluator). Build-time linkage against a user-installed Cubism SDK for Native via `LIVE2D_CUBISM_CORE_DIR`. The crate ships only glue (`build.rs`, `wrapper.h`, `bindgen` invocation, `lib.rs`); **the SDK itself is never committed** — Live2D's licence forbids redistributing it.

**Shipped:**

- **`crates/cubism-core-sys/`** — new workspace member.
  - [`Cargo.toml`](../../../crates/cubism-core-sys/Cargo.toml) — `links = "Live2DCubismCore"`, build script + bindgen build-dep.
  - [`build.rs`](../../../crates/cubism-core-sys/build.rs) — reads `LIVE2D_CUBISM_CORE_DIR`, picks lib by host triple (macOS arm64/x86_64, Linux x86_64, Windows x86/x86_64 with VS toolset + CRT flavour selectable via `CUBISM_CORE_LIB_KIND`), runs bindgen with `-IPATH/Core/include`, allowlists every `csm*` symbol. Emits `cargo:rerun-if-env-changed` for both env vars + `wrapper.h` so the build invalidates correctly when the SDK is updated.
  - [`wrapper.h`](../../../crates/cubism-core-sys/wrapper.h) — `#include <Live2DCubismCore.h>`.
  - [`src/lib.rs`](../../../crates/cubism-core-sys/src/lib.rs) — `include!`s the bindgen output + a single ABI-presence smoke test that calls `csmGetVersion()`.
  - [`CUBISM_SDK.md`](../../../crates/cubism-core-sys/CUBISM_SDK.md) — acquisition + per-platform layout + license-tier explanation (Small-Scale Operator vs PRO Operator).
- **Workspace integration** — added to `Cargo.toml::[workspace] members`.
- **`.gitignore`** — added `/sdk/` so a locally-extracted `CubismSdkForNative-*.zip` parked next to the repo never gets committed; also added `/models/live2d/` for the Aria model bundle landing in the next pass.

**Validation:**

- `cargo build -p cubism-core-sys` *without* the env var fails fast with the documented actionable message:
  > `LIVE2D_CUBISM_CORE_DIR is not set.`<br>
  > `Set it to the unpacked Cubism SDK for Native directory.`<br>
  > `See crates/cubism-core-sys/CUBISM_SDK.md for the license-gated download + how to point at it.`
- `LIVE2D_CUBISM_CORE_DIR=$PWD/sdk/CubismSdkForNative-5-r.5 cargo build -p cubism-core-sys` — clean (3.30 s cold).
- `LIVE2D_CUBISM_CORE_DIR=… cargo test -p cubism-core-sys` — 1/1 green: `smoke::linked_sdk_reports_a_nonzero_version` passes (returned version ≥ `0x05_00_00_00`).
- Generated bindings include the **post-deformer mesh accessors** that the spec §10 / phase-1 validation report flagged as load-bearing:
  - `csmGetDrawableVertexPositions`, `csmGetDrawableVertexUvs`, `csmGetDrawableIndices`, `csmGetDrawableIndexCounts`
  - `csmGetDrawableDynamicFlags`, `csmGetDrawableConstantFlags`, `csmGetDrawableBlendModes`
  - `csmGetDrawableMaskCounts`, `csmGetDrawableMasks`, `csmGetDrawableDrawOrders`, `csmGetDrawableOpacities`
  - `csmGetDrawableMultiplyColors`, `csmGetDrawableScreenColors`, `csmGetDrawableParentPartIndices`
  - Full parameter API: `csmGetParameterCount/Ids/Types/Values/Min/Max/DefaultValues/Repeats/KeyCounts/KeyValues`
  - Full part API: `csmGetPartCount/Ids/Opacities/ParentPartIndices`

**Design choices worth flagging:**

1. **Static linking, not dynamic.** The SDK ships both — we use `Core/lib/<platform>/libLive2DCubismCore.a` rather than `Core/dll/<platform>/libLive2DCubismCore.so` so the resulting Rust binary has no Cubism runtime dep at deployment. Trade: rebuild required when the SDK is updated.
2. **Bindgen runs every clean build.** The header is small (~21 KB) so this is free (~0.5 s). We could check the bindings into `src/` and skip bindgen at build time, but that pins us to one SDK version + introduces drift; better to regenerate against whatever the developer has installed.
3. **macOS fat lib pre-resolved by `cfg!(target_arch)`.** Apple Silicon → `arm64`, Rosetta/Intel → `x86_64`. Fits into the standard rustc cross-compile model.
4. **Windows toolset/CRT defaults.** Default to `143/MD` (VS 2022 + dynamic CRT — what `cc-rs` picks by default). Override via `CUBISM_CORE_LIB_KIND=142/MT` etc. Documented in `CUBISM_SDK.md`.
5. **Bindings allowlist `csm.*` only.** No spillover from system headers (stdint, etc.). Keeps the FFI surface small + stable across SDK revs.
6. **No safety wrappers.** This is a pure `-sys` crate; the safe wrapper is `cubism-core` (M4.1 next).

---

## M4.1 actuals (2026-04-28)

`cubism-core` — safe Rust wrapper around `cubism-core-sys`. Loads `.moc3` rigged-mesh files, evaluates the parameter-driven deformer chain via `csmUpdateModel`, and exposes post-deformer drawable mesh data (vertex positions, UVs, indices, opacity, render order, blend modes, masks, multiply/screen colours) as borrow-checked Rust slices. The lifetime parameter on `Model<'moc>` ties model data to the originating `Moc`; the SDK's `&self`/`&mut self` distinction is enforced naturally by the Rust borrow checker (calling `update()` invalidates outstanding drawable views).

**Shipped:**

- **`crates/cubism-core/`** — new workspace member.
  - [`src/lib.rs`](../../../crates/cubism-core/src/lib.rs) — `Moc`, `Model<'moc>`, `Vec2`/`Vec4` (`#[repr(C)]`-matched to `csmVector2`/`csmVector4` for zero-copy slice borrows), `CanvasInfo`, `Error` enum (`InconsistentMoc`, `UnsupportedMocVersion { moc_version, latest }`, `ReviveFailed`, `ModelInitFailed`).
  - [`src/buffer.rs`](../../../crates/cubism-core/src/buffer.rs) — `AlignedBuffer` (over-allocates by `align - 1`, finds the first aligned offset). Cubism requires 64-byte alignment for moc, 16 for model; `Vec<u8>` only guarantees 1.
  - [`src/drawable.rs`](../../../crates/cubism-core/src/drawable.rs) — `Drawables<'a>`, `DrawablesIter`, `DrawableView<'a>` with full accessors (`id`, `vertex_positions`, `vertex_uvs`, `indices`, `opacity`, `render_order`, `draw_order`, `dynamic_flags`, `constant_flags`, `blend_mode`, `texture_index`, `masks`, `multiply_color`, `screen_color`, `parent_part_index`); `ConstantFlags` + `DynamicFlags` bitflags; `BlendMode` enum (`Normal`/`Additive`/`Multiplicative`).
  - [`src/parameters.rs`](../../../crates/cubism-core/src/parameters.rs) — `Parameters<'a>` (read + mutate), `ParameterView<'a>` (`id`, `min`, `max`, `default`, `value`, `set_value`, `ty: ParameterType`); `Parts<'a>` + `PartView<'a>` for opacity overrides (used by M4.2 `.exp3.json` expression files).
- **`crates/cubism-core/tests/aria_smoke.rs`** — 6 tier-2 integration tests against the real Aria model, gated on `LIVE2D_TEST_MODEL_PATH`. Resolves the `.moc3` from the model3.json's `FileReferences.Moc` field via minimal serde_json (full model3.json loader is M4.2).
- **Workspace integration** — added `crates/cubism-core` to `Cargo.toml::[workspace] members`.

**Public API surface (lifetime-correct by construction):**

```rust
let moc = Moc::load_from_file("aria.moc3")?;     // owns aligned buffer
let mut model = Model::from_moc(&moc)?;          // borrows moc
{
    let params = model.parameters_mut();
    params.find("ParamJawOpen").unwrap().set_value(0.5);
}                                                // params borrow drops
model.update();                                  // requires &mut self
let drawables = model.drawables();               // borrows &model
for d in drawables.iter() {
    let positions: &[Vec2] = d.vertex_positions(); // zero-copy slice
    let indices: &[u16] = d.indices();
    // …
}
// Calling model.update() while `drawables` is alive → compile error.
```

**Tests landed (12 unit + 6 tier-2):**

| File | Tests | Notes |
|---|---|---|
| `buffer.rs` (unit) | 5 | aligned-to-64, aligned-to-16, zero-length, rejects non-power-of-two, zero-init |
| `lib.rs` (unit) | 3 | rejects all-zero bytes, rejects truncated magic, Vec layout matches |
| `drawable.rs` (unit) | 3 | BlendMode decode priority, DynamicFlags bit values match SDK constants, ConstantFlags bit values match |
| `parameters.rs` (unit) | 1 | ParameterType decode |
| `tests/aria_smoke.rs` (tier-2) | 6 | moc version sane (Aria reports `csmMocVersion_50` = 5), canvas + 103 drawables exposed (drawable[0]=`neck`, 54 verts, 234 indices), 14 masked drawables found, all 3 VBridger lip-sync axes (`ParamMouthOpenY`/`MouthForm`/`JawOpen`) findable in 86-parameter rig, parameter set→update→read round-trip is exact, BlendMode decode agrees with constant_flags across all drawables |

**Build + run matrix verified:**
- `LIVE2D_CUBISM_CORE_DIR=… cargo build -p cubism-core` — clean.
- `LIVE2D_CUBISM_CORE_DIR=… cargo test -p cubism-core --lib` — 12/12 unit tests green.
- `LIVE2D_CUBISM_CORE_DIR=… cargo test -p cubism-core` (no model env var) — tier-2 skips cleanly (6/6 reported as ok via early-return).
- `LIVE2D_CUBISM_CORE_DIR=… LIVE2D_TEST_MODEL_PATH=…/aria.model3.json cargo test -p cubism-core --test aria_smoke -- --nocapture` — 6/6 tier-2 green; SDK reports `Live2D Cubism SDK Core Version 6.0.1` once per test (linked SDK is r5.5 binary, internal version 6.0.1).

**Aria-validated invariants (loaded numbers, for downstream M4.4 work):**
- Canvas: 2873 × 3287 model-space pixels, origin (1436.5, 1643.5), pixels-per-unit 2873.
- 103 drawables, 14 of which clip against masks (M4.4 mask pre-pass scope).
- 86 parameters; the lip-sync axes the Audio2Face → ARKit → VBridger chain ultimately drives are all present.

**Design choices worth flagging:**

1. **`#[repr(C)]` `Vec2`/`Vec4` matched to `csmVector2`/`csmVector4`** — compile-time-asserted via `_ASSERT_*_LAYOUT` plus runtime size/align checks. Lets `vertex_positions()` hand out `&[Vec2]` borrowing directly from the model buffer with no copy.
2. **`Moc: Send + Sync`, `Model: Send + !Sync`** — the moc is read-only after revive; the model is mutated by `csmUpdateModel`. Per Cubism docs, multiple models can be derived from one moc concurrently, so `Sync` on `Moc` is genuinely safe.
3. **Lifetimes do the safety work, not RefCell.** `Drawables<'a>`/`Parameters<'a>` borrow `&Model`; `update(&mut Model)` requires no outstanding views. The borrow checker enforces Cubism's "don't read drawable data while updating" invariant at compile time.
4. **SDK quirk surfaced: `csmGetMocVersion` takes `*const c_void`, not `*const csmMoc`.** Wrapper casts internally; documented in the `version()` impl.
5. **SDK quirk surfaced: `csmGetRenderOrders` is the only drawable accessor without the `csmGetDrawable*` prefix.** Documented at the call site.
6. **`UnsupportedMocVersion` rejection.** SDK silently accepts mocs newer than it supports; the wrapper rejects via `csmGetLatestMocVersion()` so callers get an actionable error rather than a corrupt drawable downstream.
7. **`set_value` takes `&self` on `ParameterView`** — the underlying SDK API is `*mut`, but the wrapper enforces exclusive access via the `&mut self` requirement on `Model::parameters_mut()` (which `ParameterView` borrows from). Aliasing safety is preserved at the type level.
8. **No `physics3.json` evaluator yet.** Cubism Core itself has no physics API (it's a CubismFramework concept). Physics is M4.2 territory along with the rest of `.model3.json` resolution (textures, motions, expressions).

---

## M4.2 actuals (2026-04-28)

`.model3.json` manifest loader — pure JSON layer that sits on top of `cubism-core` and resolves the bundle's web of file references (textures, expressions, motions, physics, display info, pose, user data) into absolute paths the renderer can hand to `Moc::load_from_file` etc. Mirrors the persona-engine C# `ModelSettingObj` shape (verified against [`external/.../Live2D/Framework/ModelSettingObj.cs`](../../../external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/Live2D/Framework/ModelSettingObj.cs)).

**Shipped:**

- **[`crates/cubism-core/src/model_json.rs`](../../../crates/cubism-core/src/model_json.rs)** — full top-to-bottom parser:
  - `ModelJson::{from_file, load}` — `from_file` returns the raw parsed struct (relative paths preserved); `load` returns a `ResolvedModel` with the manifest dir captured for path joining.
  - `ResolvedModel` accessors: `moc_path`, `texture_paths`, `physics_path`, `pose_path`, `display_info_path`, `user_data_path`, `expression_path(name)`, `expressions()` (iterator over `(name, abs_path)`), `motions(group)` (iterator over `(abs_path, &MotionRef)`), `motion_group_names()`, `group_ids(name)`.
  - `FileReferences` mirrors the on-disk PascalCase: `Moc`, `Textures`, `Physics`, `Pose`, `DisplayInfo`, `UserData`, `Expressions: Vec<ExpressionRef {name, file}>`, `Motions: HashMap<String, Vec<MotionRef {file, sound, fade_in_time, fade_out_time}>>`.
  - `Group {target, name, ids}` covers `LipSync`/`EyeBlink` parameter groupings (used by the M4.3+ idle blink scheduler).
  - `ExpressionJson` (with `ExpressionParameter {id, value, blend}` + typed `ExpressionBlend::{Add, Multiply, Overwrite, Unknown(String)}` decoder) — full parser for `.exp3.json`.
  - `MotionJson` (with `MotionMeta {duration, fps, loop, are_beziers_restricted, curve_count, total_segment_count, total_point_count, …}` + `MotionCurve {target, id, fade_in_time, fade_out_time, segments: Vec<f32>}`) — manifest-level parser for `.motion3.json`. Curve **segments are surfaced raw** as `Vec<f32>`; per-tick evaluation is M4.4 territory.
  - `ModelJsonError {Io, Parse}` carries the offending path so bundle-issue diagnostics are useful.
- **[`crates/cubism-core/tests/aria_model_json.rs`](../../../crates/cubism-core/tests/aria_model_json.rs)** — 7 tier-2 tests against the actual Aria bundle.

**Tests landed (8 unit + 7 tier-2):**

| File | Tests | Notes |
|---|---|---|
| `model_json.rs` (unit) | 8 | parses Aria-shaped manifest, missing optional fields default cleanly, path resolver joins against manifest dir, motions iterator yields resolved paths (incl. empty + missing groups), group_ids lookup, parses ExpressionJson, expression-blend decode (Add/Multiply/Overwrite/Unknown), parses MotionJson skeleton |
| `aria_model_json.rs` (tier-2) | 7 | full Aria manifest load + resolves all paths (all files exist on disk), every expression file parses cleanly (5 found: `neutral`/`smug`/`happy`/`frustrated`/`sad`), every motion file parses cleanly (10 across 4 groups: `Confident`/`Happy`/`Idle`/`Talking`), moc round-trips via manifest path matches the M4.1 direct load (MOC3 magic verified), groups include `LipSync`+`EyeBlink` keys, unknown expression returns None, resolver works with absolute manifest path |

**Build + run matrix verified:**
- `LIVE2D_CUBISM_CORE_DIR=… cargo build -p cubism-core` — clean.
- `LIVE2D_CUBISM_CORE_DIR=… cargo test -p cubism-core --lib model_json` — 8/8 unit tests green.
- `LIVE2D_CUBISM_CORE_DIR=… LIVE2D_TEST_MODEL_PATH=…/aria.model3.json cargo test -p cubism-core --test aria_model_json -- --nocapture` — 7/7 tier-2 tests green; logs show 5 expressions + 10 motions across 4 groups.

**Aria-validated invariants (downstream M4.3 / M4.4 will lean on these):**
- 5 expressions: `neutral`, `smug`, `happy`, `frustrated`, `sad` (subset of the persona-engine emoji map; `cool`/`embarrassed`/`shocked`/etc. NOT present — confirms the renderer needs a fallback when an emoji has no rigged expression).
- 4 motion groups: `Idle` (3 clips), `Talking` (2), `Happy` (2), `Confident` (3) — total 10 `.motion3.json` files. `Excited`/`Sad`/`Surprised`/etc. from the canonical persona-engine table are **NOT** rigged in Aria — the renderer's emotion mapper needs to handle this gracefully (fall back to `Idle` or `Talking`).
- `LipSync` + `EyeBlink` group keys present but with empty `Ids` arrays — Aria expects the renderer to populate driving parameters from the lip-sync chain at runtime.
- All motions are 60 fps, durations ranging from a few seconds to ~10 s, all loop=true.

**Design choices worth flagging:**

1. **Manifest dir captured at load time** (`ResolvedModel::manifest_dir`) so subsequent path lookups can't accidentally drop to relative. Tested via `resolver_works_with_absolute_manifest_path`.
2. **Parse errors carry paths** (`ModelJsonError::Parse { path, source }`). Bundle-debugging is otherwise miserable — Cubism Editor occasionally emits subtly-malformed JSON when the user has a corrupt project, and "json parse failed" with no path is a non-starter.
3. **`MotionJson::Curves` segments stay raw `Vec<f32>`.** Cubism encodes them as `[time, value, segment_kind, …per-kind-specific bytes…]`; the per-segment decoder belongs in M4.4 next to the renderer's per-tick sampler. Surfacing the raw flat buffer keeps M4.2 small + makes the renderer's allocation pattern (decode once into a per-curve struct) explicit.
4. **`ExpressionBlend::Unknown(String)` instead of panic.** Cubism may extend the v3 schema with new blend kinds; surfacing unknown values keeps forward compat.
5. **Loader does NOT load the moc.** `ResolvedModel::moc_path()` returns the path; the caller pairs it with `Moc::load_from_file`. Splits I/O concerns: the loader is fast (parses only JSON), and the caller chooses when to pay the moc-parse cost.
6. **No physics evaluator.** Same reason as M4.1: Cubism Core has no physics API (it's CubismFramework). `physics_path()` returns the path so a follow-up evaluator can ingest it.
7. **Cumulative tests across `cubism-core` after M4.2: 20 unit + 13 tier-2 = 33 tests, all green.**

**Unblocks:** M4.3 (Live2DRenderState — input arbitration + idle blink scheduler can read `EyeBlink` group + emotion→expression+motion lookups via `ResolvedModel`), M4.4 (wgpu backend — knows which textures to upload via `texture_paths()`).

---

## M4.3 actuals (2026-04-28)

`Live2DRenderState` — input arbitration state machine for the Live2D renderer per spec §6.1. Pure Rust, no GPU, fully testable in CI without external deps. The wgpu+CubismCore backend (M4.4) plugs in behind the [`Live2DBackend`] trait without modifying any of this code.

**Shipped:**

- **[`crates/core/src/nodes/live2d_render/`](../../../crates/core/src/nodes/live2d_render/)** — new module gated behind `avatar-render` feature.
  - [`backend_trait.rs`](../../../crates/core/src/nodes/live2d_render/backend_trait.rs) — `Live2DBackend` trait (`render_frame(&mut self, &Pose) -> Result<RgbFrame>`, `frame_dimensions`), `RgbFrame` (R/G/B packed, `width`, `height`, `nonzero_byte_count` helper for tests), `BackendError` enum.
  - [`state.rs`](../../../crates/core/src/nodes/live2d_render/state.rs) — `Live2DRenderState`, `StateConfig`, `Pose`, `EmotionEntry`, `default_emotion_mapping()` (full persona-engine emoji table), `ArkitToVBridger` trait + `DefaultArkitMapper`. ~610 LOC.
  - [`test_support.rs`](../../../crates/core/src/nodes/live2d_render/test_support.rs) — `MockBackend` (records every `render_frame` call as `RecordedFrame { index, pose }`); gated behind `avatar-render-test-support` feature so integration tests outside `cfg(test)` can pull it.
- **Feature flags** in `crates/core/Cargo.toml`:
  - `avatar-render = ["avatar-lipsync"]` — state machine + backend trait. Pure Rust.
  - `avatar-render-test-support = ["avatar-render"]` — exposes `MockBackend` to integration test crates.

**Spec §6.1 invariants pinned:**

| Invariant | Test |
|---|---|
| Renderer samples blendshape ring at audio clock pts (linear lerp between bounding keyframes) | `samples_blendshape_keyframe_at_audio_clock_pts` (lerp at 150ms between pts 100 + 200) |
| Stale-pts eviction (`pts_ms < audio_clock_ms - 200`) | `evicts_stale_blendshape_frames_after_200ms` |
| Mouth interpolates to neutral when audio clock quiet | `interpolates_to_neutral_when_audio_clock_quiet` |
| Emotion event → expression + motion (canonical persona-engine map) | `emotion_event_drives_expression_and_motion` (🤩 → `excited_star` + `Excited`) |
| Emotion expires after `expression_hold_seconds` of wall time | `emotion_expires_after_hold_seconds_back_to_neutral` |
| Barge clears blendshape ring **but not** active emotion | `barge_clears_ring_but_preserves_emotion` |
| Idle blink fires when no emotion is active | `idle_blink_fires_when_no_emotion_active` |
| Blink suppressed during active emotion | `blink_suppressed_during_active_emotion` |
| Unknown emoji is no-op (not in default map) | `unknown_emoji_is_no_op` |
| `Talking` motion group during active audio without emotion | `talking_motion_group_picked_during_active_audio` |
| Audio clock past last keyframe holds last frame | `audio_clock_past_last_keyframe_holds_last_frame` |
| Audio clock before first keyframe holds neutral | `audio_clock_before_first_keyframe_holds_neutral` |
| Out-of-order keyframe pushes lerp correctly | `handles_out_of_order_keyframe_pushes` |

**Tests landed (16 unit + 8 integration):**

| File | Tests | Notes |
|---|---|---|
| `state.rs` (unit) | 16 | every spec §6.1 invariant + ARKit-to-VBridger mapper sanity + emoji table coverage |
| `tests/live2d_render_state_test.rs` (integration) | 8 | drives `Live2DRenderState` through `MockBackend` end-to-end: pose stream is recorded, lerped mouth values flow through, emotion metadata reaches backend, expiration flips expression to `neutral`, barge preserves emotion in pose stream, blink progresses across multiple ticks, backend dimensions consistent, recording resettable |

**Build + run matrix verified:**
- `cargo build -p remotemedia-core --features avatar-render` — clean (4.62 s).
- `cargo test -p remotemedia-core --features avatar-render --lib live2d_render` — 16/16 unit tests green.
- `cargo test -p remotemedia-core --features avatar-render-test-support --test live2d_render_state_test` — 8/8 integration tests green.
- (Pre-existing `data::ring_buffer::test_concurrent_push_overwrite` flake remains; unrelated to M4.3.)

**Default config (matches persona-engine + spec):**
- `expression_hold_seconds = 3.0` (`Live2D.md` `EXPRESSION_HOLD_DURATION_SECONDS`)
- `neutral_expression_id = "neutral"`, `neutral_motion_group = "Idle"`, `talking_motion_group = "Talking"`
- `neutral_interp_ms = 150`, `stale_blendshape_window_ms = 200` (spec §6.1)
- `blink_interval_min/max_ms = 3000/6000`, `blink_duration_ms = 200`
- `emotion_mapping`: full 17-emoji map from `external/handcrafted-persona-engine/Live2D.md`
- `mapper = DefaultArkitMapper` (VBridger canonical: `ParamJawOpen`/`ParamMouthOpenY` ← `arkit[jawOpen]`, `ParamMouthForm` = smile − frown, etc.)
- `blink_seed = 0xCAFE_F00D` (deterministic; SplitMix64 PRNG; same seed → same blink timing across runs)

**Design choices worth flagging:**

1. **Virtual wall-clock** (`tick_wall(Duration)` / `tick(elapsed_ms)`) instead of `Instant`. Makes the state machine pure: deterministic given inputs. Tests advance time by hand; the M4.5 streaming node will pass elapsed `Instant` deltas in.
2. **Blink scheduler anchors to scheduled time, not wall_now.** A long inter-tick gap (e.g. 200 ms while a single render tick lands) doesn't lose the elapsed-into-blink portion. Caught by the test that lands mid-blink at wall=150 with min=max=100, duration=200.
3. **Ring sorts by pts at sample time, not on push.** Bursty pushes from upstream don't need to be in order; the ring is small (<60 entries at 30 fps × 2 s buffer) so per-query sort is cheap. Insertion-sort on push is a follow-up if it ever profiles.
4. **Audio clock drop on barge.** Spec §6.3 says barge engages neutral interp "within one tick of `audio.out.clock` going quiet"; we make this explicit by dropping `audio_clock_ms` on barge so `compute_pose` enters the no-clock branch on the next tick.
5. **Emotion gates blink.** While an expression is active it owns the eyes; the blink scheduler is held off until the expression expires. Mirrors persona-engine's `IdleBlinkingAnimationService` priority interaction with `EmotionAnimationService`.
6. **`MockBackend` records full pose snapshots** (clones `Pose` per render). Integration tests inspect the recorded pose stream; the wgpu backend (M4.4) takes the same `&Pose` so swapping it in is a one-line change in the renderer node.
7. **No physics / no .exp3.json evaluation yet.** `Pose::part_opacities` is empty in M4.3; M4.5 will populate it from the active expression's `.exp3.json` file (loaded via `ExpressionJson` from M4.2). The state machine's seam is ready for it — just needs the wiring.

**Cumulative avatar code after M4.3: 91 + 16 = 107 unit tests, 16 + 8 + 8 + 7 + 6 = 45 integration tests, all green** (M2 lip-sync + M2.6 + M4.1 cubism-core + M4.2 model_json + M4.3 live2d_render).

**Unblocks:** M4.4 (wgpu backend implements `Live2DBackend` against the same `&Pose` shape `MockBackend` consumes; can drop into M4.3's `render_one()` flow with no state-machine changes), M4.5 (streaming node ticks the state machine + dispatches to whichever backend the manifest asks for).

---

## M4.4 actuals (2026-04-28, pass 1 — device init + ordered draw + Normal blend + readback)

**First visible Aria render.** wgpu headless backend that takes the M4.3 `Pose`, applies VBridger params to `cubism_core::Model`, runs `csmUpdateModel`, sorts drawables by render order, and rasterizes them into a 1024×1024 RGBA8 texture with premultiplied-alpha Normal blending. Reads back to RGB24 + saves a PNG preview to `target/avatar-render-tests/aria_neutral.png` for visual verification.

**Shipped:**

- **[`crates/core/src/nodes/live2d_render/wgpu_backend/mod.rs`](../../../crates/core/src/nodes/live2d_render/wgpu_backend/mod.rs)** — `WgpuBackend` (~600 LOC).
  - Headless wgpu device init (no window surface; `Backends::PRIMARY` picks Metal/DX12/Vulkan; high-performance adapter).
  - `WgpuBackend::load_model(path)` reads `.model3.json` via M4.2's `ModelJson::load`, parses the `.moc3` via M4.1's `Moc::load_from_file`, allocates one `wgpu::Texture` per `.model3.json` texture entry (sRGB), allocates per-drawable `Vertex`/`Index`/`Uniform` buffers from the post-deformer mesh data the safe wrapper exposes.
  - `WgpuBackend::render_frame(&Pose)` writes the pose's `params` HashMap into the model via `parameters_mut().find(id).set_value(v)`, calls `model.update()`, re-uploads VBs whose `DynamicFlags::VERTEX_POSITIONS_DID_CHANGE` bit is set, sorts drawables by `render_order`, runs one `RenderPass` with `BlendState::PREMULTIPLIED_ALPHA_BLENDING`, copies the offscreen texture to a row-aligned readback buffer, awaits map, strips alpha into a tightly-packed `Vec<u8>` of RGB24, returns an `RgbFrame`.
- **[`shaders/drawable.wgsl`](../../../crates/core/src/nodes/live2d_render/wgpu_backend/shaders/drawable.wgsl)** — Cubism Normal-blend shader.
  - Vertex: `clip_pos = projection * vec4(position, 0, 1)`; UV Y-flipped (Cubism authors UVs bottom-left, wgpu samples top-left).
  - Fragment: per-drawable `multiply.rgb` modulates texel; `screen.rgb` adds tint scaled by texel alpha; final RGB premultiplied by alpha for the standard premultiplied-alpha blend pipeline.
- **Workspace deps + features**:
  - Workspace adds `wgpu = "22.1"` (default-features off; `dx12 / metal / wgsl`), `image = "0.25"` (default off; `png`), `pollster = "0.4"` (one blocking await on `device.request_adapter` / `request_device`).
  - `bytemuck` workspace dep upgraded to enable `derive` feature for the `Pod`/`Zeroable` macros on `Vertex` + `DrawableUniforms`.
  - `crates/core/Cargo.toml` adds `avatar-render-wgpu = ["avatar-render", "dep:wgpu", "dep:image", "dep:pollster", "dep:cubism-core"]`.

**Tests landed (3 tier-2):**

- `tests/live2d_render_wgpu_test.rs` — gated on `LIVE2D_TEST_MODEL_PATH` (Aria) + GPU adapter availability (skips cleanly if either is absent):
  - `renders_aria_to_nontrivial_pixels` — neutral pose at 1024×1024 produces 58.68% non-zero pixel coverage; saves the result to `target/avatar-render-tests/aria_neutral.png`.
  - `renders_aria_with_open_jaw` — driving `ParamJawOpen=1.0`/`ParamMouthOpenY=1.0` produces a frame that differs from neutral by 9.67% of pixels (76,007 differing bytes at 512×512). Pins the param-set→`csmUpdateModel`→VB-reupload→render path.
  - `renders_through_state_machine_to_pixels` — full M4.3+M4.4 wiring: state machine → blendshape → mapper → backend → pixels. Confirms the `Live2DBackend` seam works end-to-end.

**Build + run matrix verified:**
- `LIVE2D_CUBISM_CORE_DIR=… cargo build -p remotemedia-core --features avatar-render-wgpu` — clean.
- `LIVE2D_CUBISM_CORE_DIR=… LIVE2D_TEST_MODEL_PATH=…/aria.model3.json cargo test -p remotemedia-core --features avatar-render-wgpu --test live2d_render_wgpu_test -- --nocapture` — 3/3 green.
- Render preview confirmed visually as Aria (eyes, lips, hair, ears, mouth all present + correctly placed).

**Known visual issues (deferred to pass 2):**

1. **Masked drawables render unclipped.** Aria's 14 masked drawables (eyes, mouth, etc.) draw their full bounding mesh instead of being clipped to mask geometry. M4.4 pass 2 implements the Cubism mask pre-pass: render mask drawables into an alpha-only offscreen texture, then sample it as a clip mask in the per-drawable fragment shader.
2. **Additive + Multiplicative blend modes share the Normal pipeline.** Aria has a small number of drawables with `BLEND_ADDITIVE` / `BLEND_MULTIPLICATIVE` in their `ConstantFlags`; they currently render with standard alpha blend, producing slight visual artifacts. Pass 2 adds dedicated `BlendState::REPLACE` (Additive: `src + dst`) and `BlendState` for Multiplicative (`src * dst`) pipelines and dispatches per drawable.
3. **Possible blue cast** on Aria's skin texture — under investigation. Could be premultiplied-alpha math interacting with sRGB framebuffer encoding, or a known authored-colour interaction with mask passes (whose absence in pass 1 means base-layer drawables show through where they shouldn't). Will resolve once the mask pre-pass lands.

**Design choices worth flagging:**

1. **Single shader, multiple pipelines.** The blend modes differ only in `BlendState`, not in shader logic. One WGSL file + three pipelines (created at backend init, dispatched per drawable in pass 2) is cheaper than three shaders.
2. **Per-drawable uniform buffer over push constants.** wgpu push constants require a feature; uniform buffers work on the default feature set. ~96 bytes × ~100 drawables × 30 fps = trivial bandwidth (~0.3 MiB/s).
3. **`Vec<u8>` readback over GPU-side YUV420p conversion.** Strips alpha CPU-side. The video track encoder downstream can do RGB→YUV in a hot path that doesn't depend on this backend (M4.5/M4.6 wiring). Optimization point: do the RGB→YUV in a compute shader if profiling shows the readback is the bottleneck.
4. **Lifetime-erased `Model<'static>`.** `Model<'moc>` borrows the `Moc`; storing both in one struct requires a lifetime trick. We `transmute` to `'static` and rely on struct field drop order (model first, moc second) for soundness. Documented inline. Alternative would be `self_cell` or `ouroboros`, but those are heavier deps for one struct.
5. **Render preview saved to `target/`.** The PNG is a side-output of the test, not an assertion — gives a developer a way to eyeball the render after a session ends. The structural assertions (non-zero pixels, jaw differs) catch regressions; the PNG catches "looks weird" regressions a human can spot but a test can't.

**Cumulative avatar code after M4.4 pass 1: 107 unit + 48 integration tests, all green** (M4.4 adds 3 to the M4.1 wgpu test side; M4.4 has no unit-test surface yet — those land in pass 2 once we have mask logic to test in isolation).

**Unblocks:** M4.4 pass 2 (mask pre-pass) — resolves visual fidelity. M4.5 (Live2DRenderNode streaming wire-up) — the backend trait is already proven; M4.5 is mostly glue.

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
