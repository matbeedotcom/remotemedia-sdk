//! `Live2DRenderNode` — streaming wire-up for the Live2D renderer.
//!
//! Per spec §6.1 the renderer is a **free-running 30 fps sampler**:
//! it ticks on its own clock, samples blendshape ring + emotion +
//! blink state into a [`Pose`], and asks the backend to render. No
//! input pressure dictates the render rate.
//!
//! This module wires the M4.3 state machine + the M4.4 backend trait
//! into an [`AsyncStreamingNode`] that:
//!
//! - Accepts four kinds of `RuntimeData::Json` input on its main port:
//!   - `{kind: "blendshapes", arkit_52, pts_ms, turn_id?}` — from
//!     `Audio2FaceLipSyncNode` / `SyntheticLipSyncNode`.
//!   - `{kind: "emotion", emoji, …}` — from `EmotionExtractorNode`.
//!   - `{kind: "audio_clock", pts_ms, …}` — from the WebRTC
//!     `AudioSender`'s `audio.out.clock` tap.
//!   - `{kind: "barge_in"}` — from coordinator-driven barge.
//!   The barge-in aux-port envelope (M2.6 plumbing) is also handled
//!   via [`AsyncStreamingNode::process_control_message`].
//!
//! - Emits `RuntimeData::Video {pixel_data, width, height, format,
//!   stream_id, …}` at the configured `framerate` (default 30 fps),
//!   stamped with the configured `video_stream_id` (default
//!   `"avatar"`).
//!
//! The tick is driven by an internal `tokio::time::interval` task
//! spawned in [`Live2DRenderNode::new_with_backend`]. Each tick:
//! 1. Drains pending inputs from the input mpsc
//! 2. Calls `state.tick(elapsed_ms)`
//! 3. `pose = state.compute_pose()`
//! 4. `backend.render_frame(&pose)`
//! 5. Pushes the resulting `RuntimeData::Video` onto the output
//!    queue
//!
//! `process_streaming` simply forwards inputs into the input mpsc
//! and drains any pending output frames into the runtime callback.

use super::backend_trait::Live2DBackend;
use super::state::{Live2DRenderState, StateConfig};
use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::lip_sync::BlendshapeFrame;
use crate::nodes::AsyncStreamingNode;
use crate::transport::session_control::{aux_port_of, BARGE_IN_PORT};
use async_trait::async_trait;
use parking_lot::Mutex as ParkMutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;

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

/// Free-running, free-emitting Live2D render node.
pub struct Live2DRenderNode {
    config: Live2DRenderConfig,
    /// One-shot input channel: `process_streaming` enqueues inputs
    /// here; the ticker task drains it on every tick.
    input_tx: mpsc::UnboundedSender<RendererInput>,
    /// Output ring: ticker task pushes rendered frames here;
    /// `process_streaming` drains all pending frames per call.
    output_rx: Arc<AsyncMutex<mpsc::UnboundedReceiver<RuntimeData>>>,
    /// JoinHandle for the ticker; aborted on drop.
    ticker: ParkMutex<Option<JoinHandle<()>>>,
    /// Frame counter for the emitted Video frames.
    frame_counter: Arc<AtomicU64>,
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

impl Drop for Live2DRenderNode {
    fn drop(&mut self) {
        if let Some(h) = self.ticker.lock().take() {
            h.abort();
        }
    }
}

/// Decoded input to the state machine. The `process_streaming`
/// dispatcher decodes incoming `RuntimeData::Json` envelopes by
/// `kind` and pushes the matching `RendererInput` onto the input
/// channel. The ticker task drains them at the next tick.
#[derive(Debug)]
enum RendererInput {
    Blendshape(BlendshapeFrame),
    Emotion(String),
    AudioClock(u64),
    BargeIn,
}

impl Live2DRenderNode {
    /// Build the node with the given backend + config and spawn the
    /// internal ticker task.
    ///
    /// Caller responsibility: the backend must already have its
    /// model loaded. For `WgpuBackend` that means calling
    /// `load_model(&config.model_path)` before this — this signature
    /// hands the backend off to the ticker so we can't load it
    /// after.
    pub fn new_with_backend(
        backend: Box<dyn Live2DBackend + Send>,
        config: Live2DRenderConfig,
    ) -> Self {
        let (input_tx, input_rx) = mpsc::unbounded_channel::<RendererInput>();
        let (output_tx, output_rx) = mpsc::unbounded_channel::<RuntimeData>();
        let frame_counter = Arc::new(AtomicU64::new(0));

        let ticker = tokio::spawn(ticker_loop(
            backend,
            Live2DRenderState::new(config.state_config.clone()),
            input_rx,
            output_tx,
            config.framerate.max(1),
            config.video_stream_id.clone(),
            frame_counter.clone(),
        ));

        Self {
            config,
            input_tx,
            output_rx: Arc::new(AsyncMutex::new(output_rx)),
            ticker: ParkMutex::new(Some(ticker)),
            frame_counter,
        }
    }

