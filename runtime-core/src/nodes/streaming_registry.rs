//! Default streaming node registry with built-in node factories

use crate::nodes::calculator::CalculatorNode;
use crate::nodes::passthrough::PassThroughNode;
use crate::nodes::python_streaming::PythonStreamingNode;
use crate::nodes::remote_pipeline::RemotePipelineNodeFactory;
use crate::nodes::whisper::RustWhisperNode;
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
use crate::nodes::{NodeContext, NodeExecutor};
use crate::Error;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

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

/// Streaming Whisper node that properly handles RuntimeData::Audio
struct WhisperStreamingNode {
    whisper: Mutex<RustWhisperNode>,
}

impl WhisperStreamingNode {
    fn new(node: RustWhisperNode) -> Self {
        Self {
            whisper: Mutex::new(node),
        }
    }
}

#[async_trait::async_trait]
impl crate::nodes::AsyncStreamingNode for WhisperStreamingNode {
    fn node_type(&self) -> &str {
        "RustWhisperNode"
    }

    async fn initialize(&self) -> Result<(), Error> {
        // Initialization happens in the factory
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let whisper = self.whisper.lock().await;

        // Extract audio samples directly from RuntimeData
        let audio_samples = match &data {
            RuntimeData::Audio { samples, sample_rate, channels, .. } => {
                tracing::debug!(
                    "WhisperStreamingNode received audio: {} samples, {}Hz, {} ch",
                    samples.len(),
                    sample_rate,
                    channels
                );
                
                // Resample to 16kHz if needed (Whisper requires 16kHz)
                let target_rate = 16000u32;
                if *sample_rate != target_rate {
                    tracing::debug!("Resampling from {}Hz to {}Hz", sample_rate, target_rate);
                    resample_audio(samples, *sample_rate, target_rate)
                } else {
                    samples.clone()
                }
            }
            RuntimeData::Json(json) => {
                // Fallback: try to extract from JSON format for backwards compatibility
                if let Some(arr) = json.as_array() {
                    if arr.len() >= 2 {
                        if let Some(audio_arr) = arr[0].as_array() {
                            audio_arr
                                .iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect()
                        } else {
                            return Err(Error::Execution(
                                "JSON input must have audio array as first element".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::Execution(
                            "JSON input must be [audio_array, sample_rate]".to_string(),
                        ));
                    }
                } else {
                    return Err(Error::Execution(format!(
                        "Expected RuntimeData::Audio or JSON array, got: {:?}",
                        json
                    )));
                }
            }
            _ => {
                return Err(Error::Execution(format!(
                    "WhisperStreamingNode expects RuntimeData::Audio, got: {}",
                    data.data_type()
                )));
            }
        };

        // Check if we have enough audio
        if audio_samples.is_empty() {
            tracing::warn!("Received empty audio, returning empty result");
            return Ok(RuntimeData::Json(serde_json::json!({
                "text": "",
                "segments": []
            })));
        }

        // Get whisper context
        let ctx = whisper.get_context().ok_or_else(|| {
            Error::Execution("Whisper model not initialized".to_string())
        })?;

        // Run transcription using rodio SamplesBuffer
        tracing::info!("Running Whisper transcription on {} samples", audio_samples.len());
        
        let ctx_guard = ctx.lock().await;
        
        // Create rodio Source from audio samples (mono, 16kHz)
        let source = rodio::buffer::SamplesBuffer::new(1, 16000, audio_samples);
        
        // Start transcription task with word-level timestamps enabled
        let mut task = ctx_guard.transcribe(source).timestamped();
        
        // Collect all segments from the stream
        use futures::StreamExt;
        let mut all_chunks: Vec<(f64, f64, String)> = Vec::new();
        let mut full_text = String::new();

        // Sample rate is 16000 Hz (Whisper requirement)
        const SAMPLE_RATE: f64 = 16000.0;
        
        // Collect word-level chunks from all segments
        while let Ok(Some(segment)) = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            task.next()
        ).await {
            let segment_range = segment.sample_range();
            let segment_start = segment_range.start as f64 / SAMPLE_RATE;
            let segment_end = segment_range.end as f64 / SAMPLE_RATE;
            
            // Try to get word-level timestamps from chunks
            let mut has_word_timestamps = false;
            for chunk in segment.chunks() {
                if let Some(ts_range) = chunk.timestamp() {
                    has_word_timestamps = true;
                    let chunk_text = chunk.text().trim();
                    if !chunk_text.is_empty() {
                        // timestamp() returns Range<f32> in seconds
                        let start = ts_range.start as f64;
                        let end = ts_range.end as f64;
                        all_chunks.push((start, end, chunk_text.to_string()));
                    }
                }
            }
            
            // Fallback: if no word-level timestamps, use the whole segment
            if !has_word_timestamps {
                let text = segment.text().trim();
                if !text.is_empty() {
                    all_chunks.push((segment_start, segment_end, text.to_string()));
                }
            }
            
            if !full_text.is_empty() {
                full_text.push(' ');
            }
            full_text.push_str(segment.text().trim());
        }

        // Group chunks into subtitle segments (target: 5-7 seconds, max 10 seconds)
        let segments = group_into_subtitles(&all_chunks, 5.0, 10.0);
        
        tracing::info!("Transcription complete: {} subtitle segments from {} word chunks", 
            segments.len(), all_chunks.len());

        Ok(RuntimeData::Json(serde_json::json!({
            "text": full_text,
            "segments": segments
        })))
    }
}

/// Group word chunks into subtitle segments of appropriate duration
/// 
/// # Arguments
/// * `chunks` - Vec of (start_time, end_time, text) tuples
/// * `target_duration` - Target duration per subtitle (e.g., 5.0 seconds)
/// * `max_duration` - Maximum duration before forcing a break (e.g., 10.0 seconds)
fn group_into_subtitles(
    chunks: &[(f64, f64, String)],
    target_duration: f64,
    max_duration: f64,
) -> Vec<serde_json::Value> {
    if chunks.is_empty() {
        return Vec::new();
    }
    
    let mut segments = Vec::new();
    let mut current_start = chunks[0].0;
    let mut current_text = String::new();
    let mut last_end = chunks[0].0;
    
    for (start, end, text) in chunks {
        let current_duration = *end - current_start;
        let would_exceed_max = current_duration > max_duration;
        
        // Check for natural break points (sentence endings)
        let is_sentence_end = current_text.ends_with('.') 
            || current_text.ends_with('!') 
            || current_text.ends_with('?');
        
        // Break if we've hit target duration at a sentence end, or exceeded max
        let should_break = (current_duration >= target_duration && is_sentence_end) 
            || would_exceed_max;
        
        if should_break && !current_text.is_empty() {
            segments.push(serde_json::json!({
                "start": current_start,
                "end": last_end,
                "text": current_text.trim()
            }));
            current_start = *start;
            current_text = String::new();
        }
        
        // Add word to current segment
        if !current_text.is_empty() {
            current_text.push(' ');
        }
        current_text.push_str(text);
        last_end = *end;
    }
    
    // Don't forget the last segment
    if !current_text.is_empty() {
        segments.push(serde_json::json!({
            "start": current_start,
            "end": last_end,
            "text": current_text.trim()
        }));
    }
    
    segments
}

/// Simple audio resampling using linear interpolation
fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let new_len = (samples.len() as f64 / ratio) as usize;
    let mut resampled = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx = i as f64 * ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(samples.len() - 1);
        let frac = src_idx.fract() as f32;

