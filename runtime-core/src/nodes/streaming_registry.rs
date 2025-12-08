//! Default streaming node registry with built-in node factories

use crate::nodes::calculator::CalculatorNode;
use crate::nodes::passthrough::PassThroughNode;
use crate::nodes::python_streaming::PythonStreamingNode;
use crate::nodes::remote_pipeline::RemotePipelineNodeFactory;
// Temporarily disabled - incomplete implementation
// use crate::nodes::sync_av::SynchronizedAudioVideoNode;
use crate::nodes::video_flip::VideoFlipNode;
// use crate::nodes::video_processor::VideoProcessorNode;
#[cfg(feature = "video")]
use crate::nodes::video::{VideoEncoderNode, VideoEncoderConfig, VideoDecoderNode, VideoDecoderConfig};
use crate::nodes::{
    AsyncNodeWrapper, StreamingNode, StreamingNodeFactory, StreamingNodeRegistry, SyncNodeWrapper,
};
use crate::Error;
use serde_json::Value;
use std::sync::Arc;

// Factory implementations for built-in streaming nodes

struct CalculatorNodeFactory;
impl StreamingNodeFactory for CalculatorNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = CalculatorNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "CalculatorNode"
    }
}

// Temporarily disabled - VideoProcessorNode has incomplete implementation
/*
struct VideoProcessorNodeFactory;
impl StreamingNodeFactory for VideoProcessorNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = VideoProcessorNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "VideoProcessorNode"
    }
}
*/

struct VideoFlipNodeFactory;
impl StreamingNodeFactory for VideoFlipNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::video_flip::VideoFlipConfig;
        let config: VideoFlipConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoFlipNode::new(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoFlip"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::video_flip::VideoFlipConfig;
        Some(
            NodeSchema::new("VideoFlip")
                .description("Flips video frames horizontally or vertically")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoFlipConfig>(),
        )
    }
}

#[cfg(feature = "video")]
struct VideoEncoderNodeFactory;

#[cfg(feature = "video")]
impl StreamingNodeFactory for VideoEncoderNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: VideoEncoderConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoEncoderNode::new(config)
            .map_err(|e| Error::Execution(format!("Failed to create VideoEncoder: {}", e)))?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoEncoder"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("VideoEncoder")
                .description("Encodes raw video frames to compressed bitstreams (VP8/AV1/H.264)")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoEncoderConfig>(),
        )
    }
}

#[cfg(feature = "video")]
struct VideoDecoderNodeFactory;

#[cfg(feature = "video")]
impl StreamingNodeFactory for VideoDecoderNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: VideoDecoderConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoDecoderNode::new(config)
            .map_err(|e| Error::Execution(format!("Failed to create VideoDecoder: {}", e)))?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoDecoder"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("VideoDecoder")
                .description("Decodes compressed video bitstreams to raw frames")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoDecoderConfig>(),
        )
    }
}

#[cfg(feature = "video")]
struct VideoScalerNodeFactory;

#[cfg(feature = "video")]
impl StreamingNodeFactory for VideoScalerNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::video::{VideoScalerNode, VideoScalerConfig};
        let config: VideoScalerConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoScalerNode::new(config)
            .map_err(|e| Error::Execution(format!("Failed to create VideoScaler: {}", e)))?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoScaler"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::video::VideoScalerConfig;
        Some(
            NodeSchema::new("VideoScaler")
                .description("Scales/resizes video frames")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoScalerConfig>(),
        )
    }
}

#[cfg(feature = "video")]
struct VideoFormatConverterNodeFactory;

#[cfg(feature = "video")]
impl StreamingNodeFactory for VideoFormatConverterNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::video::{VideoFormatConverterNode, VideoFormatConverterConfig};
        let config: VideoFormatConverterConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoFormatConverterNode::new(config)
            .map_err(|e| Error::Execution(format!("Failed to create VideoFormatConverter: {}", e)))?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoFormatConverter"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::video::VideoFormatConverterConfig;
        Some(
            NodeSchema::new("VideoFormatConverter")
                .description("Converts between pixel formats (RGB/YUV/NV12)")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoFormatConverterConfig>(),
        )
    }
}

// Temporarily disabled - SynchronizedAudioVideoNode has incomplete implementation
/*
struct SynchronizedAudioVideoNodeFactory;
impl StreamingNodeFactory for SynchronizedAudioVideoNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = SynchronizedAudioVideoNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "SynchronizedAudioVideoNode"
    }
}
*/

struct PassThroughNodeFactory;
impl StreamingNodeFactory for PassThroughNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = PassThroughNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "PassThrough"
    }
}

struct KokoroTTSNodeFactory;
impl StreamingNodeFactory for KokoroTTSNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "KokoroTTSNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "KokoroTTSNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "KokoroTTSNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Kokoro yields multiple audio chunks per text input
    }
}

struct VibeVoiceTTSNodeFactory;
impl StreamingNodeFactory for VibeVoiceTTSNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "VibeVoiceTTSNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "VibeVoiceTTSNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VibeVoiceTTSNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // VibeVoice yields multiple audio chunks per text input
    }
}

