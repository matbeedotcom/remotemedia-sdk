//! Emotion steering node
//!
//! During LLM generation, adds a scaled emotion vector to the residual
//! stream at a target layer. This shifts the model's output toward (or
//! away from) the target emotion.
//!
//! # Steering equation
//!
//! At each forward pass through layer L:
//!
//! ```text
//! residual_out = residual_out + Σ(coef_i × layer_norm × vector_i)
//! ```
//!
//! Where `coef_i ∈ [-max_coef, +max_coef]` is set per-emotion via
//! configuration or the `steering.in.coefficients` aux port.
//!
//! # Pipeline contract
//!
//! ```text
//!  EmotionExtractorNode          LLM inference
//!       │                            │
//!       │  Tensor (happy vector)     │  Tensor (activation at target layer)
//!       │  Tensor (sad vector)       │
//!       ▼                            ▼
//!  ┌──────────────────┐    ┌──────────────────┐
//!  │ EmotionSteering  │    │   LLM Node       │
//!  │     Node         │◄───│                  │
//!  └────────┬─────────┘    └──────────────────┘
//!           │
//!           ├─→ Tensor  (steering delta)
//!           └─→ Json    (steering metadata)
//! ```
//!
//! Emotion vectors arrive as `RuntimeData::Tensor` from the pipeline
//! (e.g., from `EmotionExtractorNode`). The emotion label is carried
//! in the tensor's metadata field.
//!
//! # Current implementation
//!
//! Because Candle does not yet expose forward hooks, this node operates
//! in **metadata mode**: it validates vectors, computes the steering
//! delta, and emits the coefficients as JSON alongside the text output.
//! The actual residual-stream injection is a no-op until Candle gains
//! hook support.
//!
//! When Candle forward hooks land, the `apply_steering` method will
//! install the hook at the target layer.

use super::config::{EmotionSteerConfig, SteeringVectorConfig};
use super::vector_io::compute_steering_delta;
use crate::error::{CandleNodeError, Result};
use crate::{DeviceSelector, InferenceDevice};
use async_trait::async_trait;
use remotemedia_core::capabilities::CapabilityBehavior;
use remotemedia_core::data_compat::RuntimeData;
use remotemedia_core::nodes::streaming_node::{
    AsyncStreamingNode, InitializeContext, StreamingNode, StreamingNodeFactory,
};
use remotemedia_core::Error;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Loaded emotion vector with its metadata.
#[derive(Debug, Clone)]
struct LoadedVector {
    emotion: String,
    data: Vec<f32>,
    hidden_size: usize,
}

/// Per-session steering state.
#[derive(Debug, Default, Clone)]
struct SteeringState {
    /// Current coefficients per emotion
    coefficients: HashMap<String, f32>,
    /// Last computed delta (for metadata output)
    last_delta_norm: Option<f32>,
}

/// Emotion steering node.
///
/// Accepts pre-extracted emotion vectors as `RuntimeData::Tensor` from
/// the pipeline and applies them during LLM generation via residual-stream
/// injection.
pub struct EmotionSteeringNode {
    node_id: String,
    config: EmotionSteerConfig,
    device: InferenceDevice,
    /// Loaded emotion vectors (received from pipeline as Tensors)
    vectors: RwLock<HashMap<String, LoadedVector>>,
    /// Per-session steering state
    sessions: RwLock<HashMap<String, SteeringState>>,
    /// Whether initialization is complete
    initialized: RwLock<bool>,
}

