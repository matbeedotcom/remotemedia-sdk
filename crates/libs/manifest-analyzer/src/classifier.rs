//! Pipeline classification based on node types
//!
//! Uses a static lookup table mapping `node_type` strings to expected
//! input/output RuntimeDataType values. Classifies pipelines into
//! categories like VoiceAssistant, SpeechToText, etc.

use remotemedia_core::executor::PipelineGraph;
use remotemedia_core::nodes::schema::RuntimeDataType;
use serde::{Deserialize, Serialize};

/// Classified pipeline type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineType {
    /// Audio → Text (VAD + ASR)
    SpeechToText,
    /// Text → Audio (TTS)
    TextToSpeech,
    /// Audio → Audio (VAD + ASR + LLM + TTS)
    VoiceAssistant,
    /// Audio → Audio (resample, format, mix, etc.)
    AudioProcessing,
    /// Video → Video or Video → annotated output
    VideoProcessing,
    /// Multiple data types or complex routing
    Mixed,
    /// Cannot determine from node types
    Unknown,
}

/// Info about a known node type's expected I/O
#[derive(Debug, Clone)]
pub struct NodeTypeInfo {
    pub input_type: Option<RuntimeDataType>,
    pub output_type: Option<RuntimeDataType>,
    pub category: &'static str,
}

/// Static lookup table for known node types → expected I/O
pub static NODE_TYPE_TABLE: &[(&str, NodeTypeInfo)] = &[
    // Audio processing (Rust native)
    (
        "RustResampleNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Audio),
            category: "audio",
        },
    ),
    (
        "RustVADNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Audio),
            category: "audio",
        },
    ),
    (
        "SileroVADNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Audio),
            category: "audio",
        },
    ),
    (
        "AudioChunkerNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Audio),
            category: "audio",
        },
    ),
    (
        "AudioBufferAccumulatorNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Audio),
            category: "audio",
        },
    ),
    (
        "AudioFormatNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Audio),
            category: "audio",
        },
    ),
    (
        "AudioLevelNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Json),
            category: "audio",
        },
    ),
    // STT / ASR
    (
        "WhisperXNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Text),
            category: "stt",
        },
    ),
    (
        "HFWhisperNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Text),
            category: "stt",
        },
    ),
    (
        "WhisperNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Text),
            category: "stt",
        },
    ),
    // TTS
    (
        "KokoroTTSNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Text),
            output_type: Some(RuntimeDataType::Audio),
            category: "tts",
        },
    ),
    (
        "VibeVoiceTTSNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Text),
            output_type: Some(RuntimeDataType::Audio),
            category: "tts",
        },
    ),
    // LLM / Language models
    (
        "LFM2AudioNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Text),
            category: "llm",
        },
    ),
    (
        "OllamaLLMNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Text),
            output_type: Some(RuntimeDataType::Text),
            category: "llm",
        },
    ),
    // Video processing
    (
        "VideoFlipNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Video),
            output_type: Some(RuntimeDataType::Video),
            category: "video",
        },
    ),
    (
        "YOLONode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Video),
            output_type: Some(RuntimeDataType::Json),
            category: "video",
        },
    ),
    // Generic / test nodes
    (
        "PassThroughNode",
        NodeTypeInfo {
            input_type: None,
            output_type: None,
            category: "generic",
        },
    ),
    (
        "CalculatorNode",
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Json),
            output_type: Some(RuntimeDataType::Json),
            category: "generic",
        },
    ),
    (
        "EchoNode",
        NodeTypeInfo {
            input_type: None,
            output_type: None,
            category: "generic",
        },
    ),
];

/// Look up a node type in the static table
pub fn lookup_node_type(node_type: &str) -> Option<&'static NodeTypeInfo> {
    NODE_TYPE_TABLE
        .iter()
        .find(|(name, _)| *name == node_type)
        .map(|(_, info)| info)
}

