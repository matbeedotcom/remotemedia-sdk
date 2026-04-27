# Live2D + Audio2Face + RVC Avatar Pipeline — Architectural Design

**Status:** Draft (locked for review)
**Date:** 2026-04-27
**Scope:** Architectural / end-to-end. Defines node contracts, capability declarations, data flow, and barge-in semantics for an avatar-output extension of the SDK. Implementation details for individual nodes are deferred to per-subsystem follow-up specs.
**Inspiration (not a port):** [`external/handcrafted-persona-engine`](../../../external/handcrafted-persona-engine/) — Aria-style LLM-driven Live2D avatar with realtime voice conversion. We borrow its mappings (emoji → expression+motion, ARKit→VBridger), its Audio2Face ONNX model, and its RVC pipeline shape, but re-house them as composable RemoteMedia nodes wired by a manifest.

---

## 1. Goal

Make the SDK able to produce a **livestream-grade animated Live2D avatar video track** synchronized to a speech-to-speech conversation — alongside, not in place of, the existing audio path. Specifically, given an LLM/S2S pipeline that already produces conversational text + audio (see [`crates/core/src/nodes/conversation_coordinator.rs`](../../../crates/core/src/nodes/conversation_coordinator.rs) and the `qwen_s2s_webrtc_server` example), the SDK should be able to:

- Convert the agent's voice with **realtime RVC** (`Audio → Audio` node).
- Drive **lip-sync blendshapes** from the spoken audio with a local Audio2Face ONNX model.
- Extract `[EMOTION:<emoji>]` tags from any text stream (LLM, S2S text-delta, transcripts, …) and emit them as structured events for downstream animation.
- Render a **Live2D avatar** at a steady 30 fps using the blendshape stream, the emotion stream, and an audio-playback-time clock — with idle/blinking/expression-hold/motion-pick all handled.
- Ship the rendered frames out the existing WebRTC video track path — no new transport.

Out of scope explicitly: native wgpu renderer, phoneme-driven lip sync, Spout/NDI/RTMP sinks, music separation, runtime control plane, model installer.

## 2. Non-goals & explicit deferrals

These are real and likely-needed, but **not** part of this spec — each gets its own follow-up:

