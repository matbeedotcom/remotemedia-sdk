//! Node schema metadata for TypeScript type generation
//!
//! Provides JSON Schema definitions for Candle node configurations.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Node schema definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSchema {
    /// Node type identifier
    pub node_type: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for configuration
    pub config_schema: Value,
    /// Input media type
    pub input_type: MediaType,
    /// Output media type
    pub output_type: MediaType,
}

/// Media type for node inputs/outputs
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Audio,
    Video,
    Text,
    Json,
    Any,
}

/// Get schema for Whisper node
pub fn whisper_schema() -> NodeSchema {
    NodeSchema {
        node_type: "candle-whisper".to_string(),
        description: "Speech-to-text transcription using Whisper models".to_string(),
        config_schema: json!({
            "type": "object",
            "properties": {
                "model": {
                    "type": "string",
                    "enum": ["tiny", "base", "small", "medium", "large-v3"],
                    "default": "base",
                    "description": "Whisper model variant"
                },
                "language": {
                    "type": "string",
                    "default": "en",
                    "description": "Target language code"
                },
                "device": {
                    "type": "string",
                    "default": "auto",
                    "description": "Inference device (auto, cpu, cuda, metal)"
                },
                "beam_size": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 10,
                    "default": 5,
                    "description": "Beam search width"
                }
            }
        }),
        input_type: MediaType::Audio,
        output_type: MediaType::Text,
    }
}

/// Get schema for YOLO node
pub fn yolo_schema() -> NodeSchema {
    NodeSchema {
        node_type: "candle-yolo".to_string(),
        description: "Object detection using YOLOv8 models".to_string(),
        config_schema: json!({
            "type": "object",
            "properties": {
                "model": {
                    "type": "string",
                    "enum": ["yolov8n", "yolov8s", "yolov8m", "yolov8l", "yolov8x"],
                    "default": "yolov8n",
                    "description": "YOLOv8 model variant"
                },
                "confidence_threshold": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "default": 0.5,
                    "description": "Minimum detection confidence"
                },
                "iou_threshold": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "default": 0.45,
                    "description": "NMS IoU threshold"
                },
                "device": {
                    "type": "string",
                    "default": "auto"
                }
            }
        }),
        input_type: MediaType::Video,
        output_type: MediaType::Json,
    }
}

/// Get schema for Phi node
pub fn phi_schema() -> NodeSchema {
    NodeSchema {
        node_type: "candle-phi".to_string(),
        description: "Text generation using Microsoft Phi models".to_string(),
        config_schema: json!({
            "type": "object",
            "properties": {
                "model": {
                    "type": "string",
                    "enum": ["phi-2", "phi-3-mini", "phi-3-mini-128k"],
                    "default": "phi-2",
                    "description": "Phi model variant"
                },
                "quantization": {
                    "type": "string",
                    "enum": ["f32", "f16", "q8_0", "q5_1", "q5_0", "q4_1", "q4_0"],
                    "default": "f16"
                },
                "device": {
                    "type": "string",
                    "default": "auto"
                },
                "generation": {
                    "type": "object",
                    "properties": {
                        "max_tokens": { "type": "integer", "default": 256 },
                        "temperature": { "type": "number", "default": 0.7 },
                        "top_p": { "type": "number", "default": 0.9 },
                        "top_k": { "type": "integer", "default": 40 }
                    }
                }
            }
        }),
        input_type: MediaType::Text,
        output_type: MediaType::Text,
    }
}

/// Get schema for LLaMA node
pub fn llama_schema() -> NodeSchema {
    NodeSchema {
        node_type: "candle-llama".to_string(),
        description: "Text generation using Meta LLaMA models".to_string(),
        config_schema: json!({
            "type": "object",
            "properties": {
                "model": {
                    "type": "string",
                    "enum": ["llama-3.2-1b", "llama-3.2-3b", "llama-3.1-8b"],
                    "default": "llama-3.2-1b"
                },
                "quantization": {
                    "type": "string",
                    "enum": ["f32", "f16", "q8_0", "q5_1", "q5_0", "q4_1", "q4_0"],
                    "default": "f16"
                },
                "device": {
                    "type": "string",
                    "default": "auto"
                },
                "generation": {
                    "type": "object",
                    "properties": {
                        "max_tokens": { "type": "integer", "default": 256 },
                        "temperature": { "type": "number", "default": 0.7 },
                        "top_p": { "type": "number", "default": 0.9 },
                        "top_k": { "type": "integer", "default": 40 }
                    }
                }
            }
        }),
        input_type: MediaType::Text,
        output_type: MediaType::Text,
    }
}

/// Get all Candle node schemas
pub fn all_schemas() -> Vec<NodeSchema> {
    vec![
        whisper_schema(),
        yolo_schema(),
        phi_schema(),
        llama_schema(),
    ]
}

/// Export schemas as JSON
pub fn export_schemas_json() -> Value {
    json!({
        "nodes": all_schemas()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_schema() {
        let schema = whisper_schema();
        assert_eq!(schema.node_type, "candle-whisper");
        assert_eq!(schema.input_type, MediaType::Audio);
    }

    #[test]
    fn test_yolo_schema() {
        let schema = yolo_schema();
        assert_eq!(schema.node_type, "candle-yolo");
        assert_eq!(schema.output_type, MediaType::Json);
    }

    #[test]
    fn test_all_schemas() {
        let schemas = all_schemas();
        assert_eq!(schemas.len(), 4);
    }

    #[test]
    fn test_export_json() {
        let json = export_schemas_json();
        assert!(json.get("nodes").is_some());
    }
}
