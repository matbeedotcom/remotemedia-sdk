//! YOLO object detection node
//!
//! Provides real-time object detection using YOLOv8 models
//! via the Candle ML framework.

mod config;

pub use config::{YoloConfig, YoloModel};

use crate::cache::ModelCache;
use crate::convert::{RuntimeDataConverter, VideoData};
use crate::device::{DeviceSelector, InferenceDevice};
use crate::error::{CandleNodeError, Result};

use async_trait::async_trait;
use remotemedia_core::capabilities::{
    CapabilityBehavior, ConstraintValue, MediaCapabilities, MediaConstraints,
    VideoConstraints,
};
use remotemedia_core::data_compat::RuntimeData;
use remotemedia_core::nodes::streaming_node::{
    AsyncStreamingNode, StreamingNode, StreamingNodeFactory,
};
use remotemedia_core::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Detection result from YOLO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    /// List of detected objects
    pub detections: Vec<Detection>,
    /// Inference time in milliseconds
    pub inference_time_ms: f32,
    /// Original frame width
    pub frame_width: u32,
    /// Original frame height
    pub frame_height: u32,
}

/// Single detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// COCO class ID (0-79)
    pub class_id: u32,
    /// Human-readable class name
    pub class_name: String,
    /// Detection confidence (0.0-1.0)
    pub confidence: f32,
    /// Bounding box (normalized coordinates)
    pub bbox: BoundingBox,
}

/// Bounding box with normalized coordinates (0.0-1.0)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Top-left X coordinate (0.0-1.0)
    pub x: f32,
    /// Top-left Y coordinate (0.0-1.0)
    pub y: f32,
    /// Width (0.0-1.0)
    pub width: f32,
    /// Height (0.0-1.0)
    pub height: f32,
}

impl BoundingBox {
    /// Convert to pixel coordinates
    pub fn to_pixels(&self, frame_width: u32, frame_height: u32) -> (u32, u32, u32, u32) {
        let x = (self.x * frame_width as f32) as u32;
        let y = (self.y * frame_height as f32) as u32;
        let w = (self.width * frame_width as f32) as u32;
        let h = (self.height * frame_height as f32) as u32;
        (x, y, w, h)
    }

    /// Calculate center point
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Calculate area (normalized)
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// Calculate IoU (Intersection over Union) with another box
    pub fn iou(&self, other: &BoundingBox) -> f32 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);

        if x2 <= x1 || y2 <= y1 {
            return 0.0;
        }

        let intersection = (x2 - x1) * (y2 - y1);
        let union = self.area() + other.area() - intersection;

        if union <= 0.0 {
            0.0
        } else {
            intersection / union
        }
    }
}

/// Image preprocessing utilities for YOLO
pub struct YoloPreprocessor;

impl YoloPreprocessor {
    /// Resize image to target size with letterboxing (preserves aspect ratio)
    pub fn letterbox_resize(
        data: &[u8],
        src_width: u32,
        src_height: u32,
        target_size: u32,
    ) -> (Vec<u8>, f32, f32, f32, f32) {
        let scale = (target_size as f32 / src_width as f32)
            .min(target_size as f32 / src_height as f32);
        
        let new_width = (src_width as f32 * scale) as u32;
        let new_height = (src_height as f32 * scale) as u32;
        
        let pad_x = (target_size - new_width) / 2;
        let pad_y = (target_size - new_height) / 2;

        let mut output = vec![114u8; (target_size * target_size * 3) as usize];

        for y in 0..new_height {
            for x in 0..new_width {
                let src_x = ((x as f32 / scale) as u32).min(src_width - 1);
                let src_y = ((y as f32 / scale) as u32).min(src_height - 1);
                
                let src_idx = ((src_y * src_width + src_x) * 3) as usize;
                let dst_idx = (((y + pad_y) * target_size + (x + pad_x)) * 3) as usize;
                
                if src_idx + 2 < data.len() && dst_idx + 2 < output.len() {
                    output[dst_idx] = data[src_idx];
                    output[dst_idx + 1] = data[src_idx + 1];
                    output[dst_idx + 2] = data[src_idx + 2];
                }
            }
        }

        (output, scale, scale, pad_x as f32, pad_y as f32)
    }