struct SimplePyTorchNodeFactory;
impl StreamingNodeFactory for SimplePyTorchNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "SimplePyTorchNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "SimplePyTorchNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SimplePyTorchNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }
}

struct AudioBufferAccumulatorNodeFactory;
impl StreamingNodeFactory for AudioBufferAccumulatorNodeFactory {
    fn create(
        &self,
        _node_id: String,  // Reserved for future node identification/logging
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::AudioBufferAccumulatorNode;

        let min_duration_ms = params
            .get("minUtteranceDurationMs")
            .or(params.get("min_utterance_duration_ms"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let max_duration_ms = params
            .get("maxUtteranceDurationMs")
            .or(params.get("max_utterance_duration_ms"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        let node = AudioBufferAccumulatorNode::new(min_duration_ms, max_duration_ms);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "AudioBufferAccumulatorNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Can output 0 or 1 items (when speech ends)
    }
}

struct TextCollectorNodeFactory;
impl StreamingNodeFactory for TextCollectorNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::text_collector::{TextCollectorNode, TextCollectorConfig};
        let config: TextCollectorConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = TextCollectorNode::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "TextCollectorNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Can output 0 or multiple items (complete sentences)
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::text_collector::TextCollectorConfig;
        Some(
            NodeSchema::new("TextCollectorNode")
                .description("Accumulates streaming text tokens and yields complete sentences")
                .category("text")
                .accepts([RuntimeDataType::Text])
                .produces([RuntimeDataType::Text])
                .config_schema_from::<TextCollectorConfig>(),
        )
    }
}

#[cfg(feature = "silero-vad")]
struct SileroVADNodeFactory;

#[cfg(feature = "silero-vad")]
impl StreamingNodeFactory for SileroVADNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::silero_vad::{SileroVADNode, SileroVADConfig};
        let config: SileroVADConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = SileroVADNode::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SileroVADNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Outputs 2 items: VAD event + pass-through audio
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType};
        use crate::nodes::silero_vad::SileroVADConfig;
        Some(
            NodeSchema::new("SileroVADNode")
                .description("Voice activity detection using Silero VAD ONNX model")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Json, RuntimeDataType::Audio])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: true,
                    latency_class: LatencyClass::Fast,
                })
                .config_schema_from::<SileroVADConfig>(),
        )
    }
}

// Spec 007: Speculative VAD Gate for low-latency streaming
struct SpeculativeVADGateFactory;
impl StreamingNodeFactory for SpeculativeVADGateFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::speculative_vad_gate::{SpeculativeVADConfig, SpeculativeVADGate};

        // Deserialize config directly - #[serde(default)] handles missing fields,
        // #[serde(alias = "camelCase")] handles both snake_case and camelCase
        let config: SpeculativeVADConfig = serde_json::from_value(params.clone())
            .unwrap_or_default();

        let node = SpeculativeVADGate::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SpeculativeVADGate"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Outputs audio + optional cancellation messages
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType};
        use crate::nodes::speculative_vad_gate::SpeculativeVADConfig;
        Some(
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
                .config_schema_from::<SpeculativeVADConfig>(),
        )
    }
}

struct LFM2AudioNodeFactory;
impl StreamingNodeFactory for LFM2AudioNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "LFM2AudioNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "LFM2AudioNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "LFM2AudioNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // LFM2Audio yields multiple tokens (text and audio) per input
    }
}

struct AudioChunkerNodeFactory;
impl StreamingNodeFactory for AudioChunkerNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::audio_chunker::{AudioChunkerNode, AudioChunkerConfig};
        let config: AudioChunkerConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = AudioChunkerNode::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "AudioChunkerNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Can output 0 or multiple chunks per input
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::audio_chunker::AudioChunkerConfig;
        Some(
            NodeSchema::new("AudioChunkerNode")
                .description("Splits incoming audio into fixed-size chunks")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio])
                .config_schema_from::<AudioChunkerConfig>(),
        )
    }
}

