//! LLM text generation nodes
//!
//! Provides text generation using Phi and LLaMA models
//! via the Candle ML framework.

mod config;
pub mod sampling;

pub use config::{
    GenerationConfig, LlmConfig, LlamaConfig, LlamaModel, PhiConfig, PhiModel, Quantization,
};
pub use sampling::Sampler;

use crate::cache::ModelCache;
use crate::convert::RuntimeDataConverter;
use crate::device::{DeviceSelector, InferenceDevice};
use crate::error::{CandleNodeError, Result};

use async_trait::async_trait;
use remotemedia_core::capabilities::CapabilityBehavior;
use remotemedia_core::data_compat::RuntimeData;
use remotemedia_core::nodes::streaming_node::{
    AsyncStreamingNode, StreamingNode, StreamingNodeFactory,
};
use remotemedia_core::Error;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Phi LLM text generation node
pub struct PhiNode {
    node_id: String,
    config: PhiConfig,
    device: InferenceDevice,
    cache: ModelCache,
    model_state: RwLock<Option<LlmModelState>>,
}

/// LLaMA LLM text generation node
pub struct LlamaNode {
    node_id: String,
    config: LlamaConfig,
    device: InferenceDevice,
    cache: ModelCache,
    model_state: RwLock<Option<LlmModelState>>,
}

/// Internal model state after loading
struct LlmModelState {
    #[cfg(feature = "llm")]
    candle_device: candle_core::Device,
    weights_loaded: bool,
    model_id: String,
}

impl PhiNode {
    pub fn new(node_id: impl Into<String>, config: &PhiConfig) -> Result<Self> {
        config.validate().map_err(|e| {
            CandleNodeError::configuration("candle-phi", e)
        })?;

        let device = DeviceSelector::from_config(&config.llm.device)?;

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            device,
            cache: ModelCache::new(),
            model_state: RwLock::new(None),
        })
    }

    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self> {
        let config = PhiConfig::from_json(params).map_err(|e| {
            CandleNodeError::configuration("candle-phi", e.to_string())
        })?;
        Self::new(node_id, &config)
    }

    #[cfg(feature = "llm")]
    async fn load_model(&self) -> Result<()> {
        let mut state = self.model_state.write().await;
        
        if state.is_some() {
            return Ok(());
        }

        info!("Loading Phi model: {} on {}", self.config.model, self.device);

        let model_id = self.config.model.model_id();
        
        let _weights_path = self
            .cache
            .download_model(model_id, self.config.model.weights_file(), None)
            .await?;

        let _tokenizer_path = self
            .cache
            .download_model(model_id, self.config.model.tokenizer_file(), None)
            .await?;

        let candle_device: candle_core::Device = (&self.device).try_into()?;

        info!("Phi model loaded successfully");

        *state = Some(LlmModelState {
            candle_device,
            weights_loaded: true,
            model_id: model_id.to_string(),
        });

        Ok(())
    }

    #[cfg(not(feature = "llm"))]
    async fn load_model(&self) -> Result<()> {
        Err(CandleNodeError::configuration(
            "candle-phi",
            "LLM feature not enabled at compile time",
        ))
    }

    #[cfg(feature = "llm")]
    async fn generate(&self, prompt: String) -> Result<String> {
        self.load_model().await?;

        debug!("Generating text for prompt: {}...", &prompt[..prompt.len().min(50)]);

        let state = self.model_state.read().await;
        if state.is_none() {
            return Err(CandleNodeError::inference(&self.node_id, "Model not loaded"));
        }

        // TODO: Full Phi inference implementation
        warn!("Phi inference not fully implemented - returning placeholder");
        Ok(format!("[Phi response to: {}]", &prompt[..prompt.len().min(30)]))
    }

    #[cfg(not(feature = "llm"))]
    async fn generate(&self, _prompt: String) -> Result<String> {
        Err(CandleNodeError::configuration(
            "candle-phi",
            "LLM feature not enabled at compile time",
        ))
    }
}

impl LlamaNode {
    pub fn new(node_id: impl Into<String>, config: &LlamaConfig) -> Result<Self> {
        config.validate().map_err(|e| {
            CandleNodeError::configuration("candle-llama", e)
        })?;

        let device = DeviceSelector::from_config(&config.llm.device)?;

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            device,
            cache: ModelCache::new(),
            model_state: RwLock::new(None),
        })
    }

    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self> {
        let config = LlamaConfig::from_json(params).map_err(|e| {
            CandleNodeError::configuration("candle-llama", e.to_string())
        })?;
        Self::new(node_id, &config)
    }

    #[cfg(feature = "llm")]
    async fn load_model(&self) -> Result<()> {
        let mut state = self.model_state.write().await;
        
        if state.is_some() {
            return Ok(());
        }

        info!("Loading LLaMA model: {} on {}", self.config.model, self.device);

        let model_id = self.config.model.model_id();
        
        let _weights_path = self
            .cache
            .download_model(model_id, self.config.model.weights_file(), None)
            .await?;

        let _tokenizer_path = self
            .cache
            .download_model(model_id, self.config.model.tokenizer_file(), None)
            .await?;

        let candle_device: candle_core::Device = (&self.device).try_into()?;

        info!("LLaMA model loaded successfully");

        *state = Some(LlmModelState {
            candle_device,
            weights_loaded: true,
            model_id: model_id.to_string(),
        });

        Ok(())
    }

    #[cfg(not(feature = "llm"))]
    async fn load_model(&self) -> Result<()> {
        Err(CandleNodeError::configuration(
            "candle-llama",
            "LLM feature not enabled at compile time",
        ))
    }

    #[cfg(feature = "llm")]
    async fn generate(&self, prompt: String) -> Result<String> {
        self.load_model().await?;

        debug!("Generating text for prompt: {}...", &prompt[..prompt.len().min(50)]);

        let state = self.model_state.read().await;
        if state.is_none() {
            return Err(CandleNodeError::inference(&self.node_id, "Model not loaded"));
        }

        // TODO: Full LLaMA inference implementation
        warn!("LLaMA inference not fully implemented - returning placeholder");
        Ok(format!("[LLaMA response to: {}]", &prompt[..prompt.len().min(30)]))
    }

    #[cfg(not(feature = "llm"))]
    async fn generate(&self, _prompt: String) -> Result<String> {
        Err(CandleNodeError::configuration(
            "candle-llama",
            "LLM feature not enabled at compile time",
        ))
    }
}

