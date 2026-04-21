//! Hearing aid DSP nodes for RemoteMedia pipelines.
//!
//! Provides three SDK-compatible `StreamingNode` implementations that wrap the
//! hearing-aid Rust DSP crates:
//!
//! - `WdrcNode`  — Wide Dynamic Range Compression fitted from an audiogram
//! - `CrosNode`  — Cross-feed for single-sided deafness
//! - `HrtfNode`  — 7.1-channel → stereo binaural spatialization
//!
//! All three implement `SyncStreamingNode` and are registered via
//! `HearingNodesProvider` so they are available in any pipeline manifest by
//! their node types: `WdrcNode`, `CrosNode`, `HrtfNode`.

use std::sync::Arc;

use remotemedia_core::nodes::provider::NodeProvider;
use remotemedia_core::nodes::streaming_node::StreamingNodeRegistry;

pub mod wdrc_node;
pub mod cros_node;
pub mod hrtf_node;

pub use cros_node::{CrosNode, CrosNodeFactory};
pub use hrtf_node::{HrtfNode, HrtfNodeFactory};
pub use wdrc_node::{WdrcNode, WdrcNodeFactory};

/// Registers all hearing-aid nodes with the runtime.
///
/// Usage from an embedding binary:
/// ```no_run
/// use remotemedia_core::nodes::streaming_node::StreamingNodeRegistry;
/// use remotemedia_core::nodes::provider::NodeProvider;
/// use remotemedia_hearing_nodes::HearingNodesProvider;
///
/// let mut registry = StreamingNodeRegistry::new();
/// HearingNodesProvider.register(&mut registry);
/// ```
pub struct HearingNodesProvider;

impl NodeProvider for HearingNodesProvider {
    fn provider_name(&self) -> &'static str {
        "hearing-nodes"
    }

    fn register(&self, registry: &mut StreamingNodeRegistry) {
        registry.register(Arc::new(WdrcNodeFactory));
        registry.register(Arc::new(CrosNodeFactory));
        registry.register(Arc::new(HrtfNodeFactory));
    }

    fn priority(&self) -> i32 {
        // Domain-specific — run after core nodes (priority 1000).
        500
    }
}

/// Shared helpers used by the node impls in child modules.
pub(crate) mod util {
    use remotemedia_core::data::RuntimeData;
    use remotemedia_core::Error;

    pub(crate) fn take_audio(
        data: RuntimeData,
    ) -> Result<(Vec<f32>, u32, u32, Option<String>, Option<serde_json::Value>), Error> {
        match data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                stream_id,
                metadata,
                ..
            } => Ok((samples, sample_rate, channels, stream_id, metadata)),
            other => Err(Error::Execution(format!(
                "expected RuntimeData::Audio, got {other:?}"
            ))),
        }
    }

    pub(crate) fn emit_audio(
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u32,
        stream_id: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> RuntimeData {
        RuntimeData::Audio {
            samples,
            sample_rate,
            channels,
            stream_id,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata,
        }
    }
}
