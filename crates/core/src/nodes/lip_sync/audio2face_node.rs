//! `Audio2FaceLipSyncNode` — coordinator that wires the persona-engine
//! Audio2Face bundle into a streaming `LipSyncNode`.
//!
//! Per spec [`docs/superpowers/specs/2026-04-27-live2d-audio2face-rvc-avatar-design.md`]
//! §3.4: consume `RuntimeData::Audio` at 16 kHz, run inference in 1-second
//! windows, solve vertex deltas to 39-D blendshape weights with PGD or
//! BVLS, expand to 52-D ARKit, optionally smooth, and emit one
//! `RuntimeData::Json {kind: "blendshapes", ...}` per ~33 ms output frame.
//!
//! ## Pipeline
//!
//! ```text
//! audio chunk(s)  →  buffer  →  [16000-sample window?]  →  ort.infer()
//!                                                              ↓
//!                                       (skin_flat: 30 × 72006)
//!                                                              ↓
//!                                  for each of 30 center frames:
//!                                    delta = skin - neutral_skin
//!                                    masked = gather via frontal_mask
//!                                    weights[K] = solver.solve(masked)
//!                                    arkit[52] = expand(weights, active_indices)
//!                                    arkit = arkit * multipliers + offsets
//!                                    arkit = smoother.smooth(arkit)
//!                                    emit BlendshapeFrame { arkit_52, pts_ms }
//! ```
//!
//! ## State + locking
//!
//! All per-call state (`Audio2FaceInference`, the boxed `BlendshapeSolver`,
//! the `ArkitSmoother`, and the audio accumulator) lives behind
//! `parking_lot::Mutex`. We never hold a mutex across an `.await` (the
//! inference + solver are sync), so the parking_lot variant is fine —
//! cheaper than `tokio::sync::Mutex` and the same shape as `silero_vad`'s
//! `Arc<Mutex<...>>` storage.
//!
//! ## Barge handling
//!
//! Spec §3.4 calls for `barge_in` to clear in-flight state so the renderer
//! immediately switches to the new turn. The node accepts a
//! `RuntimeData::Json {kind: "barge_in"}` envelope on its input port; on
//! receipt it calls [`Audio2FaceInference::reset_state`],
//! [`BlendshapeSolver::reset_temporal`], [`ArkitSmoother::reset`], and
//! drains the audio buffer. A direct [`Self::barge`] method is exposed for
//! tests + non-streaming callers.

use crate::data::audio_samples::AudioSamples;
use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::lip_sync::audio2face::inference::{
    Audio2FaceInference, AUDIO_BUFFER_LEN, NUM_CENTER_FRAMES, SKIN_SIZE,
};
use crate::nodes::lip_sync::audio2face::{
    Audio2FaceIdentity, BlendshapeConfig, BlendshapeData, BundlePaths, BvlsBlendshapeSolver,
    PgdBlendshapeSolver,
};
use crate::nodes::lip_sync::audio2face::solver_trait::BlendshapeSolver;
use crate::nodes::lip_sync::blendshape::{BlendshapeFrame, ARKIT_52};
use crate::nodes::lip_sync::{ArkitSmoother, LipSyncNode};
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Solver to use for the masked-delta → 39-D weight step. PGD is the
/// persona-engine default (faster, slightly less accurate); BVLS is the
/// reference (slower, scipy-equivalent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SolverChoice {
    /// Projected gradient descent with LU-warm-started initial guess.
    Pgd,
    /// Bounded-variable least squares (active-set + Cholesky).
    Bvls,
}

impl Default for SolverChoice {
    fn default() -> Self {
        SolverChoice::Pgd
    }
}

/// Configuration for [`Audio2FaceLipSyncNode`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Audio2FaceLipSyncConfig {
    /// Path to the unpacked persona-engine Audio2Face bundle (the
    /// directory containing `network.onnx`, `bs_skin_<Identity>.npz`,
    /// etc.). See `scripts/install-audio2face.sh`.
    pub bundle_path: PathBuf,
    /// Identity slot (Claire / James / Mark).
    pub identity: Audio2FaceIdentity,
    /// Solver to use.
    pub solver: SolverChoice,
    /// Whether to use GPU execution providers (currently informational —
    /// the workspace's `ort` ships CPU-only by default).
    pub use_gpu: bool,
    /// Uniform EMA alpha applied to the 52-D ARKit vector before
    /// emission. `0.0` = no smoothing (passthrough). Spec default is
    /// `0.0`; render-side per-axis smoothing lives in the renderer
    /// (per spec §3.4).
    pub smoothing_alpha: f32,
}

