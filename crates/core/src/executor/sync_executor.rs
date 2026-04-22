//! Real-time-safe, synchronous pipeline executor.
//!
//! This is a **synchronous** alternative to [`PipelineExecutor`][crate::executor::PipelineExecutor]
//! aimed at hard-real-time audio / driver-host embeddings where the
//! tokio-based async executor cannot run: Core Audio HAL IO callbacks,
//! JACK process threads, AudioUnit v3 render blocks, etc.
//!
//! # Design goals
//!
//! 1. **No tokio, no `.await`.** The hot path is a single function call
//!    chain on the calling thread.
//! 2. **No heap allocations in steady state.** Nodes hold their own
//!    scratch buffers; the executor only owns one `HashMap<String,
//!    RuntimeData>` for inter-node routing, which is cleared (not
//!    re-allocated) on every call.
//! 3. **Manifest-driven, not hand-chained.** Use the same
//!    [`Manifest`][crate::manifest::Manifest] + [`PipelineGraph`] types
//!    the async executor uses so pipelines are interchangeable.
//! 4. **Itself a [`SyncStreamingNode`].** A `SyncPipelineExecutor` is a
//!    node: drop one into [`remotemedia_rt_bridge::RtBridge::spawn`] or
//!    nest one inside another sync pipeline without special casing.
//!
//! # Supported pipelines
//!
//! The sync executor supports **linear DAGs with a single source and a
//! single sink** where every edge carries [`RuntimeData`]. This is the
//! natural shape for a DSP chain (source → filter → filter → sink).
//! Pipelines with fan-out, fan-in, or multiple sources/sinks return an
//! error at construction time — use the async executor for those.
//!
//! # Registration
//!
//! Nodes register with a [`SyncStreamingNodeRegistry`] by implementing
//! [`SyncStreamingNodeFactory`]. A factory type can implement both this
//! trait and the async [`StreamingNodeFactory`][crate::nodes::streaming_node::StreamingNodeFactory]
//! so the same node is available in both executors.
//!
//! # Example
//!
//! ```ignore
//! use remotemedia_core::executor::sync_executor::{
//!     SyncPipelineExecutor, SyncStreamingNodeRegistry,
//! };
//! use remotemedia_core::manifest::Manifest;
//!
//! // Build a manifest (usually parsed from YAML/JSON):
//! let yaml = r#"
//!   nodes:
//!     - id: wdrc
//!       node_type: WdrcNode
//!       params: { audiogram: { left: [...], right: [...] } }
//!     - id: cros
//!       node_type: CrosNode
//!       params: { mode: "Off", level_db: -6.0, head_shadow_hz: 4000.0 }
//!   connections:
//!     - { from: wdrc, to: cros }
//! "#;
//! let manifest: Manifest = serde_yaml::from_str(yaml)?;
//!
//! // Register sync-capable factories:
//! let mut registry = SyncStreamingNodeRegistry::new();
//! // registry.register(Arc::new(WdrcNodeFactory));  // SyncStreamingNodeFactory
//! // registry.register(Arc::new(CrosNodeFactory));
//!
//! // Build the pipeline. Nodes are instantiated up-front so the hot
//! // path is allocation-free.
//! let pipeline = SyncPipelineExecutor::from_manifest(&manifest, &registry)?;
//!
//! // pipeline is itself a SyncStreamingNode — use it directly or
//! // hand it to an RtBridge worker thread.
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::executor::PipelineGraph;
use crate::manifest::Manifest;
use crate::nodes::streaming_node::SyncStreamingNode;

// ── Factory & registry ──────────────────────────────────────────────────────

/// Factory trait for synchronous, RT-safe streaming nodes.
///
/// This mirrors [`StreamingNodeFactory`][crate::nodes::streaming_node::StreamingNodeFactory]
/// but returns a boxed [`SyncStreamingNode`] directly (no async wrapper).
/// A single factory type may implement both traits; the SyncPipelineExecutor
/// uses only this one.
pub trait SyncStreamingNodeFactory: Send + Sync {
    /// Instantiate a node from manifest-provided parameters.
    fn create(
        &self,
        node_id: String,
        params: &Value,
    ) -> Result<Box<dyn SyncStreamingNode>>;

    /// The `node_type` string this factory claims, matching the manifest.
    fn node_type(&self) -> &str;
}

/// Registry of [`SyncStreamingNodeFactory`] implementations keyed by
/// `node_type`.
#[derive(Default)]
pub struct SyncStreamingNodeRegistry {
    factories: HashMap<String, Arc<dyn SyncStreamingNodeFactory>>,
}

impl SyncStreamingNodeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a factory. Replaces any existing entry for the same
    /// `node_type`.
    pub fn register(&mut self, factory: Arc<dyn SyncStreamingNodeFactory>) {
        self.factories.insert(factory.node_type().to_string(), factory);
    }

    /// Look up a factory by node type. Returns `None` if unregistered.
    pub fn get(&self, node_type: &str) -> Option<&Arc<dyn SyncStreamingNodeFactory>> {
        self.factories.get(node_type)
    }

    /// Registered node type names (unsorted).
    pub fn node_types(&self) -> impl Iterator<Item = &str> {
        self.factories.keys().map(String::as_str)
    }
}

