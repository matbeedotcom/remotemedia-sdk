//! Live2D render node — input arbitration state machine + backend
//! trait. The wgpu+CubismCore backend slots in via [`Live2DBackend`]
//! in M4.4; the streaming-node wiring lands in M4.5.
//!
//! Per spec [§6.1] the renderer is a free-running 30 fps sampler:
//! it ticks on its own clock, samples the blendshape ring against
//! the audio playback clock, layers an active emotion expression +
//! motion on top, and renders one frame per tick. Input pressure
//! never gates a render.
//!
//! What lives here:
//!
//! - [`Live2DRenderState`] — the state machine. Pure Rust, no GPU.
//! - [`Live2DBackend`] — trait every render backend (wgpu, mock,
//!   future Vulkan/Metal/etc.) implements.
//! - [`StateConfig`], [`EmotionEntry`], [`Pose`] — config + output.
//! - [`MockBackend`] — test stand-in (feature `avatar-render-test-support`
//!   for cross-crate use, otherwise `cfg(test)`).

pub mod backend_trait;
pub mod state;

pub use backend_trait::{Live2DBackend, RgbFrame};
pub use state::{
    default_emotion_mapping, ArkitToVBridger, EmotionEntry, Live2DRenderState, Pose,
    StateConfig,
};

#[cfg(any(test, feature = "avatar-render-test-support"))]
pub mod test_support;
#[cfg(any(test, feature = "avatar-render-test-support"))]
pub use test_support::MockBackend;
