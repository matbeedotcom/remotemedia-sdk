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
use tracing::{debug, info, warn};

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
struct WhisperModelState {
    /// Candle device
    #[cfg(feature = "whisper")]
    candle_device: candle_core::Device,
    /// Model weights loaded flag
    weights_loaded: bool,
    /// Model ID for reference
    model_id: String,
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
        
        let _config_path = self
            .cache
            .download_model(model_id, self.config.model.config_file(), None)
            .await?;
        
        let _weights_path = self
            .cache
            .download_model(model_id, self.config.model.weights_file(), None)
            .await?;

        // Initialize candle device
        let candle_device: candle_core::Device = (&self.device).try_into()?;

        info!("Whisper model loaded successfully");

        *state = Some(WhisperModelState {
            candle_device,
            weights_loaded: true,
            model_id: model_id.to_string(),
        });

        Ok(())
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

        // TODO: Full Whisper inference implementation
        // For now, return placeholder to establish the pattern
        // The actual implementation would use candle_transformers::models::whisper
        
        let state = self.model_state.read().await;
        if state.is_none() {
            return Err(CandleNodeError::inference(&self.node_id, "Model not loaded"));
        }

        // Placeholder transcription result
        // Real implementation would:
        // 1. Convert audio to mel spectrogram
        // 2. Run encoder
        // 3. Run decoder with language tokens
        // 4. Decode token IDs to text
        
        warn!("Whisper inference not fully implemented - returning placeholder");
        Ok("[Whisper transcription placeholder]".to_string())
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