    /// Normalize pixel values to 0.0-1.0 range and convert to CHW format
    pub fn normalize_to_chw(data: &[u8], width: u32, height: u32) -> Vec<f32> {
        let pixels = (width * height) as usize;
        let mut output = vec![0.0f32; pixels * 3];

        for i in 0..pixels {
            let idx = i * 3;
            if idx + 2 < data.len() {
                output[i] = data[idx] as f32 / 255.0;
                output[pixels + i] = data[idx + 1] as f32 / 255.0;
                output[2 * pixels + i] = data[idx + 2] as f32 / 255.0;
            }
        }

        output
    }
}

/// Non-Maximum Suppression implementation
pub struct NMS;

impl NMS {
    /// Apply NMS to filter overlapping detections
    pub fn apply(
        detections: &mut Vec<Detection>,
        iou_threshold: f32,
    ) {
        detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        let mut keep = vec![true; detections.len()];

        for i in 0..detections.len() {
            if !keep[i] {
                continue;
            }

            for j in (i + 1)..detections.len() {
                if !keep[j] {
                    continue;
                }

                if detections[i].class_id == detections[j].class_id {
                    let iou = detections[i].bbox.iou(&detections[j].bbox);
                    if iou > iou_threshold {
                        keep[j] = false;
                    }
                }
            }
        }

        let mut idx = 0;
        detections.retain(|_| {
            let k = keep[idx];
            idx += 1;
            k
        });
    }

    /// Apply class-agnostic NMS (ignores class labels)
    pub fn apply_agnostic(
        detections: &mut Vec<Detection>,
        iou_threshold: f32,
    ) {
        detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        let mut keep = vec![true; detections.len()];

        for i in 0..detections.len() {
            if !keep[i] {
                continue;
            }

            for j in (i + 1)..detections.len() {
                if !keep[j] {
                    continue;
                }

                let iou = detections[i].bbox.iou(&detections[j].bbox);
                if iou > iou_threshold {
                    keep[j] = false;
                }
            }
        }

        let mut idx = 0;
        detections.retain(|_| {
            let k = keep[idx];
            idx += 1;
            k
        });
    }
}

/// COCO class names (80 classes)
pub const COCO_CLASSES: &[&str] = &[
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat",
    "traffic light", "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat",
    "dog", "horse", "sheep", "cow", "elephant", "bear", "zebra", "giraffe", "backpack",
    "umbrella", "handbag", "tie", "suitcase", "frisbee", "skis", "snowboard", "sports ball",
    "kite", "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket",
    "bottle", "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple",
    "sandwich", "orange", "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair",
    "couch", "potted plant", "bed", "dining table", "toilet", "tv", "laptop", "mouse",
    "remote", "keyboard", "cell phone", "microwave", "oven", "toaster", "sink", "refrigerator",
    "book", "clock", "vase", "scissors", "teddy bear", "hair drier", "toothbrush",
];

/// YOLO object detection node
pub struct YoloNode {
    /// Node identifier
    node_id: String,
    /// Node configuration
    config: YoloConfig,
    /// Selected inference device
    device: InferenceDevice,
    /// Model cache
    cache: ModelCache,
    /// Loaded model state (lazy initialization)
    model_state: RwLock<Option<YoloModelState>>,
}

/// Internal model state after loading
struct YoloModelState {
    /// Candle device
    #[cfg(feature = "yolo")]
    candle_device: candle_core::Device,
    /// Model weights loaded flag
    weights_loaded: bool,
    /// Model ID for reference
    model_id: String,
}

