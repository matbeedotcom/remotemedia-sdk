//! Node schema definitions for automatic TypeScript type generation
//!
//! All pipeline nodes receive and emit `RuntimeData` enum variants.
//! This module provides metadata about:
//! - Which RuntimeData variants a node accepts as input
//! - Which RuntimeData variants a node produces as output
//! - The node's configuration parameters (JSON Schema)
//!
//! # Design Philosophy
//!
//! Nodes don't have custom input/output types - they work with `RuntimeData`:
//! - `RuntimeData::Audio { samples, sample_rate, channels, .. }`
//! - `RuntimeData::Video { pixel_data, width, height, format, .. }`
//! - `RuntimeData::Json(Value)`
//! - `RuntimeData::Text(String)`
//! - `RuntimeData::Binary(Vec<u8>)`
//! - `RuntimeData::Tensor { .. }`
//! - `RuntimeData::ControlMessage { .. }`
//!
//! The schema describes which variants are accepted/produced, plus config params.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// RuntimeData variant types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDataType {
    Audio,
    Video,
    Json,
    Text,
    Binary,
    Tensor,
    Numpy,
    ControlMessage,
}

impl RuntimeDataType {
    /// Get all variant types
    pub fn all() -> &'static [RuntimeDataType] {
        &[
            RuntimeDataType::Audio,
            RuntimeDataType::Video,
            RuntimeDataType::Json,
            RuntimeDataType::Text,
            RuntimeDataType::Binary,
            RuntimeDataType::Tensor,
            RuntimeDataType::Numpy,
            RuntimeDataType::ControlMessage,
        ]
    }

    /// Convert to TypeScript type
    pub fn to_typescript(&self) -> &'static str {
        match self {
            RuntimeDataType::Audio => "'audio'",
            RuntimeDataType::Video => "'video'",
            RuntimeDataType::Json => "'json'",
            RuntimeDataType::Text => "'text'",
            RuntimeDataType::Binary => "'binary'",
            RuntimeDataType::Tensor => "'tensor'",
            RuntimeDataType::Numpy => "'numpy'",
            RuntimeDataType::ControlMessage => "'control'",
        }
    }
}

/// Complete schema for a pipeline node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSchema {
    /// Node type identifier (e.g., "KokoroTTSNode", "AudioResample")
    pub node_type: String,

    /// Human-readable description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Category for grouping (e.g., "audio", "text", "ml", "video")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Accepted input RuntimeData variants (empty = accepts any)
    #[serde(default)]
    pub accepts: Vec<RuntimeDataType>,

    /// Produced output RuntimeData variants (empty = same as input)
    #[serde(default)]
    pub produces: Vec<RuntimeDataType>,

    /// Configuration JSON Schema (for node params in manifest)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_schema: Option<serde_json::Value>,

    /// Default configuration values
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_defaults: Option<serde_json::Value>,

    /// Whether this is a Python node
    #[serde(default)]
    pub is_python: bool,

    /// Whether this node supports streaming (multiple outputs per input)
    #[serde(default = "default_true")]
    pub streaming: bool,

    /// Whether this node is multi-output (yields many outputs per input)
    #[serde(default)]
    pub multi_output: bool,

    /// Execution characteristics
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<NodeCapabilitiesSchema>,
}

fn default_true() -> bool {
    true
}

/// Execution characteristics for scheduling
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeCapabilitiesSchema {
    /// Can process inputs in parallel
    #[serde(default)]
    pub parallelizable: bool,

    /// Benefits from batched inputs
    #[serde(default)]
    pub batch_aware: bool,

    /// Supports control messages
    #[serde(default)]
    pub supports_control: bool,

    /// Typical latency class
    #[serde(default)]
    pub latency_class: LatencyClass,
}

/// Processing latency classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LatencyClass {
    /// Sub-millisecond (< 1ms) - audio resampling
    Realtime,
    /// Fast (1-10ms) - VAD, format conversion
    #[default]
    Fast,
    /// Medium (10-100ms) - text processing
    Medium,
    /// Slow (100ms-1s) - TTS synthesis
    Slow,
    /// Very slow (> 1s) - ML inference, transcription
    Batch,
}

/// Parameter type for node configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
}

/// A single configuration parameter for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: ParameterType,
    /// Human-readable description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Default value (as JSON)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Whether the parameter is required
    #[serde(default)]
    pub required: bool,
    /// Allowed values (enum constraint)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<serde_json::Value>,
    /// Minimum value (for numbers)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    /// Maximum value (for numbers)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
}

