//! WDRC streaming node.
//!
//! # Real-time safety
//!
//! This node is **RT-safe** under the following constraints:
//!
//! - Called via [`remotemedia_rt_bridge::RtBridge`] (single-consumer), so
//!   the `parking_lot::Mutex` is uncontended and its fast path is a
//!   single CAS.
//! - Input arrives as `AudioSamples::Vec` or `AudioSamples::Pooled`. An
//!   `AudioSamples::Arc` would force a one-time copy (see [`crate::util::take_audio`]).
//! - The `sample_rate` is configured once at node creation (or on the
//!   first call). A *change* in sample rate between calls will trigger
//!   a filterbank + engine rebuild — **not RT-safe**, but expected to
//!   happen only at session boundaries.
//! - The DSP core (`dsp_core::filterbank::Filterbank`,
//!   `wdrc::WdrcEngine`) must be RT-safe per `process_sample` — audit
//!   those crates if you are deploying to a HAL callback.

use std::sync::Arc;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;

use audiogram::{Audiogram, EarAudiogram};
use dsp_core::filterbank::{Filterbank, NUM_BANDS};
use wdrc::{fitting::fit_audiogram, WdrcEngine};

use remotemedia_core::data::RuntimeData;
use remotemedia_core::Error;
use remotemedia_core::nodes::streaming_node::{
    StreamingNode, StreamingNodeFactory, SyncNodeWrapper, SyncStreamingNode,
};

use crate::util;

/// JSON shape accepted by the factory.
///
/// ```json
/// {
///   "audiogram": {
///     "left":  [10, 15, 25, 40, 50, 55, 60, 65],
///     "right": [10, 15, 25, 40, 50, 55, 60, 65]
///   },
///   "sample_rate": 48000
/// }
/// ```
/// `sample_rate` is optional — if omitted, the first incoming audio chunk's
/// rate is used to build the engine lazily.
#[derive(Debug, Deserialize)]
struct Params {
    audiogram: AudiogramParams,
    #[serde(default)]
    sample_rate: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AudiogramParams {
    left: [f32; 8],
    right: [f32; 8],
}

impl From<AudiogramParams> for Audiogram {
    fn from(p: AudiogramParams) -> Self {
        Audiogram {
            left: EarAudiogram { thresholds: p.left, ucl: None },
            right: EarAudiogram { thresholds: p.right, ucl: None },
            name: "wdrc-node".into(),
            date: String::new(),
        }
    }
}

/// Stateful DSP held behind an Arc<Mutex<_>> so the node can mutate state
/// across `&self` calls from the runtime.
struct WdrcState {
    audiogram: Audiogram,
    sample_rate: Option<u32>,
    filterbank: Option<Filterbank>,
    engine: Option<WdrcEngine>,
}

impl WdrcState {
    fn ensure(&mut self, sample_rate: u32) {
        if self.sample_rate == Some(sample_rate) && self.engine.is_some() {
            return;
        }
        let params = fit_audiogram(&self.audiogram);
        self.filterbank = Some(Filterbank::new(sample_rate as f32));
        self.engine = Some(WdrcEngine::new(&params, sample_rate as f32));
        self.sample_rate = Some(sample_rate);
    }
}

/// WDRC node.
///
/// Takes stereo (channels == 2) or mono (channels == 1) f32 PCM and applies
/// per-ear, per-band wide-dynamic-range compression fitted from an audiogram.
///
/// For mono input, the same compression is applied using the left ear's
/// parameters. For multichannel input with channels > 2, only the first two
/// channels are processed; remaining channels pass through untouched.
pub struct WdrcNode {
    state: Arc<Mutex<WdrcState>>,
}

impl WdrcNode {
    pub fn new(audiogram: Audiogram, sample_rate: Option<u32>) -> Self {
        Self {
            state: Arc::new(Mutex::new(WdrcState {
                audiogram,
                sample_rate: None,
                filterbank: None,
                engine: None,
            })),
        }
        .tap(|n| {
            if let Some(sr) = sample_rate {
                // Construction-time warm-up — never on RT path.
                n.state.lock().ensure(sr);
            }
        })
    }
}

trait Tap: Sized {
    fn tap(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
}
impl<T> Tap for T {}

impl SyncStreamingNode for WdrcNode {
    fn node_type(&self) -> &str {
        "WdrcNode"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // RT-safe path: `take_audio` is a move for Vec/Pooled variants;
        // `emit_audio` is a move back. The Vec is mutated in place;
        // no allocations per call.
        let (mut samples, sr, ch, sid, meta) = util::take_audio(data)?;
        if ch == 0 || samples.is_empty() {
            return Ok(util::emit_audio(samples, sr, ch, sid, meta));
        }
        // Uncontended parking_lot::Mutex fast path (single CAS) under
        // the single-consumer rt-bridge invariant.
        let mut st = self.state.lock();
        // `ensure` is a no-op after the first call at a given sample
        // rate; only a sample-rate change re-builds state (non-RT).
        st.ensure(sr);
        // Split-borrow: get independent mut refs to the two fields via the struct.
        let WdrcState { filterbank, engine, .. } = &mut *st;
        let fb = filterbank.as_mut().unwrap();
        let engine = engine.as_mut().unwrap();

        let channels = ch as usize;
        let frames = samples.len() / channels;

        // Process first channel (and second if stereo) through WDRC.
        // Other channels pass through untouched.
        for f in 0..frames {
            // Left ear = channel 0
            let x = samples[f * channels];
            let bands = fb.process_sample(0, x);
            let mut y = 0.0f32;
            for b in 0..NUM_BANDS {
                y += engine.process_sample(0, b, bands[b]);
            }
            samples[f * channels] = y;

            if channels >= 2 {
                let x = samples[f * channels + 1];
                // Use channel 1 of the filterbank for right-ear analysis.
                // Filterbank::process_sample is per-channel stateful, so we
                // route right-ear through a distinct channel index to keep
                // state separate.
                let bands = fb.process_sample(1, x);
                let mut y = 0.0f32;
                for b in 0..NUM_BANDS {
                    y += engine.process_sample(1, b, bands[b]);
                }
                samples[f * channels + 1] = y;
            }
        }

        Ok(util::emit_audio(samples, sr, ch, sid, meta))
    }
}

fn build_wdrc(params: &Value) -> Result<WdrcNode, Error> {
    let p: Params = serde_json::from_value(params.clone())
        .map_err(|e| Error::Execution(format!("WdrcNode params: {e}")))?;
    let ag: Audiogram = p.audiogram.into();
    Ok(WdrcNode::new(ag, p.sample_rate))
}

/// Factory for `WdrcNode`. Registers as node type `"WdrcNode"`.
///
/// Implements both the async [`StreamingNodeFactory`] and the RT-safe
/// [`remotemedia_core::executor::sync_executor::SyncStreamingNodeFactory`],
/// so a single registration works in both executors.
pub struct WdrcNodeFactory;

impl StreamingNodeFactory for WdrcNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        Ok(Box::new(SyncNodeWrapper(build_wdrc(params)?)))
    }

    fn node_type(&self) -> &str {
        "WdrcNode"
    }
}

impl remotemedia_core::executor::sync_executor::SyncStreamingNodeFactory for WdrcNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
    ) -> Result<Box<dyn SyncStreamingNode>, Error> {
        Ok(Box::new(build_wdrc(params)?))
    }

    fn node_type(&self) -> &str {
        "WdrcNode"
    }
}