    /// Total frames emitted since construction (read for tests +
    /// diagnostics).
    pub fn frames_emitted(&self) -> u64 {
        self.frame_counter.load(Ordering::Relaxed)
    }

    /// Decode one input envelope and push it to the ticker. Returns
    /// `true` if the envelope was a recognized renderer input,
    /// `false` if it should be passed through (the caller's
    /// responsibility).
    fn dispatch_envelope(&self, data: &RuntimeData) -> bool {
        // Wrapped barge envelope from the session router (M2.6 path).
        if matches!(aux_port_of(data), Some(BARGE_IN_PORT)) {
            let _ = self.input_tx.send(RendererInput::BargeIn);
            return true;
        }
        let RuntimeData::Json(v) = data else {
            return false;
        };
        let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        match kind {
            "blendshapes" => {
                if let Ok(frame) = BlendshapeFrame::from_json(v) {
                    let _ = self.input_tx.send(RendererInput::Blendshape(frame));
                    return true;
                }
                false
            }
            "emotion" => {
                if let Some(emoji) = v.get("emoji").and_then(|e| e.as_str()) {
                    let _ = self.input_tx.send(RendererInput::Emotion(emoji.to_string()));
                    return true;
                }
                false
            }
            "audio_clock" => {
                if let Some(pts) = v.get("pts_ms").and_then(|p| p.as_u64()) {
                    let _ = self.input_tx.send(RendererInput::AudioClock(pts));
                    return true;
                }
                false
            }
            "barge_in" => {
                let _ = self.input_tx.send(RendererInput::BargeIn);
                true
            }
            _ => false,
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
        // Push input into the ticker (no-op if envelope shape is
        // unrecognized; we don't pass it through because the
        // renderer is a sink — its outputs are Video, never
        // forwarded inputs).
        self.dispatch_envelope(&data);

        // Drain all queued Video frames produced by the ticker
        // since the last call. This is what gets the frames into
        // the runtime's fan-out.
        let mut emitted = 0;
        let mut rx = self.output_rx.lock().await;
        while let Ok(frame) = rx.try_recv() {
            callback(frame)?;
            emitted += 1;
        }
        Ok(emitted)
    }

    /// Universal barge handler — the session router (M2.6) forwards
    /// `<node>.in.barge_in` aux-port envelopes here in addition to
    /// firing `cancel.notify_waiters()`. We push a `BargeIn` input
    /// to the ticker so the next tick clears the blendshape ring +
    /// snaps mouth to neutral (per spec §6.3).
    async fn process_control_message(
        &self,
        message: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<bool> {
        if matches!(aux_port_of(&message), Some(BARGE_IN_PORT)) {
            let _ = self.input_tx.send(RendererInput::BargeIn);
            return Ok(true);
        }
        Ok(false)
    }
}

/// Internal ticker. Owns the backend + state machine. Wakes every
/// `1000/framerate` ms, drains pending inputs into the state, ticks
/// the wall clock, computes pose, renders, emits Video.
async fn ticker_loop(
    mut backend: Box<dyn Live2DBackend + Send>,
    mut state: Live2DRenderState,
    mut input_rx: mpsc::UnboundedReceiver<RendererInput>,
    output_tx: mpsc::UnboundedSender<RuntimeData>,
    framerate: u32,
    stream_id: String,
    frame_counter: Arc<AtomicU64>,
) {
    let frame_interval = Duration::from_millis(1000_u64 / framerate as u64);
    let mut interval = tokio::time::interval(frame_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_tick = Instant::now();
    let session_start = Instant::now();
    let (width, height) = backend.frame_dimensions();

    // Audio-clock synthesis — for pipelines without an external
    // `audio.out.clock` tap (e.g. the disk-capture chain has no
    // WebRTC AudioSender), the renderer fakes a playback clock so
    // the state machine actually samples its ring instead of
    // returning neutral mouth forever.
    //
    // Naïve "wall_time_since_first_blendshape" overshoots the
    // buffer: Audio2Face produces blendshapes in 1-second batches
    // every ~few seconds (cold inference is 3.6s; warm is
    // ~100ms), and a real-time clock leaves the ring stale-evicted
    // between batches. We advance the synth clock by elapsed wall
    // time **capped at the latest blendshape's pts**. When the
    // buffer is ahead, playback is smooth real-time; when the
    // buffer is starved, the clock waits at the leading edge so
    // the renderer holds the most recent pose instead of falling
    // off the end of the ring.
    //
    // WebRTC pipelines that publish explicit AudioClock events
    // override this — we suppress synth for 200 ms after each
    // explicit publish.
    let mut latest_blendshape_pts: Option<u64> = None;
    let mut synth_pts: u64 = 0;
    let mut last_synth_at: Option<Instant> = None;
    let mut last_explicit_clock_at: Option<Instant> = None;
    loop {
        // Wait for the next interval tick. (We used to also `select!`
        // an `input_rx.recv()` arm with an `if false` guard — but
        // tokio::select! still POLLS the future at least once before
        // checking the guard, which silently consumed values from
        // the unbounded channel under burst load. Drainage then only
        // showed the first 1-2 inputs of every batch. Use plain
        // `interval.tick()` and let the parent's drop close
        // input_rx; we'll see Err::Disconnected on try_recv and
        // exit gracefully.)
        interval.tick().await;

        // Drain pending inputs (non-blocking). Disconnected = parent
        // dropped → exit cleanly.
        loop {
            let input = match input_rx.try_recv() {
                Ok(i) => i,
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return,
            };
            match input {
                RendererInput::Blendshape(f) => {
                    let pts = f.pts_ms;
                    if last_synth_at.is_none() {
                        // First blendshape arrived — anchor the
                        // synth clock here.
                        synth_pts = pts;
                        last_synth_at = Some(Instant::now());
                    }
                    latest_blendshape_pts = Some(
                        latest_blendshape_pts.map_or(pts, |old| old.max(pts)),
                    );
                    state.push_blendshape(f);
                }
                RendererInput::Emotion(e) => state.push_emotion(&e),
                RendererInput::AudioClock(pts) => {
                    last_explicit_clock_at = Some(Instant::now());
                    state.update_audio_clock(pts);
                }
                RendererInput::BargeIn => {
                    // Reset the synth state too — post-barge a new
                    // blendshape stream re-anchors.
                    latest_blendshape_pts = None;
                    last_synth_at = None;
                    synth_pts = 0;
                    state.handle_barge();
                }
            }
        }

        // Synthesize audio_clock from wall time (capped at the
        // latest blendshape pts) when no explicit audio.out.clock
        // tap is firing. "Recent explicit" = within 200ms.
        let explicit_clock_recent = last_explicit_clock_at
            .map(|t| t.elapsed() <= Duration::from_millis(200))
            .unwrap_or(false);
        if !explicit_clock_recent {
            if let (Some(latest), Some(prev_synth_at)) =
                (latest_blendshape_pts, last_synth_at)
            {
                let now = Instant::now();
                let elapsed_ms = now.duration_since(prev_synth_at).as_millis() as u64;
                last_synth_at = Some(now);
                synth_pts = synth_pts.saturating_add(elapsed_ms).min(latest);
                state.update_audio_clock(synth_pts);
            }
        }

        // Advance virtual wall clock by the wall ms since last tick.
        let now = Instant::now();
        let elapsed_ms = now.duration_since(last_tick).as_millis() as u64;
        last_tick = now;
        state.tick(elapsed_ms);

        // Render.
        let pose = state.compute_pose();
        let frame = match backend.render_frame(&pose) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    "Live2DRenderNode backend.render_frame failed: {} \
                     (skipping frame)",
                    e
                );
                continue;
            }
        };

        // Convert RgbFrame → RuntimeData::Video. We expose RGB24
        // raw frames; downstream encoders (M4.6) take it from there.
        let frame_number = frame_counter.fetch_add(1, Ordering::AcqRel);
        let pts_us = now.duration_since(session_start).as_micros() as u64;
        let video = RuntimeData::Video {
            pixel_data: frame.pixels,
            width: frame.width,
            height: frame.height,
            format: crate::data::video::PixelFormat::Rgb24,
            codec: None,
            frame_number,
            timestamp_us: pts_us,
            is_keyframe: true, // Raw frames are inherently independent
            stream_id: Some(stream_id.clone()),
            arrival_ts_us: None,
        };
        let _ = (width, height); // consumed via frame.width/height above

        if output_tx.send(video).is_err() {
            // Parent dropped → exit.
            break;
        }
    }
}
