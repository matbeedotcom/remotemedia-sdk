//! Brickwall limiter streaming node.
//!
//! Wraps [`dsp_core::limiter::BrickwallLimiter`] so it can sit at the
//! end of a hearing-aid DSP chain. Typical placement:
//!
//!   WDRC → CROS → HRTF → **Limiter**
//!
//! WDRC can push peaks >0 dBFS on rapid high-frequency content, and the
//! HRTF convolver constructively sums 7 speaker streams into 2 ears
//! which can add another 3–9 dB of headroom pressure. The limiter
//! catches the final peaks so the user doesn't hear hard clipping when
//! real hardware re-quantizes to the DAC output.
//!
//! # Real-time safety
//!
//! - The limiter's `process_stereo` is two branches + four multiplies
//!   per frame; zero allocations.
//! - Lock held across a tight per-frame loop via `parking_lot::Mutex`
//!   (single-consumer RT contract from `rt-bridge` keeps it uncontended).
//! - Reconfigure on sample-rate change is the only path that can
//!   reallocate; happens at most once per session boundary, not on the
//!   hot path.

use std::sync::Arc;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;

use dsp_core::limiter::BrickwallLimiter;

use remotemedia_core::data::RuntimeData;
use remotemedia_core::Error;
use remotemedia_core::nodes::streaming_node::{
    StreamingNode, StreamingNodeFactory, SyncNodeWrapper, SyncStreamingNode,
};

use crate::util;

/// JSON shape:
/// ```json
/// {
///   "threshold_db": -1.0,
///   "release_ms": 50.0,
///   "sample_rate": 48000
/// }
/// ```
/// All three are optional; defaults apply piecewise.
#[derive(Debug, Deserialize)]
struct Params {
    #[serde(default = "default_threshold_db")]
    threshold_db: f32,
    #[serde(default = "default_release_ms")]
    release_ms: f32,
    #[serde(default)]
    sample_rate: Option<u32>,
}

fn default_threshold_db() -> f32 {
    -1.0
}
fn default_release_ms() -> f32 {
    50.0
}

struct LimiterState {
    threshold_db: f32,
    release_ms: f32,
    /// Sample rate the current engine was built for. A new incoming
    /// chunk at a different rate triggers a reconfigure.
    sample_rate: u32,
    engine: BrickwallLimiter,
}

pub struct LimiterNode {
    state: Arc<Mutex<LimiterState>>,
}

impl LimiterNode {
    pub fn new(threshold_db: f32, release_ms: f32, sample_rate: u32) -> Self {
        let engine = BrickwallLimiter::new(threshold_db, release_ms, sample_rate as f32);
        Self {
            state: Arc::new(Mutex::new(LimiterState {
                threshold_db,
                release_ms,
                sample_rate,
                engine,
            })),
        }
    }
}

impl SyncStreamingNode for LimiterNode {
    fn node_type(&self) -> &str {
        "LimiterNode"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let (mut samples, sr, ch, sid, meta) = util::take_audio(data)?;

        // Skip if not enough channels for stereo processing. Mono gets
        // treated as "limit against itself" (l == r).
        if ch == 0 || samples.is_empty() {
            return Ok(util::emit_audio(samples, sr, ch, sid, meta));
        }

        let mut st = self.state.lock();

        // Rebuild if the sample rate changed from what the engine was
        // built for. Not RT-safe, but a sample-rate change is a session
        // boundary, not a per-cycle event.
        if st.sample_rate != sr {
            let (threshold_db, release_ms) = (st.threshold_db, st.release_ms);
            st.engine.reconfigure(threshold_db, release_ms, sr as f32);
            st.sample_rate = sr;
        }

        let channels = ch as usize;
        let frames = samples.len() / channels;

        match channels {
            1 => {
                // Mono: feed the same sample as L and R, use the same
                // gain reduction for both (limiter internally tracks
                // state based on peak of the pair).
                for f in 0..frames {
                    let mut s = samples[f];
                    let mut copy = s;
                    st.engine.process_stereo(&mut s, &mut copy);
                    samples[f] = s;
                }
            }
            _ => {
                // ≥2 channels: apply to front L/R only (channels 0 and 1).
                // Other channels pass through. This matches WDRC/CROS
                // conventions in this crate and is the right choice for
                // a post-HRTF stereo-out pipeline where only channels
                // 0/1 carry audio in the first place.
                for f in 0..frames {
                    let base = f * channels;
                    // Split borrow: take two disjoint mutable refs out
                    // of the slice.
                    let (l_slice, rest) = samples[base..base + 2].split_at_mut(1);
                    let l = &mut l_slice[0];
                    let r = &mut rest[0];
                    st.engine.process_stereo(l, r);
                }
            }
        }

        Ok(util::emit_audio(samples, sr, ch, sid, meta))
    }
}

fn build_limiter(params: &Value) -> Result<LimiterNode, Error> {
    let p: Params = serde_json::from_value(params.clone())
        .map_err(|e| Error::Execution(format!("LimiterNode params: {e}")))?;
    let sr = p.sample_rate.unwrap_or(48000);
    Ok(LimiterNode::new(p.threshold_db, p.release_ms, sr))
}

pub struct LimiterNodeFactory;

impl StreamingNodeFactory for LimiterNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        Ok(Box::new(SyncNodeWrapper(build_limiter(params)?)))
    }

    fn node_type(&self) -> &str {
        "LimiterNode"
    }
}

impl remotemedia_core::executor::sync_executor::SyncStreamingNodeFactory for LimiterNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
    ) -> Result<Box<dyn SyncStreamingNode>, Error> {
        Ok(Box::new(build_limiter(params)?))
    }

    fn node_type(&self) -> &str {
        "LimiterNode"
    }
}
