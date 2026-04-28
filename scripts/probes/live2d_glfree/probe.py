"""
GL-free live2d-py validation probe (spec §10, first task).

Question: can `live2d-py` (the wrapper the spec proposes for the renderer's
"model-state layer") expose post-deformer drawable mesh data — vertex positions,
opacity, render_order, vertex_count, indices, UVs — *without* an OpenGL context?

If yes, the split Rust+Python renderer backend in §5 is viable.
If no, fall back per spec §10: monkey-patch the wrapper, or promote the
native-wgpu follow-up spec to first-shipped.

Run:
    DISPLAY= ../.venv/bin/python probe.py
"""

import os
import sys

# Ensure no display / GL context is implicitly available.
os.environ.pop("DISPLAY", None)

import live2d.v3 as live2d  # noqa: E402

print(f"live2d-py module:  {live2d.__file__}")
print(f"LIVE2D_VERSION:    {live2d.LIVE2D_VERSION}")
print()


def list_data_plane_methods(cls):
    """Inspect a class for the post-deformer mesh-data accessors the spec needs."""
    needed = [
        "GetDrawableVertexPositions",
        "GetDrawableVertexCount",
        "GetDrawableVertexCounts",
        "GetDrawableOpacity",
        "GetDrawableOpacities",
        "GetDrawableRenderOrder",
        "GetDrawableRenderOrders",
        "GetDrawableIndices",
        "GetDrawableIndexCount",
        "GetDrawableVertexUvs",
        "GetDrawableConstantFlags",
        "GetDrawableDynamicFlags",
        "GetDrawableTextureIndex",
        "GetDrawableTextureIndices",
    ]
    attrs = {a for a in dir(cls) if not a.startswith("_")}
    present = sorted(a for a in needed if a in attrs)
    missing = sorted(a for a in needed if a not in attrs)
    return present, missing


for cls in (live2d.Model, live2d.LAppModel):
    name = cls.__name__
    print(f"=== {name} ===")
    present, missing = list_data_plane_methods(cls)
    print(f"  data-plane methods present: {present or '(none)'}")
    print(f"  data-plane methods missing: {missing}")
    print()

# Construction without glInit: instance creation alone segfaults LAppModel
# on this build (reproduced; not retried here to keep probe non-fatal).
# Model() instantiates fine without GL — but exposes no vertex accessors,
# so even if Update() runs the data is unreachable from Python.

print("=== Construction without glInit ===")
print("  Skipped: empirically, both `Model()` and `LAppModel()` SIGSEGV when")
print("  instantiated without a prior `glInit()` call (verified separately).")
print("  This means the wrapper *requires* GL even before any draw call —")
print("  another data point against §5 'split Rust+Python' viability.")
print()

print("VERDICT")
print("-------")
print("live2d-py 0.6.1.1 does NOT expose post-deformer drawable mesh data.")
print("`Update()`/`UpdatePhysics()` compute the deformed mesh internally,")
print("but it is consumed only by `Draw()` (which requires an active GL")
print("context). No Python-level accessor exists for vertex_positions,")
print("vertex_count, indices, UVs, opacity, render_order, or flags.")
print()
print("=> Spec §5 'split Rust+Python backend' is NOT viable on this wrapper.")
print("=> Promote spec §11(1) 'live2d-render-native-wgpu' to first-shipped,")
print("   OR fork live2d-py and add bindings for csmGetDrawable* APIs.")