- **Native (wgpu / Cubism Core in Rust) renderer backend.** The render layer trait is defined; only the split Rust+Python backend ships in this spec.
- **Phoneme-driven lip-sync solver.** The `LipSyncNode` interface accommodates `metadata.phoneme_track` on input audio; only the Audio2Face ONNX impl ships.
- **Spout / NDI / RTMP video sinks.** Renderer emits `RuntimeData::Video`; existing WebRTC video track is the only sink wired in this spec.
- **Music separation, background-music ducking** (persona-engine has it; we don't).
- **OpenAI Realtime / Qwen S2S phoneme extraction.** Some S2S models could expose phoneme timings; out of scope. The abstract `LipSyncNode` interface unblocks this.
- **Avatar runtime control plane** (live emotion poke, motion override). Deferred — no aux ports on the render node beyond `barge_in` in this spec.
- **Automatic model fetching / installation.** Models assumed on-disk at configured paths.
- **Subtitle / UI overlay rendering.** Display text already flows via the existing `coordinator.out` `display_text` envelope; subtitle rendering is a frontend concern.
- **Hybrid `RuntimeData` variant** (`Tagged { primary, metadata }`). Multi-output streaming covers the EmotionExtractor's needs; a future spec can consolidate if other nodes accumulate the same need.

## 3. Subsystem inventory

Five units land in the SDK; one is an interface, four are concrete nodes. All are independent and individually testable. None of them modify the existing `ConversationCoordinatorNode` — its behaviour is preserved.

```
┌──────────────────┐  ┌────────┐  ┌────────────────┐  ┌────────────────┐  ┌──────────────────┐
│ EmotionExtractor │  │  RVC   │  │ LipSyncNode    │  │ Audio2Face     │  │ Live2DRenderNode │
│  Text→(Text,Json)│  │Audio→  │  │ (interface)    │  │ LipSyncNode    │  │ (interface)      │
│  multi-output    │  │ Audio  │  │ Audio→Json     │  │ (impl)         │  │ Json+Json+clock  │
│                  │  │        │  │ ARKit-52 frame │  │                │  │  → Video         │
└──────────────────┘  └────────┘  └────────────────┘  └────────────────┘  └──────────────────┘
                                          ▲                                       ▲
                                          │ contract                              │ contract
                                          ▼                                       ▼
                                   one impl in this spec               one backend in this spec
                                                                       (split Rust+Python)
```

### 3.1 `EmotionExtractorNode` *(concrete, Rust)*

A streaming text transform. Source-agnostic: not coupled to TTS, not coupled to coordinator, not coupled to audio. Wire it wherever a text stream needs `[EMOTION:<emoji>]` tags lifted out.

**Per input frame, emits two output frames on the data path:**

| | |
|---|---|
| **Input** | `RuntimeData::Text` — any text stream. LLM tokens, S2S text-delta, transcript replay, interleaved audio+text generators, static prompt, etc. Channel-aware via `split_text_str` (`tts`, `ui`, `think`, …); channel tags are opaque. |
| **Output frame 1 (data path)** | `RuntimeData::Text` — original text on the original channel, with all tags removed. Always emitted, even when no tags matched (downstream consumers like TTS rely on continuous text flow). |
| **Output frame 2 (data path)** | `RuntimeData::Json` — `{kind:"emotion", emoji, alias?, source_offset_chars, ts_ms, turn_id?}`. Emitted **only** when at least one tag matched in this input frame. Multiple matches in one input → multiple Json frames in source order, each with its own `source_offset_chars`. `turn_id` is forwarded only if the upstream frame carries it under the conventional `metadata.turn_id` key (where the coordinator's `display_text` envelopes already carry it); the node itself does not track turns. Absent → field omitted. |
| **Multi-output declaration** | Factory declares `is_multi_output_streaming = true`. Schema declares `produces([Text, Json])`. Existing pattern (`silero_vad` already does this for `(audio, vad_event)`). |
| **Coordinator coupling** | None. Coordinator is not modified by this spec. |
| **Config** | `tag_pattern` (regex with one capture group; default `\[EMOTION:([^\]]+)\]`). `emoji_aliases` (optional `Map<String,String>`, e.g. `"happy" → "😊"` — applied before tag emission so the Json carries the canonical emoji). |

**Why two data-path frames, not a control-bus tap?** The coordinator uses `publish_tap` for `turn_state` / `display_text` specifically because that data path leads to Kokoro-style TTS that would stringify and speak any JSON it received. EmotionExtractor doesn't have that risk because *the manifest edges* decide who sees the Json — TTS is wired off the Text output only. Same shape `silero_vad` already ships.

### 3.2 `RVCNode` *(concrete, Rust + ONNX Runtime)*

Plain `Audio → Audio` streaming node. Drops in anywhere on the audio path.

| | |
|---|---|
| **Input** | `RuntimeData::Audio (samples=f32, channels=1)` |
| **Output** | `RuntimeData::Audio (voice-converted, possibly different sample rate)` |
| **Capability behaviour** | `Configured`. Declares its required input sample rate (model-dependent, typically 16 kHz) and its output sample rate (model-dependent, typically 40 kHz). Capability resolver (spec 023) inserts resamplers as needed. |
| **Latency** | Streaming, chunked. Introduces buffer-of-N-samples latency consistent with other ONNX audio nodes. |
| **Manifest placement** | User-driven. Common wirings: `TTS → RVC → audio_track`, or `TTS → fan-out → [RVC → audio_track, lipsync]`, or `TTS → fan-out → [audio_track, lipsync]; RVC absent`. |
| **Config** | `model_path`, `index_path` (optional retrieval index), `f0_method` (`crepe`/`rmvpe`), `pitch_shift_semitones`, `index_rate`, `sample_rate_in`, `sample_rate_out`, `device` (`cuda`/`cpu`), `chunk_ms`. |

### 3.3 `LipSyncNode` *(interface / port)*

Defines the **blendshape-stream contract** any lip-sync solver must satisfy. This spec ships **one** concrete implementation; future implementations (phoneme-driven, NVIDIA cloud Audio2Face, …) plug into the same interface.

| | |
|---|---|
| **Input** | `RuntimeData::Audio` with `timestamp_us` populated. Optional `metadata.phoneme_track` is ignored by the Audio2Face impl, used by future phoneme impl. |
| **Output** | `RuntimeData::Json {kind:"blendshapes", arkit_52: [f32; 52], pts_ms: u64, turn_id?: u64}` per keyframe — a *timed blendshape stream*. Renderer treats consecutive keyframes as a sampleable timeline and interpolates between them. |
| **Frame rate** | Solver-dependent. Audio2Face emits at its native frame rate (typically 30–60 Hz). Renderer interpolates. |
| **Wire format = ARKit 52** | Renderer-agnostic. The ARKit→Live2D-VBridger mapping happens in the renderer, so a future NVIDIA-cloud-Audio2Face / RealityKit / iOS / non-Live2D target reuses the same stream. Mapping cost on a 30 fps render tick is sub-microsecond. |
| **Capability declaration** | Implementations declare required audio sample rate (Audio2Face: 16 kHz mono). |

### 3.4 `Audio2FaceLipSyncNode` *(concrete, Rust + ONNX Runtime)*

The shipped implementation. Wraps the persona-engine's local Audio2Face ONNX model (audio waveform → 52 ARKit blendshape predictions per frame) plus the PGD/BVLS solvers from [`external/handcrafted-persona-engine/.../TTS/Synthesis/LipSync/Audio2Face/`](../../../external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/TTS/Synthesis/LipSync/Audio2Face/).

This is **not** NVIDIA's cloud Audio2Face — it's a local ONNX model, named for compatibility with the persona-engine's terminology.

| | |
|---|---|
| **Models** | `audio2face.onnx` (audio→raw blendshape predictions), runs PGD or BVLS solver to refine into clean ARKit 52 values. |
| **Output** | Conforms to `LipSyncNode`: `Json {kind:"blendshapes", arkit_52, pts_ms, turn_id?}`. |
| **No ARKit→Live2D mapping in this node.** Renderer owns it. Wire format stays renderer-agnostic. |
| **Behaviour invariants** | (1) Streams output keyframes **ahead of audio playback** — i.e. as soon as input audio is processed, not gated on what the listener has heard. The renderer samples the timeline by audio playback clock, so the node is always producing for the future. (2) `pts_ms` on each output frame matches the audio frame it was derived from, NOT wall time. (3) Implements a `barge_in` aux port that clears its internal audio buffer + pending blendshape outputs. |
| **Config** | `model_path`, `solver` (`pgd`/`bvls`), `output_framerate`, `device`, `smoothing_alpha`. |

### 3.5 `Live2DRenderNode` *(interface + first concrete backend)*

Abstract trait at the manifest level (`node_type: "Live2DRenderNode"`). One concrete backend in this spec; native-wgpu backend deferred to follow-up.

#### Inputs (all wired in the manifest)

1. `RuntimeData::Json` blendshape keyframes from a `LipSyncNode` (ARKit 52 + `pts_ms`).
2. `RuntimeData::Json` emotion events from an `EmotionExtractorNode` (`{kind:"emotion", emoji, …}`).
3. `audio_clock_ms` updates via control-bus subscription (see §3.6).

#### Output

`RuntimeData::Video {format: Rgb24 | Yuv420p, width, height, frame_number, timestamp_us, stream_id: Some(<configured>)}` at the configured framerate (default 30 fps).

#### Internal responsibilities (renderer-side, not manifest-visible)

- Holds the `.model3.json` model + textures + drawable graph.
- ARKit 52 → VBridger param mapper (default ships in code; overridable via `arkit_to_live2d_map` config).
- Idle/blink scheduler (free-running tick, not pipeline events).
- Expression/motion library indexed by emoji; expression-hold timer; neutral fallback.
- Blendshape keyframe ring (last ~200 ms by `pts_ms`).
- Neutral-pose interpolation when no audio is playing.

#### Backend trait

```
trait Live2DBackend {
    fn render_frame(&mut self, vbridger_params, expression_id, motion_id) -> RgbFrame;
}
```

This spec ships one impl: **split Rust+Python backend** (§5).

#### Config

```yaml
node_type: Live2DRenderNode
params:
  backend: "rust_wgpu_python_state"          # only one impl in this spec
  model_path: "models/live2d/aria/aria.model3.json"
  width: 1280
  height: 720
  framerate: 30
  pixel_format: "rgb24"                       # rgb24 | yuv420p
  background: "transparent"                   # transparent | "#hex" | image_path
  video_stream_id: "avatar"
  idle_motion_group: "Idle"
  neutral_expression_id: "neutral"
  neutral_talking_motion_group: "Talking"
  expression_hold_seconds: 3.0
  blink_interval_min_ms: 2000
  blink_interval_max_ms: 6000
  emotion_map:                                # emoji → (expression_id, motion_group)
    "😊": { expression: "happy", motion_group: "Happy" }
    "🤩": { expression: "excited_star", motion_group: "Excited" }
    # … defaults from external/handcrafted-persona-engine/Live2D.md table
  arkit_to_live2d_map:                        # ARKit blendshape name → VBridger param + scale
    JawOpen: { param: "ParamJawOpen", scale: 1.0 }
    MouthSmileLeft: { param: "ParamMouthForm", scale: 0.5 }
    # … full mapping ships baked-in; override via config
  audio_clock_node_id: "audio"                # which AudioSender's clock tap to subscribe to
  emotion_node_id: "emotion_extractor"        # which EmotionExtractor's Json output to consume
```

### 3.6 Audio playback clock tap *(transport-side, one new publish)*

The renderer needs to know "what `pts_ms` is the listener currently hearing?" so it can sample the right blendshape keyframe. This is a transport-layer concern: only the audio sender knows the playback wavefront.

Inside [`crates/transports/webrtc/src/media/audio_sender.rs`](../../../crates/transports/webrtc/src/media/audio_sender.rs)'s dequeue loop, after a frame is committed for transmission, **add one publish** to a tap address `audio.out.clock`:

```rust
if let Some(ctrl) = control.as_ref() {
    let _ = ctrl.publish_tap(
        "audio",                         // node_id (the audio track's logical name)
        Some("clock"),                   // port suffix → "audio.out.clock"
        RuntimeData::Json(json!({
            "kind": "audio_clock",
            "pts_ms": frame_pts_ms,
            "stream_id": stream_id,
        })),
    );
}
```

`AudioSender` doesn't currently hold a `SessionControl`. The only transport-layer change in this spec: thread `SessionControl` into `AudioSender::new` (or via `set_control_handle`) and do the one publish per dequeued frame. Renderer subscribes to `ControlAddress::node_out("audio").with_port("clock")`.

When the audio ring buffer is empty, no publishes happen — that *is* the signal the renderer uses to start interpolating to neutral pose.

(Optional 500 ms heartbeat with `pts_ms = null` to distinguish "silence" from "transport stalled" is deferred to the implementation plan.)

This tap is the **only** transport-layer change in the spec.

## 4. Reference manifests (illustrative — manifest is user-driven)

### 4.1 Pipeline A — TTS + RVC + avatar

Lip-sync taps post-RVC here; swap by moving one edge.

```
mic (webrtc)
  └─→ silero_vad ──→ coordinator ──→ openai_chat ──→ emotion_extractor
                                                       ├─(text)→ kokoro_tts ──→ rvc ─┬─→ audio_track (webrtc out, default stream)
                                                       │                              └─→ audio2face_lipsync ─┐
                                                       └─(json) ──────────────────────────────────────────────┤
                                                                                                                ├→ live2d_render ──→ video_track (webrtc out)
   audio_track.AudioSender ──tap──→ control_bus("audio.out.clock") ─────────────────────────────────────────────┘
```

### 4.2 Pipeline B — Native S2S model with no TTS, interleaved text+audio out

Demonstrates EmotionExtractor's TTS-independence.

```
mic ─→ silero_vad ─→ coordinator ─→ qwen_s2s ─┬─(text)→ emotion_extractor ─┬─(text)→ ui_text_sink (data-channel)
                                              │                              └─(json)→ live2d_render
                                              └─(audio)→ rvc ─┬─→ audio_track
                                                              └─→ audio2face_lipsync ─→ live2d_render
   audio_track.AudioSender ──tap──→ control_bus("audio.out.clock") ───────────────────→ live2d_render
```

Same five new nodes. Same renderer. Different upstream graph. Capability resolver (spec 023) is the load-bearing piece — it inserts resamplers automatically between RVC's output rate and Audio2Face's 16 kHz input, and between S2S audio output and the audio track if rates disagree.

## 5. Renderer backend — split Rust+Python design

The renderer is a single `Live2DRenderNode` from the manifest's POV. Internally, the first backend is **split into two layers** to eliminate the OpenGL-context-lifetime risk of a Python-owned-everything design.

```
Live2DRenderNode (one node from the manifest)
├── Render layer — Rust + wgpu (in-process)
│     • owns the wgpu device, surface-less render-to-texture
│     • holds the model's textures (loaded once at session init from .model3.json)
│     • holds the ARKit→VBridger mapper
│     • holds the idle/blink scheduler and emotion expiration logic
│     • per frame: receives mesh data over IPC, draws drawables in Cubism's
│       order with its blend modes (Normal/Additive/Multiply) and mask passes,
│       reads back to RGB24/YUV420p, emits RuntimeData::Video
│
└── Model-state layer — Python subprocess (existing MultiprocessNode machinery)
      • uses live2d-py / CubismFramework's Python binding *without* GL init
      • per frame, given VBridger params + expression_id + motion_id from the
        Rust render layer:
          - applies parameters
          - runs physics
          - advances motion playback
          - emits post-deformer mesh data per drawable
            (vertex positions + opacity + render order + visibility)
      • iceoryx2 zero-copy for variable-size mesh buffers
```

### 5.1 Why this split

- **Eliminates the GL-context-lifetime risk** of the original Python-owns-everything proposal. There is no GL in Python; Python is a deterministic CPU-only consumer of `live2d-py`'s data-only API.
- **wgpu owns the GPU side**, where every other GPU concern in this codebase already lives (video encode, future native renderers).
- **Clean staging step toward the native-wgpu follow-up spec.** That spec just replaces the Python model-state layer with a Rust Cubism Core binding. The render layer stays exactly as written.
- **Manifest interface is unchanged** — `Live2DRenderNode` is still one `node_type`. The split is an implementation detail of the backend.

### 5.2 IPC schema (Python → Rust, per render tick)

```text
struct ModelFrame {
    drawables: Vec<DrawableState>,    // length = model's drawable count, fixed at load
    pts_ms: u64,                      // matches the audio_clock_ms the renderer is rendering at
}
struct DrawableState {
    drawable_id: u16,                 // index into the model
    vertex_positions: Slice<f32>,     // 2 * vertex_count, post-deformer
    opacity: f32,
    render_order: u16,
    is_visible: bool,
    // texture, UVs, indices, blend mode, mask refs, culling, AND vertex_count
    // are STATIC per drawable — loaded once at session init from the
    // .model3.json and cached on the Rust side keyed by drawable_id. The
    // iceoryx2 message buffer is sized at session init using these static
    // counts, so per-tick payloads are fixed-shape.
}
```

Static side (textures, UV arrays, index buffers, blend mode, mask graph) loads once at session init from the `.model3.json` and is cached on the Rust side keyed by `drawable_id`. Per-tick IPC carries only dynamic state; bandwidth bounded.

### 5.3 Rust render layer responsibilities

- wgpu device + render-to-texture pipeline (no OS surface).
- Texture upload from PNG paths at session init.
- Per-tick: receive `ModelFrame`, look up static state by `drawable_id`, build draw list, run mask pre-passes for masked drawables, draw in `render_order`, read back to CPU buffer.
- ARKit 52 → VBridger param mapping (consumed before the next IPC outbound to Python).
- Idle / blink / expression / motion scheduling (the "what should the model be doing right now" state machine).
- Frame format conversion (RGB24 / YUV420p) before emitting `RuntimeData::Video`.

### 5.4 Python model-state layer responsibilities

- Load `.model3.json` model via `live2d-py` (or whichever CubismFramework Python binding is selected).
- Apply VBridger params received from Rust each tick.
- Run physics, motion playback, expression interpolation.
- Expose post-deformer drawable mesh data via the Python wrapper's data accessors.
- Ship `ModelFrame` over iceoryx2 to Rust.
- Receive `barge_in` over the existing aux-port channel; clear motion-playback state if needed.

## 6. End-to-end data flow & barge-in semantics

### 6.1 Renderer input arbitration (free-running 30 fps)

Renderer is a `MultiprocessNode` ticking on its own clock. Each tick:

1. **Drain pending input frames** since last tick: Json blendshapes, Json emotions, Json clock-ticks via tap.
2. **Update internal state:**
   - **Blendshape ring**: insert new keyframe(s); evict any with `pts_ms < audio_clock_ms - 200`.
   - **Emotion state**: any new emotion event sets `current_emotion + emotion_started_wall_ms`; expires after `expression_hold_seconds` of wall time.
   - **Idle/blink scheduler**: tick wall-clock; pick a new idle motion if current expired; fire blink if interval elapsed.
3. **Compute pose:**
   - If `audio_clock_ms` exists *and* a blendshape keyframe with `pts_ms ≤ audio_clock_ms` exists in the ring: sample (linear interp between bounding keyframes), apply ARKit→VBridger mapper, that's the mouth.
   - Else (silence / no audio playing / barge cleared the ring): interpolate mouth toward neutral over ~150 ms.
   - Eyes/brows/blush: emotion → expression. If no active emotion: neutral expression + blink scheduler.
   - Body/head: emotion → motion-group pick. If none: idle motion-group pick.
4. **Render frame** in the backend (Rust render layer; Python model-state layer).
5. **Emit** `RuntimeData::Video {…, stream_id: Some(<configured>)}` downstream.

**Key invariant:** no input pressure dictates render rate. Bursty audio, ONNX inference jitter, missing emotion events — none of these stall a video frame. Worst case, pose freezes on the most-recent state for one tick.

### 6.2 Frame clock model (decoupled timeline)

Audio is bursty (TTS generates faster-than-realtime; audio track has a 30 s ring buffer that paces it out at 20 ms/frame). Video over WebRTC needs a steady ~30 fps. To reconcile:

- Lip-sync runs **ahead-of-audio-playback**, computing blendshapes as audio is processed. Each output keyframe carries `pts_ms` matching the audio frame it was derived from.
- Renderer is a free-running 30 fps **sampler** against that timeline, keyed by *audio playback clock* (control-bus tap, §3.6) — **not** enqueue time. This survives barge-in cleanly because the audio clock tap stops publishing when the ring buffer is flushed.
- When no audio is playing, renderer interpolates toward neutral pose.

### 6.3 Barge-in propagation

The existing barge mechanism (coordinator → `ctrl.publish(node_in("audio").barge_in)` and `ctrl.request_flush_audio()`) drains the audio ring buffer, killing what the listener hears. Without explicit propagation, two stale streams remain:

- Lip-sync's internal buffer (any keyframes already computed for now-flushed audio).
- Renderer's blendshape ring (already-delivered keyframes for the same flushed audio).

Both auto-resolve as soon as the audio clock stops advancing — the renderer's "interpolate to neutral over ~150 ms" engages within one tick of `audio.out.clock` going quiet, and stale ring entries age out (`pts_ms < audio_clock_ms - 200`) once a new turn starts publishing fresher pts.

To close the 150–200 ms gap cleanly:

- **Coordinator's `barge_in_targets` config** (already a list, currently `["llm", "audio"]`) gains two entries in avatar manifests: `["llm", "audio", "audio2face_lipsync", "live2d_render"]`.
- **`Audio2FaceLipSyncNode`** implements an `in.barge_in` aux port: clears its internal audio buffer and stops emitting further keyframes until next audio arrives. **Note:** "stops emitting" is the contract — frames already published to iceoryx2 but not yet consumed by the renderer will still arrive. They get evicted by the renderer's stale-pts ring rule (`pts_ms < audio_clock_ms - 200`) once a fresh turn starts publishing newer pts. Don't try to yank in-flight IPC frames.
- **`Live2DRenderNode`** implements an `in.barge_in` aux port: clears the blendshape ring; snaps mouth-target toward neutral immediately (interpolation still runs, but from the now-cleared state).
- **Emotion expression on the renderer is NOT cleared by barge** — the expression hangs around for its `expression_hold_seconds`. The avatar shouldn't go emotionally blank just because the user interrupted.

This is purely additive — coordinator's existing `dispatch_barge` loop already iterates `barge_in_targets` and publishes `in.barge_in` for each. The avatar manifest just lists the two new target node-ids.

### 6.4 Multi-track / `stream_id`

Renderer config carries `video_stream_id: String` (default `"avatar"`). Each emitted `RuntimeData::Video` frame stamps that on its `stream_id`. The existing `TrackRegistry` in [`server_peer.rs`](../../../crates/transports/webrtc/src/peer/server_peer.rs) auto-creates a video track on first frame with a new `stream_id`.

A single peer can run **multiple avatars** simultaneously by wiring two `Live2DRenderNode` instances with different `video_stream_id`s. Out of scope to demonstrate; the contract makes it free.

## 7. Capability declarations (spec 023 integration)

Each new node declares its `MediaCapabilities` so the resolver inserts resamplers / format converters automatically and surfaces mismatches with actionable errors at session creation.

| Node | Behaviour | Input caps | Output caps |
|---|---|---|---|
| `EmotionExtractorNode` | `Passthrough` (text→text) + multi-output Json | `text` | `text`, `json` |
| `RVCNode` | `Configured` | `audio(sample_rate=<configured>, channels=1, format=F32)` | `audio(sample_rate=<configured>, channels=1, format=F32)` |
| `LipSyncNode` (interface) | trait declares `RequiresAudio` | `audio(channels=1, format=F32, sample_rate=<impl>)` | `json` |
| `Audio2FaceLipSyncNode` (impl) | `Static` | `audio(sample_rate=16000, channels=1, format=F32)` | `json` |
| `Live2DRenderNode` | `Configured` (config dictates fps/dims) | `json` (multi-input via manifest edges) | `video(width=<cfg>, height=<cfg>, format=Rgb24, framerate=<cfg>)` |

The resolver will demand a resampler between RVC's typical 40 kHz output and Audio2Face's 16 kHz input whenever they're directly wired — same way it already handles whisper. No special handling needed.

## 8. Failure modes (spec-level only)

Implementation/recovery details deferred to the plan:

| Failure | Pipeline behaviour |
|---|---|
| **RVC model fails to load** | Node construction fails; pipeline fails fast at session creation. Listener gets a session-creation error, not a half-built pipeline. |
| **Audio2Face ONNX inference error mid-turn** | Node logs, drops the offending audio chunk, emits no blendshape for that span. Renderer interpolates to neutral mouth for the gap. Audio still flows to the listener (lip-sync is not on the audio path). |
| **Renderer Python subprocess crashes** | Existing `MultiprocessNode` health monitor detects, kills the session — consistent with how every other multiprocess Python node behaves today. No special handling. |
| **EmotionExtractor regex throws** | Node treats input frame as opaque; emits original Text unmodified, no Json output, logs at warn. |
| **No audio clock subscriber** | Renderer enters "no audio" mode (everything is silence-time); shows neutral mouth + idle/blink + emotion as scheduled. Useful for "silent avatar" / text-only previews. |
| **wgpu device init fails on render layer** | Node construction fails; pipeline fails fast at session creation. |

## 9. Testing strategy

Each node testable in isolation; renderer testable without a GPU at the unit-test layer.

| Node | Unit test surface | Integration test |
|---|---|---|
| `EmotionExtractorNode` | Tag parsing on synthetic text streams; multi-output ordering; channel preservation; alias substitution; `turn_id` forwarding | Wire into a fake LLM stream; assert two output edges receive correctly partitioned data |
| `RVCNode` | ONNX inference shape conformance; chunk boundary continuity (no clicks at frame boundaries); device dispatch | A→B sine sweep through RVC; assert RMS levels and pitch shift bounds; gate on a smoke ONNX model so CI stays cheap |
| `Audio2FaceLipSyncNode` | Blendshape output is 52-vector with valid ranges; `pts_ms` monotonic and matches input audio timestamps; barge clears internal buffer | Synthetic audio → assert blendshape stream rate matches `output_framerate` |
| `Live2DRenderNode` (input arbitration) | A `MockBackend` that records `render_frame` calls; assert input arbitration logic (blendshape sampling, emotion expiration, idle/blink scheduling, barge clears ring) drives the right calls | — |
| `Live2DRenderNode` (Rust render layer) | wgpu render-to-texture against a synthetic `ModelFrame`; pixel-count + alpha sanity check on output | Headless mode driving a checked-in tiny model; emit a few frames; non-zero-pixel sanity check (no image-diff in this spec) |
| `Live2DRenderNode` (Python model-state layer) | Snapshot test: load model, apply known params, assert vertex output stable across runs | iceoryx2 round-trip integration: Python → Rust mesh delivery latency. **Soft target**: under 5 ms median on the dev machine; treat as a perf budget recorded in the implementation plan, not a CI pass/fail gate (CI hosts vary too much). |
| Audio clock tap | Existing transport tests gain one assertion: `AudioSender` publishes one `audio.out.clock` Json per dequeued frame when a SessionControl is installed | End-to-end: drive synthetic audio through the audio track; subscribe to `audio.out.clock`; assert pts_ms monotonicity |

## 10. Risks worth flagging now

- **`live2d-py` GL-free initialization is a validation gate.** Confirm `live2d-py` (or whichever CubismFramework Python binding is selected) can load a model and expose post-deformer mesh data — `csmGetDrawableVertexPositions` and friends — without requiring a GL context. The persona-engine's C# path uses Cubism Core directly with its own renderer, so this is feasible at the C SDK level; we're betting that the Python wrapper exposes the same data plane. **First task in the implementation plan: write a 30-line Python script that loads a model and prints its drawable count and one frame of vertex data, in a process with `DISPLAY=` unset.** If that fails, fall back to either monkey-patching the wrapper or promoting the native-wgpu follow-up spec to first-shipped.
- **Cubism rendering semantics in wgpu.** Mask passes (clip-masking, inverted masks), three blend modes (Normal/Additive/Multiply), premultiplied-alpha quirks, drawable cull-flags — all non-trivial. Render layer must match `live2d-py`'s native renderer's *visual* output bit-for-bit-ish, not just "looks like a Live2D model." Mitigation: cross-reference the official Cubism C++ SDK reference renderer (`CubismRenderer_OpenGLES2`), which is open-source, and reuse its mask-pre-pass + ordered-draw structure. Carries the largest implementation-time risk in the spec.
- **Per-frame mesh IPC bandwidth.** Typical Live2D model: ~30 drawables × ~5k vertices × 8 bytes per vertex (xy as f32). At 30 fps that's ~36 MB/s peak — comfortable on iceoryx2's shared-memory path on a sane host. Worth recording the observed number in the implementation plan's perf section so a misbehaving complex model gets caught early.
- **Audio2Face ONNX latency.** Reportedly ~5–15 ms/inference on a recent NVIDIA GPU. CPU fallback may be too slow for realtime. Spec assumes GPU; CPU is best-effort.
- **ARKit → VBridger mapper as config.** Letting users override the mapping in YAML is powerful but error-prone (off-by-one params nuke lip-sync). Default ships baked-in; override is opt-in.
- **`audio_clock` tap couples renderer to transport.** If a deployment swaps WebRTC for a non-WebRTC transport without porting the same publish, the renderer falls into "no audio" mode. Documented; not load-bearing.
- **"Transport stalled" vs "intentional silence" is observationally indistinguishable** without a heartbeat on `audio.out.clock`. The implementation plan must make a definitive call on whether to ship the optional 500 ms heartbeat (`pts_ms = null`) — without it, an upstream audio bug that wedges the AudioSender thread looks identical to the listener being quiet, and the renderer will silently park on neutral pose. Recommend shipping the heartbeat in the first iteration.
- **Live2D model licensing.** Aria is included in the persona-engine binary distribution under its own terms; we don't redistribute. Users supply their own models.

## 11. Follow-up specs (rough sketch, in priority order)

1. **`live2d-render-native-wgpu`** — second backend behind the `Live2DRenderNode` trait. Replaces the Python model-state layer with a Rust Cubism Core binding (`cubism-rs` or direct FFI to `Live2DCubismCore.dll`/`.so`/`.dylib`). Render layer unchanged.
2. **`phoneme-lipsync`** — `PhonemeLipSyncNode` consuming Kokoro's `TimedPhoneme[]` metadata for tighter sync on TTS-only paths.
3. **`spout-video-sink`** — Windows-only sink so OBS picks the avatar up locally without round-tripping through WebRTC.
4. **`avatar-control-plane`** — aux ports on the renderer for runtime emotion override, expression freeze, motion poke. Useful for moderation tooling.

## 12. Summary

This spec adds five components to the SDK: a source-agnostic multi-output text → `(Text, Json)` `EmotionExtractorNode`; a leaf `Audio→Audio` `RVCNode`; an abstract `LipSyncNode` interface with one concrete `Audio2FaceLipSyncNode` (audio → ARKit-52 blendshape stream tagged with `pts_ms`); and an abstract `Live2DRenderNode` interface with one concrete backend (Rust wgpu render layer + Python `live2d-py` model-state subprocess, communicating via iceoryx2 mesh-data IPC, no GL in Python) that consumes the blendshape stream + emotion stream + audio playback clock and emits 30 fps `RuntimeData::Video` frames. A single transport-side hook is added: the WebRTC `AudioSender` publishes an `audio.out.clock` tap on each dequeued frame so the renderer samples the blendshape timeline by what the listener actually hears, not what was enqueued. Barge-in propagation gains two new aux-port `barge_in` handlers on the lip-sync and renderer nodes; coordinator behaviour is unchanged. Capability resolution (spec 023) is the load-bearing piece that makes manifests with these nodes correct-by-construction. Out-of-scope items (native wgpu renderer, phoneme lip-sync, Spout/NDI sinks, runtime control plane) are unblocked by the interfaces defined here and are sketched as follow-up specs.
