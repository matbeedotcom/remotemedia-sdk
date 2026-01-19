//! Default streaming node registry with built-in node factories

use crate::capabilities::{
    AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue, MediaCapabilities,
    MediaConstraints,
};
use crate::nodes::calculator::CalculatorNode;
use crate::nodes::passthrough::PassThroughNode;
use crate::nodes::python_streaming::PythonStreamingNode;
use crate::nodes::health_emitter::HealthEmitterNodeFactory;
use crate::nodes::remote_pipeline::RemotePipelineNodeFactory;
use crate::nodes::audio_level::AudioLevelNodeFactory;
use crate::nodes::clipping_detector::ClippingDetectorNodeFactory;
use crate::nodes::channel_balance::ChannelBalanceNodeFactory;
use crate::nodes::silence_detector::SilenceDetectorNodeFactory;
use crate::nodes::speech_presence::SpeechPresenceNodeFactory;
use crate::nodes::conversation_flow::ConversationFlowNodeFactory;
use crate::nodes::session_health::SessionHealthNodeFactory;
use crate::nodes::timing_drift::TimingDriftNodeFactory;
use crate::nodes::event_correlator::EventCorrelatorNodeFactory;
use crate::nodes::audio_evidence::AudioEvidenceNodeFactory;
// Temporarily disabled - incomplete implementation
// use crate::nodes::sync_av::SynchronizedAudioVideoNode;
use crate::nodes::video_flip::VideoFlipNode;
// use crate::nodes::video_processor::VideoProcessorNode;
#[cfg(feature = "video")]
use crate::nodes::video::{VideoEncoderNode, VideoEncoderConfig, VideoDecoderNode, VideoDecoderConfig};
use crate::nodes::{
    AsyncNodeWrapper, StreamingNode, StreamingNodeFactory, StreamingNodeRegistry, SyncNodeWrapper,
};
use crate::data::RuntimeData;
// Note: NodeContext and NodeExecutor are available via crate::nodes if needed
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

/// WhisperX transcription node (Python) - provides word-level timestamps via alignment
struct WhisperXNodeFactory;
impl StreamingNodeFactory for WhisperXNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "WhisperXTranscriber", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "WhisperXTranscriber", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "WhisperXNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("WhisperXNode")
                .description("Speech-to-text with word-level timestamps using WhisperX (Python)")
                .category("ml")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Json])
        )
    }
}

/// HuggingFace Whisper transcription node (Python) - word-level timestamps via transformers
struct HFWhisperNodeFactory;
impl StreamingNodeFactory for HFWhisperNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "WhisperTranscriptionNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "WhisperTranscriptionNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "HFWhisperNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Yields WordUpdate objects for each word
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("HFWhisperNode")
                .description("Speech-to-text with word-level timestamps using HuggingFace Whisper (Python)")
                .category("ml")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Json])
        )
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
        use crate::nodes::speculative_vad_gate::{SpeculativeVADGate, SpeculativeVADGateConfig};

        // Deserialize config directly - #[serde(default)] handles missing fields,
        // #[serde(alias = "camelCase")] handles both snake_case and camelCase
        let config: SpeculativeVADGateConfig = serde_json::from_value(params.clone())
            .unwrap_or_default();

        let node = SpeculativeVADGate::new(config);
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
        use crate::nodes::speculative_vad_gate::SpeculativeVADGateConfig;
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
                .config_schema_from::<SpeculativeVADGateConfig>(),
        )
    }
}

