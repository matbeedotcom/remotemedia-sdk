//! LlamaCppActivationNode — hidden-state activation extraction via llama.cpp
//!
//! Uses llama.cpp's `TensorCapture` callback to extract per-token hidden
//! states at arbitrary transformer layers during `llama_decode`.
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

use super::config::LlamaCppActivationConfig;

/// Llama.cpp activation extraction node.
pub struct LlamaCppActivationNode {
    node_id: String,
    config: LlamaCppActivationConfig,
    initialized: RwLock<bool>,
}

impl LlamaCppActivationNode {
    /// Create a new activation extraction node.
    pub fn new(
        node_id: impl Into<String>,
        config: &LlamaCppActivationConfig,
    ) -> Result<Self, Error> {
        config.validate().map_err(|e| Error::Execution(format!("Invalid config: {}", e)))?;

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            initialized: RwLock::new(false),
        })
    }

    /// Create from JSON parameters.
    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self, Error> {
        let config: LlamaCppActivationConfig = serde_json::from_value(params.clone())
            .map_err(|e| Error::Execution(format!("Invalid config JSON: {}", e)))?;
        Self::new(node_id, &config)
    }

    /// Extract activations for a text prompt at configured layers.
    #[cfg(feature = "llama-cpp")]
    async fn extract(&self, text: &str) -> Result<Vec<super::inference::ActivationCapture>, Error> {
        let config = self.config.clone();
        let layers = config.layers.clone();
        let text = text.to_string();

        let result = tokio::task::spawn_blocking(move || {
            super::inference::run_activation(
                &config.model_path,
                &text,
                &layers,
                config.context_size,
                config.batch_size,
                config.backend.gpu_offload,
                config.backend.flash_attention,
                config.backend.threads,
                config.pooling,
            )
        })
        .await
        .map_err(|e| Error::Execution(format!("Task join failed: {}", e)))??;

        Ok(result)
    }

    #[cfg(not(feature = "llama-cpp"))]
    async fn extract(&self, _text: &str) -> Result<Vec<super::inference::ActivationCapture>, Error> {
        Ok(vec![])
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
impl AsyncStreamingNode for LlamaCppActivationNode {
    fn node_type(&self) -> &str {
        "LlamaCppActivationNode"
    }

    async fn initialize(&self, ctx: &InitializeContext) -> Result<(), Error> {
        info!(
            node = "llama-cpp-activation",
            model = %self.config.model_path,
            layers = ?self.config.layers,
            pooling = ?self.config.pooling,
            "Initializing LlamaCppActivationNode"
        );

        ctx.emit_progress(
            "loading_model",
            &format!(
                "Loading model for activation extraction: {} (layers: {:?})",
                self.config.model_path, self.config.layers
            ),
        );

        *self.initialized.write().await = true;
        ctx.emit_progress("ready", "LlamaCppActivationNode ready");
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
                    "LlamaCppActivationNode accepts Text or Json, got {}",
                    other.data_type()
                )));
            }
        };

        let emotion_label = match &data {
            RuntimeData::Json(value) => value
                .get("emotion")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            _ => None,
        };

        let captures = self.extract(&text).await?;

        // Emit activation for the first captured layer
        let capture = captures.first().ok_or_else(|| {
            Error::Execution("No activations captured".to_string())
        })?;

        let mut vector = capture.activation.clone();
        if self.config.normalize {
            self.l2_normalize(&mut vector);
        }

        let tensor_data: Vec<u8> = vector.iter().flat_map(|&x| x.to_le_bytes()).collect();

        let mut metadata = serde_json::json!({
            "model": self.config.model_path,
            "layer": capture.layer,
            "hidden_size": capture.hidden_size,
            "pooling": format!("{:?}", self.config.pooling),
            "normalized": self.config.normalize,
            "raw_norm": capture.raw_norm,
        });

        if let Some(emotion) = &emotion_label {
            metadata["emotion"] = serde_json::json!(emotion);
        }

        Ok(RuntimeData::Tensor {
            data: tensor_data,
            shape: vec![vector.len() as i32],
            dtype: 0,
            metadata: Some(metadata),
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
                    "LlamaCppActivationNode accepts Text or Json, got {}",
                    other.data_type()
                )));
            }
        };

        let emotion_label = match &data {
            RuntimeData::Json(value) => value
                .get("emotion")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            _ => None,
        };

        let captures = self.extract(&text).await?;
        let mut count = 0;

        for capture in captures {
            let mut vector = capture.activation.clone();
            if self.config.normalize {
                self.l2_normalize(&mut vector);
            }

            let tensor_data: Vec<u8> =
                vector.iter().flat_map(|&x| x.to_le_bytes()).collect();

            let mut metadata = serde_json::json!({
                "model": self.config.model_path,
                "layer": capture.layer,
                "hidden_size": capture.hidden_size,
                "pooling": format!("{:?}", self.config.pooling),
                "normalized": self.config.normalize,
                "raw_norm": capture.raw_norm,
            });

            if let Some(emotion) = &emotion_label {
                metadata["emotion"] = serde_json::json!(emotion);
            }

            callback(RuntimeData::Tensor {
                data: tensor_data,
                shape: vec![vector.len() as i32],
                dtype: 0,
                metadata: Some(metadata),
            })?;
            count += 1;
        }

        Ok(count)
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
pub struct LlamaCppActivationNodeWrapper(pub Arc<LlamaCppActivationNode>);

#[async_trait::async_trait]
impl StreamingNode for LlamaCppActivationNodeWrapper {
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

/// Factory for LlamaCppActivationNode.
pub struct LlamaCppActivationNodeFactory;

impl Default for LlamaCppActivationNodeFactory {
    fn default() -> Self {
        Self
    }
}

impl StreamingNodeFactory for LlamaCppActivationNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = LlamaCppActivationNode::from_params(node_id, params)?;
        Ok(Box::new(LlamaCppActivationNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "LlamaCppActivationNode"
    }

    fn capability_behavior(&self) -> crate::capabilities::CapabilityBehavior {
        crate::capabilities::CapabilityBehavior::Static
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // One tensor per layer
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{
            LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType,
        };
        Some(
            NodeSchema::new("LlamaCppActivationNode")
                .description(
                    "Extracts hidden-state activations at arbitrary transformer layers \
                     via llama.cpp's TensorCapture callback. \
                     Accepts RuntimeData::Text and emits RuntimeData::Tensor \
                     with pooled activation vectors. \
                     Compatible with EmotionExtractorNode for activation-vector analysis. \
                     Runs inference on a blocking thread (llama.cpp types are not Send).",
                )
                .category("ml")
                .accepts([RuntimeDataType::Text, RuntimeDataType::Json])
                .produces([RuntimeDataType::Tensor])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: true,
                    batch_aware: false,
                    supports_control: false,
                    latency_class: LatencyClass::Slow,
                })
                .config_schema_from::<LlamaCppActivationConfig>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let mut config = LlamaCppActivationConfig::default();
        config.model_path = "/path/to/model.gguf".to_string();
        let node = LlamaCppActivationNode::new("test-act", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_factory() {
        let factory = LlamaCppActivationNodeFactory;
        assert_eq!(factory.node_type(), "LlamaCppActivationNode");
        assert!(factory.is_multi_output_streaming());
    }

    #[test]
    fn test_config_validation() {
        let config = LlamaCppActivationConfig::default();
        assert!(config.validate().is_err()); // empty model_path

        let mut config2 = LlamaCppActivationConfig::default();
        config2.model_path = "/path/to/model.gguf".to_string();
        assert!(config2.validate().is_ok());
    }
}
