//! Silero VAD (Voice Activity Detection) node
//!
//! Provides voice activity detection using Silero VAD ONNX model
//! via the Candle ML framework.

mod config;

pub use config::{VadConfig, VadSampleRate};

use crate::cache::ModelCache;
use crate::convert::{AudioData, RuntimeDataConverter};
use crate::device::{DeviceSelector, InferenceDevice};
use crate::error::{CandleNodeError, Result};

use async_trait::async_trait;
use remotemedia_core::capabilities::{
    AudioConstraints, CapabilityBehavior, ConstraintValue, MediaCapabilities, MediaConstraints,
};
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::streaming_node::{
    AsyncStreamingNode, StreamingNode, StreamingNodeFactory,
};
use remotemedia_core::Error;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[cfg(feature = "vad")]
use candle_core::{DType, Device, Tensor};

/// Silero VAD node for voice activity detection
pub struct SileroVadNode {
    /// Node identifier
    node_id: String,
    /// Node configuration
    config: VadConfig,
    /// Selected inference device
    device: InferenceDevice,
    /// Model cache
    cache: ModelCache,
    /// Loaded model state (lazy initialization)
    model_state: RwLock<Option<VadModelState>>,
}

/// Internal model state after loading
#[cfg(feature = "vad")]
struct VadModelState {
    /// Candle device
    candle_device: Device,
    /// ONNX model
    model: candle_onnx::onnx::ModelProto,
    /// Sample rate tensor
    sample_rate_tensor: Tensor,
    /// Hidden state (2, 1, 128)
    state: Tensor,
    /// Context buffer for overlapping frames
    context: Tensor,
    /// Frame size in samples
    frame_size: usize,
    /// Context size in samples
    context_size: usize,
}

#[cfg(not(feature = "vad"))]
struct VadModelState {
    _placeholder: (),
}

/// Speech segment detected by VAD
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpeechSegment {
    /// Start time in milliseconds
    pub start_ms: u64,
    /// End time in milliseconds
    pub end_ms: u64,
    /// Average speech probability for the segment
    pub probability: f32,
}

/// VAD output - either raw probabilities or detected segments
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum VadOutput {
    /// Raw speech probability (0.0 - 1.0)
    Probability(f32),
    /// Detected speech segments
    Segments(Vec<SpeechSegment>),
}

