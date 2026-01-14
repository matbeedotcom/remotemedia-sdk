//! Configuration for YOLO object detection node

use serde::{Deserialize, Serialize};

/// YOLO model variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum YoloModel {
    #[default]
    #[serde(rename = "yolov8n")]
    Yolov8n,
    #[serde(rename = "yolov8s")]
    Yolov8s,
    #[serde(rename = "yolov8m")]
    Yolov8m,
}

/// Configuration for YoloNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloConfig {
    #[serde(default)]
    pub model: YoloModel,
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f32,
    #[serde(default = "default_nms")]
    pub nms_threshold: f32,
    #[serde(default = "default_device")]
    pub device: String,
    #[serde(default)]
    pub classes: Vec<u32>,
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

impl Default for YoloConfig {
    fn default() -> Self {
        Self {
            model: YoloModel::default(),
            confidence_threshold: default_confidence(),
            nms_threshold: default_nms(),
            device: default_device(),
            classes: Vec::new(),
        }
    }
}
