//! `Live2DRenderNode` — streaming wire-up for the Live2D renderer.
//!
//! Audio-clock-driven: each input envelope synchronously produces at
//! most one rendered Video frame, stamped with the input's audio-time
//! pts (relative to the first input's pts as t=0). Audio2Face emits
//! 30 blendshapes per second of audio, so the resulting video is
//! 30 fps with frame_N at audio-time `N * 33 ms` — perfectly
//! pts-aligned with the saved `audio.wav`, so `ffmpeg -i video.y4m
//! -i audio.wav out.mp4` muxes them in lip-sync without re-stamping.
//!
//! This module wires the M4.3 state machine + the M4.4 backend trait
//! into an [`AsyncStreamingNode`] that:
//!
//! - Accepts four kinds of `RuntimeData::Json` input on its main port:
//!   - `{kind: "blendshapes", arkit_52, pts_ms, turn_id?}` — from
//!     `Audio2FaceLipSyncNode` / `SyntheticLipSyncNode`. Renders
//!     a frame.
//!   - `{kind: "emotion", emoji, …}` — from `EmotionExtractorNode`.
//!     Updates state but does NOT render.
//!   - `{kind: "audio_clock", pts_ms, …}` — from the WebRTC
//!     `AudioSender`'s `audio.out.clock` tap. Renders a frame.
//!   - `{kind: "barge_in"}` — from coordinator-driven barge. Resets
//!     state; the next blendshape re-anchors the audio timeline.
//!   The barge-in aux-port envelope (M2.6 plumbing) is also handled
//!   via [`AsyncStreamingNode::process_control_message`].
//!
//! - Emits `RuntimeData::Video {pixel_data, width, height, format,
//!   stream_id, …}` synchronously through the runtime callback,
//!   stamped with the configured `video_stream_id` (default
//!   `"avatar"`) and a `timestamp_us` rooted at the first input's
//!   pts as audio-time zero.
//!
//! Idle/blink animation during silence is not emitted — when no
//! blendshape arrives, no Video frame is produced. The state
//! machine's `tick_blink_scheduler` runs on every emit, so blinks
//! fire during the speaking section.

use super::backend_trait::Live2DBackend;
use super::state::{Live2DRenderState, StateConfig};
use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::lip_sync::BlendshapeFrame;
use crate::nodes::AsyncStreamingNode;
use crate::transport::session_control::{aux_port_of, BARGE_IN_PORT};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

/// Configuration for [`Live2DRenderNode`].
///
/// Not currently `Serialize`/`Deserialize` because [`StateConfig`]
/// holds an `Arc<dyn ArkitToVBridger>` (the ARKit→VBridger mapper)
/// that doesn't have a single canonical wire shape. M4.6 will wire
/// a factory that builds this from a manifest's flat YAML/JSON
/// fields directly.
#[derive(Debug, Clone)]
pub struct Live2DRenderConfig {
    /// Path to the Live2D `.model3.json` to render. Backend
    /// implementations (e.g. `WgpuBackend::load_model`) consume
    /// this on construction. The state-machine-only path used by
    /// `MockBackend` ignores it.
    pub model_path: Option<PathBuf>,
    /// Output framerate (frames per second). Defaults to 30 per
    /// spec §6.1.
    pub framerate: u32,
    /// Output `RuntimeData::Video.stream_id`. Defaults to `"avatar"`.
    /// Useful for multi-avatar pipelines (one renderer per avatar
    /// with a different `stream_id`).
    pub video_stream_id: String,
    /// State machine knobs forwarded to [`Live2DRenderState`].
    /// Defaults match persona-engine's published values.
    pub state_config: StateConfig,
}

impl Default for Live2DRenderConfig {
    fn default() -> Self {
        Self {
            model_path: None,
            framerate: 30,
            video_stream_id: "avatar".to_string(),
            state_config: StateConfig::default_config(),
        }
    }
}

/// Synchronous, input-driven Live2D render node.
///
/// One incoming envelope → at most one rendered Video frame, forwarded
/// inline through the runtime callback. No ticker task, no internal
/// queues: rendering happens on the caller's tokio worker, and the
/// `callback` (provided by `SessionRouter`) is the only forwarding
/// path. This keeps producer and drainer in lockstep — every frame
/// produced is also dispatched in the same `process_streaming` call.
pub struct Live2DRenderNode {
    config: Live2DRenderConfig,
    /// Backend + state machine + ARKit-time anchor. Behind a mutex
    /// because `process_streaming` takes `&self` (the runtime calls
    /// it with shared borrow) but we need exclusive access to the
    /// backend's `&mut self` and to the state machine's mutators.
    /// Concurrent calls serialize through this lock.
    inner: Arc<AsyncMutex<RendererInner>>,
    /// Frame counter for the emitted Video frames.
    frame_counter: Arc<AtomicU64>,
}

struct RendererInner {
    backend: Box<dyn Live2DBackend + Send>,
    state: Live2DRenderState,
    /// First blendshape (or AudioClock) pts becomes audio-time zero.
    /// All subsequent video pts are relative to this so video.y4m
    /// starts at frame 0 = audio sample 0 of the saved audio.wav.
    anchor_pts_ms: Option<u64>,
}

impl std::fmt::Debug for Live2DRenderNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Live2DRenderNode")
            .field("framerate", &self.config.framerate)
            .field("video_stream_id", &self.config.video_stream_id)
            .field(
                "frames_emitted",
                &self.frame_counter.load(Ordering::Relaxed),
            )
            .finish()
    }
}