// ── Executor ────────────────────────────────────────────────────────────────

/// A compiled pipeline: instantiated nodes + a linear execution order.
///
/// Construction is the only non-RT-safe part — it allocates the node
/// boxes and the routing map. `process` itself is allocation-free
/// in steady state.
///
/// `SyncPipelineExecutor` implements [`SyncStreamingNode`], so you can
/// hand it directly to `RtBridge::spawn` or nest it inside another
/// `SyncPipelineExecutor` (treating a whole pipeline as one node).
pub struct SyncPipelineExecutor {
    /// Nodes in topological execution order. The vector index IS the
    /// execution step — we don't look up by id on the hot path.
    order: Vec<StepNode>,

    /// Display name for `SyncStreamingNode::node_type`.
    name: String,
}

impl std::fmt::Debug for SyncPipelineExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncPipelineExecutor")
            .field("name", &self.name)
            .field("node_count", &self.order.len())
            .finish()
    }
}

struct StepNode {
    #[allow(dead_code)]
    id: String,
    node: Box<dyn SyncStreamingNode>,
}

impl SyncPipelineExecutor {
    /// Build a pipeline from a manifest using the provided registry.
    ///
    /// The manifest is validated at build time: every `node_type` must
    /// have a registered factory; the graph must be a single linear
    /// chain (one source, one sink, each intermediate node has exactly
    /// one input and one output).
    pub fn from_manifest(
        manifest: &Manifest,
        registry: &SyncStreamingNodeRegistry,
    ) -> Result<Self> {
        let graph = PipelineGraph::from_manifest(manifest)?;
        Self::from_graph(&graph, registry, None)
    }

    /// Build a pipeline from a manifest, overriding the display name.
    pub fn from_manifest_named(
        manifest: &Manifest,
        registry: &SyncStreamingNodeRegistry,
        name: impl Into<String>,
    ) -> Result<Self> {
        let graph = PipelineGraph::from_manifest(manifest)?;
        Self::from_graph(&graph, registry, Some(name.into()))
    }

    /// Shared builder used by both `from_manifest` constructors.
    fn from_graph(
        graph: &PipelineGraph,
        registry: &SyncStreamingNodeRegistry,
        name: Option<String>,
    ) -> Result<Self> {
        // Validate: linear chain only (one source, one sink, each interior
        // node has exactly one input and one output). This keeps the
        // executor's data plane to a single `RuntimeData` value flowing
        // between steps — no per-frame `HashMap` lookups required.
        if graph.sources.len() != 1 {
            return Err(Error::Manifest(format!(
                "SyncPipelineExecutor requires exactly 1 source, got {} ({:?})",
                graph.sources.len(),
                graph.sources
            )));
        }
        if graph.sinks.len() != 1 {
            return Err(Error::Manifest(format!(
                "SyncPipelineExecutor requires exactly 1 sink, got {} ({:?})",
                graph.sinks.len(),
                graph.sinks
            )));
        }
        for id in &graph.execution_order {
            let node = graph.get_node(id).expect("execution_order node exists");
            if !node.inputs.is_empty() && node.inputs.len() != 1 {
                return Err(Error::Manifest(format!(
                    "SyncPipelineExecutor requires each non-source node to have exactly 1 input; \
                     node '{}' has {}",
                    id,
                    node.inputs.len()
                )));
            }
            if !node.outputs.is_empty() && node.outputs.len() != 1 {
                return Err(Error::Manifest(format!(
                    "SyncPipelineExecutor requires each non-sink node to have exactly 1 output; \
                     node '{}' has {}",
                    id,
                    node.outputs.len()
                )));
            }
        }

        // Instantiate nodes in topological order. This is the only
        // allocation-heavy phase; the hot path reuses these boxes.
        let mut order: Vec<StepNode> = Vec::with_capacity(graph.execution_order.len());
        for id in &graph.execution_order {
            let gn = graph.get_node(id).expect("topo-order node exists");
            let factory = registry.get(&gn.node_type).ok_or_else(|| {
                Error::Manifest(format!(
                    "SyncPipelineExecutor: no factory registered for node_type '{}'",
                    gn.node_type
                ))
            })?;
            let node = factory.create(gn.id.clone(), &gn.params)?;
            order.push(StepNode { id: gn.id.clone(), node });
        }

        let name = name.unwrap_or_else(|| "SyncPipelineExecutor".to_string());
        Ok(Self { order, name })
    }

    /// Number of nodes in the pipeline.
    pub fn node_count(&self) -> usize {
        self.order.len()
    }
}

impl SyncStreamingNode for SyncPipelineExecutor {
    fn node_type(&self) -> &str {
        &self.name
    }

