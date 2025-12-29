//! Pipeline manifest parsing and validation
//!
//! This module handles JSON manifest parsing, schema validation,
//! and conversion to internal pipeline representations.
//!
//! Schema specification: ../schemas/manifest.v1.json

use crate::capabilities::MediaCapabilities;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// Pipeline manifest structure (v1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Schema version
    pub version: String,

    /// Pipeline metadata
    pub metadata: ManifestMetadata,

    /// List of nodes in the pipeline
    pub nodes: Vec<NodeManifest>,

    /// Connections between nodes
    pub connections: Vec<Connection>,
}

/// Pipeline metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestMetadata {
    /// Pipeline name
    #[serde(default)]
    pub name: String,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    /// Enable automatic capability negotiation (spec 022, FR-014).
    ///
    /// When true, the system automatically inserts conversion nodes
    /// to resolve capability mismatches. When false, mismatches result
    /// in validation errors.
    #[serde(default)]
    pub auto_negotiate: bool,
}

/// Node manifest entry
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeManifest {
    /// Unique node ID within pipeline
    pub id: String,

    /// Node type (e.g., "AudioSource", "HFPipelineNode")
    pub node_type: String,

    /// Node-specific parameters
    #[serde(default)]
    pub params: serde_json::Value,

    /// Whether this is a streaming node (async generator process method)
    #[serde(default)]
    pub is_streaming: bool,

    /// Whether this node should stream outputs to the client (spec 021, User Story 3)
    ///
    /// By default, only terminal nodes (sinks - nodes with no outputs) send data to
    /// the client. Setting `is_output_node: true` allows intermediate nodes to also
    /// stream their outputs to the client alongside terminal nodes.
    ///
    /// Use cases:
    /// - Debugging: see intermediate processing results
    /// - Monitoring: track VAD results while also getting final transcription
    /// - Branching: receive outputs from multiple stages of the pipeline
    #[serde(default)]
    pub is_output_node: bool,

    /// Optional capability requirements (GPU, CPU, memory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<CapabilityRequirements>,

    /// Media format capabilities for input/output constraints (spec 022).
    ///
    /// Declares what media formats this node accepts as input and produces
    /// as output. Used for capability negotiation and pipeline validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_capabilities: Option<MediaCapabilities>,

    /// Optional execution host
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Optional runtime hint (Phase 1.10.5)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_hint: Option<RuntimeHint>,

    /// Execution placement (Phase 1.3.6 - capability-aware execution)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionMetadata>,

    /// Docker configuration (integrated into multiprocess system)
    #[cfg(feature = "docker")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker: Option<crate::python::multiprocess::docker_support::DockerNodeConfig>,
}

/// Runtime hint for Python node execution (Phase 1.10.5)
///
/// Specifies which Python runtime to use for executing the node.
/// This allows fine-grained control over runtime selection on a per-node basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHint {
    /// Use RustPython embedded interpreter (pure Rust, limited stdlib)
    RustPython,

    /// Use CPython via PyO3 in-process (full Python ecosystem, C-extensions)
    Cpython,

    /// Use CPython compiled to WASM (sandboxed, Phase 3)
    CpythonWasm,

    /// Automatically select runtime based on node requirements
    Auto,
}

/// Execution metadata for capability-aware placement (Phase 1.3.6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    /// Execution placement strategy
    #[serde(default)]
    pub placement: String, // "local", "remote", "prefer_local", "prefer_remote", "auto"

    /// Reason for execution placement (e.g., "requires_native_libs")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Fallback node if this one can't execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

/// Capability requirements for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRequirements {
    /// GPU requirements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<GpuRequirement>,

    /// CPU requirements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<CpuRequirement>,

    /// Memory requirements (GB)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_gb: Option<f64>,
}

/// GPU capability requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuRequirement {
    /// GPU type (cuda, rocm, metal)
    #[serde(rename = "type")]
    pub gpu_type: String,

    /// Minimum memory (GB)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_memory_gb: Option<f64>,

    /// Whether GPU is required or optional
    #[serde(default = "default_required")]
    pub required: bool,
}

