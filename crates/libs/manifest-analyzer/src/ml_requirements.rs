//! ML requirements detection — identifies model/GPU/Python dependencies per node

use remotemedia_core::manifest::NodeManifest;
use serde::{Deserialize, Serialize};

/// An ML requirement for a specific node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlRequirement {
    /// Node ID from manifest
    pub node_id: String,
    /// Node type string
    pub node_type: String,
    /// Model name if known
    pub model_name: Option<String>,
    /// Whether this node requires GPU acceleration
    pub requires_gpu: bool,
    /// Whether this node requires a Python runtime
    pub requires_python: bool,
}

/// Known ML node types and their requirements
struct MlNodeSpec {
    node_type: &'static str,
    model_name: Option<&'static str>,
    requires_gpu: bool,
    requires_python: bool,
}

static ML_NODE_SPECS: &[MlNodeSpec] = &[
    MlNodeSpec {
        node_type: "SileroVADNode",
        model_name: Some("silero-vad"),
        requires_gpu: false,
        requires_python: false,
    },
    MlNodeSpec {
        node_type: "RustVADNode",
        model_name: Some("silero-vad"),
        requires_gpu: false,
        requires_python: false,
    },
    MlNodeSpec {
        node_type: "WhisperXNode",
        model_name: Some("whisperx"),
        requires_gpu: true,
        requires_python: true,
    },
    MlNodeSpec {
        node_type: "HFWhisperNode",
        model_name: Some("whisper"),
        requires_gpu: true,
        requires_python: true,
    },
    MlNodeSpec {
        node_type: "WhisperNode",
        model_name: Some("whisper"),
        requires_gpu: true,
        requires_python: false, // Candle-based
    },
    MlNodeSpec {
        node_type: "candle-whisper",
        model_name: Some("whisper"),
        requires_gpu: false, // Can run on CPU
        requires_python: false, // Native Rust via Candle
    },
    MlNodeSpec {
        node_type: "KokoroTTSNode",
        model_name: Some("kokoro-tts"),
        requires_gpu: false,
        requires_python: true,
    },
    MlNodeSpec {
        node_type: "VibeVoiceTTSNode",
        model_name: Some("vibe-voice"),
        requires_gpu: true,
        requires_python: true,
    },
    MlNodeSpec {
        node_type: "LFM2AudioNode",
        model_name: Some("lfm2-audio"),
        requires_gpu: true,
        requires_python: true,
    },
    MlNodeSpec {
        node_type: "OllamaLLMNode",
        model_name: Some("ollama"),
        requires_gpu: false,
        requires_python: false,
    },
    MlNodeSpec {
        node_type: "YOLONode",
        model_name: Some("yolo"),
        requires_gpu: true,
        requires_python: false, // Candle-based
    },
];

/// Detect ML requirements from manifest nodes
pub fn detect(nodes: &[NodeManifest]) -> Vec<MlRequirement> {
    let mut requirements = Vec::new();

    for node in nodes {
        if let Some(spec) = ML_NODE_SPECS
            .iter()
            .find(|s| s.node_type == node.node_type)
        {
            requirements.push(MlRequirement {
                node_id: node.id.clone(),
                node_type: node.node_type.clone(),
                model_name: spec.model_name.map(String::from),
                requires_gpu: spec.requires_gpu,
                requires_python: spec.requires_python,
            });
        } else {
            // Heuristic: check if runtime_hint suggests Python
            let requires_python = node
                .runtime_hint
                .as_ref()
                .map(|h| {
                    matches!(
                        h,
                        remotemedia_core::manifest::RuntimeHint::Cpython
                            | remotemedia_core::manifest::RuntimeHint::CpythonWasm
                    )
                })
                .unwrap_or(false);

            if requires_python {
                requirements.push(MlRequirement {
                    node_id: node.id.clone(),
                    node_type: node.node_type.clone(),
                    model_name: None,
                    requires_gpu: false,
                    requires_python: true,
                });
            }
        }
    }

    requirements
}
