//! LlamaCppEmbeddingNode — text embeddings via llama.cpp
//!
//! Accepts `RuntimeData::Text` and emits `RuntimeData::Tensor`
//! containing the dense embedding vector.
//!
//! Runs inference on a blocking thread (llama.cpp types are not Send).

use crate::data::RuntimeData;
use crate::error::Error;
use crate::nodes::streaming_node::{
    AsyncStreamingNode, InitializeContext, StreamingNode, StreamingNodeFactory,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use super::config::{EmbeddingPooling, LlamaCppEmbeddingConfig};

/// Llama.cpp embedding node.
pub struct LlamaCppEmbeddingNode {
    node_id: String,
    config: LlamaCppEmbeddingConfig,
    initialized: RwLock<bool>,
}

impl LlamaCppEmbeddingNode {
    /// Create a new embedding node.
    pub fn new(node_id: impl Into<String>, config: &LlamaCppEmbeddingConfig) -> Result<Self, Error> {
        config.validate().map_err(|e| Error::Execution(format!("Invalid config: {}", e)))?;

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            initialized: RwLock::new(false),
        })
    }

    /// Create from JSON parameters.
    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self, Error> {
        let config: LlamaCppEmbeddingConfig = serde_json::from_value(params.clone())
            .map_err(|e| Error::Execution(format!("Invalid config JSON: {}", e)))?;
        Self::new(node_id, &config)
    }

    /// Compute embedding for a text string.
    #[cfg(feature = "llama-cpp")]
    async fn embed(&self, text: &str) -> Result<(Vec<f32>, usize), Error> {
        let config = self.config.clone();
        let text = text.to_string();

        let result = tokio::task::spawn_blocking(move || {
            super::inference::run_embedding(
                &config.model_path,
                &text,
                config.context_size,
                config.batch_size,
                config.backend.gpu_offload,
                config.backend.flash_attention,
                config.backend.threads,
            )
        })
        .await
        .map_err(|e| Error::Execution(format!("Task join failed: {}", e)))??;

        Ok((result.embedding, result.hidden_size))
    }

    #[cfg(not(feature = "llama-cpp"))]
    async fn embed(&self, _text: &str) -> Result<(Vec<f32>, usize), Error> {
        Ok((vec![0.0; 768], 768))
    }

    /// L2-normalize a vector.
    fn l2_normalize(&self, vector: &mut Vec<f32>) {
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vector.iter_mut() {
                *v /= norm;
            }
        }
    }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for LlamaCppEmbeddingNode {
    fn node_type(&self) -> &str {
        "LlamaCppEmbeddingNode"
    }

    async fn initialize(&self, ctx: &InitializeContext) -> Result<(), Error> {
        info!(
            node = "llama-cpp-embedding",
            model = %self.config.model_path,
            "Initializing LlamaCppEmbeddingNode"
        );

        ctx.emit_progress(
            "loading_model",
            &format!("Loading embedding model: {}", self.config.model_path),
        );

        *self.initialized.write().await = true;
        ctx.emit_progress("ready", "LlamaCppEmbeddingNode ready");
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let text = match &data {
            RuntimeData::Text(text) => text.clone(),
            RuntimeData::Json(value) => value
                .get("text")
                .or(value.get("prompt"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.to_string()),
            other => {
                return Err(Error::Execution(format!(
                    "LlamaCppEmbeddingNode accepts Text or Json, got {}",
                    other.data_type()
                )));
            }
        };

        let (mut embedding, hidden_size) = self.embed(&text).await?;

        if self.config.l2_normalize {
            self.l2_normalize(&mut embedding);
        }

        let tensor_data: Vec<u8> = embedding.iter().flat_map(|&x| x.to_le_bytes()).collect();

        Ok(RuntimeData::Tensor {
            data: tensor_data,
            shape: vec![hidden_size as i32],
            dtype: 0, // float32
            metadata: Some(serde_json::json!({
                "model": self.config.model_path,
                "pooling": format!("{:?}", self.config.pooling),
                "normalized": self.config.l2_normalize,
            })),
        })
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        _session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        let output = self.process(data).await?;
        callback(output)?;
        Ok(1)
    }

    async fn process_control_message(
        &self,
        _message: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<bool, Error> {
        Ok(false)
    }
}

/// Wrapper for StreamingNode trait.
pub struct LlamaCppEmbeddingNodeWrapper(pub Arc<LlamaCppEmbeddingNode>);

#[async_trait::async_trait]
impl StreamingNode for LlamaCppEmbeddingNodeWrapper {
    fn node_type(&self) -> &str {
        self.0.node_type()
    }

    fn node_id(&self) -> &str {
        &self.0.node_id
    }

    async fn initialize(&self, ctx: &InitializeContext) -> Result<(), Error> {
        AsyncStreamingNode::initialize(self.0.as_ref(), ctx).await
    }

    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        self.0.process(data).await
    }

    async fn process_multi_async(
        &self,
        inputs: std::collections::HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
        if let Some((_, data)) = inputs.into_iter().next() {
            self.process_async(data).await
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    fn is_multi_input(&self) -> bool {
        false
    }

    fn capability_behavior(&self) -> crate::capabilities::CapabilityBehavior {
        crate::capabilities::CapabilityBehavior::Static
    }
}

/// Factory for LlamaCppEmbeddingNode.
pub struct LlamaCppEmbeddingNodeFactory;

impl Default for LlamaCppEmbeddingNodeFactory {
    fn default() -> Self {
        Self
    }
}

impl StreamingNodeFactory for LlamaCppEmbeddingNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = LlamaCppEmbeddingNode::from_params(node_id, params)?;
        Ok(Box::new(LlamaCppEmbeddingNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "LlamaCppEmbeddingNode"
    }

    fn capability_behavior(&self) -> crate::capabilities::CapabilityBehavior {
        crate::capabilities::CapabilityBehavior::Static
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{
            LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType,
        };
        Some(
            NodeSchema::new("LlamaCppEmbeddingNode")
                .description(
                    "Text-to-embedding via llama.cpp (GGUF models). \
                     Accepts RuntimeData::Text and emits RuntimeData::Tensor \
                     with the dense embedding vector. \
                     Runs inference on a blocking thread (llama.cpp types are not Send).",
                )
                .category("ml")
                .accepts([RuntimeDataType::Text, RuntimeDataType::Json])
                .produces([RuntimeDataType::Tensor])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: true,
                    batch_aware: false,
                    supports_control: false,
                    latency_class: LatencyClass::Medium,
                })
                .config_schema_from::<LlamaCppEmbeddingConfig>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let mut config = LlamaCppEmbeddingConfig::default();
        config.model_path = "/path/to/model.gguf".to_string();
        let node = LlamaCppEmbeddingNode::new("test-emb", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_factory() {
        let factory = LlamaCppEmbeddingNodeFactory;
        assert_eq!(factory.node_type(), "LlamaCppEmbeddingNode");
    }

    #[test]
    fn test_l2_normalize() {
        let mut config = LlamaCppEmbeddingConfig::default();
        config.model_path = "/path/to/model.gguf".to_string();
        let node = LlamaCppEmbeddingNode::new("test", &config).unwrap();

        let mut v = vec![3.0, 4.0];
        node.l2_normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }
}
