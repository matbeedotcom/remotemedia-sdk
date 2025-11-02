//! Default streaming node registry with built-in node factories

use crate::nodes::calculator::CalculatorNode;
use crate::nodes::passthrough::PassThroughNode;
use crate::nodes::python_streaming::PythonStreamingNode;
use crate::nodes::sync_av::SynchronizedAudioVideoNode;
use crate::nodes::video_processor::VideoProcessorNode;
use crate::nodes::{AsyncNodeWrapper, SyncNodeWrapper, StreamingNode, StreamingNodeFactory, StreamingNodeRegistry};
use crate::Error;
use serde_json::Value;
use std::sync::Arc;

// Factory implementations for built-in streaming nodes

struct CalculatorNodeFactory;
impl StreamingNodeFactory for CalculatorNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = CalculatorNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "CalculatorNode"
    }
}

struct VideoProcessorNodeFactory;
impl StreamingNodeFactory for VideoProcessorNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = VideoProcessorNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "VideoProcessorNode"
    }
}

struct SynchronizedAudioVideoNodeFactory;
impl StreamingNodeFactory for SynchronizedAudioVideoNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = SynchronizedAudioVideoNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "SynchronizedAudioVideoNode"
    }
}

struct PassThroughNodeFactory;
impl StreamingNodeFactory for PassThroughNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
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
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "KokoroTTSNode", params)?;
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

struct SimplePyTorchNodeFactory;
impl StreamingNodeFactory for SimplePyTorchNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "SimplePyTorchNode", params)?;
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
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::AudioBufferAccumulatorNode;

        let min_duration_ms = params.get("minUtteranceDurationMs").or(params.get("min_utterance_duration_ms")).and_then(|v| v.as_u64()).map(|v| v as u32);
        let max_duration_ms = params.get("maxUtteranceDurationMs").or(params.get("max_utterance_duration_ms")).and_then(|v| v.as_u64()).map(|v| v as u32);

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

#[cfg(feature = "silero-vad")]
struct SileroVADNodeFactory;

#[cfg(feature = "silero-vad")]
impl StreamingNodeFactory for SileroVADNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::SileroVADNode;

        // Extract parameters
        let threshold = params.get("threshold").and_then(|v| v.as_f64()).map(|v| v as f32);
        let sampling_rate = params.get("samplingRate").or(params.get("sampling_rate")).and_then(|v| v.as_u64()).map(|v| v as u32);
        let min_speech_ms = params.get("minSpeechDurationMs").or(params.get("min_speech_duration_ms")).and_then(|v| v.as_u64()).map(|v| v as u32);
        let min_silence_ms = params.get("minSilenceDurationMs").or(params.get("min_silence_duration_ms")).and_then(|v| v.as_u64()).map(|v| v as u32);
        let speech_pad_ms = params.get("speechPadMs").or(params.get("speech_pad_ms")).and_then(|v| v.as_u64()).map(|v| v as u32);

        let node = SileroVADNode::new(
            threshold,
            sampling_rate,
            min_speech_ms,
            min_silence_ms,
            speech_pad_ms,
        );

        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SileroVADNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Outputs 2 items: VAD event + pass-through audio
    }
}

struct LFM2AudioNodeFactory;
impl StreamingNodeFactory for LFM2AudioNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "LFM2AudioNode", params)?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "LFM2AudioNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }
}

struct AudioChunkerNodeFactory;
impl StreamingNodeFactory for AudioChunkerNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::AudioChunkerNode;

        let chunk_size = params.get("chunkSize")
            .or(params.get("chunk_size"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let node = AudioChunkerNode::new(chunk_size);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "AudioChunkerNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Can output 0 or multiple chunks per input
    }
}

// Test node factories for Python streaming nodes
struct ExpanderNodeFactory;
impl StreamingNodeFactory for ExpanderNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "ExpanderNode", params)?;
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
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "RangeGeneratorNode", params)?;
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
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "TransformAndExpandNode", params)?;
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
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "ChainedTransformNode", params)?;
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
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "ConditionalExpanderNode", params)?;
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
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let node = PythonStreamingNode::new(node_id, "FilterNode", params)?;
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
    registry.register(Arc::new(VideoProcessorNodeFactory));
    registry.register(Arc::new(SynchronizedAudioVideoNodeFactory));
    registry.register(Arc::new(PassThroughNodeFactory));

    // Register audio processing nodes
    registry.register(Arc::new(AudioChunkerNodeFactory));
    registry.register(Arc::new(AudioBufferAccumulatorNodeFactory));

    // Register Silero VAD node (Rust ONNX)
    #[cfg(feature = "silero-vad")]
    registry.register(Arc::new(SileroVADNodeFactory));

    // Register Python TTS nodes
    registry.register(Arc::new(KokoroTTSNodeFactory));

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
