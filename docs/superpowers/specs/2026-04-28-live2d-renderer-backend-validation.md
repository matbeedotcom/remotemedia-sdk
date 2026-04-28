# Live2D Renderer Backend — Phase-1 Validation Probe Report

**Status:** Done
**Date:** 2026-04-28
**Probe:** [scripts/probes/live2d_glfree/probe.py](../../../scripts/probes/live2d_glfree/probe.py)
**Result file:** [scripts/probes/live2d_glfree/probe-result.txt](../../../scripts/probes/live2d_glfree/probe-result.txt)
**Parent spec:** [2026-04-27-live2d-audio2face-rvc-avatar-design.md](2026-04-27-live2d-audio2face-rvc-avatar-design.md) §10 (validation gate), §5 (split backend), §11(1) (native-wgpu follow-up)

## Question

Can `live2d-py` (the Python `CubismFramework` wrapper the parent spec proposed for the renderer's "model-state layer") expose **post-deformer drawable mesh data** — vertex positions, opacity, render order, vertex/index counts, UVs, drawable flags — *without* an OpenGL context?

If yes → §5 split Rust+Python backend is viable; ship it first.
If no  → §10 fallback path: monkey-patch the wrapper, or promote §11(1) `live2d-render-native-wgpu` to first-shipped.

## Method

`live2d-py 0.6.1.1` (the only PyPI distribution; prebuilt macOS wheels for cp311–cp313). Probe:

1. Inspect `live2d.v3.Model` and `live2d.v3.LAppModel` class surface for the 14 `csmGetDrawable*`-shaped accessors the parent spec's IPC schema (§5.2) requires.
2. Cross-check against the bundled `.pyi` (the wrapper's own declared API).
3. Attempt construction without `glInit()` to see whether the wrapper is GL-free at the *instantiation* level, never mind the data-access level.

## Findings

### 1. Zero post-deformer mesh accessors are exposed

Probe output for `Model` (and identically for `LAppModel`):

```
data-plane methods present: (none)
data-plane methods missing: [
  GetDrawableConstantFlags, GetDrawableDynamicFlags,
  GetDrawableIndexCount,    GetDrawableIndices,
  GetDrawableOpacities,     GetDrawableOpacity,
  GetDrawableRenderOrder,   GetDrawableRenderOrders,
  GetDrawableTextureIndex,  GetDrawableTextureIndices,
  GetDrawableVertexCount,   GetDrawableVertexCounts,
  GetDrawableVertexPositions, GetDrawableVertexUvs
]
```

The wrapper exposes only:
- **Identifiers**: `GetDrawableIds`, `GetPartIds`, `GetParameterIds`
- **Setters**: drawable colors (`SetDrawableMultiplyColor`, `SetDrawableScreenColor`), part opacity/colors
- **Hit-testing**: `IsDrawableHit`, `HitDrawable`
- **Compute**: `Update`, `UpdatePhysics`, `UpdatePose`, `UpdateBlink`, `UpdateBreath`, `UpdateMotion`, `UpdateExpression`
- **Render**: `Draw` — the *only* consumer of the deformed mesh, and it requires GL.

The C SDK functions (`csmGetDrawableVertexPositions`, etc.) exist in `Live2DCubismCore` and are called internally by the wrapper's `Draw()`, but their results never cross the Python boundary.

### 2. The wrapper is not GL-free at construction either

Both `live2d.v3.Model()` and `live2d.v3.LAppModel()` **SIGSEGV (exit 139)** when instantiated without a prior `glInit()` call. This is verified by direct invocation; no Python exception is raised, so a `try/except` around construction doesn't help. The wrapper assumes a live GL context exists for the entire model lifetime — it isn't merely required at draw time.

### 3. `live2d-py` source build also requires Cubism SDK

Attempting `pip install live2d-py` from sdist (Python 3.10, no wheel) fails because the package's `setup.py` downloads the Cubism SDK from `live2d.com/download/cubism-sdk/`, which returns HTTP 403 to automated requests. Manual download requires accepting the Live2D license. Forking the wrapper to add `csmGetDrawable*` bindings is therefore not free — it inherits the same Cubism SDK acquisition friction plus a cross-platform native build matrix.

## Verdict

**Spec §5 "split Rust+Python backend" is not viable as written.** The Python layer was designed to be a deterministic CPU-only emitter of post-deformer mesh data over iceoryx2; that data is unreachable from `live2d-py` and the wrapper isn't even GL-free at instantiation.

## Recommendation

Adopt the §10 fallback: **promote §11(1) `live2d-render-native-wgpu` to first-shipped renderer backend** and remove the split design from the implementation plan.

That backend is:

- **Rust + wgpu** for GPU work (already the codebase's GPU lane — video encode, future native renderers).
- **Direct FFI to `Live2DCubismCore.{dll,so,dylib}`** (the C SDK that `live2d-py` itself wraps, just without the wrapper). Either via [`cubism-rs`](https://github.com/Veykril/cubism-rs) (community Rust binding, MIT) or direct `bindgen` against the public C header. The C SDK's `csmGetDrawable*` functions return raw pointers into the model's internal buffers — exactly the post-deformer mesh data the parent spec's §5.2 IPC schema describes, just without the IPC.

This eliminates several risks the parent spec already flagged:

- The §10 "GL-context-lifetime" risk and the §10 "Python wrapper might not expose mesh data" risk both vanish (no Python in the renderer).
- The §10 "per-frame mesh IPC bandwidth" risk vanishes (mesh data stays in-process).
- The §5.1 "clean staging step toward native-wgpu" justification becomes moot — there's nothing to stage toward; we ship the target directly.

What's left is the §10 risk we *can't* dodge by changing backends: **"Cubism rendering semantics in wgpu"** — mask passes (clip + inverted), three blend modes, premultiplied-alpha quirks, drawable cull flags, ordered draw. That work is identical under either backend; the split design only deferred it, never removed it. Reference: Cubism's open-source `CubismRenderer_OpenGLES2` (also `D3D11`, `Metal`) — port its pre-pass + ordered-draw structure to wgpu.

## Knock-on changes to the parent spec

When the implementation plan is written (phase 2), it should reflect:

1. **Drop §5.2 IPC schema** and §5.4 "Python model-state layer responsibilities" entirely. The renderer is one in-process Rust component.
2. **Update §3.5** "Live2DRenderNode → ships one backend in this spec → split Rust+Python" → "ships one backend in this spec → native Rust+wgpu+CubismCore".
3. **Update §10 risks**: the `live2d-py` validation gate is resolved (failed); the Python-wrapper / IPC-bandwidth risks come off the list; the wgpu-Cubism-semantics risk gets promoted to **the** primary risk.
4. **Drop §11(1)** from the follow-ups list (it's now the spec).

The other four nodes — `EmotionExtractorNode`, `RVCNode`, `LipSyncNode` interface, `Audio2FaceLipSyncNode` — are unaffected. The renderer's manifest-level interface (`Live2DRenderNode` node_type, its inputs/outputs/config keys) is also unaffected; the split-vs-native choice is a backend implementation detail.

## Cubism Core acquisition

The native backend still needs `Live2DCubismCore` on disk at build time — the C SDK is not redistributable, so it stays an external download requiring license acceptance. Build flow:

1. User downloads Cubism SDK for Native from live2d.com (one-time, license accepted).
2. Sets `LIVE2D_CUBISM_CORE_DIR=/path/to/CubismSdkForNative-5.x` in the build environment.
3. `build.rs` for the renderer crate reads the env var, links the static lib, generates bindings.
4. Asset model files (`.model3.json` + textures + motions) supplied by the user at runtime — same as the parent spec already specifies for §3.5 config.

This matches how every other proprietary-license native dependency is handled in the codebase.

## Next steps (phase 2 inputs)

The phase-2 implementation plan can now be written against a fixed renderer backend choice. Recommended structure (modeled after [docs/superpowers/plans/2026-04-28-llama-cpp-liquid-audio.md](../plans/2026-04-28-llama-cpp-liquid-audio.md)):

1. `EmotionExtractorNode` — pure Rust, no external deps. Smallest blast radius; ship first; covers the integration-test path with synthetic `[EMOTION:🤩]` keywords.
2. `audio.out.clock` tap on `AudioSender` — single transport-side hook (parent §3.6), enables decoupled-timeline testing.
3. `LipSyncNode` trait + `Audio2FaceLipSyncNode` — ONNX runtime + PGD/BVLS solver port from `external/handcrafted-persona-engine/.../LipSync/Audio2Face/`. Synthetic-emotion integration test for the WebRTC avatar path lives here (audio in → blendshapes out, exercised end-to-end without needing the renderer).
4. `RVCNode` — ONNX inference pipeline. Independent of avatar output; ships alongside but its testing is self-contained.
5. `Live2DRenderNode` — native Rust+wgpu+CubismCore backend per this report's recommendation. Largest scope; ships last; depends on Cubism SDK acquisition.
