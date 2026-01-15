//! Configuration for YOLO object detection node

use serde::{Deserialize, Serialize};

/// YOLO model variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum YoloModel {
    /// YOLOv8 nano (~3.2M params, fastest)
    #[default]
    #[serde(rename = "yolov8n")]
    Yolov8n,
    /// YOLOv8 small (~11.2M params)
    #[serde(rename = "yolov8s")]
    Yolov8s,
    /// YOLOv8 medium (~25.9M params)
    #[serde(rename = "yolov8m")]
    Yolov8m,
    /// YOLOv8 large (~43.7M params)
    #[serde(rename = "yolov8l")]
    Yolov8l,
    /// YOLOv8 extra-large (~68.2M params, most accurate)
    #[serde(rename = "yolov8x")]
    Yolov8x,
}

impl YoloModel {
    /// Get the HuggingFace model ID
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::Yolov8n => "lmz/candle-yolo-v8",
            Self::Yolov8s => "lmz/candle-yolo-v8",
            Self::Yolov8m => "lmz/candle-yolo-v8",
            Self::Yolov8l => "lmz/candle-yolo-v8",
            Self::Yolov8x => "lmz/candle-yolo-v8",
        }
    }

    /// Get the model weights filename
    pub fn weights_file(&self) -> &'static str {
        match self {
            Self::Yolov8n => "yolov8n.safetensors",
            Self::Yolov8s => "yolov8s.safetensors",
            Self::Yolov8m => "yolov8m.safetensors",
            Self::Yolov8l => "yolov8l.safetensors",
            Self::Yolov8x => "yolov8x.safetensors",
        }
    }

    /// Get expected input size (width, height)
    pub fn input_size(&self) -> (u32, u32) {
        (640, 640)
    }

    /// Get approximate model size in bytes
    pub fn approx_size(&self) -> u64 {
        match self {
            Self::Yolov8n => 6_500_000,
            Self::Yolov8s => 22_500_000,
            Self::Yolov8m => 52_000_000,
            Self::Yolov8l => 87_500_000,
            Self::Yolov8x => 136_500_000,
        }
    }
}

impl std::fmt::Display for YoloModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Yolov8n => write!(f, "yolov8n"),
            Self::Yolov8s => write!(f, "yolov8s"),
            Self::Yolov8m => write!(f, "yolov8m"),
            Self::Yolov8l => write!(f, "yolov8l"),
            Self::Yolov8x => write!(f, "yolov8x"),
        }
    }
}

impl std::str::FromStr for YoloModel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "yolov8n" | "nano" | "n" => Ok(Self::Yolov8n),
            "yolov8s" | "small" | "s" => Ok(Self::Yolov8s),
            "yolov8m" | "medium" | "m" => Ok(Self::Yolov8m),
            "yolov8l" | "large" | "l" => Ok(Self::Yolov8l),
            "yolov8x" | "xlarge" | "x" => Ok(Self::Yolov8x),
            other => Err(format!("Unknown YOLO model: {}", other)),
        }
    }
}

/// Configuration for YoloNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloConfig {
    /// Model variant to use
    #[serde(default)]
    pub model: YoloModel,

    /// Confidence threshold for detections (0.0-1.0)
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f32,

    /// Non-maximum suppression IoU threshold (0.0-1.0)
    #[serde(default = "default_nms")]
    pub nms_threshold: f32,

    /// Inference device ("auto", "cpu", "cuda", "metal")
    #[serde(default = "default_device")]
    pub device: String,

    /// Filter to specific class IDs (empty = all classes)
    #[serde(default)]
    pub classes: Vec<u32>,

    /// Draw bounding boxes on output frames
    #[serde(default)]
    pub draw_boxes: bool,

    /// Include class names in output (requires loading labels)
    #[serde(default = "default_true")]
    pub include_names: bool,
}

fn default_confidence() -> f32 {
    0.5
}

fn default_nms() -> f32 {
    0.45
}

fn default_device() -> String {
    "auto".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for YoloConfig {
    fn default() -> Self {
        Self {
            model: YoloModel::default(),
            confidence_threshold: default_confidence(),
            nms_threshold: default_nms(),
            device: default_device(),
            classes: Vec::new(),
            draw_boxes: false,
            include_names: true,
        }
    }
}

impl YoloConfig {
    /// Create config from JSON value
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.confidence_threshold < 0.0 || self.confidence_threshold > 1.0 {
            return Err(format!(
                "confidence_threshold must be 0.0-1.0, got {}",
                self.confidence_threshold
            ));
        }

        if self.nms_threshold < 0.0 || self.nms_threshold > 1.0 {
            return Err(format!(
                "nms_threshold must be 0.0-1.0, got {}",
                self.nms_threshold
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = YoloConfig::default();
        assert_eq!(config.model, YoloModel::Yolov8n);
        assert_eq!(config.confidence_threshold, 0.5);
        assert!(config.include_names);
    }

    #[test]
    fn test_model_from_str() {
        assert_eq!("yolov8n".parse::<YoloModel>().unwrap(), YoloModel::Yolov8n);
        assert_eq!("small".parse::<YoloModel>().unwrap(), YoloModel::Yolov8s);
    }
}