impl Default for Audio2FaceLipSyncConfig {
    fn default() -> Self {
        Self {
            bundle_path: PathBuf::new(),
            identity: Audio2FaceIdentity::Claire,
            solver: SolverChoice::Pgd,
            use_gpu: false,
            smoothing_alpha: 0.0,
        }
    }
}

/// Inference + solve coordinator implementing the [`LipSyncNode`] trait.
pub struct Audio2FaceLipSyncNode {
    config: Audio2FaceLipSyncConfig,
    inference: Arc<Mutex<Audio2FaceInference>>,
    solver: Arc<Mutex<Box<dyn BlendshapeSolver + Send>>>,
    smoother: Arc<Mutex<ArkitSmoother>>,
    bs_config: Arc<BlendshapeConfig>,
    data: Arc<BlendshapeData>,
    /// Audio accumulator — appended on every Audio chunk; drained in
    /// [`AUDIO_BUFFER_LEN`]-sample windows.
    audio_buffer: Arc<Mutex<Vec<f32>>>,
    /// Cumulative ms of audio that has been *fully consumed* by an
    /// inference call (i.e. multiples of 1000). The pts_ms of frame `f`
    /// emitted from a window is `cum_window_ms + f * 1000 / 30`.
    cum_window_ms: Arc<AtomicU64>,
}