impl NodeSchema {
    /// Create a new node schema
    pub fn new(node_type: impl Into<String>) -> Self {
        Self {
            node_type: node_type.into(),
            description: None,
            category: None,
            accepts: Vec::new(),
            produces: Vec::new(),
            config_schema: None,
            config_defaults: None,
            is_python: false,
            streaming: true,
            multi_output: false,
            capabilities: None,
        }
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set category
    pub fn category(mut self, cat: impl Into<String>) -> Self {
        self.category = Some(cat.into());
        self
    }

    /// Set accepted input types
    pub fn accepts(mut self, types: impl IntoIterator<Item = RuntimeDataType>) -> Self {
        self.accepts = types.into_iter().collect();
        self
    }

    /// Set produced output types
    pub fn produces(mut self, types: impl IntoIterator<Item = RuntimeDataType>) -> Self {
        self.produces = types.into_iter().collect();
        self
    }

    /// Set config schema from JSON Schema value
    pub fn config_schema(mut self, schema: serde_json::Value) -> Self {
        self.config_schema = Some(schema);
        self
    }

    /// Set config schema from a type that implements schemars::JsonSchema
    #[cfg(feature = "schemars")]
    pub fn config_schema_from<T: schemars::JsonSchema>(mut self) -> Self {
        let schema = schemars::schema_for!(T);
        self.config_schema = Some(serde_json::to_value(schema).unwrap_or_default());
        self
    }

    /// Set default config values
    pub fn config_defaults(mut self, defaults: serde_json::Value) -> Self {
        self.config_defaults = Some(defaults);
        self
    }

    /// Mark as Python node
    pub fn python(mut self) -> Self {
        self.is_python = true;
        self
    }

    /// Mark as multi-output streaming
    pub fn multi_output(mut self) -> Self {
        self.multi_output = true;
        self
    }

    /// Set capabilities
    pub fn capabilities(mut self, caps: NodeCapabilitiesSchema) -> Self {
        self.capabilities = Some(caps);
        self
    }

    /// Extract parameters from config_schema as a structured list
    pub fn get_parameters(&self) -> Vec<NodeParameter> {
        let Some(schema) = &self.config_schema else {
            return Vec::new();
        };

        let Some(obj) = schema.as_object() else {
            return Vec::new();
        };

        let Some(props) = obj.get("properties").and_then(|p| p.as_object()) else {
            return Vec::new();
        };

        // Get required fields
        let required: Vec<&str> = obj
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        props
            .iter()
            .map(|(name, prop)| {
                let prop_obj = prop.as_object();

                let param_type = prop_obj
                    .and_then(|o| o.get("type"))
                    .and_then(|t| t.as_str())
                    .map(|t| match t {
                        "string" => ParameterType::String,
                        "number" => ParameterType::Number,
                        "integer" => ParameterType::Integer,
                        "boolean" => ParameterType::Boolean,
                        "array" => ParameterType::Array,
                        "object" => ParameterType::Object,
                        _ => ParameterType::String,
                    })
                    .unwrap_or(ParameterType::String);

                let description = prop_obj
                    .and_then(|o| o.get("description"))
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());

                let default = prop_obj.and_then(|o| o.get("default")).cloned();

                let enum_values = prop_obj
                    .and_then(|o| o.get("enum"))
                    .and_then(|e| e.as_array())
                    .cloned()
                    .unwrap_or_default();

                let minimum = prop_obj
                    .and_then(|o| o.get("minimum"))
                    .and_then(|m| m.as_f64());

                let maximum = prop_obj
                    .and_then(|o| o.get("maximum"))
                    .and_then(|m| m.as_f64());

                NodeParameter {
                    name: name.clone(),
                    param_type,
                    description,
                    default,
                    required: required.contains(&name.as_str()),
                    enum_values,
                    minimum,
                    maximum,
                }
            })
            .collect()
    }
}

/// Trait for types that provide their schema
pub trait HasNodeSchema {
    /// Get the schema for this node type
    fn schema() -> NodeSchema;
}

/// Registry of all node schemas
#[derive(Debug, Default, Clone)]
pub struct NodeSchemaRegistry {
    schemas: HashMap<String, NodeSchema>,
}

impl NodeSchemaRegistry {
    /// Create empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a schema
    pub fn register(&mut self, schema: NodeSchema) {
        self.schemas.insert(schema.node_type.clone(), schema);
    }

    /// Register from HasNodeSchema implementor
    pub fn register_node<T: HasNodeSchema>(&mut self) {
        self.register(T::schema());
    }

    /// Get schema by node type
    pub fn get(&self, node_type: &str) -> Option<&NodeSchema> {
        self.schemas.get(node_type)
    }

    /// Iterate all schemas
    pub fn iter(&self) -> impl Iterator<Item = &NodeSchema> {
        self.schemas.values()
    }

    /// Get all schemas as JSON array
    pub fn to_json(&self) -> serde_json::Value {
        let schemas: Vec<_> = self.schemas.values().collect();
        serde_json::to_value(schemas).unwrap_or_default()
    }

