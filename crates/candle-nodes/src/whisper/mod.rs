//! Whisper speech-to-text node
//!
//! Provides speech-to-text transcription using OpenAI's Whisper models
//! via the Candle ML framework.

mod config;

pub use config::{WhisperConfig, WhisperModel};

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
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[cfg(feature = "whisper")]
use candle_core::{Device, IndexOp, Tensor};
#[cfg(feature = "whisper")]
use candle_nn::VarBuilder;
#[cfg(feature = "whisper")]
use candle_transformers::models::whisper::{self as m, audio, Config};
#[cfg(feature = "whisper")]
use tokenizers::Tokenizer;

/// Whisper speech-to-text node
pub struct WhisperNode {
    /// Node identifier
    node_id: String,
    /// Node configuration
    config: WhisperConfig,
    /// Selected inference device
    device: InferenceDevice,
    /// Model cache
    cache: ModelCache,
    /// Loaded model state (lazy initialization)
    model_state: RwLock<Option<WhisperModelState>>,
}

/// Internal model state after loading
#[cfg(feature = "whisper")]
struct WhisperModelState {
    /// Candle device
    candle_device: Device,
    /// Whisper model
    model: m::model::Whisper,
    /// Tokenizer
    tokenizer: Tokenizer,
    /// Model config
    config: Config,
    /// Mel filters for audio processing
    mel_filters: Vec<f32>,
}

#[cfg(not(feature = "whisper"))]
struct WhisperModelState {
    _placeholder: (),
}

impl WhisperNode {
    /// Create a new Whisper node
    pub fn new(node_id: impl Into<String>, config: &WhisperConfig) -> Result<Self> {
        config.validate().map_err(|e| {
            CandleNodeError::configuration("candle-whisper", e)
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
        let config = WhisperConfig::from_json(params).map_err(|e| {
            CandleNodeError::configuration("candle-whisper", e.to_string())
        })?;
        Self::new(node_id, &config)
    }

    /// Load model weights
    #[cfg(feature = "whisper")]
    async fn load_model(&self) -> Result<()> {
        let mut state = self.model_state.write().await;
        
        if state.is_some() {
            return Ok(());
        }

        info!(
            "Loading Whisper model: {} on {}",
            self.config.model,
            self.device
        );

        // Download model files if not cached
        let model_id = self.config.model.model_id();
        
        let config_path = self
            .cache
            .download_model(model_id, "config.json", None)
            .await?;
        
        let tokenizer_path = self
            .cache
            .download_model(model_id, "tokenizer.json", None)
            .await?;
        
        let weights_path = self
            .cache
            .download_model(model_id, "model.safetensors", None)
            .await?;

        // Initialize candle device
        let candle_device: Device = (&self.device).try_into()?;

        // Load config
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| CandleNodeError::model_load(config_path.display().to_string(), e.to_string()))?;
        let config: Config = serde_json::from_str(&config_str)
            .map_err(|e| CandleNodeError::model_load("config.json", e.to_string()))?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| CandleNodeError::model_load(tokenizer_path.display().to_string(), e.to_string()))?;

        // Load mel filters (80 bins for most models)
        let mel_filters = Self::get_mel_filters(config.num_mel_bins)?;

        // Load model weights
        info!("Loading model weights from {:?}", weights_path);
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], m::DTYPE, &candle_device)
                .map_err(|e| CandleNodeError::model_load("weights", e.to_string()))?
        };
        
        let model = m::model::Whisper::load(&vb, config.clone())
            .map_err(|e| CandleNodeError::model_load("whisper", e.to_string()))?;

        info!("Whisper model loaded successfully");

        *state = Some(WhisperModelState {
            candle_device,
            model,
            tokenizer,
            config,
            mel_filters,
        });

        Ok(())
    }

    /// Get mel filter bank coefficients
    #[cfg(feature = "whisper")]
    fn get_mel_filters(num_mel_bins: usize) -> Result<Vec<f32>> {
        // These are precomputed mel filter coefficients for Whisper
        // For 80 mel bins (most models)
        let mel_bytes: &[u8] = match num_mel_bins {
            80 => include_bytes!("mel_filters_80.bin"),
            128 => include_bytes!("mel_filters_128.bin"),
            _ => return Err(CandleNodeError::configuration(
                "whisper",
                format!("Unsupported num_mel_bins: {}", num_mel_bins),
            )),
        };
        
        let mut mel_filters = vec![0f32; mel_bytes.len() / 4];
        for (i, chunk) in mel_bytes.chunks(4).enumerate() {
            if chunk.len() == 4 {
                mel_filters[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
        }
        Ok(mel_filters)
    }

    /// Transcribe audio data
    #[cfg(feature = "whisper")]
    async fn transcribe(&self, audio: AudioData) -> Result<String> {
        // Ensure model is loaded
        self.load_model().await?;

        // Prepare audio (resample to 16kHz mono)
        let prepared = audio.prepare_for_whisper()?;

        debug!(
            "Transcribing {} samples at {}Hz",
            prepared.samples.len(),
            prepared.sample_rate
        );

        // Use write lock since model inference requires mutable state
        let mut state = self.model_state.write().await;
        let state = state.as_mut()
            .ok_or_else(|| CandleNodeError::inference(&self.node_id, "Model not loaded"))?;

        // Convert audio to mel spectrogram
        let mel = audio::pcm_to_mel(&state.config, &prepared.samples, &state.mel_filters);
        let mel_len = mel.len();
        let mel = Tensor::from_vec(
            mel,
            (1, state.config.num_mel_bins, mel_len / state.config.num_mel_bins),
            &state.candle_device,
        ).map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

        debug!("Mel spectrogram shape: {:?}", mel.dims());

        // Run encoder
        let audio_features = state.model.encoder.forward(&mel, true)
            .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

        // Get special tokens
        let sot_token = self.token_id(&state.tokenizer, m::SOT_TOKEN)?;
        let transcribe_token = self.token_id(&state.tokenizer, m::TRANSCRIBE_TOKEN)?;
        let eot_token = self.token_id(&state.tokenizer, m::EOT_TOKEN)?;
        let no_timestamps_token = self.token_id(&state.tokenizer, m::NO_TIMESTAMPS_TOKEN)?;

        // Get language token if multilingual
        let language_token = if self.config.model.is_multilingual() {
            let lang_str = format!("<|{}|>", self.config.language);
            self.token_id(&state.tokenizer, &lang_str).ok()
        } else {
            None
        };

        // Build initial tokens
        let mut tokens = vec![sot_token];
        if let Some(lang_token) = language_token {
            tokens.push(lang_token);
        }
        tokens.push(transcribe_token);
        tokens.push(no_timestamps_token);

        // Decode loop
        let sample_len = state.config.max_target_positions / 2;
        for i in 0..sample_len {
            let tokens_t = Tensor::new(tokens.as_slice(), &state.candle_device)
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?
                .unsqueeze(0)
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

            let ys = state.model.decoder.forward(&tokens_t, &audio_features, i == 0)
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

            let (_, seq_len, _) = ys.dims3()
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;
            
            let logits = state.model.decoder.final_linear(&ys.i((..1, seq_len - 1..))
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?)
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?
                .i(0)
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?
                .i(0)
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

            // Greedy decoding - take argmax
            let logits_v: Vec<f32> = logits.to_vec1()
                .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;
            let next_token = logits_v
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(i, _)| i as u32)
                .unwrap_or(eot_token);

            if next_token == eot_token || tokens.len() > state.config.max_target_positions {
                break;
            }
            tokens.push(next_token);
        }

        // Decode tokens to text
        let text = state.tokenizer.decode(&tokens, true)
            .map_err(|e| CandleNodeError::inference(&self.node_id, e.to_string()))?;

        Ok(text)
    }

    /// Get token ID from tokenizer
    #[cfg(feature = "whisper")]
    fn token_id(&self, tokenizer: &Tokenizer, token: &str) -> Result<u32> {
        tokenizer.token_to_id(token)
            .ok_or_else(|| CandleNodeError::inference(&self.node_id, format!("Token not found: {}", token)))
    }

    #[cfg(not(feature = "whisper"))]
    async fn transcribe(&self, _audio: AudioData) -> Result<String> {
        Err(CandleNodeError::configuration(
            "candle-whisper",
            "Whisper feature not enabled at compile time",
        ))
    }

    #[cfg(not(feature = "whisper"))]
    async fn load_model(&self) -> Result<()> {
        Err(CandleNodeError::configuration(
            "candle-whisper",
            "Whisper feature not enabled at compile time",
        ))
    }
}

