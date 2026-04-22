//! HRTF streaming node — 7.1 → stereo binaural spatialization.
//!
//! # Real-time safety
//!
//! RT-safe in steady state under the following constraints:
//!
//! - **Bounded maximum frame count.** The convolver's scratch output
//!   buffer is sized to `max_frames * 2` during construction (or on the
//!   first call, as a one-time grow). Subsequent calls with
//!   `frames <= max_frames` run without any heap operations. A call
//!   with `frames > max_frames` triggers a `Vec::resize` and emits a
//!   `tracing::warn!` — treat this as a configuration bug; increase
//!   `max_frames` in the node params to match your HAL buffer size.
//! - **Output reuses the input allocation.** The 7.1 → stereo
//!   convolution outputs `frames * 2` samples — half of what the
//!   8-channel input holds (`frames * 8`). We reuse the input `Vec`
//!   for the output by truncating to `frames * 2` and copying from
//!   scratch, avoiding the `to_vec()` allocation the previous
//!   implementation did on every call.
//! - **Lock held across convolution.** `parking_lot::Mutex` fast-path
//!   is ~1ns under single-consumer (rt-bridge) usage.
//! - **`HrtfConvolver::process` must itself be RT-safe.** Audit the
//!   `hrtf` crate for deployment.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;

use hrtf::HrtfConvolver;
use hrtf_synth::SpeakerIrSet;

use remotemedia_core::data::RuntimeData;
use remotemedia_core::Error;
use remotemedia_core::nodes::streaming_node::{
    StreamingNode, StreamingNodeFactory, SyncNodeWrapper, SyncStreamingNode,
};

use crate::util;

/// JSON shape:
/// ```json
/// { "ir_path": "/path/to/hrtf_irs.bin", "max_frames": 480 }
/// ```
///
/// `max_frames` is the largest audio frame count you expect to pass
/// through `process` (e.g., 480 = 10ms @ 48kHz). It is used to
/// pre-allocate the scratch output so the hot path never resizes.
/// Defaults to 4096 if omitted (large enough for typical HAL buffer
/// sizes up to ~85ms @ 48kHz, but allocates more memory than needed).
#[derive(Debug, Deserialize)]
struct Params {
    ir_path: String,
    #[serde(default = "default_max_frames")]
    max_frames: usize,
}

fn default_max_frames() -> usize {
    4096
}

struct HrtfState {
    convolver: HrtfConvolver,
    /// Pre-sized stereo output scratch. Length = `max_frames * 2`.
    /// The hot path requires `frames * 2 <= scratch_out.len()`.
    scratch_out: Vec<f32>,
    /// Advertised maximum frames; a `frames > max_frames` call on the
    /// hot path is a configuration bug and causes a one-time
    /// (RT-unsafe) grow.
    max_frames: usize,
}

pub struct HrtfNode {
    state: Arc<Mutex<HrtfState>>,
}

impl HrtfNode {
    /// Create a new HRTF node with a convolver and a pre-allocated
    /// scratch buffer sized for `max_frames` stereo frames. Call this
    /// at session-setup time, not on the RT path.
    pub fn new(convolver: HrtfConvolver, max_frames: usize) -> Self {
        // Pre-size scratch to max_frames * 2 so the hot path never
        // resizes. `vec![0.0; N]` is a one-time alloc + zero-fill at
        // construction; not on the RT path.
        let scratch_out = vec![0.0_f32; max_frames * 2];
        Self {
            state: Arc::new(Mutex::new(HrtfState {
                convolver,
                scratch_out,
                max_frames,
            })),
        }
    }
}

impl SyncStreamingNode for HrtfNode {
    fn node_type(&self) -> &str {
        "HrtfNode"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let (samples, sr, ch, sid, meta) = util::take_audio(data)?;
        if ch != 8 {
            // Pass through unchanged for non-7.1 inputs — zero work.
            return Ok(util::emit_audio(samples, sr, ch, sid, meta));
        }
        let frames = samples.len() / 8;
        if frames == 0 {
            return Ok(util::emit_audio(samples, sr, 2, sid, meta));
        }

        let mut st = self.state.lock();

        // Hot-path invariant: scratch is pre-sized. If the caller
        // violates the max_frames contract we grow once and warn;
        // that grow is the RT-unsafe path and signals a config bug.
        if st.scratch_out.len() < frames * 2 {
            tracing::warn!(
                frames,
                max_frames = st.max_frames,
                "HrtfNode: scratch resized on hot path (exceeded max_frames — \
                 increase the `max_frames` node param to match your HAL buffer size)",
            );
            st.scratch_out.resize(frames * 2, 0.0);
            st.max_frames = frames;
        }

        // Split-borrow so we can pass both fields without an alias
        // conflict.
        let HrtfState {
            convolver,
            scratch_out,
            ..
        } = &mut *st;

        // Convolve 8-channel input into the pre-allocated 2-channel
        // scratch. No allocation.
        convolver.process(&samples, &mut scratch_out[..frames * 2], frames);

        // Reuse the input Vec as the output buffer: the input has
        // `frames * 8` samples and `frames * 2 < frames * 8`, so the
        // existing allocation (capacity preserved) serves the output
        // exactly. Truncate + copy_from_slice; zero allocation.
        let mut out = samples;
        out.truncate(frames * 2);
        out.copy_from_slice(&scratch_out[..frames * 2]);

        // Drop the lock before returning so downstream consumers
        // aren't serialized through it.
        drop(st);

        Ok(util::emit_audio(out, sr, 2, sid, meta))
    }
}

fn build_hrtf(params: &Value) -> Result<HrtfNode, Error> {
    let p: Params = serde_json::from_value(params.clone())
        .map_err(|e| Error::Execution(format!("HrtfNode params: {e}")))?;
    let ir_set = SpeakerIrSet::load(Path::new(&p.ir_path))
        .map_err(|e| Error::Execution(format!("HrtfNode load {}: {e}", p.ir_path)))?;
    let ir_length = ir_set.ir_length;
    let convolver = HrtfConvolver::new(ir_set.irs.into_boxed_slice(), ir_length);
    Ok(HrtfNode::new(convolver, p.max_frames))
}

pub struct HrtfNodeFactory;

impl StreamingNodeFactory for HrtfNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        Ok(Box::new(SyncNodeWrapper(build_hrtf(params)?)))
    }

    fn node_type(&self) -> &str {
        "HrtfNode"
    }
}

impl remotemedia_core::executor::sync_executor::SyncStreamingNodeFactory for HrtfNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
    ) -> Result<Box<dyn SyncStreamingNode>, Error> {
        Ok(Box::new(build_hrtf(params)?))
    }

    fn node_type(&self) -> &str {
        "HrtfNode"
    }
}