impl SileroVadNode {
    /// Create a new Silero VAD node
    pub fn new(node_id: impl Into<String>, config: &VadConfig) -> Result<Self> {
        config.validate().map_err(|e| {
            CandleNodeError::configuration("silero-vad", e)
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
        let config = VadConfig::from_json(params).map_err(|e| {
            CandleNodeError::configuration("silero-vad", e.to_string())
        })?;
        Self::new(node_id, &config)
    }

    /// Load ONNX model
    #[cfg(feature = "vad")]
    async fn load_model(&self) -> Result<()> {
        let mut state = self.model_state.write().await;
        
        if state.is_some() {
            return Ok(());
        }

        info!(
            "Loading Silero VAD model with sample rate {}",
            self.config.sample_rate
        );

        // Download model file if not cached
        let model_path = self
            .cache
            .download_model("onnx-community/silero-vad", "onnx/model.onnx", None)
            .await?;

        // Initialize candle device
        let candle_device: Device = (&self.device).try_into()?;

        // Load ONNX model
        info!("Loading ONNX model from {:?}", model_path);
        let model = candle_onnx::read_file(&model_path)
            .map_err(|e| CandleNodeError::model_load("silero-vad", e.to_string()))?;

        // Initialize state tensors
        let frame_size = self.config.sample_rate.frame_size();
        let context_size = self.config.sample_rate.context_size();
        let sample_rate = self.config.sample_rate.hz() as i64;

        let sample_rate_tensor = Tensor::new(sample_rate, &candle_device)
            .map_err(|e| CandleNodeError::model_load("sample_rate", e.to_string()))?;

        let hidden_state = Tensor::zeros((2, 1, 128), DType::F32, &candle_device)
            .map_err(|e| CandleNodeError::model_load("hidden_state", e.to_string()))?;

        let context = Tensor::zeros((1, context_size), DType::F32, &candle_device)
            .map_err(|e| CandleNodeError::model_load("context", e.to_string()))?;

        info!("Silero VAD model loaded successfully");

        *state = Some(VadModelState {
            candle_device,
            model,
            sample_rate_tensor,
            state: hidden_state,
            context,
            frame_size,
            context_size,
        });

        Ok(())
    }

    /// Reset VAD state (call between separate audio streams)
    #[cfg(feature = "vad")]
    pub async fn reset_state(&self) -> Result<()> {
        let mut state = self.model_state.write().await;
        
        if let Some(ref mut s) = *state {
            s.state = Tensor::zeros((2, 1, 128), DType::F32, &s.candle_device)
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;
            s.context = Tensor::zeros((1, s.context_size), DType::F32, &s.candle_device)
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;
        }

        Ok(())
    }

    /// Process a single audio chunk and return speech probability
    #[cfg(feature = "vad")]
    async fn process_chunk(&self, samples: &[f32]) -> Result<f32> {
        let mut state = self.model_state.write().await;
        let state = state.as_mut()
            .ok_or_else(|| CandleNodeError::inference(&self.node_id, "Model not loaded"))?;

        if samples.len() < state.frame_size {
            return Err(CandleNodeError::inference(
                &self.node_id,
                format!("Chunk too small: {} < {}", samples.len(), state.frame_size),
            ));
        }

        // Extract next context from end of current chunk
        let next_context_start = state.frame_size - state.context_size;
        let next_context = Tensor::from_slice(
            &samples[next_context_start..state.frame_size],
            (1, state.context_size),
            &state.candle_device,
        ).map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

        // Create input tensor from samples
        let chunk = Tensor::from_slice(
            &samples[..state.frame_size],
            (1, state.frame_size),
            &state.candle_device,
        ).map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

        // Concatenate context with chunk
        let input = Tensor::cat(&[&state.context, &chunk], 1)
            .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

        // Prepare inputs for ONNX model
        let inputs = HashMap::from_iter([
            ("input".to_string(), input),
            ("sr".to_string(), state.sample_rate_tensor.clone()),
            ("state".to_string(), state.state.clone()),
        ]);

        // Run inference
        let outputs = candle_onnx::simple_eval(&state.model, inputs)
            .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

        // Get output names from model graph
        let out_names = &state.model.graph.as_ref()
            .ok_or_else(|| CandleNodeError::inference(&self.node_id, "No graph in model"))?
            .output;

        // Extract output probability
        let output = outputs.get(&out_names[0].name)
            .ok_or_else(|| CandleNodeError::inference(&self.node_id, "Missing output tensor"))?;
        
        // Update hidden state
        state.state = outputs.get(&out_names[1].name)
            .ok_or_else(|| CandleNodeError::inference(&self.node_id, "Missing state tensor"))?
            .clone();

        // Update context for next iteration
        state.context = next_context;

        // Extract probability value
        let prob_vec: Vec<f32> = output.flatten_all()
            .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?
            .to_vec1()
            .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

        Ok(prob_vec.first().copied().unwrap_or(0.0))
    }

    /// Process audio and detect speech
    #[cfg(feature = "vad")]
    async fn detect_speech(&self, audio: AudioData) -> Result<VadOutput> {
        // Ensure model is loaded
        self.load_model().await?;

        // Resample audio if needed
        let target_rate = self.config.sample_rate.hz();
        let resampled = if audio.sample_rate != target_rate {
            debug!("Resampling from {} to {} Hz", audio.sample_rate, target_rate);
            audio.resample(target_rate)?
        } else {
            audio.clone()
        };

        // Convert to mono if needed
        let mono = if resampled.channels > 1 {
            resampled.to_mono()
        } else {
            resampled
        };
        let samples = mono.samples;

        let frame_size = self.config.sample_rate.frame_size();
        let mut probabilities = Vec::new();

        // Process audio in frames
        for chunk_start in (0..samples.len()).step_by(frame_size) {
            let chunk_end = (chunk_start + frame_size).min(samples.len());
            if chunk_end - chunk_start < frame_size {
                break; // Skip incomplete final frame
            }

            let prob = self.process_chunk(&samples[chunk_start..chunk_end]).await?;
            probabilities.push(prob);
        }

        if self.config.output_segments {
            // Convert probabilities to speech segments
            let segments = self.probabilities_to_segments(&probabilities);
            Ok(VadOutput::Segments(segments))
        } else {
            // Return average probability
            let avg = if probabilities.is_empty() {
                0.0
            } else {
                probabilities.iter().sum::<f32>() / probabilities.len() as f32
            };
            Ok(VadOutput::Probability(avg))
        }
    }

    /// Convert frame probabilities to speech segments
    #[cfg(feature = "vad")]
    fn probabilities_to_segments(&self, probabilities: &[f32]) -> Vec<SpeechSegment> {
        let frame_size = self.config.sample_rate.frame_size();
        let sample_rate = self.config.sample_rate.hz();
        let frame_duration_ms = (frame_size as f32 / sample_rate as f32 * 1000.0) as u64;
        
        let threshold = self.config.threshold;
        let min_speech_frames = (self.config.min_speech_duration_ms as f32 / frame_duration_ms as f32).ceil() as usize;
        let min_silence_frames = (self.config.min_silence_duration_ms as f32 / frame_duration_ms as f32).ceil() as usize;

        let mut segments = Vec::new();
        let mut in_speech = false;
        let mut speech_start = 0u64;
        let mut silence_count = 0usize;
        let mut speech_probs = Vec::new();

        for (i, &prob) in probabilities.iter().enumerate() {
            let time_ms = i as u64 * frame_duration_ms;

            if prob >= threshold {
                if !in_speech {
                    in_speech = true;
                    speech_start = time_ms;
                    speech_probs.clear();
                }
                silence_count = 0;
                speech_probs.push(prob);
            } else if in_speech {
                silence_count += 1;
                if silence_count >= min_silence_frames {
                    // End of speech segment
                    let speech_duration_frames = speech_probs.len();
                    if speech_duration_frames >= min_speech_frames {
                        let avg_prob = speech_probs.iter().sum::<f32>() / speech_probs.len() as f32;
                        segments.push(SpeechSegment {
                            start_ms: speech_start,
                            end_ms: time_ms - (silence_count as u64 - 1) * frame_duration_ms,
                            probability: avg_prob,
                        });
                    }
                    in_speech = false;
                }
            }
        }

        // Handle speech at end of audio
        if in_speech && speech_probs.len() >= min_speech_frames {
            let avg_prob = speech_probs.iter().sum::<f32>() / speech_probs.len() as f32;
            let end_ms = probabilities.len() as u64 * frame_duration_ms;
            segments.push(SpeechSegment {
                start_ms: speech_start,
                end_ms,
                probability: avg_prob,
            });
        }

        segments
    }

    #[cfg(not(feature = "vad"))]
    async fn detect_speech(&self, _audio: AudioData) -> Result<VadOutput> {
        Err(CandleNodeError::configuration(
            "silero-vad",
            "VAD feature not enabled at compile time",
        ))
    }

    #[cfg(not(feature = "vad"))]
    async fn load_model(&self) -> Result<()> {
        Err(CandleNodeError::configuration(
            "silero-vad",
            "VAD feature not enabled at compile time",
        ))
    }
}

#[async_trait]
impl AsyncStreamingNode for SileroVadNode {
    fn node_type(&self) -> &str {
        "silero-vad"
    }

    async fn initialize(&self) -> std::result::Result<(), Error> {
        self.load_model()
            .await
            .map_err(|e| Error::Execution(e.to_string()))
    }

    async fn process(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        // Extract audio from input
        let audio = RuntimeDataConverter::extract_audio(&data, &self.node_id)
            .map_err(|e| Error::Execution(e.to_string()))?;

        // Run VAD
        let output = self
            .detect_speech(audio)
            .await
            .map_err(|e| Error::Execution(e.to_string()))?;

        // Return as JSON
        let json = serde_json::to_value(&output)
            .map_err(|e| Error::Execution(e.to_string()))?;

        Ok(RuntimeData::Json(json))
    }
}

/// Wrapper to make SileroVadNode a StreamingNode
pub struct SileroVadNodeWrapper(pub Arc<SileroVadNode>);

#[async_trait]
impl StreamingNode for SileroVadNodeWrapper {
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
        let sample_rate = self.0.config.sample_rate.hz();
        Some(MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(sample_rate)),
                channels: Some(ConstraintValue::Exact(1)),
                ..Default::default()
            },
        )))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Static
    }
}

/// Factory for creating SileroVadNode instances
pub struct SileroVadNodeFactory;

impl SileroVadNodeFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SileroVadNodeFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingNodeFactory for SileroVadNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> std::result::Result<Box<dyn StreamingNode>, Error> {
        let node = SileroVadNode::from_params(node_id, params)
            .map_err(|e| Error::Execution(e.to_string()))?;
        Ok(Box::new(SileroVadNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "silero-vad"
    }

    fn media_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
        let config = VadConfig::from_json(params).ok()?;
        let sample_rate = config.sample_rate.hz();
        Some(MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(sample_rate)),
                channels: Some(ConstraintValue::Exact(1)),
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
    fn test_vad_config_default() {
        let config = VadConfig::default();
        assert_eq!(config.sample_rate, VadSampleRate::Sr16k);
        assert_eq!(config.threshold, 0.5);
    }

    #[test]
    fn test_vad_node_creation() {
        let config = VadConfig::default();
        let node = SileroVadNode::new("test-vad", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_factory_node_type() {
        let factory = SileroVadNodeFactory::new();
        assert_eq!(factory.node_type(), "silero-vad");
    }
}
