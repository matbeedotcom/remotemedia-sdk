//! HRTF streaming node — 7.1 → stereo binaural spatialization.

use std::path::Path;
use std::sync::{Arc, Mutex};

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
/// { "ir_path": "/path/to/hrtf_irs.bin" }
/// ```
#[derive(Debug, Deserialize)]
struct Params {
    ir_path: String,
}

struct HrtfState {
    convolver: HrtfConvolver,
    scratch_out: Vec<f32>,
}

pub struct HrtfNode {
    state: Arc<Mutex<HrtfState>>,
}

impl HrtfNode {
    pub fn new(convolver: HrtfConvolver) -> Self {
        Self {
            state: Arc::new(Mutex::new(HrtfState {
                convolver,
                scratch_out: Vec::new(),
            })),
        }
    }
}

impl SyncStreamingNode for HrtfNode {
    fn node_type(&self) -> &str { "HrtfNode" }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let (samples, sr, ch, sid, meta) = util::take_audio(data)?;
        if ch != 8 {
            // Pass through unchanged for non-7.1 inputs.
            return Ok(util::emit_audio(samples, sr, ch, sid, meta));
        }
        let frames = samples.len() / 8;
        let mut st = self.state.lock().unwrap();
        if st.scratch_out.len() < frames * 2 {
            st.scratch_out.resize(frames * 2, 0.0);
        }
        let HrtfState { convolver, scratch_out } = &mut *st;
        convolver.process(&samples, &mut scratch_out[..frames * 2], frames);
        let out = scratch_out[..frames * 2].to_vec();
        Ok(util::emit_audio(out, sr, 2, sid, meta))
    }
}

pub struct HrtfNodeFactory;

impl StreamingNodeFactory for HrtfNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let p: Params = serde_json::from_value(params.clone())
            .map_err(|e| Error::Execution(format!("HrtfNode params: {e}")))?;
        let ir_set = SpeakerIrSet::load(Path::new(&p.ir_path))
            .map_err(|e| Error::Execution(format!("HrtfNode load {}: {e}", p.ir_path)))?;
        let ir_length = ir_set.ir_length;
        let convolver = HrtfConvolver::new(ir_set.irs.into_boxed_slice(), ir_length);
        Ok(Box::new(SyncNodeWrapper(HrtfNode::new(convolver))))
    }

    fn node_type(&self) -> &str { "HrtfNode" }
}