        let sample = samples[idx0] * (1.0 - frac) + samples[idx1] * frac;
        resampled.push(sample);
    }

    resampled
}

/// Factory for RustWhisperNode (Whisper speech-to-text)
struct RustWhisperNodeFactory;

impl StreamingNodeFactory for RustWhisperNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let mut node = RustWhisperNode::new();

        // Create context for initialization
        let context = NodeContext {
            node_id: node_id.clone(),
            node_type: "RustWhisperNode".to_string(),
            params: params.clone(),
            session_id: None,
            metadata: HashMap::new(),
        };

        // Initialize synchronously using tokio runtime
        // This loads the model
        let init_result = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                node.initialize(&context).await?;
                Ok::<_, Error>(node)
            })
        })
        .join()
        .map_err(|_| Error::Execution("Whisper initialization thread panicked".to_string()))?;

        let initialized_node = init_result?;
        let streaming_node = WhisperStreamingNode::new(initialized_node);

        Ok(Box::new(AsyncNodeWrapper(Arc::new(streaming_node))))
    }

    fn node_type(&self) -> &str {
        "RustWhisperNode"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("RustWhisperNode")
                .description("Speech-to-text transcription using Whisper (Rust implementation)")
                .category("ml")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Json, RuntimeDataType::Text])
        )
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

    // Register Whisper transcription node (Rust rwhisper)
    registry.register(Arc::new(RustWhisperNodeFactory));

    // Register Speculative VAD Gate (Spec 007 - low-latency streaming)
    registry.register(Arc::new(SpeculativeVADGateFactory));

    // Register Speculative VAD Coordinator (integrates forwarding + Silero VAD)
    registry.register(Arc::new(SpeculativeVADCoordinatorFactory));

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

        if let Some(segments) = json.get("segments").and_then(|s| s.as_array()) {
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