impl Audio2FaceLipSyncNode {
    /// Load the bundle from disk and assemble the inference + solver
    /// stack. Heavy: reads ~700 MiB of model weights + ~150 MiB of NPZ
    /// data + builds K-D solver matrices.
    pub fn load(config: Audio2FaceLipSyncConfig) -> Result<Self> {
        let paths = BundlePaths::new(&config.bundle_path, config.identity);

        let bs_config = BlendshapeConfig::from_path(paths.bs_skin_config())
            .map_err(|e| Error::Execution(format!("blendshape config: {e}")))?;
        let data = BlendshapeData::load(paths.bs_skin_npz(), paths.model_data_npz(), &bs_config)
            .map_err(|e| Error::Execution(format!("blendshape data: {e}")))?;
        let inference = Audio2FaceInference::load(paths.network_onnx(), config.use_gpu)?;

        let solver: Box<dyn BlendshapeSolver + Send> = match config.solver {
            SolverChoice::Pgd => Box::new(PgdBlendshapeSolver::new(
                &data.delta_matrix,
                data.masked_position_count,
                data.active_count,
                &data.neutral_flat,
                bs_config.template_bb_size,
                bs_config.strength_l2,
                bs_config.strength_l1,
                bs_config.strength_temporal,
            )),
            SolverChoice::Bvls => Box::new(BvlsBlendshapeSolver::new(
                &data.delta_matrix,
                data.masked_position_count,
                data.active_count,
                &data.neutral_flat,
                bs_config.template_bb_size,
                bs_config.strength_l2,
                bs_config.strength_l1,
                bs_config.strength_temporal,
            )),
        };

        let smoother = ArkitSmoother::new(config.smoothing_alpha);

        Ok(Self {
            config,
            inference: Arc::new(Mutex::new(inference)),
            solver: Arc::new(Mutex::new(solver)),
            smoother: Arc::new(Mutex::new(smoother)),
            bs_config: Arc::new(bs_config),
            data: Arc::new(data),
            audio_buffer: Arc::new(Mutex::new(Vec::with_capacity(AUDIO_BUFFER_LEN * 2))),
            cum_window_ms: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Drop all in-flight state: GRU, solver temporal pull, smoother,
    /// and the audio accumulator. Idempotent. Used by `barge_in`.
    pub fn barge(&self) {
        self.inference.lock().reset_state();
        self.solver.lock().reset_temporal();
        self.smoother.lock().reset();
        self.audio_buffer.lock().clear();
        self.cum_window_ms.store(0, Ordering::Release);
    }

    /// Process one inference window's worth of audio: run the model,
    /// solve each of the 30 center frames, and feed BlendshapeFrame
    /// JSON envelopes to `emit`.
    fn process_window<F>(&self, window: &[f32], window_start_ms: u64, mut emit: F) -> Result<usize>
    where
        F: FnMut(RuntimeData) -> Result<()>,
    {
        let identity_idx = self.config.identity.one_hot_index();
        let out = {
            let mut infer = self.inference.lock();
            infer.infer(window, identity_idx)?
        };
        debug_assert_eq!(out.frame_count, NUM_CENTER_FRAMES);

        let mut solver = self.solver.lock();
        let mut smoother = self.smoother.lock();

        // Step is f64 to avoid drift when summed over many seconds.
        let frame_ms_step = 1000.0_f64 / NUM_CENTER_FRAMES as f64;

        let mut emitted = 0usize;
        for f in 0..NUM_CENTER_FRAMES {
            let skin_frame = &out.skin_flat[f * SKIN_SIZE..(f + 1) * SKIN_SIZE];
            let arkit = self.skin_frame_to_arkit(skin_frame, solver.as_mut(), &mut smoother);
            let pts_ms = window_start_ms + (f as f64 * frame_ms_step) as u64;
            let frame = BlendshapeFrame::new(arkit, pts_ms, None);
            emit(RuntimeData::Json(frame.to_json()))?;
            emitted += 1;
        }
        Ok(emitted)
    }

    /// Convert one skin frame (full 24002-vertex ×3 deltas) into a
    /// 52-D ARKit blendshape vector via the solver chain.
    fn skin_frame_to_arkit(
        &self,
        skin_frame: &[f32],
        solver: &mut (dyn BlendshapeSolver + Send),
        smoother: &mut ArkitSmoother,
    ) -> [f32; ARKIT_52] {
        // 1. Compute full vertex delta = skin_frame - neutral_skin.
        //    Then gather to the masked subset. We could fuse the two
        //    into one pass, but keeping them separate matches the C#
        //    reference and is easier to verify.
        let mask = &self.data.frontal_mask;
        let neutral = &self.data.neutral_skin_flat;
        let masked_count = self.data.masked_position_count;
        let mut masked_delta = vec![0.0f32; masked_count];
        for (m, &vi_i32) in mask.iter().enumerate() {
            let vi = vi_i32 as usize;
            let base = vi * 3;
            // Bounds-check defensively — masked indices come from the
            // bundle and are validated at load, but a pathological
            // bundle shouldn't crash a streaming session.
            if base + 2 >= skin_frame.len() || base + 2 >= neutral.len() {
                continue;
            }
            masked_delta[m * 3] = skin_frame[base] - neutral[base];
            masked_delta[m * 3 + 1] = skin_frame[base + 1] - neutral[base + 1];
            masked_delta[m * 3 + 2] = skin_frame[base + 2] - neutral[base + 2];
        }

        // 2. Solve → K-D weights for the active blendshapes.
        let weights = solver.solve(&masked_delta);

        // 3. Expand to 52-D, then apply per-blendshape multiplier+offset.
        let mut arkit = [0.0f32; ARKIT_52];
        for (k, &pose_index) in self.bs_config.active_indices.iter().enumerate() {
            if pose_index < ARKIT_52 {
                arkit[pose_index] = weights[k];
            }
        }
        for i in 0..ARKIT_52 {
            arkit[i] = arkit[i] * self.bs_config.multipliers[i] + self.bs_config.offsets[i];
        }

        // 4. Smooth (passthrough when alpha == 0).
        smoother.smooth(&arkit)
    }
}

#[async_trait]
impl AsyncStreamingNode for Audio2FaceLipSyncNode {
    fn node_type(&self) -> &str {
        "Audio2FaceLipSyncNode"
    }

    async fn process(&self, _data: RuntimeData) -> Result<RuntimeData> {
        Err(Error::Execution(
            "Audio2FaceLipSyncNode requires streaming mode — use process_streaming()".into(),
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
        // Barge envelope: {kind: "barge_in"}. Clears state and emits nothing.
        if let RuntimeData::Json(v) = &data {
            if v.get("kind").and_then(|k| k.as_str()) == Some("barge_in") {
                self.barge();
                return Ok(0);
            }
        }

        let (samples, sample_rate) = match data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                ..
            } => (samples, sample_rate),
            other => {
                // Pass non-Audio through untouched (matches synthetic + silero_vad).
                callback(other)?;
                return Ok(1);
            }
        };

        if sample_rate != 16_000 {
            return Err(Error::InvalidData(format!(
                "Audio2FaceLipSyncNode requires 16 kHz audio (capability \
                 resolver should insert a resampler upstream); got {sample_rate}"
            )));
        }

        // Drain windows out of the buffer. Doing the drain inside a
        // scoped lock means we only hold `audio_buffer` while copying;
        // inference + solve happen with the buffer mutex released, so
        // concurrent input chunks aren't blocked on inference.
        let windows: Vec<Vec<f32>> = {
            let mut buf = self.audio_buffer.lock();
            buf.extend_from_slice(samples.as_slice());
            let mut ws = Vec::new();
            while buf.len() >= AUDIO_BUFFER_LEN {
                let drained: Vec<f32> = buf.drain(..AUDIO_BUFFER_LEN).collect();
                ws.push(drained);
            }
            ws
        };

        let mut emitted = 0;
        for window in windows {
            let window_start_ms = self.cum_window_ms.fetch_add(1000, Ordering::AcqRel);
            emitted += self.process_window(&window, window_start_ms, &mut callback)?;
        }
        Ok(emitted)
    }
}

impl LipSyncNode for Audio2FaceLipSyncNode {
    fn required_sample_rate(&self) -> u32 {
        16_000
    }
}

// `AudioSamples::as_slice` is a nicety we lean on above. Make sure it
// stays referenced by the compiler so feature-gated builds catch any
// upstream rename early.
#[allow(dead_code)]
fn _audio_samples_as_slice_witness(s: &AudioSamples) -> &[f32] {
    s.as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Solver-choice serde round-trips with the documented lowercase form.
    #[test]
    fn solver_choice_serde_lowercase() {
        let pgd = serde_json::to_string(&SolverChoice::Pgd).unwrap();
        let bvls = serde_json::to_string(&SolverChoice::Bvls).unwrap();
        assert_eq!(pgd, "\"pgd\"");
        assert_eq!(bvls, "\"bvls\"");
        let back: SolverChoice = serde_json::from_str("\"bvls\"").unwrap();
        assert_eq!(back, SolverChoice::Bvls);
    }

    /// Config defaults match the spec (Claire / PGD / no smoothing).
    #[test]
    fn config_defaults_match_spec() {
        let c = Audio2FaceLipSyncConfig::default();
        assert_eq!(c.identity, Audio2FaceIdentity::Claire);
        assert_eq!(c.solver, SolverChoice::Pgd);
        assert_eq!(c.smoothing_alpha, 0.0);
        assert!(!c.use_gpu);
    }

    /// `process` is the non-streaming path; the node mandates streaming.
    /// Worth pinning so anyone refactoring `AsyncStreamingNode`'s default
    /// process-streaming bridge sees this fall over loudly.
    #[tokio::test]
    async fn process_is_unsupported_without_loaded_state() {
        // We can't actually `load` without the bundle on disk; use a
        // bench-style assertion that the error message comes from the
        // documented branch by constructing a minimal failure case
        // through the public API. (Skipped when no bundle.)
        if std::env::var("AUDIO2FACE_TEST_BUNDLE").is_err() {
            eprintln!("skipping: AUDIO2FACE_TEST_BUNDLE not set");
            return;
        }
        let cfg = Audio2FaceLipSyncConfig {
            bundle_path: PathBuf::from(std::env::var("AUDIO2FACE_TEST_BUNDLE").unwrap()),
            ..Audio2FaceLipSyncConfig::default()
        };
        let node = Audio2FaceLipSyncNode::load(cfg).expect("load");
        let err = node.process(RuntimeData::Text("x".into())).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("requires streaming mode"),
            "unexpected error: {msg}"
        );
    }
}
