//! Manifest Analyzer — Pure analysis of RemoteMedia pipeline manifests
//!
//! Parses a manifest (YAML/JSON), classifies the pipeline type,
//! detects applicable transports, and identifies ML requirements.
//! Zero execution dependencies.

mod classifier;
mod ml_requirements;
mod transport_detector;

use remotemedia_core::executor::PipelineGraph;
use remotemedia_core::manifest::Manifest;
use remotemedia_core::nodes::schema::RuntimeDataType;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use classifier::{NodeTypeInfo, PipelineType, NODE_TYPE_TABLE};
pub use ml_requirements::MlRequirement;
pub use transport_detector::ApplicableTransport;

/// Result of analyzing a manifest
#[derive(Debug)]
pub struct AnalysisResult {
    /// Classified pipeline type
    pub pipeline_type: PipelineType,
    /// Data types expected at source nodes
    pub source_input_types: Vec<RuntimeDataType>,
    /// Data types produced at sink nodes
    pub sink_output_types: Vec<RuntimeDataType>,
    /// Whether the pipeline requires streaming execution
    pub execution_mode: ExecutionMode,
    /// Transports applicable to this pipeline
    pub applicable_transports: Vec<ApplicableTransport>,
    /// ML model/runtime requirements per node
    pub ml_requirements: Vec<MlRequirement>,
    /// Validated pipeline graph (topologically sorted)
    pub graph: PipelineGraph,
}

/// Execution mode for a pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Single request → single response
    Unary,
    /// Bidirectional streaming
    Streaming,
}

/// Analyze a manifest from a file path
pub fn analyze_file(path: &Path) -> Result<AnalysisResult, AnalyzerError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AnalyzerError::Io(format!("{}: {}", path.display(), e)))?;

    let manifest = parse_manifest(path, &content)?;
    analyze(&manifest)
}

/// Load and parse a manifest from a file path (handles camelCase keys)
pub fn load_manifest(path: &Path) -> Result<Manifest, AnalyzerError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AnalyzerError::Io(format!("{}: {}", path.display(), e)))?;
    parse_manifest(path, &content)
}

/// Analyze a manifest struct directly
pub fn analyze(manifest: &Manifest) -> Result<AnalysisResult, AnalyzerError> {
    let graph = PipelineGraph::from_manifest(manifest)
        .map_err(|e| AnalyzerError::InvalidManifest(e.to_string()))?;

    let source_input_types = classifier::infer_source_input_types(&graph);
    let sink_output_types = classifier::infer_sink_output_types(&graph);
    let pipeline_type = classifier::classify(&graph);

    let has_streaming = manifest.nodes.iter().any(|n| n.is_streaming);
    let execution_mode = if has_streaming {
        ExecutionMode::Streaming
    } else {
        ExecutionMode::Unary
    };

    let applicable_transports =
        transport_detector::detect(&execution_mode, &source_input_types, &sink_output_types);

    let ml_requirements = ml_requirements::detect(&manifest.nodes);

    Ok(AnalysisResult {
        pipeline_type,
        source_input_types,
        sink_output_types,
        execution_mode,
        applicable_transports,
        ml_requirements,
        graph,
    })
}

/// Parse a manifest from content, detecting format by file extension.
/// Handles both camelCase and snake_case JSON keys.
fn parse_manifest(path: &Path, content: &str) -> Result<Manifest, AnalyzerError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("json");

    // Parse to serde_json::Value first so we can normalize keys
    let value: serde_json::Value = match ext {
        "yaml" | "yml" => {
            serde_yaml::from_str(content).map_err(|e| AnalyzerError::Parse(e.to_string()))?
        }
        _ => {
            serde_json::from_str(content).map_err(|e| AnalyzerError::Parse(e.to_string()))?
        }
    };

    // Normalize camelCase keys to snake_case
    let normalized = normalize_keys(value);
    serde_json::from_value(normalized).map_err(|e| AnalyzerError::Parse(e.to_string()))
}

/// Parse a manifest from a JSON string (handles camelCase)
pub fn parse_manifest_json(json: &str) -> Result<Manifest, AnalyzerError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| AnalyzerError::Parse(e.to_string()))?;
    let normalized = normalize_keys(value);
    serde_json::from_value(normalized).map_err(|e| AnalyzerError::Parse(e.to_string()))
}

/// Recursively convert camelCase keys to snake_case
fn normalize_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (key, val) in map {
                let snake_key = camel_to_snake(&key);
                new_map.insert(snake_key, normalize_keys(val));
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(normalize_keys).collect())
        }
        other => other,
    }
}