// Speculative VAD Coordinator - integrates speculative forwarding with Silero VAD
struct SpeculativeVADCoordinatorFactory;
impl StreamingNodeFactory for SpeculativeVADCoordinatorFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::speculative_vad_coordinator::{SpeculativeVADCoordinator, SpeculativeVADCoordinatorConfig};

        let config: SpeculativeVADCoordinatorConfig = serde_json::from_value(params.clone())
            .unwrap_or_default();

        let node = SpeculativeVADCoordinator::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SpeculativeVADCoordinator"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Outputs audio + VAD JSON + optional cancellation messages
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType};
        use crate::nodes::speculative_vad_coordinator::SpeculativeVADCoordinatorConfig;
        Some(
            NodeSchema::new("SpeculativeVADCoordinator")
                .description("Speculative VAD coordinator integrating immediate forwarding with Silero VAD for false positive detection")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio, RuntimeDataType::Json, RuntimeDataType::ControlMessage])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: true,
                    latency_class: LatencyClass::Realtime,
                })
                .config_schema_from::<SpeculativeVADCoordinatorConfig>(),
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
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::audio::{FastResampleNode, ResampleQuality};
        use crate::nodes::audio_resample_streaming::{AutoResampleConfig, AutoResampleStreamingNode, ResampleStreamingNode};

        // Parse optional source_rate (can be "auto" or omitted for auto-detection)
        let source_rate = params
            .get("sourceRate")
            .or(params.get("source_rate"))
            .and_then(|v| {
                // Allow "auto" string to mean auto-detect
                if v.as_str() == Some("auto") {
                    None
                } else {
                    v.as_u64().map(|n| n as u32)
                }
            });

        // Parse optional target_rate (can be "auto" or omitted for passthrough/adaptive)
        let target_rate = params
            .get("targetRate")
            .or(params.get("target_rate"))
            .and_then(|v| {
                if v.as_str() == Some("auto") {
                    None
                } else {
                    v.as_u64().map(|n| n as u32)
                }
            });

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

        let channels = params.get("channels").and_then(|v| v.as_u64()).map(|n| n as usize);

        // Use AutoResampleStreamingNode when source or target rate is not specified
        // This enables lazy initialization with auto-detection from incoming data
        if source_rate.is_none() || target_rate.is_none() {
            use crate::nodes::audio_resample_streaming::AutoResampleStreamingNodeWrapper;

            let config = AutoResampleConfig {
                source_rate,
                target_rate,
                quality,
                channels,
            };
            let node = AutoResampleStreamingNode::new(node_id, config);
            // Use AutoResampleStreamingNodeWrapper for spec 025 configure_from_upstream support
            return Ok(Box::new(AutoResampleStreamingNodeWrapper::new(node)));
        }

        // Both rates specified - use the original fixed-rate resampler
        let source_rate = source_rate.unwrap();
        let target_rate = target_rate.unwrap();
        let channels = channels.unwrap_or(1);

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
                .description("High-quality audio resampling using sinc interpolation. Supports auto-detection of sample rates from connected nodes.")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio])
                .config_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source_rate": {
                            "oneOf": [
                                { "type": "integer", "minimum": 8000, "maximum": 192000 },
                                { "type": "string", "enum": ["auto"] }
                            ],
                            "description": "Source sample rate in Hz. Use 'auto' or omit to detect from incoming audio."
                        },
                        "target_rate": {
                            "oneOf": [
                                { "type": "integer", "minimum": 8000, "maximum": 192000 },
                                { "type": "string", "enum": ["auto"] }
                            ],
                            "description": "Target sample rate in Hz. Use 'auto' or omit to adapt to downstream requirements."
                        },
                        "quality": {
                            "type": "string",
                            "description": "Resampling quality",
                            "enum": ["Low", "Medium", "High"],
                            "default": "Medium"
                        },
                        "channels": {
                            "type": "integer",
                            "description": "Number of audio channels. Omit to detect from incoming audio.",
                            "minimum": 1,
                            "maximum": 8
                        }
                    }
                })),
        )
    }

    fn media_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
        // Check if explicit source and target rates are provided
        let source_rate = params
            .get("sourceRate")
            .or(params.get("source_rate"))
            .and_then(|v| {
                if v.as_str() == Some("auto") { None } else { v.as_u64().map(|n| n as u32) }
            });

        let target_rate = params
            .get("targetRate")
            .or(params.get("target_rate"))
            .and_then(|v| {
                if v.as_str() == Some("auto") { None } else { v.as_u64().map(|n| n as u32) }
            });

        let channels = params
            .get("channels")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        // When both source and target rates are explicit, return Configured capabilities
        // Note: Channels are kept flexible on input (range 1-8) to allow the resample node
        // to accept any channel count and output the configured count.
        // This is similar to how a resampler can accept various input rates.
        if let (Some(_source), Some(target)) = (source_rate, target_rate) {
            // Output channel constraint (exact if specified, else flexible)
            let output_channel_constraint = channels
                .map(ConstraintValue::Exact)
                .unwrap_or(ConstraintValue::Range { min: 1, max: 8 });

            return Some(MediaCapabilities::with_input_output(
                // Input: accept wide range of sample rates and channels
                MediaConstraints::Audio(AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                }),
                // Output: exact target rate with optional exact channels
                MediaConstraints::Audio(AudioConstraints {
                    sample_rate: Some(ConstraintValue::Exact(target)),
                    channels: Some(output_channel_constraint),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                }),
            ));
        }

        // When auto-configuration is needed, return Adaptive capabilities:
        // - Input: accepts a wide range of sample rates (8kHz - 192kHz)
        // - Output: None initially - adapts to downstream requirements during reverse pass
        Some(MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Range {
                    min: 8000,
                    max: 192000,
                }),
                channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        )))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        // Default to Adaptive - the actual behavior is determined by media_capabilities():
        // - If media_capabilities() returns both input AND output, it acts as Configured
        // - If media_capabilities() returns only input, it acts as Adaptive
        // The resolver checks for output capabilities to determine if adaptation is needed.
        CapabilityBehavior::Adaptive
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

    // Register Whisper transcription nodes (Python-based, rwhisper removed in favor of candle-nodes)
    registry.register(Arc::new(WhisperXNodeFactory));     // Python WhisperX with alignment
    registry.register(Arc::new(HFWhisperNodeFactory));    // Python HuggingFace with word timestamps

    // Register health monitoring nodes (spec 027)
    registry.register(Arc::new(HealthEmitterNodeFactory));
    
    // Register audio analysis nodes for fault detection (spec 027)
    registry.register(Arc::new(AudioLevelNodeFactory));
    registry.register(Arc::new(ClippingDetectorNodeFactory));
    registry.register(Arc::new(ChannelBalanceNodeFactory));
    registry.register(Arc::new(SilenceDetectorNodeFactory));

    // Register stream health monitoring nodes (business layer)
    registry.register(Arc::new(SpeechPresenceNodeFactory));
    registry.register(Arc::new(ConversationFlowNodeFactory));
    registry.register(Arc::new(SessionHealthNodeFactory));

    // Register stream health monitoring nodes (technical layer)
    registry.register(Arc::new(TimingDriftNodeFactory));
    registry.register(Arc::new(EventCorrelatorNodeFactory));
    registry.register(Arc::new(AudioEvidenceNodeFactory));

    // Register Speculative VAD Gate (Spec 007 - low-latency streaming)
    registry.register(Arc::new(SpeculativeVADGateFactory));

    // Register Speculative VAD Coordinator (integrates forwarding + Silero VAD)
    registry.register(Arc::new(SpeculativeVADCoordinatorFactory));

    // Register speaker diarization node (identifies who spoke when)
    #[cfg(feature = "speaker-diarization")]
    registry.register(Arc::new(crate::nodes::speaker_diarization::SpeakerDiarizationNodeFactory));

    // Register audio channel splitter node (routes audio by speaker)
    registry.register(Arc::new(crate::nodes::audio_channel_splitter::AudioChannelSplitterNodeFactory));


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

    // Register output formatters
    registry.register(Arc::new(SrtOutputNodeFactory));

    registry
}