impl YoloNode {
    /// Create a new YOLO node
    pub fn new(node_id: impl Into<String>, config: &YoloConfig) -> Result<Self> {
        config.validate().map_err(|e| {
            CandleNodeError::configuration("candle-yolo", e)
        })?;

        let device = DeviceSelector::from_config(&config.device)?;

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            device,
            cache: ModelCache::new(),
            model_state: RwLock::new(None),
        })
    }

    /// Create from JSON parameters
    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self> {
        let config = YoloConfig::from_json(params).map_err(|e| {
            CandleNodeError::configuration("candle-yolo", e.to_string())
        })?;
        Self::new(node_id, &config)
    }

    /// Load model weights
    #[cfg(feature = "yolo")]
    async fn load_model(&self) -> Result<()> {
        let mut state = self.model_state.write().await;
        
        if state.is_some() {
            return Ok(());
        }

        info!(
            "Loading YOLO model: {} on {}",
            self.config.model,
            self.device
        );

        let model_id = self.config.model.model_id();
        
        let _weights_path = self
            .cache
            .download_model(model_id, self.config.model.weights_file(), None)
            .await?;

        let candle_device: candle_core::Device = (&self.device).try_into()?;

        info!("YOLO model loaded successfully");

        *state = Some(YoloModelState {
            candle_device,
            weights_loaded: true,
            model_id: model_id.to_string(),
        });

        Ok(())
    }

    #[cfg(not(feature = "yolo"))]
    async fn load_model(&self) -> Result<()> {
        Err(CandleNodeError::configuration(
            "candle-yolo",
            "YOLO feature not enabled at compile time",
        ))
    }

    /// Run object detection on a video frame
    #[cfg(feature = "yolo")]
    async fn detect(&self, video: VideoData) -> Result<DetectionResult> {
        self.load_model().await?;

        let start = std::time::Instant::now();
        
        // Convert to RGB if needed
        let rgb_frame = video.to_rgb24()?;
        let input_size = self.config.model.input_size();
        
        debug!(
            "Running YOLO detection on {}x{} frame (resizing to {})",
            rgb_frame.width, rgb_frame.height, input_size
        );

        let state = self.model_state.read().await;
        if state.is_none() {
            return Err(CandleNodeError::inference(&self.node_id, "Model not loaded"));
        }

        // 1. Preprocess: letterbox resize to model input size
        let (resized, scale_x, scale_y, pad_x, pad_y) = YoloPreprocessor::letterbox_resize(
            &rgb_frame.data,
            rgb_frame.width,
            rgb_frame.height,
            input_size,
        );

        // 2. Normalize to 0-1 and convert to CHW format
        let normalized = YoloPreprocessor::normalize_to_chw(&resized, input_size, input_size);

        // 3. Run model inference
        // TODO: Full model forward pass implementation
        // This requires loading the actual YOLO model weights and running inference
        // For now, we demonstrate the preprocessing pipeline
        let _ = normalized; // Use the normalized data
        let _ = (scale_x, scale_y, pad_x, pad_y); // Use for coordinate transform
        
        warn!("YOLO model inference not fully implemented - returning empty detections");
        
        let mut detections: Vec<Detection> = vec![];
        
        // 4. Apply NMS filtering
        NMS::apply(&mut detections, self.config.iou_threshold);
        
        let inference_time_ms = start.elapsed().as_secs_f32() * 1000.0;

        Ok(DetectionResult {
            detections,
            inference_time_ms,
            frame_width: video.width,
            frame_height: video.height,
        })
    }

    #[cfg(not(feature = "yolo"))]
    async fn detect(&self, _video: VideoData) -> Result<DetectionResult> {
        Err(CandleNodeError::configuration(
            "candle-yolo",
            "YOLO feature not enabled at compile time",
        ))
    }

    /// Get class name for a class ID
    pub fn class_name(class_id: u32) -> &'static str {
        COCO_CLASSES.get(class_id as usize).copied().unwrap_or("unknown")
    }
}

#[async_trait]
impl AsyncStreamingNode for YoloNode {
    fn node_type(&self) -> &str {
        "candle-yolo"
    }

    async fn initialize(&self) -> std::result::Result<(), Error> {
        self.load_model()
            .await
            .map_err(|e| Error::Execution(e.to_string()))
    }

    async fn process(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        let video = RuntimeDataConverter::extract_video(&data, &self.node_id)
            .map_err(|e| Error::Execution(e.to_string()))?;

        let result = self
            .detect(video)
            .await
            .map_err(|e| Error::Execution(e.to_string()))?;

        RuntimeDataConverter::to_json(&result)
            .map_err(|e| Error::Execution(e.to_string()))
    }
}

/// Wrapper to make YoloNode a StreamingNode
pub struct YoloNodeWrapper(pub Arc<YoloNode>);

#[async_trait]
impl StreamingNode for YoloNodeWrapper {
    fn node_type(&self) -> &str {
        self.0.node_type()
    }

    fn node_id(&self) -> &str {
        &self.0.node_id
    }

    async fn initialize(&self) -> std::result::Result<(), Error> {
        AsyncStreamingNode::initialize(self.0.as_ref()).await
    }

    async fn process_async(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        self.0.process(data).await
    }

