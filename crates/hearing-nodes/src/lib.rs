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
//!
//! # Real-time safety
//!
//! All three nodes are designed to run inside a real-time audio callback
//! via [`remotemedia_rt_bridge::RtBridge`]. The RT contract:
//!
//! - **Single-consumer invariant.** Each node state is guarded by a
//!   `parking_lot::Mutex` whose fast path is a single CAS when the lock
//!   is uncontended — which is the case under `rt-bridge` (one worker
//!   thread, sole caller).
//! - **Move-in / move-out the input `Vec<f32>`.** Construction and
//!   destructuring of `RuntimeData::Audio` are alloc-free moves when the
//!   input is `AudioSamples::Vec` or `AudioSamples::Pooled`. An
//!   `AudioSamples::Arc` input forces a one-time copy at `take_audio`
//!   (see module [`util`]); keep Arc off the HAL hot path.
//! - **No per-call heap.** WDRC and CROS mutate the input Vec in place.
//!   HRTF reuses the input allocation as output (7.1 in → 2ch out, so
//!   `len` shrinks but capacity stays); scratch is pre-sized at node
//!   creation via `max_frames`.
//! - **Stable sample rate across calls.** A sample-rate change between
//!   calls rebuilds the filterbank / engine / convolver-config, which
//!   allocates. Set the sample rate at session start (or in the node
//!   params) and keep it fixed.
//! - **Upstream DSP crates must also be RT-safe per-sample / per-frame.**
//!   The `wdrc`, `cros`, `hrtf`, and `dsp-core::filterbank` crates are
//!   sibling crates in the hearing-aid workspace; audit their
//!   `process_sample` / `process_frame` / `process` methods before a
//!   production HAL deployment.

use std::sync::Arc;

use remotemedia_core::nodes::provider::NodeProvider;
use remotemedia_core::nodes::streaming_node::StreamingNodeRegistry;

pub mod wdrc_node;
pub mod cros_node;
pub mod hrtf_node;
pub mod limiter_node;

pub use cros_node::{CrosNode, CrosNodeFactory};
pub use hrtf_node::{HrtfNode, HrtfNodeFactory};
pub use limiter_node::{LimiterNode, LimiterNodeFactory};
pub use wdrc_node::{WdrcNode, WdrcNodeFactory};

/// Register all hearing-aid factories into the RT-safe
/// [`SyncStreamingNodeRegistry`][remotemedia_core::executor::sync_executor::SyncStreamingNodeRegistry]
/// used by [`SyncPipelineExecutor`][remotemedia_core::executor::sync_executor::SyncPipelineExecutor].
///
/// This is the sync counterpart of [`HearingNodesProvider`]; the factory
/// structs (`WdrcNodeFactory`, `CrosNodeFactory`, `HrtfNodeFactory`)
/// implement both the async `StreamingNodeFactory` and the sync
/// `SyncStreamingNodeFactory`, so the same registration is available in
/// both executor variants.
pub fn register_sync_hearing_nodes(
    registry: &mut remotemedia_core::executor::sync_executor::SyncStreamingNodeRegistry,
) {
    registry.register(Arc::new(WdrcNodeFactory));
    registry.register(Arc::new(CrosNodeFactory));
    registry.register(Arc::new(HrtfNodeFactory));
    registry.register(Arc::new(LimiterNodeFactory));
}

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
        registry.register(Arc::new(LimiterNodeFactory));
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

    /// Destructure a `RuntimeData::Audio` into its owned `Vec<f32>` and
    /// sidecar metadata.
    ///
    /// # RT-safety cost table
    ///
    /// | input variant                  | heap op |
    /// |--------------------------------|---------|
    /// | `AudioSamples::Vec(v)`         | none — moves the existing `Vec<f32>` |
    /// | `AudioSamples::Pooled(buf)`    | none — detaches the pool buffer; pool won't recycle it |
    /// | `AudioSamples::Arc(a)`         | **one copy** — `Arc<[f32]>` is shared, must be materialized |
    ///
    /// Only the `Arc` variant allocates. Keep Arc-typed audio off the
    /// HAL hot path; an `rt-bridge` producer should hand in `Vec` or
    /// `Pooled` variants.
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
            } => Ok((samples.into_vec(), sample_rate, channels, stream_id, metadata)),
            other => Err(Error::Execution(format!(
                "expected RuntimeData::Audio, got {other:?}"
            ))),
        }
    }

    /// Wrap a processed `Vec<f32>` back into a `RuntimeData::Audio`.
    /// Zero-copy: `Vec` → `AudioSamples::Vec` is a pointer/len/cap move.
    pub(crate) fn emit_audio(
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u32,
        stream_id: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> RuntimeData {
        RuntimeData::Audio {
            samples: samples.into(),
            sample_rate,
            channels,
            stream_id,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata,
        }
    }
}