/// SRT output node that converts Whisper JSON segments to SRT subtitle format
struct SrtOutputStreamingNode {
    include_numbers: bool,
    max_line_length: usize,
    segment_counter: std::sync::atomic::AtomicUsize,
}

impl SrtOutputStreamingNode {
    fn new(include_numbers: bool, max_line_length: usize) -> Self {
        Self {
            include_numbers,
            max_line_length,
            segment_counter: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn seconds_to_timecode(seconds: f64) -> String {
        let total_ms = (seconds * 1000.0).round() as u64;
        let ms = total_ms % 1000;
        let total_secs = total_ms / 1000;
        let secs = total_secs % 60;
        let total_mins = total_secs / 60;
        let mins = total_mins % 60;
        let hours = total_mins / 60;
        format!("{:02}:{:02}:{:02},{:03}", hours, mins, secs, ms)
    }

    fn wrap_text(text: &str, max_len: usize) -> String {
        if max_len == 0 || text.len() <= max_len {
            return text.to_string();
        }

        let mut lines = Vec::new();
        let mut current_line = String::new();

        for word in text.split_whitespace() {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= max_len {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        lines.join("\n")
    }
}

#[async_trait::async_trait]
impl crate::nodes::AsyncStreamingNode for SrtOutputStreamingNode {
    fn node_type(&self) -> &str {
        "SrtOutput"
    }

    async fn initialize(&self) -> Result<(), Error> {
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let json = match &data {
            RuntimeData::Json(j) => j.clone(),
            RuntimeData::Text(t) => {
                // Plain text - wrap in a single segment
                serde_json::json!({
                    "text": t,
                    "segments": [{"start": 0.0, "end": 10.0, "text": t}]
                })
            }
            _ => {
                return Err(Error::Execution(format!(
                    "SrtOutput expects JSON or Text, got: {}",
                    data.data_type()
                )));
            }
        };

        let mut srt_output = String::new();

        // Check if this is a WordUpdate from Python HFWhisper (has "word" field)
        if json.get("word").is_some() {
            // Single word update - accumulate for streaming mode
            // For now, just format it as a single subtitle
            let word = json.get("word").and_then(|v| v.as_str()).unwrap_or("");
            let start = json.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let end = json.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0);
            
            if !word.trim().is_empty() {
                let counter = self.segment_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                let start_tc = Self::seconds_to_timecode(start);
                let end_tc = Self::seconds_to_timecode(end);
                
                if self.include_numbers {
                    srt_output.push_str(&format!(
                        "{}\n{} --> {}\n{}\n\n",
                        counter, start_tc, end_tc, word.trim()
                    ));
                } else {
                    srt_output.push_str(&format!(
                        "{} --> {}\n{}\n\n",
                        start_tc, end_tc, word.trim()
                    ));
                }
            }
        } else if let Some(segments) = json.get("segments").and_then(|s| s.as_array()) {
            // Standard segments format from Rust Whisper
            for segment in segments {
                let start = segment.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let end = segment.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let text = segment.get("text").and_then(|v| v.as_str()).unwrap_or("");

                if !text.trim().is_empty() {
                    let counter = self.segment_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    let start_tc = Self::seconds_to_timecode(start);
                    let end_tc = Self::seconds_to_timecode(end);
                    let formatted_text = Self::wrap_text(text.trim(), self.max_line_length);

                    if self.include_numbers {
                        srt_output.push_str(&format!(
                            "{}\n{} --> {}\n{}\n\n",
                            counter, start_tc, end_tc, formatted_text
                        ));
                    } else {
                        srt_output.push_str(&format!(
                            "{} --> {}\n{}\n\n",
                            start_tc, end_tc, formatted_text
                        ));
                    }
                }
            }
        }

        Ok(RuntimeData::Text(srt_output))
    }
}

struct SrtOutputNodeFactory;

impl StreamingNodeFactory for SrtOutputNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let include_numbers = params
            .get("include_numbers")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let max_line_length = params
            .get("max_line_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let node = SrtOutputStreamingNode::new(include_numbers, max_line_length);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SrtOutput"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("SrtOutput")
                .description("Converts Whisper JSON output to SRT subtitle format")
                .category("utility")
                .accepts([RuntimeDataType::Json, RuntimeDataType::Text])
                .produces([RuntimeDataType::Text])
        )
    }
}
