//! CROS cross-feed streaming node.
//!
//! # Real-time safety
//!
//! RT-safe under the same contract as [`crate::wdrc_node::WdrcNode`]:
//! single-consumer access via `rt-bridge`, Vec/Pooled input variants,
//! and stable sample rate across calls. `CrossFeedProcessor::process_frame`
//! must be RT-safe per frame — audit the `cros` crate for deployment.

use std::sync::Arc;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;

use cros::{CrossFeedConfig, CrossFeedMode, CrossFeedProcessor};

use remotemedia_core::data::RuntimeData;
use remotemedia_core::Error;
use remotemedia_core::nodes::streaming_node::{
    StreamingNode, StreamingNodeFactory, SyncNodeWrapper, SyncStreamingNode,
};

use crate::util;

/// JSON shape:
/// ```json
/// {
///   "mode": "Off" | "RightToLeft" | "LeftToRight",
///   "level_db": -6.0,
///   "head_shadow_hz": 4000.0,
///   "cross_surround": false
/// }
/// ```
#[derive(Debug, Deserialize)]
struct Params {
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default = "default_level")]
    level_db: f32,
    #[serde(default = "default_shadow")]
    head_shadow_hz: f32,
    #[serde(default)]
    cross_surround: bool,
}

fn default_mode() -> String { "Off".into() }
fn default_level() -> f32 { -6.0 }
fn default_shadow() -> f32 { 4000.0 }

fn parse_mode(s: &str) -> CrossFeedMode {
    match s {
        "RightToLeft" => CrossFeedMode::RightToLeft,
        "LeftToRight" => CrossFeedMode::LeftToRight,
        _ => CrossFeedMode::Off,
    }
}

struct CrosState {
    cfg: CrossFeedConfig,
    proc: CrossFeedProcessor,
    sample_rate: Option<u32>,
}

impl CrosState {
    fn ensure(&mut self, sample_rate: u32) {
        if self.sample_rate != Some(sample_rate) {
            self.proc.configure(&self.cfg, sample_rate as f32);
            self.sample_rate = Some(sample_rate);
        }
    }
}

pub struct CrosNode {
    state: Arc<Mutex<CrosState>>,
}

impl CrosNode {
    pub fn new(cfg: CrossFeedConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(CrosState {
                cfg,
                proc: CrossFeedProcessor::new(),
                sample_rate: None,
            })),
        }
    }
}

impl SyncStreamingNode for CrosNode {
    fn node_type(&self) -> &str { "CrosNode" }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // RT path: owned Vec in → in-place DSP → owned Vec back out.
        // No heap operations on the steady-state path (after the first
        // call at a given sample rate).
        let (mut samples, sr, ch, sid, meta) = util::take_audio(data)?;
        if ch == 0 || samples.is_empty() {
            return Ok(util::emit_audio(samples, sr, ch, sid, meta));
        }
        let mut st = self.state.lock();
        st.ensure(sr);
        let channels = ch as usize;
        let frames = samples.len() / channels;
        for f in 0..frames {
            let start = f * channels;
            let frame = &mut samples[start..start + channels];
            st.proc.process_frame(frame, channels);
        }
        Ok(util::emit_audio(samples, sr, ch, sid, meta))
    }
}

fn build_cros(params: &Value) -> Result<CrosNode, Error> {
    let p: Params = serde_json::from_value(params.clone())
        .map_err(|e| Error::Execution(format!("CrosNode params: {e}")))?;
    let cfg = CrossFeedConfig {
        mode: parse_mode(&p.mode),
        level_db: p.level_db,
        head_shadow_hz: p.head_shadow_hz,
        cross_surround: p.cross_surround,
    };
    Ok(CrosNode::new(cfg))
}

pub struct CrosNodeFactory;

impl StreamingNodeFactory for CrosNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        Ok(Box::new(SyncNodeWrapper(build_cros(params)?)))
    }

    fn node_type(&self) -> &str { "CrosNode" }
}

impl remotemedia_core::executor::sync_executor::SyncStreamingNodeFactory for CrosNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
    ) -> Result<Box<dyn SyncStreamingNode>, Error> {
        Ok(Box::new(build_cros(params)?))
    }

    fn node_type(&self) -> &str { "CrosNode" }
}