impl EmotionSteeringNode {
    pub fn new(node_id: impl Into<String>, config: &EmotionSteerConfig) -> Result<Self> {
        config.validate().map_err(|e| {
            CandleNodeError::configuration("emotion-steering", e)
        })?;

        let device = DeviceSelector::from_config(&config.model)
            .unwrap_or(InferenceDevice::Cpu);

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            device,
            vectors: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            initialized: RwLock::new(false),
        })
    }

    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self> {
        let config: EmotionSteerConfig = serde_json::from_value(params.clone())
            .map_err(|e| CandleNodeError::configuration("emotion-steering", e.to_string()))?;
        Self::new(node_id, &config)
    }

    /// Extract f32 vector from a `RuntimeData::Tensor`.
    fn tensor_to_f32(tensor: &RuntimeData) -> Result<Vec<f32>> {
        match tensor {
            RuntimeData::Tensor {
                data: bytes,
                shape,
                dtype,
                metadata: _,
            } => {
                if *dtype != 0 {
                    return Err(CandleNodeError::configuration(
                        "emotion-steering",
                        format!("Unsupported tensor dtype: {} (expected 0=float32)", dtype),
                    ));
                }

                if bytes.len() % 4 != 0 {
                    return Err(CandleNodeError::configuration(
                        "emotion-steering",
                        "Tensor data length is not a multiple of 4",
                    ));
                }

                let n_elements = bytes.len() / 4;
                let expected = shape.iter().map(|&x| x as usize).product::<usize>();
                if expected > 0 && n_elements != expected {
                    return Err(CandleNodeError::configuration(
                        "emotion-steering",
                        format!(
                            "Tensor has {} elements but shape {:?} implies {}",
                            n_elements, shape, expected
                        ),
                    ));
                }

                let mut vector = Vec::with_capacity(n_elements);
                for chunk in bytes.chunks_exact(4) {
                    let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    vector.push(val);
                }
                Ok(vector)
            }
            other => Err(CandleNodeError::invalid_input(
                "emotion-steering",
                "Tensor",
                format!("{:?}", other.data_type()),
            )),
        }
    }

    /// Extract emotion label from tensor metadata.
    fn extract_emotion_label(tensor: &RuntimeData) -> String {
        match tensor {
            RuntimeData::Tensor { metadata, .. } => metadata
                .as_ref()
                .and_then(|m| m.get("emotion").and_then(|e| e.as_str()))
                .unwrap_or("default")
                .to_string(),
            _ => "default".to_string(),
        }
    }

    /// Register an emotion vector received from the pipeline.
    async fn register_vector(&self, tensor: &RuntimeData) -> Result<()> {
        let data = Self::tensor_to_f32(tensor)?;
        let emotion = Self::extract_emotion_label(tensor);
        let hidden_size = data.len();
        let emotion_log = emotion.clone();

        let mut vectors = self.vectors.write().await;
        vectors.insert(emotion_log.clone(), LoadedVector {
            emotion,
            data,
            hidden_size,
        });

        info!(
            node = "emotion-steering",
            emotion = %emotion_log,
            hidden_size,
            "Registered emotion vector from pipeline"
        );

        Ok(())
    }

    /// Get or create steering state for a session.
    async fn get_session_state(&self, session_id: &str) -> SteeringState {
        let mut sessions = self.sessions.write().await;

        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| {
                let mut state = SteeringState::default();
                for vec_cfg in &self.config.vectors {
                    state
                        .coefficients
                        .insert(vec_cfg.emotion.clone(), vec_cfg.coefficient);
                }
                state
            })
            .clone()
    }

    /// Update coefficients for a session.
    async fn update_coefficients(
        &self,
        session_id: &str,
        new_coefficients: HashMap<String, f32>,
    ) {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            // Clamp coefficients
            for (emotion, coef) in new_coefficients {
                let clamped = coef.max(-self.config.max_coefficient)
                    .min(self.config.max_coefficient);
                state.coefficients.insert(emotion, clamped);
            }
            info!(
                node = "emotion-steering",
                session = %session_id,
                coefficients = ?state.coefficients,
                "Updated steering coefficients"
            );
        }
    }

    /// Compute the steering delta from current coefficients.
    async fn compute_delta(&self, session_id: &str) -> Result<(Vec<f32>, HashMap<String, f32>)> {
        let vectors = self.vectors.read().await;
        let state = self.get_session_state(session_id).await;

        let mut vec_data: Vec<Vec<f32>> = Vec::new();
        let mut coefficients: Vec<f32> = Vec::new();

        // Collect vectors that have non-zero coefficients
        let mut emotions: Vec<_> = vectors.iter().collect();
        emotions.sort_by_key(|(k, _)| k.clone());

        for (emotion, loaded) in emotions {
            if let Some(&coef) = state.coefficients.get(emotion) {
                if coef.abs() > 1e-8 {
                    vec_data.push(loaded.data.clone());
                    coefficients.push(coef);
                }
            }
        }

        let hidden_size = vectors.values().next().map_or(0, |v| v.hidden_size);
        let delta = compute_steering_delta(&vec_data, &coefficients, self.config.layer_norm, hidden_size)?;

        Ok((delta, state.coefficients))
    }

    /// Apply steering to the model.
    ///
    /// **NOTE**: This is a metadata-only implementation. The actual
    /// residual-stream injection requires Candle forward hooks, which
    /// are not yet available. When Candle gains hook support, this
    /// method will install the forward hook at the target layer.
    ///
    /// Currently, this method validates the steering configuration
    /// and returns the computed delta for metadata output.
    async fn apply_steering(&self, session_id: &str) -> Result<SteeringResult> {
        let (delta, coefficients) = self.compute_delta(session_id).await?;

        let delta_norm: f32 = delta.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Update session state
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            state.last_delta_norm = Some(delta_norm);
        }

        Ok(SteeringResult {
            delta,
            delta_norm,
            coefficients,
            layer: self.config.layer,
            layer_norm: self.config.layer_norm,
            applied: false, // Will be true when Candle hooks land
        })
    }
}