#[async_trait]
impl AsyncStreamingNode for PhiNode {
    fn node_type(&self) -> &str {
        "candle-phi"
    }

    async fn initialize(&self) -> std::result::Result<(), Error> {
        self.load_model()
            .await
            .map_err(|e| Error::Execution(e.to_string()))
    }

    async fn process(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        let prompt = RuntimeDataConverter::extract_text(&data, &self.node_id)
            .map_err(|e| Error::Execution(e.to_string()))?;

        let response = self
            .generate(prompt)
            .await
            .map_err(|e| Error::Execution(e.to_string()))?;

        Ok(RuntimeData::Text(response))
    }
}

#[async_trait]
impl AsyncStreamingNode for LlamaNode {
    fn node_type(&self) -> &str {
        "candle-llama"
    }

    async fn initialize(&self) -> std::result::Result<(), Error> {
        self.load_model()
            .await
            .map_err(|e| Error::Execution(e.to_string()))
    }

    async fn process(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        let prompt = RuntimeDataConverter::extract_text(&data, &self.node_id)
            .map_err(|e| Error::Execution(e.to_string()))?;

        let response = self
            .generate(prompt)
            .await
            .map_err(|e| Error::Execution(e.to_string()))?;

        Ok(RuntimeData::Text(response))
    }
}

/// Wrapper for PhiNode
pub struct PhiNodeWrapper(pub Arc<PhiNode>);

#[async_trait]
impl StreamingNode for PhiNodeWrapper {
    fn node_type(&self) -> &str { self.0.node_type() }
    fn node_id(&self) -> &str { &self.0.node_id }

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

    fn is_multi_input(&self) -> bool { false }
    fn capability_behavior(&self) -> CapabilityBehavior { CapabilityBehavior::Static }
}

/// Wrapper for LlamaNode
pub struct LlamaNodeWrapper(pub Arc<LlamaNode>);

#[async_trait]
impl StreamingNode for LlamaNodeWrapper {
    fn node_type(&self) -> &str { self.0.node_type() }
    fn node_id(&self) -> &str { &self.0.node_id }

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

    fn is_multi_input(&self) -> bool { false }
    fn capability_behavior(&self) -> CapabilityBehavior { CapabilityBehavior::Static }
}

/// Phi node factory
pub struct PhiNodeFactory;

impl PhiNodeFactory {
    pub fn new() -> Self { Self }
}

impl Default for PhiNodeFactory {
    fn default() -> Self { Self::new() }
}

impl StreamingNodeFactory for PhiNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> std::result::Result<Box<dyn StreamingNode>, Error> {
        let node = PhiNode::from_params(node_id, params)
            .map_err(|e| Error::Execution(e.to_string()))?;
        Ok(Box::new(PhiNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str { "candle-phi" }
    fn capability_behavior(&self) -> CapabilityBehavior { CapabilityBehavior::Static }
}

/// LLaMA node factory
pub struct LlamaNodeFactory;

impl LlamaNodeFactory {
    pub fn new() -> Self { Self }
}

impl Default for LlamaNodeFactory {
    fn default() -> Self { Self::new() }
}

impl StreamingNodeFactory for LlamaNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> std::result::Result<Box<dyn StreamingNode>, Error> {
        let node = LlamaNode::from_params(node_id, params)
            .map_err(|e| Error::Execution(e.to_string()))?;
        Ok(Box::new(LlamaNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str { "candle-llama" }
    fn capability_behavior(&self) -> CapabilityBehavior { CapabilityBehavior::Static }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phi_node_creation() {
        let config = PhiConfig::default();
        let node = PhiNode::new("test-phi", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_llama_node_creation() {
        let config = LlamaConfig::default();
        let node = LlamaNode::new("test-llama", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_phi_factory() {
        let factory = PhiNodeFactory::new();
        assert_eq!(factory.node_type(), "candle-phi");
    }

    #[test]
    fn test_llama_factory() {
        let factory = LlamaNodeFactory::new();
        assert_eq!(factory.node_type(), "candle-llama");
    }
}
