//! YOLO object detection node (placeholder)
//!
//! Full implementation in Phase 4 (User Story 2)

mod config;

pub use config::YoloConfig;

use crate::error::Result;
use remotemedia_core::capabilities::{CapabilityBehavior, MediaCapabilities};
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::streaming_node::{StreamingNode, StreamingNodeFactory};
use remotemedia_core::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

/// Detection result from YOLO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub detections: Vec<Detection>,
    pub inference_time_ms: f32,
    pub frame_width: u32,
    pub frame_height: u32,
}

/// Single detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub class_id: u32,
    pub class_name: String,
    pub confidence: f32,
    pub bbox: BoundingBox,
}

/// Bounding box (normalized 0-1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// YOLO node (placeholder)
pub struct YoloNode {
    node_id: String,
    config: YoloConfig,
}

impl YoloNode {
    pub fn new(node_id: impl Into<String>, config: &YoloConfig) -> Result<Self> {
        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
        })
    }
}

/// YOLO node factory
pub struct YoloNodeFactory;

impl YoloNodeFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for YoloNodeFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingNodeFactory for YoloNodeFactory {
    fn create(
        &self,
        node_id: String,
        _params: &Value,
        _session_id: Option<String>,
    ) -> std::result::Result<Box<dyn StreamingNode>, Error> {
        Err(Error::Execution("YOLO node not yet implemented".to_string()))
    }

    fn node_type(&self) -> &str {
        "candle-yolo"
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Static
    }
}