/// Result of a steering computation.
#[derive(Debug, Clone)]
pub struct SteeringResult {
    /// The computed delta vector (would be added to residual stream)
    pub delta: Vec<f32>,
    /// L2 norm of the delta
    pub delta_norm: f32,
    /// Coefficients that were applied
    pub coefficients: HashMap<String, f32>,
    /// Target layer
    pub layer: usize,
    /// Layer norm used for scaling
    pub layer_norm: f32,
    /// Whether the delta was actually applied to the model
    /// (false until Candle forward hooks are available)
    pub applied: bool,
}

#[async_trait]
impl AsyncStreamingNode for EmotionSteeringNode {
    fn node_type(&self) -> &str {
        "EmotionSteeringNode"
    }

    async fn initialize(&self, ctx: &InitializeContext) -> std::result::Result<(), Error> {
        info!(
            node = "emotion-steering",
            model = %self.config.model,
            layer = self.config.layer,
            layer_norm = self.config.layer_norm,
            n_emotions = self.config.vectors.len(),
            "Initializing EmotionSteeringNode"
        );

        ctx.emit_progress(
            "loading_node",
            &format!(
                "EmotionSteeringNode: configured for {} emotions at layer {}",
                self.config.vectors.len(),
                self.config.layer
            ),
        );

        warn!(
            node = "emotion-steering",
            "EmotionSteeringNode is in metadata-only mode. \
             Residual-stream injection requires Candle forward hooks (not yet available). \
             The node validates vectors, computes deltas, and emits coefficients as JSON. \
             Text output passes through unchanged."
        );

        *self.initialized.write().await = true;

        ctx.emit_progress("ready", "EmotionSteeringNode ready (metadata mode)");
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        let session_id = "default".to_string();

        match &data {
            // Primary path: Tensor emotion vector from pipeline
            RuntimeData::Tensor { .. } => {
                self.register_vector(&data).await
                    .map_err(|e| Error::Execution(e.to_string()))?;

                let emotion = Self::extract_emotion_label(&data);
                Ok(RuntimeData::Json(serde_json::json!({
                    "status": "vector_registered",
                    "emotion": emotion,
                })))
            }
            // Text prompt for steering computation
            RuntimeData::Text(text) => {
                // Compute steering delta
                let result = self.apply_steering(&session_id).await
                    .map_err(|e| Error::Execution(e.to_string()))?;

                // Emit steering metadata as JSON
                let metadata = serde_json::json!({
                    "steering": {
                        "layer": result.layer,
                        "layer_norm": result.layer_norm,
                        "delta_norm": result.delta_norm,
                        "applied": result.applied,
                        "coefficients": result.coefficients,
                        "mode": if result.applied { "active" } else { "metadata" },
                    },
                    "prompt": text,
                });

                Ok(RuntimeData::Json(metadata))
            }
            // JSON input: coefficient updates or prompt
            RuntimeData::Json(value) => {
                if let Some(coefs) = value.get("coefficients").and_then(|v| v.as_object()) {
                    let mut new_coefs = HashMap::new();
                    for (emotion, coef) in coefs {
                        if let Some(c) = coef.as_f64() {
                            new_coefs.insert(emotion.clone(), c as f32);
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

                // Treat as prompt
                let prompt = value.get("prompt")
                    .or(value.get("text"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| value.to_string());

                let result = self.apply_steering(&session_id).await
                    .map_err(|e| Error::Execution(e.to_string()))?;

                let metadata = serde_json::json!({
                    "steering": {
                        "layer": result.layer,
                        "layer_norm": result.layer_norm,
                        "delta_norm": result.delta_norm,
                        "applied": result.applied,
                        "coefficients": result.coefficients,
                        "mode": if result.applied { "active" } else { "metadata" },
                    },
                    "prompt": prompt,
                });

                Ok(RuntimeData::Json(metadata))
            }
            other => Err(Error::Execution(format!(
                "EmotionSteeringNode accepts Tensor, Text, or Json input, got {}",
                other.data_type()
            ))),
        }
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> std::result::Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> std::result::Result<(), Error> + Send,
    {
        let sid = session_id.unwrap_or_else(|| "default".to_string());

        // Handle Tensor vectors
        if let RuntimeData::Tensor { .. } = &data {
            self.register_vector(&data).await
                .map_err(|e| Error::Execution(e.to_string()))?;

            let emotion = Self::extract_emotion_label(&data);
            callback(RuntimeData::Json(serde_json::json!({
                "status": "vector_registered",
                "emotion": emotion,
            })))?;
            return Ok(1);
        }

        // Handle coefficient updates via JSON
        if let RuntimeData::Json(ref value) = data {
            if let Some(coefs) = value.get("coefficients").and_then(|v| v.as_object()) {
                let mut new_coefs = HashMap::new();
                for (emotion, coef) in coefs {
                    if let Some(c) = coef.as_f64() {
                        new_coefs.insert(emotion.clone(), c as f32);
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

        // Process as normal
        let output = self.process(data).await?;
        callback(output)?;
        Ok(1)
    }

    async fn process_control_message(
        &self,
        message: RuntimeData,
        session_id: Option<String>,
    ) -> std::result::Result<bool, Error> {
        let sid = session_id.unwrap_or_else(|| "default".to_string());

        // Control message handling - pass through for now
        if let RuntimeData::ControlMessage { .. } = &message {
            debug!("Received control message for emotion steering");
        }

        // Handle coefficient updates via control messages
        if let RuntimeData::Json(value) = message {
            if let Some(coefs) = value.get("coefficients").and_then(|v| v.as_object()) {
                let mut new_coefs = HashMap::new();
                for (emotion, coef) in coefs {
                    if let Some(c) = coef.as_f64() {
                        new_coefs.insert(emotion.clone(), c as f32);
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

/// Wrapper for EmotionSteeringNode
pub struct EmotionSteeringNodeWrapper(pub Arc<EmotionSteeringNode>);

#[async_trait]
impl StreamingNode for EmotionSteeringNodeWrapper {
    fn node_type(&self) -> &str { self.0.node_type() }
    fn node_id(&self) -> &str { &self.0.node_id }

    async fn initialize(&self, ctx: &InitializeContext) -> std::result::Result<(), Error> {
        AsyncStreamingNode::initialize(self.0.as_ref(), ctx).await
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

/// Emotion steering node factory
pub struct EmotionSteeringNodeFactory;

impl EmotionSteeringNodeFactory {
    pub fn new() -> Self { Self }
}

impl Default for EmotionSteeringNodeFactory {
    fn default() -> Self { Self::new() }
}

impl StreamingNodeFactory for EmotionSteeringNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> std::result::Result<Box<dyn StreamingNode>, Error> {
        let node = EmotionSteeringNode::from_params(node_id, params)
            .map_err(|e| Error::Execution(e.to_string()))?;
        Ok(Box::new(EmotionSteeringNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str { "EmotionSteeringNode" }
    fn capability_behavior(&self) -> CapabilityBehavior { CapabilityBehavior::Static }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }

    fn schema(&self) -> Option<remotemedia_core::nodes::schema::NodeSchema> {
        use remotemedia_core::nodes::schema::{
            LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType,
        };
        Some(
            NodeSchema::new("EmotionSteeringNode")
                .description(
                    "Applies emotion steering to LLM generation by adding scaled \
                     emotion vectors to the residual stream. Accepts pre-extracted \
                     vectors as RuntimeData::Tensor from the pipeline (e.g., from \
                     EmotionExtractorNode). The emotion label is carried in tensor \
                     metadata. Coefficients can be updated at runtime via JSON.",
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
                .config_schema_from::<EmotionSteerConfig>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let mut config = EmotionSteerConfig::default();
        config.vectors.push(SteeringVectorConfig {
            emotion: "happy".to_string(),
            coefficient: 0.5,
        });
        let result = EmotionSteeringNode::new("test-steering", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_factory() {
        let factory = EmotionSteeringNodeFactory::new();
        assert_eq!(factory.node_type(), "EmotionSteeringNode");
        assert!(factory.is_multi_output_streaming());
    }

    #[test]
    fn test_config_validation() {
        let config = EmotionSteerConfig::default();
        assert!(config.validate().is_err()); // No vectors

        let mut config2 = EmotionSteerConfig::default();
        config2.vectors.push(SteeringVectorConfig {
            emotion: "happy".to_string(),
            coefficient: 0.5,
        });
        assert!(config2.validate().is_ok());
    }

    #[test]
    fn test_steering_result_metadata() {
        let result = SteeringResult {
            delta: vec![0.0; 4096],
            delta_norm: 0.0,
            coefficients: HashMap::new(),
            layer: 21,
            layer_norm: 14.7,
            applied: false,
        };
        assert_eq!(result.layer, 21);
        assert!(!result.applied);
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

        let vec = EmotionSteeringNode::tensor_to_f32(&tensor).unwrap();
        assert_eq!(vec.len(), 3);
        assert!((vec[0] - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_register_vector_from_tensor() {
        let mut config = EmotionSteerConfig::default();
        config.vectors.push(SteeringVectorConfig {
            emotion: "happy".to_string(),
            coefficient: 0.5,
        });
        let node = EmotionSteeringNode::new("test", &config).unwrap();

        let bytes: Vec<u8> = [1.0f32, 0.0f32, -1.0f32, 0.5f32]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();

        let tensor = RuntimeData::Tensor {
            data: bytes,
            shape: vec![4],
            dtype: 0,
            metadata: Some(serde_json::json!({"emotion": "happy"})),
        };

        node.register_vector(&tensor).await.unwrap();

        let vectors = node.vectors.read().await;
        assert!(vectors.contains_key("happy"));
        assert_eq!(vectors["happy"].data.len(), 4);
    }
}
