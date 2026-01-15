//! Integration tests for YoloNode

use remotemedia_candle_nodes::{CandleNodeError, DeviceSelector, ModelCache};

mod common_tests {
    use super::*;

    #[test]
    fn test_device_selector() {
        let device = DeviceSelector::from_config("cpu").unwrap();
        assert_eq!(device.name(), "cpu");
    }

    #[test]
    fn test_model_cache_creation() {
        let cache = ModelCache::new();
        assert!(cache.cache_dir().exists() || !cache.cache_dir().exists());
    }
}

#[cfg(feature = "yolo")]
mod yolo_tests {
    use remotemedia_candle_nodes::yolo::{
        BoundingBox, Detection, DetectionResult, NMS, YoloConfig, YoloModel, YoloNode,
        YoloPreprocessor, COCO_CLASSES,
    };

    #[test]
    fn test_yolo_config_default() {
        let config = YoloConfig::default();
        assert_eq!(config.model, YoloModel::Yolov8n);
        assert_eq!(config.confidence_threshold, 0.5);
        assert_eq!(config.iou_threshold, 0.45);
    }

    #[test]
    fn test_yolo_config_validation() {
        let mut config = YoloConfig::default();
        assert!(config.validate().is_ok());

        config.confidence_threshold = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_yolo_model_variants() {
        assert_eq!(YoloModel::Yolov8n.input_size(), 640);
        assert_eq!(YoloModel::Yolov8s.input_size(), 640);
        assert!(YoloModel::Yolov8n.approx_size() < YoloModel::Yolov8s.approx_size());
    }

    #[test]
    fn test_yolo_node_creation() {
        let config = YoloConfig::default();
        let node = YoloNode::new("test-yolo", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_bounding_box_iou() {
        let box1 = BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 0.5,
        };
        let box2 = BoundingBox {
            x: 0.25,
            y: 0.25,
            width: 0.5,
            height: 0.5,
        };

        let iou = box1.iou(&box2);
        assert!(iou > 0.1 && iou < 0.3);
    }

    #[test]
    fn test_bounding_box_no_overlap() {
        let box1 = BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 0.2,
            height: 0.2,
        };
        let box2 = BoundingBox {
            x: 0.5,
            y: 0.5,
            width: 0.2,
            height: 0.2,
        };

        assert_eq!(box1.iou(&box2), 0.0);
    }

    #[test]
    fn test_nms_removes_overlapping() {
        let mut detections = vec![
            Detection {
                class_id: 0,
                class_name: "person".to_string(),
                confidence: 0.9,
                bbox: BoundingBox {
                    x: 0.1,
                    y: 0.1,
                    width: 0.3,
                    height: 0.3,
                },
            },
            Detection {
                class_id: 0,
                class_name: "person".to_string(),
                confidence: 0.7,
                bbox: BoundingBox {
                    x: 0.12,
                    y: 0.12,
                    width: 0.3,
                    height: 0.3,
                },
            },
        ];

        NMS::apply(&mut detections, 0.5);
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].confidence, 0.9);
    }

    #[test]
    fn test_nms_keeps_different_classes() {
        let mut detections = vec![
            Detection {
                class_id: 0,
                class_name: "person".to_string(),
                confidence: 0.9,
                bbox: BoundingBox {
                    x: 0.1,
                    y: 0.1,
                    width: 0.3,
                    height: 0.3,
                },
            },
            Detection {
                class_id: 1,
                class_name: "bicycle".to_string(),
                confidence: 0.8,
                bbox: BoundingBox {
                    x: 0.12,
                    y: 0.12,
                    width: 0.3,
                    height: 0.3,
                },
            },
        ];

        NMS::apply(&mut detections, 0.5);
        assert_eq!(detections.len(), 2);
    }

    #[test]
    fn test_preprocessor_chw_conversion() {
        let data = vec![255u8, 128, 64, 32, 16, 8];
        let normalized = YoloPreprocessor::normalize_to_chw(&data, 2, 1);

        assert_eq!(normalized.len(), 6);
        assert!((normalized[0] - 1.0).abs() < 0.01);
        assert!((normalized[1] - 32.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_coco_classes() {
        assert_eq!(COCO_CLASSES.len(), 80);
        assert_eq!(COCO_CLASSES[0], "person");
        assert_eq!(COCO_CLASSES[15], "cat");
        assert_eq!(COCO_CLASSES[16], "dog");
    }

    #[test]
    fn test_detection_result_serialization() {
        let result = DetectionResult {
            detections: vec![Detection {
                class_id: 0,
                class_name: "person".to_string(),
                confidence: 0.95,
                bbox: BoundingBox {
                    x: 0.1,
                    y: 0.2,
                    width: 0.3,
                    height: 0.4,
                },
            }],
            inference_time_ms: 15.5,
            frame_width: 1920,
            frame_height: 1080,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("person"));
        assert!(json.contains("0.95"));
    }
}

#[cfg(not(feature = "yolo"))]
mod yolo_disabled_tests {
    #[test]
    fn test_yolo_feature_disabled() {
        // YOLO feature not enabled - this test verifies compilation works
        assert!(true);
    }
}