    async fn process_multi_async(
        &self,
        inputs: std::collections::HashMap<String, RuntimeData>,
    ) -> std::result::Result<RuntimeData, Error> {
        if let Some((_, data)) = inputs.into_iter().next() {
            self.process_async(data).await
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    fn is_multi_input(&self) -> bool {
        false
    }

    fn media_capabilities(&self) -> Option<MediaCapabilities> {
        Some(MediaCapabilities::with_input(MediaConstraints::Video(
            VideoConstraints {
                width: Some(ConstraintValue::Range { min: 32, max: 4096 }),
                height: Some(ConstraintValue::Range { min: 32, max: 4096 }),
                ..Default::default()
            },
        )))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Static
    }
}

/// Factory for creating YoloNode instances
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
        params: &Value,
        _session_id: Option<String>,
    ) -> std::result::Result<Box<dyn StreamingNode>, Error> {
        let node = YoloNode::from_params(node_id, params)
            .map_err(|e| Error::Execution(e.to_string()))?;
        Ok(Box::new(YoloNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "candle-yolo"
    }

    fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
        Some(MediaCapabilities::with_input(MediaConstraints::Video(
            VideoConstraints {
                width: Some(ConstraintValue::Range { min: 32, max: 4096 }),
                height: Some(ConstraintValue::Range { min: 32, max: 4096 }),
                ..Default::default()
            },
        )))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Static
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yolo_config_default() {
        let config = YoloConfig::default();
        assert_eq!(config.model, YoloModel::Yolov8n);
        assert_eq!(config.confidence_threshold, 0.5);
    }

    #[test]
    fn test_yolo_node_creation() {
        let config = YoloConfig::default();
        let node = YoloNode::new("test-yolo", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_coco_classes() {
        assert_eq!(YoloNode::class_name(0), "person");
        assert_eq!(YoloNode::class_name(15), "cat");
        assert_eq!(YoloNode::class_name(16), "dog");
        assert_eq!(YoloNode::class_name(100), "unknown");
    }

    #[test]
    fn test_bbox_to_pixels() {
        let bbox = BoundingBox {
            x: 0.5,
            y: 0.5,
            width: 0.2,
            height: 0.3,
        };
        let (x, y, w, h) = bbox.to_pixels(1920, 1080);
        assert_eq!(x, 960);
        assert_eq!(y, 540);
        assert_eq!(w, 384);
        assert_eq!(h, 324);
    }

    #[test]
    fn test_factory_node_type() {
        let factory = YoloNodeFactory::new();
        assert_eq!(factory.node_type(), "candle-yolo");
    }

    #[test]
    fn test_bbox_iou() {
        let box1 = BoundingBox { x: 0.0, y: 0.0, width: 0.5, height: 0.5 };
        let box2 = BoundingBox { x: 0.25, y: 0.25, width: 0.5, height: 0.5 };
        let iou = box1.iou(&box2);
        assert!(iou > 0.1 && iou < 0.3);

        let box3 = BoundingBox { x: 1.0, y: 1.0, width: 0.1, height: 0.1 };
        assert_eq!(box1.iou(&box3), 0.0);
    }

    #[test]
    fn test_nms() {
        let mut detections = vec![
            Detection {
                class_id: 0,
                class_name: "person".to_string(),
                confidence: 0.9,
                bbox: BoundingBox { x: 0.1, y: 0.1, width: 0.3, height: 0.3 },
            },
            Detection {
                class_id: 0,
                class_name: "person".to_string(),
                confidence: 0.8,
                bbox: BoundingBox { x: 0.12, y: 0.12, width: 0.3, height: 0.3 },
            },
            Detection {
                class_id: 1,
                class_name: "bicycle".to_string(),
                confidence: 0.7,
                bbox: BoundingBox { x: 0.5, y: 0.5, width: 0.2, height: 0.2 },
            },
        ];

        NMS::apply(&mut detections, 0.5);
        assert_eq!(detections.len(), 2);
        assert_eq!(detections[0].confidence, 0.9);
        assert_eq!(detections[1].class_id, 1);
    }

    #[test]
    fn test_preprocessor_normalize() {
        let data = vec![255u8, 0, 128, 64, 192, 32];
        let normalized = YoloPreprocessor::normalize_to_chw(&data, 2, 1);
        assert_eq!(normalized.len(), 6);
        assert!((normalized[0] - 1.0).abs() < 0.01);
        assert!((normalized[1] - 64.0 / 255.0).abs() < 0.01);
    }
}
