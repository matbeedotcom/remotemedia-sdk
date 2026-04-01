//! AudioOutput node - passthrough sink for audio data
//!
//! This is an I/O boundary node used in pipeline manifests to declare
//! that the pipeline produces audio output. The CLI captures the output
//! from this node and writes it to --output. The node passes audio
//! through unchanged.

use serde::Deserialize;

use remotemedia_core::capabilities::{
    AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue, MediaCapabilities,
    MediaConstraints,
};
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::streaming_node::{SyncStreamingNode, SyncNodeWrapper, StreamingNodeFactory, StreamingNode};
use remotemedia_core::Error;
use serde_json::Value;

/// Configuration for AudioOutput node
#[derive(Debug, Clone, Deserialize)]
pub struct AudioOutputConfig {
    /// Expected sample rate
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    /// Expected number of channels
    #[serde(default = "default_channels")]
    pub channels: u16,
    /// Audio format (informational)
    #[serde(default)]
    pub format: Option<String>,
}

impl Default for AudioOutputConfig {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            channels: default_channels(),
            format: None,
        }
    }
}

fn default_sample_rate() -> u32 {
    24000
}

fn default_channels() -> u16 {
    1
}

/// AudioOutput streaming node - passes audio through to CLI output
struct AudioOutputStreamingNode;

impl SyncStreamingNode for AudioOutputStreamingNode {
    fn node_type(&self) -> &str {
        "AudioOutput"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // Pass through - the CLI handles writing output to file/stdout
        Ok(data)
    }
}

/// Factory for creating AudioOutput nodes
pub struct AudioOutputNodeFactory;

impl StreamingNodeFactory for AudioOutputNodeFactory {
    fn create(
        &self,
        _node_id: String,
        _params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        Ok(Box::new(SyncNodeWrapper(AudioOutputStreamingNode)))
    }

    fn node_type(&self) -> &str {
        "AudioOutput"
    }

    fn media_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
        let config: AudioOutputConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        Some(MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(config.sample_rate)),
                channels: Some(ConstraintValue::Exact(config.channels as u32)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        )))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Passthrough
    }
}