/// CPU capability requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuRequirement {
    /// Minimum number of cores
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u32>,

    /// CPU architecture preference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

/// Connection between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// Source node ID
    pub from: String,

    /// Target node ID
    pub to: String,
}

fn default_required() -> bool {
    true
}

/// Parse a JSON manifest string into a Manifest struct
pub fn parse(json: &str) -> Result<Manifest> {
    serde_json::from_str(json)
        .map_err(|e| Error::Manifest(format!("Failed to parse manifest: {}", e)))
}

/// Validate a manifest for correctness
pub fn validate(manifest: &Manifest) -> Result<()> {
    // Check version
    if manifest.version != "v1" {
        return Err(Error::Manifest(format!(
            "Unsupported manifest version: {}",
            manifest.version
        )));
    }

    // Check nodes are not empty
    if manifest.nodes.is_empty() {
        return Err(Error::Manifest(
            "Manifest must contain at least one node".to_string(),
        ));
    }

    // Validate node IDs are unique
    let mut seen_ids = std::collections::HashSet::new();
    for node in &manifest.nodes {
        if !seen_ids.insert(&node.id) {
            return Err(Error::Manifest(format!("Duplicate node ID: {}", node.id)));
        }
    }

    // Validate connections reference valid nodes
    let node_ids: std::collections::HashSet<_> = manifest.nodes.iter().map(|n| &n.id).collect();
    for conn in &manifest.connections {
        if !node_ids.contains(&conn.from) {
            return Err(Error::Manifest(format!(
                "Connection references unknown source node: {}",
                conn.from
            )));
        }
        if !node_ids.contains(&conn.to) {
            return Err(Error::Manifest(format!(
                "Connection references unknown target node: {}",
                conn.to
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_manifest() {
        let json = r#"{
            "version": "v1",
            "metadata": {
                "name": "test-pipeline"
            },
            "nodes": [
                {
                    "id": "node1",
                    "node_type": "AudioSource",
                    "params": {}
                }
            ],
            "connections": []
        }"#;

        let manifest = parse(json).unwrap();
        assert_eq!(manifest.version, "v1");
        assert_eq!(manifest.metadata.name, "test-pipeline");
        assert_eq!(manifest.nodes.len(), 1);
    }

    #[test]
    fn test_validate_empty_nodes() {
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "test".to_string(),
                ..Default::default()
            },
            nodes: vec![],
            connections: vec![],
        };

        assert!(validate(&manifest).is_err());
    }

    /// Test is_output_node field parsing from JSON (spec 021 User Story 3)
    #[test]
    fn test_parse_is_output_node_field() {
        // Test with is_output_node explicitly set to true
        let json = r#"{
            "version": "v1",
            "metadata": { "name": "test-pipeline" },
            "nodes": [
                {
                    "id": "node1",
                    "node_type": "AudioSource",
                    "params": {},
                    "is_output_node": true
                },
                {
                    "id": "node2",
                    "node_type": "AudioSink",
                    "params": {},
                    "is_output_node": false
                }
            ],
            "connections": [{"from": "node1", "to": "node2"}]
        }"#;

        let manifest = parse(json).unwrap();
        assert_eq!(manifest.nodes.len(), 2);
        assert!(manifest.nodes[0].is_output_node);  // Explicitly true
        assert!(!manifest.nodes[1].is_output_node); // Explicitly false
    }

    /// Test is_output_node defaults to false when not specified
    #[test]
    fn test_is_output_node_defaults_to_false() {
        let json = r#"{
            "version": "v1",
            "metadata": { "name": "test-pipeline" },
            "nodes": [
                {
                    "id": "node1",
                    "node_type": "AudioSource",
                    "params": {}
                }
            ],
            "connections": []
        }"#;

        let manifest = parse(json).unwrap();
        assert!(!manifest.nodes[0].is_output_node); // Defaults to false
    }
}