#[async_trait]
impl AsyncStreamingNode for WhisperNode {
    fn node_type(&self) -> &str {
        "candle-whisper"
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

        // Transcribe
        let transcription = self
            .transcribe(audio)
            .await
            .map_err(|e| Error::Execution(e.to_string()))?;

        Ok(RuntimeData::Text(transcription))
    }
}

/// Wrapper to make WhisperNode a StreamingNode
pub struct WhisperNodeWrapper(pub Arc<WhisperNode>);

#[async_trait]
impl StreamingNode for WhisperNodeWrapper {
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
        Some(MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(16000)),
                channels: Some(ConstraintValue::Exact(1)),
                ..Default::default()
            },
        )))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Static
    }
}

/// Factory for creating WhisperNode instances
pub struct WhisperNodeFactory;

impl WhisperNodeFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WhisperNodeFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingNodeFactory for WhisperNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> std::result::Result<Box<dyn StreamingNode>, Error> {
        let node = WhisperNode::from_params(node_id, params)
            .map_err(|e| Error::Execution(e.to_string()))?;
        Ok(Box::new(WhisperNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "candle-whisper"
    }

    fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
        Some(MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(16000)),
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
    fn test_whisper_config_default() {
        let config = WhisperConfig::default();
        assert_eq!(config.model, WhisperModel::Base);
    }

    #[test]
    fn test_whisper_node_creation() {
        let config = WhisperConfig::default();
        let node = WhisperNode::new("test-whisper", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_factory_node_type() {
        let factory = WhisperNodeFactory::new();
        assert_eq!(factory.node_type(), "candle-whisper");
    }
}