/// Decoded input parsed from a `RuntimeData::Json` envelope.
#[derive(Debug)]
enum RendererInput {
    Blendshape(BlendshapeFrame),
    Emotion(String),
    AudioClock(u64),
    BargeIn,
}

impl Live2DRenderNode {
    /// Build the node with the given backend + config.
    ///
    /// Caller responsibility: the backend must already have its
    /// model loaded. For `WgpuBackend` that means calling
    /// `load_model(&config.model_path)` before this.
    pub fn new_with_backend(
        backend: Box<dyn Live2DBackend + Send>,
        config: Live2DRenderConfig,
    ) -> Self {
        let inner = RendererInner {
            backend,
            state: Live2DRenderState::new(config.state_config.clone()),
            anchor_pts_ms: None,
        };
        Self {
            config,
            inner: Arc::new(AsyncMutex::new(inner)),
            frame_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Total frames emitted since construction (read for tests +
    /// diagnostics).
    pub fn frames_emitted(&self) -> u64 {
        self.frame_counter.load(Ordering::Relaxed)
    }

    /// Parse one envelope into a [`RendererInput`]. Returns `None`
    /// for envelopes the renderer doesn't recognize.
    fn decode_envelope(data: &RuntimeData) -> Option<RendererInput> {
        // Wrapped barge envelope from the session router (M2.6 path).
        if matches!(aux_port_of(data), Some(BARGE_IN_PORT)) {
            return Some(RendererInput::BargeIn);
        }
        let RuntimeData::Json(v) = data else {
            return None;
        };
        let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        match kind {
            "blendshapes" => {
                BlendshapeFrame::from_json(v).ok().map(RendererInput::Blendshape)
            }
            "emotion" => v
                .get("emoji")
                .and_then(|e| e.as_str())
                .map(|s| RendererInput::Emotion(s.to_string())),
            "audio_clock" => v
                .get("pts_ms")
                .and_then(|p| p.as_u64())
                .map(RendererInput::AudioClock),
            "barge_in" => Some(RendererInput::BargeIn),
            _ => None,
        }
    }
}

#[async_trait]
impl AsyncStreamingNode for Live2DRenderNode {
    fn node_type(&self) -> &str {
        "Live2DRenderNode"
    }

    async fn process(&self, _data: RuntimeData) -> Result<RuntimeData> {
        Err(Error::Execution(
            "Live2DRenderNode requires streaming mode — use process_streaming()".into(),
        ))
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        _session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize>
    where
        F: FnMut(RuntimeData) -> Result<()> + Send,
    {
        let Some(input) = Self::decode_envelope(&data) else {
            // Unrecognized envelope. The renderer is a sink — its
            // outputs are Video, never forwarded inputs — so just
            // drop unknown envelopes silently.
            return Ok(0);
        };

        let mut inner = self.inner.lock().await;

        // Decide whether to render a frame for this input + the audio-
        // time pts to stamp it with.
        let emit_pts: Option<u64> = match input {
            RendererInput::Blendshape(f) => {
                let pts = f.pts_ms;
                if inner.anchor_pts_ms.is_none() {
                    inner.anchor_pts_ms = Some(pts);
                }
                inner.state.push_blendshape(f);
                Some(pts)
            }
            RendererInput::Emotion(e) => {
                inner.state.push_emotion(&e);
                None
            }
            RendererInput::AudioClock(pts) => {
                if inner.anchor_pts_ms.is_none() {
                    inner.anchor_pts_ms = Some(pts);
                }
                Some(pts)
            }
            RendererInput::BargeIn => {
                inner.state.handle_barge();
                inner.anchor_pts_ms = None;
                None
            }
        };

        let Some(absolute_pts) = emit_pts else {
            return Ok(0);
        };
        let anchor = inner.anchor_pts_ms.expect("anchor set above");
        let cursor_ms = absolute_pts.saturating_sub(anchor);
        let frame_interval_ms: u64 =
            1000_u64 / self.config.framerate.max(1) as u64;

        // Drive the state machine to this audio-time, then render.
        inner.state.update_audio_clock(absolute_pts);
        inner.state.tick(frame_interval_ms);
        let pose = inner.state.compute_pose();

        let frame = match inner.backend.render_frame(&pose) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    "Live2DRenderNode backend.render_frame failed: {} (skipping frame)",
                    e
                );
                return Ok(0);
            }
        };

        let frame_number = self.frame_counter.fetch_add(1, Ordering::AcqRel);
        let video = RuntimeData::Video {
            pixel_data: frame.pixels,
            width: frame.width,
            height: frame.height,
            format: crate::data::video::PixelFormat::Rgb24,
            codec: None,
            frame_number,
            // pts in microseconds, relative to anchor (t=0). The saved
            // video.y4m and audio.wav share this origin.
            timestamp_us: cursor_ms.saturating_mul(1000),
            is_keyframe: true,
            stream_id: Some(self.config.video_stream_id.clone()),
            arrival_ts_us: None,
        };
        callback(video)?;
        Ok(1)
    }

    /// Universal barge handler — the session router (M2.6) forwards
    /// `<node>.in.barge_in` aux-port envelopes here in addition to
    /// firing `cancel.notify_waiters()`. We reset the renderer's
    /// state + anchor so the next blendshape stream re-anchors at
    /// its own pts.
    async fn process_control_message(
        &self,
        message: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<bool> {
        if matches!(aux_port_of(&message), Some(BARGE_IN_PORT)) {
            let mut inner = self.inner.lock().await;
            inner.state.handle_barge();
            inner.anchor_pts_ms = None;
            return Ok(true);
        }
        Ok(false)
    }
}