    /// Drive `data` through each node in topological order.
    ///
    /// The hot path does no heap allocation and no map lookups — we
    /// walk `self.order` in index order, passing the `RuntimeData`
    /// through `process_sync` on each node. Each inner node is free
    /// to mutate the audio buffer in place (the `Vec<f32>` behind
    /// [`AudioSamples::Vec`][crate::data::AudioSamples::Vec] is moved
    /// across the call, not cloned).
    fn process(&self, mut data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        for step in &self.order {
            data = step.node.process(data)?;
        }
        Ok(data)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{AudioSamples, RuntimeData};
    use crate::manifest::{Connection, Manifest, NodeManifest};
    use serde_json::json;

    /// Trivial test node that multiplies all samples by a constant gain.
    struct GainNode(f32);

    impl SyncStreamingNode for GainNode {
        fn node_type(&self) -> &str {
            "Gain"
        }
        fn process(&self, data: RuntimeData) -> Result<RuntimeData> {
            if let RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                stream_id,
                metadata,
                ..
            } = data
            {
                let mut v = samples.into_vec();
                for s in &mut v {
                    *s *= self.0;
                }
                Ok(RuntimeData::Audio {
                    samples: AudioSamples::Vec(v),
                    sample_rate,
                    channels,
                    stream_id,
                    timestamp_us: None,
                    arrival_ts_us: None,
                    metadata,
                })
            } else {
                Err(Error::Execution("GainNode: expected Audio".into()))
            }
        }
    }

    struct GainFactory;
    impl SyncStreamingNodeFactory for GainFactory {
        fn create(
            &self,
            _node_id: String,
            params: &Value,
        ) -> Result<Box<dyn SyncStreamingNode>> {
            let g = params.get("gain").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
            Ok(Box::new(GainNode(g)))
        }
        fn node_type(&self) -> &str {
            "Gain"
        }
    }

    fn build_chain(gains: &[f32]) -> Manifest {
        let nodes = gains
            .iter()
            .enumerate()
            .map(|(i, g)| NodeManifest {
                id: format!("g{i}"),
                node_type: "Gain".into(),
                params: json!({ "gain": g }),
                ..Default::default()
            })
            .collect();
        let connections = (0..gains.len() - 1)
            .map(|i| Connection {
                from: format!("g{i}"),
                to: format!("g{}", i + 1),
            })
            .collect();
        Manifest {
            version: "1".into(),
            nodes,
            connections,
            metadata: Default::default(),
            python_env: None,
        }
    }

    fn audio(samples: Vec<f32>) -> RuntimeData {
        RuntimeData::Audio {
            samples: AudioSamples::Vec(samples),
            sample_rate: 48_000,
            channels: 1,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata: None,
        }
    }

    #[test]
    fn linear_chain_applies_each_node_in_order() {
        let manifest = build_chain(&[2.0, 3.0]);
        let mut reg = SyncStreamingNodeRegistry::new();
        reg.register(Arc::new(GainFactory));
        let pipe = SyncPipelineExecutor::from_manifest(&manifest, &reg).unwrap();
        assert_eq!(pipe.node_count(), 2);

        let out = pipe.process(audio(vec![1.0, 2.0, 3.0])).unwrap();
        if let RuntimeData::Audio { samples, .. } = out {
            let v = samples.into_vec();
            // 2.0 * 3.0 = 6.0 gain
            assert_eq!(v, vec![6.0, 12.0, 18.0]);
        } else {
            panic!("expected Audio");
        }
    }

    #[test]
    fn unregistered_node_type_fails_at_build() {
        let nodes = vec![NodeManifest {
            id: "x".into(),
            node_type: "Unknown".into(),
            params: json!({}),
            ..Default::default()
        }];
        let manifest = Manifest {
            version: "1".into(),
            nodes,
            connections: vec![],
            metadata: Default::default(),
            python_env: None,
        };
        let reg = SyncStreamingNodeRegistry::new();
        let Err(err) = SyncPipelineExecutor::from_manifest(&manifest, &reg) else {
            panic!("unregistered type should fail");
        };
        assert!(format!("{err}").contains("Unknown"));
    }

    #[test]
    fn non_linear_graph_rejected() {
        // Fan-out: g0 → g1 and g0 → g2 — multiple outputs from g0.
        let nodes = ["g0", "g1", "g2"]
            .iter()
            .map(|id| NodeManifest {
                id: id.to_string(),
                node_type: "Gain".into(),
                params: json!({ "gain": 1.0 }),
                ..Default::default()
            })
            .collect();
        let connections = vec![
            Connection {
                from: "g0".into(),
                to: "g1".into(),
            },
            Connection {
                from: "g0".into(),
                to: "g2".into(),
            },
        ];
        let manifest = Manifest {
            version: "1".into(),
            nodes,
            connections,
            metadata: Default::default(),
            python_env: None,
        };
        let mut reg = SyncStreamingNodeRegistry::new();
        reg.register(Arc::new(GainFactory));
        let Err(err) = SyncPipelineExecutor::from_manifest(&manifest, &reg) else {
            panic!("fan-out should be rejected");
        };
        // Either "exactly 1 sink" (g1 and g2 both sinks) or
        // "exactly 1 output" (g0 has 2 outputs) — both are valid linear-chain
        // violations that must be rejected.
        let msg = format!("{err}");
        assert!(
            msg.contains("exactly 1 sink") || msg.contains("exactly 1 output"),
            "unexpected rejection message: {msg}"
        );
    }
}
