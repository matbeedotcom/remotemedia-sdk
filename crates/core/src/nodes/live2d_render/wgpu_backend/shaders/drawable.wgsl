// Live2D drawable shader.
//
// Implements Cubism's Normal blend mode (premultiplied alpha).
// Additive + Multiplicative blends share this shader but use
// different render-pipeline `BlendState`s configured Rust-side
// (see `wgpu_backend/mod.rs::pipelines_for_blend_mode`).
//
// Coordinate convention:
// - vertex positions are in Cubism model-space (units of "model
//   units"; not pixels).
// - the projection uniform converts model-space → NDC by:
//     ndc = (vert * pixels_per_unit - canvas_origin) * (2 / canvas_size)
//   so vert=0 maps to canvas-center pixel, vert=±0.5 to canvas
//   edges. UVs are Y-flipped to match Cubism's bottom-left UV
//   origin.

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct DrawableUniforms {
    // Cubism's combined "model→NDC" transform. Computed Rust-side
    // from canvas size + origin + a fit-into-framebuffer scale.
    projection: mat4x4<f32>,
    // RGBA tint × premultiplied opacity. The renderer multiplies
    // `vec4(multiply.rgb, 1.0) * vec4(1, 1, 1, opacity) * screen`
    // into one uniform per drawable per frame.
    multiply: vec4<f32>,
    screen: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: DrawableUniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip_pos = u.projection * vec4<f32>(in.position, 0.0, 1.0);
    // Cubism authors UVs with bottom-left origin; wgpu textures
    // sample with top-left origin. Flip on the way through.
    out.uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = textureSample(tex, samp, in.uv);
    // Cubism's Normal blend mode expects premultiplied-alpha input.
    // Per-drawable multiply colour modulates RGB; the W (alpha)
    // component carries the per-drawable opacity. Screen colour is
    // additive tint:
    //   out_rgb = (texel.rgb * multiply.rgb) + screen.rgb * texel.a
    //   out_a   = texel.a * multiply.a
    let modulated_rgb = texel.rgb * u.multiply.rgb + u.screen.rgb * texel.a;
    let modulated_a = texel.a * u.multiply.a;
    // Premultiply alpha into RGB so the standard
    // `BlendState::PREMULTIPLIED_ALPHA_BLENDING` pipeline produces
    // the right composite (`src + dst*(1-src.a)`).
    return vec4<f32>(modulated_rgb * modulated_a, modulated_a);
}