/// Convert a camelCase string to snake_case
fn camel_to_snake(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

/// Errors from manifest analysis
#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    #[error("Failed to read manifest: {0}")]
    Io(String),
    #[error("Failed to parse manifest: {0}")]
    Parse(String),
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice_assistant_json() -> &'static str {
        r#"{
            "version": "v1",
            "metadata": { "name": "test-voice-assistant" },
            "nodes": [
                { "id": "chunker", "nodeType": "AudioChunkerNode", "isStreaming": true },
                { "id": "vad", "nodeType": "SileroVADNode", "isStreaming": true },
                { "id": "buffer", "nodeType": "AudioBufferAccumulatorNode", "isStreaming": true },
                { "id": "asr", "nodeType": "LFM2AudioNode", "isStreaming": true },
                { "id": "tts", "nodeType": "KokoroTTSNode", "isStreaming": true }
            ],
            "connections": [
                { "from": "chunker", "to": "vad" },
                { "from": "vad", "to": "buffer" },
                { "from": "buffer", "to": "asr" },
                { "from": "asr", "to": "tts" }
            ]
        }"#
    }

    #[test]
    fn test_classify_voice_assistant() {
        let manifest = parse_manifest_json(voice_assistant_json()).unwrap();
        let result = analyze(&manifest).unwrap();
        assert_eq!(result.pipeline_type, classifier::PipelineType::VoiceAssistant);
        assert_eq!(result.execution_mode, ExecutionMode::Streaming);
    }

    #[test]
    fn test_source_input_types_audio() {
        let manifest = parse_manifest_json(voice_assistant_json()).unwrap();
        let result = analyze(&manifest).unwrap();
        assert!(result.source_input_types.contains(&RuntimeDataType::Audio));
    }

    #[test]
    fn test_transport_detection_streaming() {
        let manifest = parse_manifest_json(voice_assistant_json()).unwrap();
        let result = analyze(&manifest).unwrap();
        assert!(result.applicable_transports.contains(&ApplicableTransport::Direct));
        assert!(result.applicable_transports.contains(&ApplicableTransport::GrpcStreaming));
        assert!(result.applicable_transports.contains(&ApplicableTransport::WebRtc));
    }

    #[test]
    fn test_ml_requirements_detected() {
        let manifest = parse_manifest_json(voice_assistant_json()).unwrap();
        let result = analyze(&manifest).unwrap();
        assert_eq!(result.ml_requirements.len(), 3); // SileroVAD, LFM2Audio, KokoroTTS
        assert!(result.ml_requirements.iter().any(|r| r.node_type == "SileroVADNode"));
        assert!(result.ml_requirements.iter().any(|r| r.node_type == "LFM2AudioNode"));
        assert!(result.ml_requirements.iter().any(|r| r.node_type == "KokoroTTSNode"));
    }

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("nodeType"), "node_type");
        assert_eq!(camel_to_snake("isStreaming"), "is_streaming");
        assert_eq!(camel_to_snake("id"), "id");
        assert_eq!(camel_to_snake("autoNegotiate"), "auto_negotiate");
    }

    #[test]
    fn test_invalid_connection_error() {
        let json = r#"{
            "version": "v1",
            "metadata": { "name": "broken" },
            "nodes": [{ "id": "a", "nodeType": "EchoNode" }],
            "connections": [{ "from": "a", "to": "nonexistent" }]
        }"#;
        let manifest = parse_manifest_json(json).unwrap();
        let result = analyze(&manifest);
        assert!(result.is_err());
    }

    #[test]
    fn test_classify_audio_processing() {
        let json = r#"{
            "version": "v1",
            "metadata": { "name": "audio-only" },
            "nodes": [
                { "id": "resample", "nodeType": "RustResampleNode" },
                { "id": "vad", "nodeType": "RustVADNode" }
            ],
            "connections": [{ "from": "resample", "to": "vad" }]
        }"#;
        let manifest = parse_manifest_json(json).unwrap();
        let result = analyze(&manifest).unwrap();
        assert_eq!(result.pipeline_type, classifier::PipelineType::AudioProcessing);
        assert_eq!(result.execution_mode, ExecutionMode::Unary);
    }

    #[test]
    fn test_unary_transport_detection() {
        let json = r#"{
            "version": "v1",
            "metadata": { "name": "unary" },
            "nodes": [
                { "id": "calc", "nodeType": "CalculatorNode" }
            ],
            "connections": []
        }"#;
        let manifest = parse_manifest_json(json).unwrap();
        let result = analyze(&manifest).unwrap();
        assert_eq!(result.execution_mode, ExecutionMode::Unary);
        assert!(result.applicable_transports.contains(&ApplicableTransport::Http));
        assert!(result.applicable_transports.contains(&ApplicableTransport::GrpcUnary));
    }
}