/// Heuristic lookup when node type is not in the table
fn infer_from_name(node_type: &str) -> NodeTypeInfo {
    let lower = node_type.to_lowercase();
    if lower.contains("whisper") || lower.contains("stt") || lower.contains("asr") {
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Text),
            category: "stt",
        }
    } else if lower.contains("tts") || lower.contains("kokoro") || lower.contains("vibe") {
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Text),
            output_type: Some(RuntimeDataType::Audio),
            category: "tts",
        }
    } else if lower.contains("vad") || lower.contains("resample") || lower.contains("audio") {
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Audio),
            output_type: Some(RuntimeDataType::Audio),
            category: "audio",
        }
    } else if lower.contains("video") || lower.contains("yolo") {
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Video),
            output_type: Some(RuntimeDataType::Video),
            category: "video",
        }
    } else if lower.contains("llm") || lower.contains("lfm") {
        NodeTypeInfo {
            input_type: Some(RuntimeDataType::Text),
            output_type: Some(RuntimeDataType::Text),
            category: "llm",
        }
    } else {
        NodeTypeInfo {
            input_type: None,
            output_type: None,
            category: "unknown",
        }
    }
}

/// Get info for a node type (table lookup with heuristic fallback)
pub fn get_node_info(node_type: &str) -> NodeTypeInfo {
    lookup_node_type(node_type)
        .cloned()
        .unwrap_or_else(|| infer_from_name(node_type))
}

/// Infer input types for source nodes (nodes with no incoming connections)
pub fn infer_source_input_types(graph: &PipelineGraph) -> Vec<RuntimeDataType> {
    let mut types = Vec::new();
    for source_id in &graph.sources {
        if let Some(node) = graph.nodes.get(source_id) {
            let info = get_node_info(&node.node_type);
            if let Some(input_type) = info.input_type {
                if !types.contains(&input_type) {
                    types.push(input_type);
                }
            }
        }
    }
    // Default to Audio if we can't determine
    if types.is_empty() {
        types.push(RuntimeDataType::Audio);
    }
    types
}

/// Infer output types for sink nodes (nodes with no outgoing connections)
pub fn infer_sink_output_types(graph: &PipelineGraph) -> Vec<RuntimeDataType> {
    let mut types = Vec::new();
    for sink_id in &graph.sinks {
        if let Some(node) = graph.nodes.get(sink_id) {
            let info = get_node_info(&node.node_type);
            if let Some(output_type) = info.output_type {
                if !types.contains(&output_type) {
                    types.push(output_type);
                }
            }
        }
    }
    types
}

/// Classify a pipeline based on its node composition
pub fn classify(graph: &PipelineGraph) -> PipelineType {
    let mut categories: Vec<&str> = Vec::new();
    for node in graph.nodes.values() {
        let info = get_node_info(&node.node_type);
        if !categories.contains(&info.category) {
            categories.push(info.category);
        }
    }

    let has_stt = categories.contains(&"stt");
    let has_tts = categories.contains(&"tts");
    let has_llm = categories.contains(&"llm");
    let has_audio = categories.contains(&"audio");
    let has_video = categories.contains(&"video");

    // Voice assistant: STT + (LLM or TTS), or audio → LLM → TTS
    if (has_stt || has_audio) && has_llm && has_tts {
        return PipelineType::VoiceAssistant;
    }
    if (has_stt || has_audio) && has_tts {
        return PipelineType::VoiceAssistant;
    }

    // Speech to text: has STT nodes, produces text
    if has_stt {
        return PipelineType::SpeechToText;
    }

    // Audio to text via LLM (e.g., LFM2Audio)
    if has_audio && has_llm {
        return PipelineType::SpeechToText;
    }

    // Text to speech: has TTS nodes
    if has_tts {
        return PipelineType::TextToSpeech;
    }

    // Video processing
    if has_video && !has_audio && !has_stt && !has_tts {
        return PipelineType::VideoProcessing;
    }

    // Audio processing only
    if has_audio && !has_stt && !has_tts && !has_llm && !has_video {
        return PipelineType::AudioProcessing;
    }

    // Mixed
    if categories.len() > 1 {
        return PipelineType::Mixed;
    }

    PipelineType::Unknown
}
