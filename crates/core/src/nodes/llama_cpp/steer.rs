//! LlamaCppSteerNode — activation steering via llama.cpp
//!
//! Injects pre-computed activation deltas (emotion vectors, direction-of-change
//! vectors, etc.) into the model's hidden states during generation.
//!
//! Runs inference on a blocking thread (llama.cpp types are not Send).

use crate::data::RuntimeData;
use crate::error::Error;
use crate::nodes::streaming_node::{
    AsyncStreamingNode, InitializeContext, StreamingNode, StreamingNodeFactory,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::config::{LlamaCppSteerConfig, LlamaCppSteerVector};

/// Loaded steering vector with metadata.
#[derive(Debug, Clone)]
struct LoadedVector {
    label: String,
    data: Vec<f32>,
    hidden_size: usize,
}

/// Per-session steering state.
#[derive(Debug, Default, Clone)]
struct SteeringState {
    coefficients: HashMap<String, f32>,
    last_delta_norm: Option<f32>,
}

/// Llama.cpp activation steering node.
pub struct LlamaCppSteerNode {
    node_id: String,
    config: LlamaCppSteerConfig,
    vectors: RwLock<HashMap<String, LoadedVector>>,
    sessions: RwLock<HashMap<String, SteeringState>>,
    initialized: RwLock<bool>,
}

impl LlamaCppSteerNode {
    /// Create a new steering node.
    pub fn new(node_id: impl Into<String>, config: &LlamaCppSteerConfig) -> Result<Self, Error> {
        config.validate().map_err(|e| Error::Execution(format!("Invalid config: {}", e)))?;

        let mut sessions = HashMap::new();
        let default_state = SteeringState {
            coefficients: config
                .vectors
                .iter()
                .map(|v| (v.label.clone(), v.coefficient))
                .collect(),
            last_delta_norm: None,
        };
        sessions.insert("default".to_string(), default_state);

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            vectors: RwLock::new(HashMap::new()),
            sessions: RwLock::new(sessions),
            initialized: RwLock::new(false),
        })
    }

    /// Create from JSON parameters.
    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self, Error> {
        let config: LlamaCppSteerConfig = serde_json::from_value(params.clone())
            .map_err(|e| Error::Execution(format!("Invalid config JSON: {}", e)))?;
        Self::new(node_id, &config)
    }

    /// Extract f32 vector from a `RuntimeData::Tensor`.
    fn tensor_to_f32(tensor: &RuntimeData) -> Result<Vec<f32>, Error> {
        match tensor {
            RuntimeData::Tensor {
                data: bytes,
                dtype,
                ..
            } => {
                if *dtype != 0 {
                    return Err(Error::Execution(format!(
                        "Unsupported tensor dtype: {} (expected 0=float32)",
                        dtype
                    )));
                }

                if bytes.len() % 4 != 0 {
                    return Err(Error::Execution(
                        "Tensor data length is not a multiple of 4".to_string(),
                    ));
                }

                let mut vector = Vec::with_capacity(bytes.len() / 4);
                for chunk in bytes.chunks_exact(4) {
                    let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    vector.push(val);
                }
                Ok(vector)
            }
            other => Err(Error::Execution(format!(
                "Expected Tensor, got {}",
                other.data_type()
            ))),
        }
    }

    /// Extract label from tensor metadata.
    fn extract_label(tensor: &RuntimeData) -> String {
        match tensor {
            RuntimeData::Tensor { metadata, .. } => metadata
                .as_ref()
                .and_then(|m| m.get("emotion").or(m.get("label")))
                .and_then(|e| e.as_str())
                .unwrap_or("default")
                .to_string(),
            _ => "default".to_string(),
        }
    }

    /// Register a steering vector received from the pipeline.
    async fn register_vector(&self, tensor: &RuntimeData) -> Result<(), Error> {
        let data = Self::tensor_to_f32(tensor)?;
        let label = Self::extract_label(tensor);
        let hidden_size = data.len();

        let mut vectors = self.vectors.write().await;
        vectors.insert(label.clone(), LoadedVector {
            label: label.clone(),
            data,
            hidden_size,
        });

        info!(
            node = "llama-cpp-steer",
            label = %label,
            "Registered steering vector from pipeline"
        );

        Ok(())
    }

    /// Update coefficients for a session.
    async fn update_coefficients(&self, session_id: &str, new_coefficients: HashMap<String, f32>) {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            for (label, coef) in new_coefficients {
                let clamped = coef
                    .max(-self.config.max_coefficient)
                    .min(self.config.max_coefficient);
                state.coefficients.insert(label, clamped);
            }
        }
    }

    /// Compute the steering delta: Σ(coef_i × layer_norm × vec_i).
    async fn compute_delta(&self, session_id: &str) -> Result<(Vec<f32>, HashMap<String, f32>), Error> {
        let vectors = self.vectors.read().await;

        // Get session coefficients
        let sessions = self.sessions.read().await;
        let coefficients = sessions
            .get(session_id)
            .map(|s| s.coefficients.clone())
            .unwrap_or_default();
        drop(sessions);

        let mut vec_data: Vec<Vec<f32>> = Vec::new();
        let mut coefs: Vec<f32> = Vec::new();

        let mut labels: Vec<_> = vectors.iter().collect();
        labels.sort_by_key(|(k, _)| k.clone());

        for (label, loaded) in labels {
            if let Some(&coef) = coefficients.get(label) {
                if coef.abs() > 1e-8 {
                    vec_data.push(loaded.data.clone());
                    coefs.push(coef);
                }
            }
        }

        let hidden_size = vectors.values().next().map_or(0, |v| v.hidden_size);
        let delta = Self::compute_steering_delta(
            &vec_data,
            &coefs,
            self.config.layer_norm_value,
            hidden_size,
        )?;

        Ok((delta, coefficients))
    }

    /// Compute the steering delta.
    fn compute_steering_delta(
        vectors: &[Vec<f32>],
        coefficients: &[f32],
        layer_norm: f32,
        hidden_size: usize,
    ) -> Result<Vec<f32>, Error> {
        if vectors.len() != coefficients.len() {
            return Err(Error::Execution(format!(
                "Vector count ({}) != coefficient count ({})",
                vectors.len(),
                coefficients.len()
            )));
        }

        let mut delta = vec![0.0f32; hidden_size];

        for (vec, &coef) in vectors.iter().zip(coefficients) {
            if vec.len() != hidden_size {
                return Err(Error::Execution(format!(
                    "Vector length {} != hidden_size {}",
                    vec.len(),
                    hidden_size
                )));
            }
            let scale = coef * layer_norm;
            for (d, &v) in delta.iter_mut().zip(vec) {
                *d += v * scale;
            }
        }

        Ok(delta)
    }

    /// Generate steered text.
    #[cfg(feature = "llama-cpp")]
    async fn generate_steered(
        &self,
        prompt: &str,
        session_id: &str,
    ) -> Result<(Vec<String>, f32, HashMap<String, f32>), Error> {
        // Compute steering delta
        let (delta, coefficients) = self.compute_delta(session_id).await?;
        let delta_norm: f32 = delta.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Update session state
        {
            let mut sessions = self.sessions.write().await;
            if let Some(state) = sessions.get_mut(session_id) {
                state.last_delta_norm = Some(delta_norm);
            }
        }

        // Run generation on blocking thread
        let gen_config = self.config.generation.clone();
        let prompt = prompt.to_string();

        let chunks = tokio::task::spawn_blocking(move || {
            super::inference::run_generation(&gen_config, &prompt)
        })
        .await
        .map_err(|e| Error::Execution(format!("Task join failed: {}", e)))??
        .chunks;

        Ok((chunks, delta_norm, coefficients))
    }

    #[cfg(not(feature = "llama-cpp"))]
    async fn generate_steered(
        &self,
        prompt: &str,
        session_id: &str,
    ) -> Result<(Vec<String>, f32, HashMap<String, f32>), Error> {
        let (delta, coefficients) = self.compute_delta(session_id).await?;
        let delta_norm: f32 = delta.iter().map(|x| x * x).sum::<f32>().sqrt();

        Ok((
            vec![format!(
                "[llama-cpp disabled: {}]",
                &prompt[..prompt.len().min(30)]
            )],
            delta_norm,
            coefficients,
        ))
    }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for LlamaCppSteerNode {
    fn node_type(&self) -> &str {
        "LlamaCppSteerNode"
    }

    async fn initialize(&self, ctx: &InitializeContext) -> Result<(), Error> {
        info!(
            node = "llama-cpp-steer",
            model = %self.config.model_path,
            layer = self.config.layer,
            n_vectors = self.config.vectors.len(),
            "Initializing LlamaCppSteerNode"
        );

        ctx.emit_progress(
            "loading_model",
            &format!(
                "Loading model for steering: {} (layer {})",
                self.config.model_path, self.config.layer
            ),
        );

        warn!(
            node = "llama-cpp-steer",
            "LlamaCppSteerNode: KV-cache injection not yet implemented. \
             The node computes steering deltas and emits them as metadata. \
             Text output passes through without hidden-state modification."
        );

        *self.initialized.write().await = true;
        ctx.emit_progress("ready", "LlamaCppSteerNode ready (metadata mode)");
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let session_id = "default".to_string();

        match &data {
            RuntimeData::Tensor { .. } => {
                self.register_vector(&data).await?;
                let label = Self::extract_label(&data);
                Ok(RuntimeData::Json(serde_json::json!({
                    "status": "vector_registered",
                    "label": label,
                })))
            }
            RuntimeData::Text(text) => {
                let (chunks, delta_norm, coefficients) =
                    self.generate_steered(text, &session_id).await?;

                Ok(RuntimeData::Json(serde_json::json!({
                    "steering": {
                        "layer": self.config.layer,
                        "layer_norm": self.config.layer_norm_value,
                        "delta_norm": delta_norm,
                        "applied": false,
                        "coefficients": coefficients,
                        "mode": "metadata",
                    },
                    "text": chunks.join(""),
                })))
            }
            RuntimeData::Json(value) => {
                if let Some(coefs) = value.get("coefficients").and_then(|v| v.as_object()) {
                    let mut new_coefs = HashMap::new();
                    for (label, coef) in coefs {
                        if let Some(c) = coef.as_f64() {
                            new_coefs.insert(label.clone(), c as f32);
                        }
                    }
                    if !new_coefs.is_empty() {
                        self.update_coefficients(&session_id, new_coefs).await;
                        return Ok(RuntimeData::Json(serde_json::json!({
                            "status": "coefficients_updated",
                            "session": session_id,
                        })));
                    }
                }

                let prompt = value
                    .get("prompt")
                    .or(value.get("text"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| value.to_string());

                let (chunks, delta_norm, coefficients) =
                    self.generate_steered(&prompt, &session_id).await?;

                Ok(RuntimeData::Json(serde_json::json!({
                    "steering": {
                        "layer": self.config.layer,
                        "layer_norm": self.config.layer_norm_value,
                        "delta_norm": delta_norm,
                        "applied": false,
                        "coefficients": coefficients,
                        "mode": "metadata",
                    },
                    "text": chunks.join(""),
                })))
            }
            other => Err(Error::Execution(format!(
                "LlamaCppSteerNode accepts Tensor, Text, or Json, got {}",
                other.data_type()
            ))),
        }
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        let sid = session_id.unwrap_or_else(|| "default".to_string());

        if let RuntimeData::Tensor { .. } = &data {
            self.register_vector(&data).await?;
            let label = Self::extract_label(&data);
            callback(RuntimeData::Json(serde_json::json!({
                "status": "vector_registered",
                "label": label,
            })))?;
            return Ok(1);
        }

        if let RuntimeData::Json(ref value) = data {
            if let Some(coefs) = value.get("coefficients").and_then(|v| v.as_object()) {
                let mut new_coefs = HashMap::new();
                for (label, coef) in coefs {
                    if let Some(c) = coef.as_f64() {
                        new_coefs.insert(label.clone(), c as f32);
                    }
                }
                if !new_coefs.is_empty() {
                    self.update_coefficients(&sid, new_coefs).await;
                    callback(RuntimeData::Json(serde_json::json!({
                        "status": "coefficients_updated",
                        "session": sid,
                    })))?;
                    return Ok(1);
                }
            }
        }

        let prompt = match &data {
            RuntimeData::Text(text) => text.clone(),
            RuntimeData::Json(value) => value
                .get("prompt")
                .or(value.get("text"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.to_string()),
            other => {
                return Err(Error::Execution(format!(
                    "LlamaCppSteerNode accepts Tensor, Text, or Json, got {}",
                    other.data_type()
                )));
            }
        };

        let (chunks, delta_norm, coefficients) = self.generate_steered(&prompt, &sid).await?;

        let n_chunks = chunks.len();
        for chunk in &chunks {
            callback(RuntimeData::Text(chunk.clone()))?;
        }

        callback(RuntimeData::Json(serde_json::json!({
            "steering": {
                "layer": self.config.layer,
                "layer_norm": self.config.layer_norm_value,
                "delta_norm": delta_norm,
                "applied": false,
                "coefficients": coefficients,
                "mode": "metadata",
            },
        })))?;

        Ok(n_chunks + 1)
    }

    async fn process_control_message(
        &self,
        message: RuntimeData,
        session_id: Option<String>,
    ) -> Result<bool, Error> {
        let sid = session_id.unwrap_or_else(|| "default".to_string());

        if let RuntimeData::ControlMessage { message_type, .. } = &message {
            match message_type {
                crate::data::ControlMessageType::CancelSpeculation { .. } => {
                    debug!("Received cancel speculation message for LlamaCpp steering");
                    return Ok(true);
                }
                _ => {}
            }
        }

        if let RuntimeData::Json(value) = message {
            if let Some(coefs) = value.get("coefficients").and_then(|v| v.as_object()) {
                let mut new_coefs = HashMap::new();
                for (label, coef) in coefs {
                    if let Some(c) = coef.as_f64() {
                        new_coefs.insert(label.clone(), c as f32);
                    }
                }
                if !new_coefs.is_empty() {
                    self.update_coefficients(&sid, new_coefs).await;
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

/// Wrapper for StreamingNode trait.
pub struct LlamaCppSteerNodeWrapper(pub Arc<LlamaCppSteerNode>);

#[async_trait::async_trait]
impl StreamingNode for LlamaCppSteerNodeWrapper {
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

/// Factory for LlamaCppSteerNode.
pub struct LlamaCppSteerNodeFactory;

impl Default for LlamaCppSteerNodeFactory {
    fn default() -> Self {
        Self
    }
}

impl StreamingNodeFactory for LlamaCppSteerNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = LlamaCppSteerNode::from_params(node_id, params)?;
        Ok(Box::new(LlamaCppSteerNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "LlamaCppSteerNode"
    }

    fn capability_behavior(&self) -> crate::capabilities::CapabilityBehavior {
        crate::capabilities::CapabilityBehavior::Static
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{
            LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType,
        };
        Some(
            NodeSchema::new("LlamaCppSteerNode")
                .description(
                    "Applies activation steering to LLM generation by adding scaled \
                     emotion vectors to hidden states. Accepts pre-extracted vectors \
                     as RuntimeData::Tensor from the pipeline. \
                     Coefficients can be updated at runtime via JSON. \
                     Currently operates in metadata mode (KV-cache injection pending). \
                     Runs inference on a blocking thread (llama.cpp types are not Send).",
                )
                .category("ml")
                .accepts([
                    RuntimeDataType::Tensor,
                    RuntimeDataType::Text,
                    RuntimeDataType::Json,
                ])
                .produces([RuntimeDataType::Text, RuntimeDataType::Json])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: true,
                    latency_class: LatencyClass::Slow,
                })
                .config_schema_from::<LlamaCppSteerConfig>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let mut config = LlamaCppSteerConfig::default();
        config.model_path = "/path/to/model.gguf".to_string();
        config.vectors.push(LlamaCppSteerVector {
            label: "happy".to_string(),
            coefficient: 0.5,
        });
        let node = LlamaCppSteerNode::new("test-steer", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_factory() {
        let factory = LlamaCppSteerNodeFactory;
        assert_eq!(factory.node_type(), "LlamaCppSteerNode");
        assert!(factory.is_multi_output_streaming());
    }

    #[test]
    fn test_steering_delta() {
        let vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let coefs = vec![0.5, 0.3];
        let delta =
            LlamaCppSteerNode::compute_steering_delta(&vectors, &coefs, 10.0, 2).unwrap();
        assert!((delta[0] - 5.0).abs() < 1e-6);
        assert!((delta[1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_tensor_to_f32() {
        let bytes: Vec<u8> = [1.0f32, 2.0f32, 3.0f32]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();

        let tensor = RuntimeData::Tensor {
            data: bytes,
            shape: vec![3],
            dtype: 0,
            metadata: Some(serde_json::json!({"emotion": "happy"})),
        };

        let vec = LlamaCppSteerNode::tensor_to_f32(&tensor).unwrap();
        assert_eq!(vec.len(), 3);
        assert!((vec[0] - 1.0).abs() < 1e-6);
    }
}