    /// Number of registered schemas
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }
}

// =============================================================================
// Built-in node schemas
// =============================================================================

/// Create registry with schemas for all built-in nodes
pub fn create_builtin_schema_registry() -> NodeSchemaRegistry {
    let mut registry = NodeSchemaRegistry::new();

    // Audio nodes
    registry.register(
        NodeSchema::new("AudioResample")
            .description("Resamples audio to target sample rate")
            .category("audio")
            .accepts([RuntimeDataType::Audio])
            .produces([RuntimeDataType::Audio])
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "target_sample_rate": {
                        "type": "integer",
                        "description": "Target sample rate in Hz",
                        "default": 16000,
                        "minimum": 8000,
                        "maximum": 48000
                    }
                }
            }))
            .capabilities(NodeCapabilitiesSchema {
                parallelizable: true,
                latency_class: LatencyClass::Realtime,
                ..Default::default()
            }),
    );

    registry.register(
        NodeSchema::new("AudioChunker")
            .description("Splits audio into fixed-size chunks")
            .category("audio")
            .accepts([RuntimeDataType::Audio])
            .produces([RuntimeDataType::Audio])
            .multi_output()
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "chunk_size_ms": {
                        "type": "integer",
                        "description": "Chunk duration in milliseconds",
                        "default": 20
                    }
                }
            })),
    );

    // VAD nodes
    registry.register(
        NodeSchema::new("SileroVAD")
            .description("Voice Activity Detection using Silero VAD model")
            .category("audio")
            .accepts([RuntimeDataType::Audio])
            .produces([RuntimeDataType::Audio, RuntimeDataType::ControlMessage])
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "threshold": {
                        "type": "number",
                        "description": "Speech probability threshold",
                        "default": 0.5,
                        "minimum": 0.0,
                        "maximum": 1.0
                    },
                    "min_speech_duration_ms": {
                        "type": "integer",
                        "description": "Minimum speech duration in ms",
                        "default": 250
                    },
                    "min_silence_duration_ms": {
                        "type": "integer",
                        "description": "Minimum silence duration in ms",
                        "default": 100
                    }
                }
            }))
            .capabilities(NodeCapabilitiesSchema {
                parallelizable: true,
                supports_control: true,
                latency_class: LatencyClass::Fast,
                ..Default::default()
            }),
    );

    // Text/ML nodes
    registry.register(
        NodeSchema::new("KokoroTTSNode")
            .description("Text-to-speech synthesis using Kokoro TTS")
            .category("ml")
            .accepts([RuntimeDataType::Text])
            .produces([RuntimeDataType::Audio])
            .python()
            .multi_output()
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "voice": {
                        "type": "string",
                        "description": "Voice ID to use",
                        "default": "af_bella",
                        "enum": ["af_bella", "af_nicole", "af_sarah", "af_sky", "am_adam", "am_michael", "bf_emma", "bf_isabella", "bm_george", "bm_lewis"]
                    },
                    "language": {
                        "type": "string",
                        "description": "Language code",
                        "default": "en-us",
                        "enum": ["en-us", "en-gb", "es", "fr", "de", "it", "ja", "ko", "pt-br", "zh"]
                    },
                    "speed": {
                        "type": "number",
                        "description": "Speech speed multiplier",
                        "default": 1.0,
                        "minimum": 0.5,
                        "maximum": 2.0
                    }
                }
            }))
            .capabilities(NodeCapabilitiesSchema {
                parallelizable: false,
                batch_aware: true,
                latency_class: LatencyClass::Slow,
                ..Default::default()
            }),
    );

    registry.register(
        NodeSchema::new("WhisperNode")
            .description("Speech-to-text transcription using Whisper")
            .category("ml")
            .accepts([RuntimeDataType::Audio])
            .produces([RuntimeDataType::Text, RuntimeDataType::Json])
            .python()
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "model": {
                        "type": "string",
                        "description": "Whisper model size",
                        "default": "base",
                        "enum": ["tiny", "base", "small", "medium", "large", "large-v3"]
                    },
                    "language": {
                        "type": "string",
                        "description": "Language code (null for auto-detect)"
                    },
                    "task": {
                        "type": "string",
                        "description": "Task type",
                        "default": "transcribe",
                        "enum": ["transcribe", "translate"]
                    }
                }
            }))
            .capabilities(NodeCapabilitiesSchema {
                parallelizable: false,
                batch_aware: true,
                latency_class: LatencyClass::Batch,
                ..Default::default()
            }),
    );

    // Utility nodes
    registry.register(
        NodeSchema::new("Echo")
            .description("Passes input through unchanged (for testing)")
            .category("utility")
            .accepts(RuntimeDataType::all().iter().copied())
            .produces(RuntimeDataType::all().iter().copied()),
    );

    registry.register(
        NodeSchema::new("PassThrough")
            .description("Passes input through unchanged")
            .category("utility")
            .accepts(RuntimeDataType::all().iter().copied())
            .produces(RuntimeDataType::all().iter().copied()),
    );

    registry.register(
        NodeSchema::new("CalculatorNode")
            .description("Performs arithmetic operations on JSON input")
            .category("utility")
            .accepts([RuntimeDataType::Json])
            .produces([RuntimeDataType::Json])
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "precision": {
                        "type": "integer",
                        "description": "Decimal precision for results",
                        "default": 10
                    }
                }
            })),
    );

    registry.register(
        NodeSchema::new("TextCollector")
            .description("Collects text chunks into complete utterances")
            .category("text")
            .accepts([RuntimeDataType::Text])
            .produces([RuntimeDataType::Text])
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "delimiter": {
                        "type": "string",
                        "description": "Delimiter to split on",
                        "default": ""
                    },
                    "flush_on_silence": {
                        "type": "boolean",
                        "description": "Flush buffer when silence detected",
                        "default": true
                    }
                }
            })),
    );

    // Speculative VAD Gate (low-latency speculative forwarding)
    registry.register(
        NodeSchema::new("SpeculativeVADGate")
            .description("Speculative VAD gate for low-latency voice interaction")
            .category("audio")
            .accepts([RuntimeDataType::Audio])
            .produces([RuntimeDataType::Audio, RuntimeDataType::ControlMessage])
            .capabilities(NodeCapabilitiesSchema {
                parallelizable: false,
                batch_aware: false,
                supports_control: true,
                latency_class: LatencyClass::Realtime,
            })
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "lookback_ms": {
                        "type": "integer",
                        "description": "Lookback window in milliseconds (audio to keep for cancellation)",
                        "default": 150,
                        "minimum": 0,
                        "maximum": 1000
                    },
                    "lookahead_ms": {
                        "type": "integer",
                        "description": "Lookahead window in milliseconds (wait before confirming)",
                        "default": 50,
                        "minimum": 0,
                        "maximum": 500
                    },
                    "sample_rate": {
                        "type": "integer",
                        "description": "Sample rate of audio in Hz",
                        "default": 16000,
                        "minimum": 8000,
                        "maximum": 48000
                    },
                    "vad_threshold": {
                        "type": "number",
                        "description": "VAD confidence threshold for speech detection (0.0-1.0)",
                        "default": 0.5,
                        "minimum": 0.0,
                        "maximum": 1.0
                    },
                    "min_speech_ms": {
                        "type": "integer",
                        "description": "Minimum speech duration in milliseconds to trigger forwarding",
                        "default": 250,
                        "minimum": 0,
                        "maximum": 5000
                    },
                    "min_silence_ms": {
                        "type": "integer",
                        "description": "Minimum silence duration in milliseconds to end speech segment",
                        "default": 100,
                        "minimum": 0,
                        "maximum": 5000
                    },
                    "pad_ms": {
                        "type": "integer",
                        "description": "Padding before/after speech in milliseconds",
                        "default": 30,
                        "minimum": 0,
                        "maximum": 500
                    }
                }
            })),
    );

    // Video nodes
    registry.register(
        NodeSchema::new("VideoFlip")
            .description("Flips video frames horizontally or vertically")
            .category("video")
            .accepts([RuntimeDataType::Video])
            .produces([RuntimeDataType::Video])
            .config_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "horizontal": {
                        "type": "boolean",
                        "description": "Flip horizontally",
                        "default": true
                    },
                    "vertical": {
                        "type": "boolean",
                        "description": "Flip vertically",
                        "default": false
                    }
                }
            })),
    );

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_builder() {
        let schema = NodeSchema::new("TestNode")
            .description("A test node")
            .category("test")
            .accepts([RuntimeDataType::Audio, RuntimeDataType::Text])
            .produces([RuntimeDataType::Json])
            .python();

        assert_eq!(schema.node_type, "TestNode");
        assert_eq!(schema.accepts.len(), 2);
        assert_eq!(schema.produces.len(), 1);
        assert!(schema.is_python);
    }

    #[test]
    fn test_registry() {
        let registry = create_builtin_schema_registry();

        assert!(registry.get("Echo").is_some());
        assert!(registry.get("KokoroTTSNode").is_some());
        assert!(registry.get("NonExistent").is_none());
    }

    #[test]
    fn test_json_export() {
        let registry = create_builtin_schema_registry();
        let json = registry.to_json();

        assert!(json.is_array());
        let arr = json.as_array().unwrap();
        assert!(!arr.is_empty());
    }
}