struct FastResampleNodeFactory;
impl StreamingNodeFactory for FastResampleNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::audio::{FastResampleNode, ResampleQuality};
        use crate::nodes::audio_resample_streaming::ResampleStreamingNode;

        let source_rate = params
            .get("sourceRate")
            .or(params.get("source_rate"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput {
                message: "sourceRate parameter required".into(),
                node_id: "FastResampleNode".into(),
                context: "create".into(),
            })? as u32;

        let target_rate = params
            .get("targetRate")
            .or(params.get("target_rate"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput {
                message: "targetRate parameter required".into(),
                node_id: "FastResampleNode".into(),
                context: "create".into(),
            })? as u32;

        let quality_str = params
            .get("quality")
            .and_then(|v| v.as_str())
            .unwrap_or("Medium");

        let quality = match quality_str {
            "Low" => ResampleQuality::Low,
            "Medium" => ResampleQuality::Medium,
            "High" => ResampleQuality::High,
            _ => ResampleQuality::Medium,
        };

        let channels = params.get("channels").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

        let inner = FastResampleNode::new(source_rate, target_rate, quality, channels)?;
        let node = ResampleStreamingNode::new(inner, target_rate);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "FastResampleNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        false // Always outputs exactly 1 chunk per input
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("FastResampleNode")
                .description("High-quality audio resampling using sinc interpolation")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio])
                .config_schema(serde_json::json!({
                    "type": "object",
                    "required": ["source_rate", "target_rate"],
                    "properties": {
                        "source_rate": {
                            "type": "integer",
                            "description": "Source sample rate in Hz",
                            "minimum": 8000,
                            "maximum": 192000
                        },
                        "target_rate": {
                            "type": "integer",
                            "description": "Target sample rate in Hz",
                            "minimum": 8000,
                            "maximum": 192000
                        },
                        "quality": {
                            "type": "string",
                            "description": "Resampling quality",
                            "enum": ["Low", "Medium", "High"],
                            "default": "Medium"
                        },
                        "channels": {
                            "type": "integer",
                            "description": "Number of audio channels",
                            "default": 1,
                            "minimum": 1,
                            "maximum": 8
                        }
                    }
                })),
        )
    }
}

// Test node factories for Python streaming nodes
struct ExpanderNodeFactory;
impl StreamingNodeFactory for ExpanderNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "ExpanderNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "ExpanderNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "ExpanderNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct RangeGeneratorNodeFactory;
impl StreamingNodeFactory for RangeGeneratorNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "RangeGeneratorNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "RangeGeneratorNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "RangeGeneratorNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct TransformAndExpandNodeFactory;
impl StreamingNodeFactory for TransformAndExpandNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "TransformAndExpandNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "TransformAndExpandNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "TransformAndExpandNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct ChainedTransformNodeFactory;
impl StreamingNodeFactory for ChainedTransformNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "ChainedTransformNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "ChainedTransformNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "ChainedTransformNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct ConditionalExpanderNodeFactory;
impl StreamingNodeFactory for ConditionalExpanderNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "ConditionalExpanderNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "ConditionalExpanderNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "ConditionalExpanderNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct FilterNodeFactory;
impl StreamingNodeFactory for FilterNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "FilterNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "FilterNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "FilterNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // May output 0 or 1 items per input
    }
}

/// Create a default streaming node registry with all built-in nodes registered
pub fn create_default_streaming_registry() -> StreamingNodeRegistry {
    let mut registry = StreamingNodeRegistry::new();

    // Register all built-in streaming nodes
    registry.register(Arc::new(CalculatorNodeFactory));
    // Temporarily disabled - incomplete implementations
    // registry.register(Arc::new(VideoProcessorNodeFactory));
    registry.register(Arc::new(VideoFlipNodeFactory));

    // Register video codec nodes (Spec 012: Video Codec Support)
    #[cfg(feature = "video")]
    {
        registry.register(Arc::new(VideoEncoderNodeFactory));
        registry.register(Arc::new(VideoDecoderNodeFactory));
        registry.register(Arc::new(VideoScalerNodeFactory));
        registry.register(Arc::new(VideoFormatConverterNodeFactory));
    }

    // registry.register(Arc::new(SynchronizedAudioVideoNodeFactory));
    registry.register(Arc::new(PassThroughNodeFactory));

    // Register audio processing nodes
    registry.register(Arc::new(AudioChunkerNodeFactory));
    registry.register(Arc::new(AudioBufferAccumulatorNodeFactory));
    registry.register(Arc::new(FastResampleNodeFactory));

    // Register text processing nodes
    registry.register(Arc::new(TextCollectorNodeFactory));

    // Register remote pipeline node
    registry.register(Arc::new(RemotePipelineNodeFactory));

    // Register Silero VAD node (Rust ONNX)
    #[cfg(feature = "silero-vad")]
    registry.register(Arc::new(SileroVADNodeFactory));

    // Register Speculative VAD Gate (Spec 007 - low-latency streaming)
    registry.register(Arc::new(SpeculativeVADGateFactory));

    // Register Python TTS nodes
    registry.register(Arc::new(KokoroTTSNodeFactory));
    registry.register(Arc::new(VibeVoiceTTSNodeFactory));

    // Register Python speech-to-speech nodes
    registry.register(Arc::new(LFM2AudioNodeFactory));

    // Register Python test nodes
    registry.register(Arc::new(SimplePyTorchNodeFactory));

    // Register Python streaming test nodes for integration tests
    registry.register(Arc::new(ExpanderNodeFactory));
    registry.register(Arc::new(RangeGeneratorNodeFactory));
    registry.register(Arc::new(TransformAndExpandNodeFactory));
    registry.register(Arc::new(ChainedTransformNodeFactory));
    registry.register(Arc::new(ConditionalExpanderNodeFactory));
    registry.register(Arc::new(FilterNodeFactory));

    registry
}
