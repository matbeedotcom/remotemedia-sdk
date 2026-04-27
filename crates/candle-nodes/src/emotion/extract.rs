//! Emotion vector extraction node
//!
//! Given pre-captured residual-stream activations (as `RuntimeData::Tensor`),
//! computes mean-subtraction emotion direction vectors.
//!
//! # Pipeline contract
//!
//! ```text
//!  activation source (Python/Candle)
//!       │
//!       │  Tensor { metadata: { emotion: "happy" } }
//!       │  Tensor { metadata: { emotion: "neutral" } }
//!       │  Tensor { metadata: { emotion: "sad" } }
//!       ▼
//!  ┌──────────────────┐
//!  │ EmotionExtractor │  ← Text "compute" (trigger)
//!  │     Node         │  ← Text "reset" (clear)
//!  └────────┬─────────┘
//!           │
//!           ├─→ Tensor  (happy direction vector)
//!           ├─→ Json    (EmotionVectorMetadata)
//!           ├─→ Tensor  (sad direction vector)
//!           └─→ Json    (EmotionVectorMetadata)
//! ```
//!
//! Each activation arrives as `RuntimeData::Tensor` with the emotion label
//! carried in the tensor's metadata field. The node accumulates per-emotion
//! sums, then on a `"compute"` trigger emits the mean-subtraction vectors.

use super::config::{EmotionExtractConfig, EmotionVectorMetadata, PoolingMode};
use super::vector_io::{l2_normalize, subtract_vectors};
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

/// Accumulated activation data per emotion label.
#[derive(Debug, Default, Clone)]
struct ActivationAccumulator {
    /// Running sum of all activations for this emotion
    sum: Vec<f32>,
    /// Count of activations
    count: usize,
    /// Hidden size (from first activation)
    hidden_size: usize,
}

impl ActivationAccumulator {
    fn add(&mut self, activation: &[f32]) -> Result<()> {
        if self.count == 0 {
            self.hidden_size = activation.len();
            self.sum = activation.to_vec();
        } else {
            if activation.len() != self.hidden_size {
                return Err(CandleNodeError::configuration(
                    "emotion-extractor",
                    format!(
                        "Activation length {} != expected {}",
                        activation.len(),
                        self.hidden_size
                    ),
                ));
            }
            for (s, &v) in self.sum.iter_mut().zip(activation) {
                *s += v;
            }
        }
        self.count += 1;
        Ok(())
    }

    fn mean(&self) -> Result<Vec<f32>> {
        if self.count == 0 {
            return Err(CandleNodeError::configuration(
                "emotion-extractor",
                "No activations accumulated",
            ));
        }
        let count = self.count as f32;
        Ok(self.sum.iter().map(|&x| x / count).collect())
    }
}

/// Emotion vector extraction node.
///
/// Accumulates `RuntimeData::Tensor` activations keyed by emotion label
/// (carried in tensor metadata), then computes mean-subtraction vectors.
pub struct EmotionExtractorNode {
    node_id: String,
    config: EmotionExtractConfig,
    device: InferenceDevice,
    /// Per-emotion activation accumulators
    accumulators: RwLock<HashMap<String, ActivationAccumulator>>,
    /// Current default emotion label (set via control message)
    current_emotion: RwLock<String>,
    /// Whether initialization is complete
    initialized: RwLock<bool>,
}

impl EmotionExtractorNode {
    pub fn new(node_id: impl Into<String>, config: &EmotionExtractConfig) -> Result<Self> {
        config.validate().map_err(|e| {
            CandleNodeError::configuration("emotion-extractor", e)
        })?;

        let device = DeviceSelector::from_config(&config.model)
            .unwrap_or(InferenceDevice::Cpu);

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            device,
            accumulators: RwLock::new(HashMap::new()),
            current_emotion: RwLock::new("default".to_string()),
            initialized: RwLock::new(false),
        })
    }

    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self> {
        let config: EmotionExtractConfig = serde_json::from_value(params.clone())
            .map_err(|e| CandleNodeError::configuration("emotion-extractor", e.to_string()))?;
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
                        "emotion-extractor",
                        format!("Unsupported tensor dtype: {} (expected 0=float32)", dtype),
                    ));
                }

                if bytes.len() % 4 != 0 {
                    return Err(CandleNodeError::configuration(
                        "emotion-extractor",
                        "Tensor data length is not a multiple of 4",
                    ));
                }

                let n_elements = bytes.len() / 4;
                let expected = shape.iter().map(|&x| x as usize).product::<usize>();
                if expected > 0 && n_elements != expected {
                    return Err(CandleNodeError::configuration(
                        "emotion-extractor",
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
                "emotion-extractor",
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

    /// Accumulate an activation vector for the given emotion.
    async fn accumulate(&self, emotion: &str, activation: Vec<f32>) -> Result<()> {
        let mut accums = self.accumulators.write().await;
        accums
            .entry(emotion.to_string())
            .or_default()
            .add(&activation)?;
        Ok(())
    }

    /// Compute and emit emotion vectors from accumulated data.
    async fn compute_vectors(&self) -> Result<Vec<RuntimeData>> {
        let accums = self.accumulators.read().await;

        let neutral = match accums.get("neutral") {
            Some(n) => n.mean()?,
            None => {
                warn!(
                    node = "emotion-extractor",
                    "No neutral baseline found; vectors will be raw means (not difference vectors)"
                );
                vec![0.0; self.config.hidden_size]
            }
        };

        let mut outputs = Vec::new();

        // Collect and sort for deterministic output order
        let mut emotions: Vec<_> = accums.iter().collect();
        emotions.sort_by_key(|(k, _)| k.clone());

        for (emotion, accum) in emotions {
            if emotion == "neutral" {
                continue; // Skip neutral — it's the baseline
            }

            let mean = accum.mean()?;
            let mut diff = subtract_vectors(&mean, &neutral)?;
            let raw_norm = diff.iter().map(|x| x * x).sum::<f32>().sqrt();

            if self.config.normalize {
                l2_normalize(&mut diff);
            }

            // Emit as Tensor
            let tensor_data: Vec<u8> = diff
                .iter()
                .flat_map(|&x| x.to_le_bytes())
                .collect();

            outputs.push(RuntimeData::Tensor {
                data: tensor_data,
                shape: vec![diff.len() as i32],
                dtype: 0, // float32
                metadata: Some(serde_json::json!({
                    "emotion": emotion,
                    "layer": self.config.layers[0],
                    "hidden_size": diff.len(),
                    "normalized": self.config.normalize,
                    "raw_norm": raw_norm,
                })),
            });

            // Emit metadata as Json
            let metadata = EmotionVectorMetadata {
                model: self.config.model.clone(),
                layer: self.config.layers[0],
                hidden_size: diff.len(),
                emotion: emotion.clone(),
                pooling: self.config.pooling,
                n_positive: accum.count,
                n_neutral: accums.get("neutral").map(|n| n.count).unwrap_or(0),
                raw_norm: raw_norm as f32,
                dataset_hash: String::new(),
                system_prompt: self.config.system_prompt.clone(),
                normalized: self.config.normalize,
            };

            outputs.push(RuntimeData::Json(serde_json::to_value(&metadata).unwrap_or_default()));

            info!(
                node = "emotion-extractor",
                emotion = %emotion,
                layer = ?self.config.layers[0],
                n_samples = accum.count,
                raw_norm = raw_norm,
                "Computed emotion vector"
            );
        }

        Ok(outputs)
    }

    /// Reset all accumulators.
    async fn reset(&self) {
        let mut accums = self.accumulators.write().await;
        accums.clear();
    }
}

#[async_trait]
impl AsyncStreamingNode for EmotionExtractorNode {
    fn node_type(&self) -> &str {
        "EmotionExtractorNode"
    }

    async fn initialize(&self, ctx: &InitializeContext) -> std::result::Result<(), Error> {
        info!(
            node = "emotion-extractor",
            model = %self.config.model,
            layers = ?self.config.layers,
            hidden_size = self.config.hidden_size,
            "Initializing EmotionExtractorNode"
        );

        ctx.emit_progress(
            "loading_node",
            &format!(
                "EmotionExtractorNode: model={}, layers={:?}",
                self.config.model, self.config.layers
            ),
        );

        *self.initialized.write().await = true;
        ctx.emit_progress("ready", "EmotionExtractorNode ready");
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        match &data {
            // Primary path: Tensor activation with emotion label in metadata
            RuntimeData::Tensor { .. } => {
                let activation = Self::tensor_to_f32(&data)
                    .map_err(|e| Error::Execution(e.to_string()))?;
                let emotion = Self::extract_emotion_label(&data);

                self.accumulate(&emotion, activation).await
                    .map_err(|e| Error::Execution(e.to_string()))?;

                Ok(RuntimeData::Json(serde_json::json!({
                    "status": "accumulated",
                    "emotion": emotion,
                })))
            }
            // Control: compute trigger
            RuntimeData::Text(text) if text.trim() == "compute" => {
                let vectors = self.compute_vectors().await
                    .map_err(|e| Error::Execution(e.to_string()))?;

                Ok(vectors.into_iter().next().unwrap_or_else(|| {
                    RuntimeData::Json(serde_json::json!({
                        "status": "no_vectors",
                        "message": "No non-neutral emotions accumulated"
                    }))
                }))
            }
            // Control: reset trigger
            RuntimeData::Text(text) if text.trim() == "reset" => {
                self.reset().await;
                Ok(RuntimeData::Json(serde_json::json!({
                    "status": "reset"
                })))
            }
            // Control: set emotion label via JSON
            RuntimeData::Json(value) => {
                if let Some(emotion) = value.get("emotion").and_then(|v| v.as_str()) {
                    *(self.current_emotion.write().await) = emotion.to_string();
                    Ok(RuntimeData::Json(serde_json::json!({
                        "status": "emotion_set",
                        "emotion": emotion,
                    })))
                } else if value.get("compute").map_or(false, |v| v.as_bool().unwrap_or(false)) {
                    let vectors = self.compute_vectors().await
                        .map_err(|e| Error::Execution(e.to_string()))?;
                    Ok(vectors.into_iter().next().unwrap_or_else(|| {
                        RuntimeData::Json(serde_json::json!({
                            "status": "no_vectors"
                        }))
                    }))
                } else if value.get("reset").map_or(false, |v| v.as_bool().unwrap_or(false)) {
                    self.reset().await;
                    Ok(RuntimeData::Json(serde_json::json!({
                        "status": "reset"
                    })))
                } else {
                    Err(Error::Execution(
                        "EmotionExtractorNode: JSON input must contain 'emotion', 'compute', or 'reset' field".into(),
                    ))
                }
            }
            other => Err(Error::Execution(format!(
                "EmotionExtractorNode accepts Tensor or control messages, got {}",
                other.data_type()
            ))),
        }
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        _session_id: Option<String>,
        mut callback: F,
    ) -> std::result::Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> std::result::Result<(), Error> + Send,
    {
        // For "compute" trigger, emit all vectors
        if let RuntimeData::Text(ref text) = data {
            if text.trim() == "compute" {
                let vectors = self.compute_vectors().await
                    .map_err(|e| Error::Execution(e.to_string()))?;

                let mut count = 0;
                for vec in vectors {
                    callback(vec)?;
                    count += 1;
                }
                return Ok(count);
            }
        }

        // Default: single output
        let output = self.process(data).await?;
        callback(output)?;
        Ok(1)
    }

    async fn process_control_message(
        &self,
        message: RuntimeData,
        _session_id: Option<String>,
    ) -> std::result::Result<bool, Error> {
        // Control message handling - pass through for now
        if let RuntimeData::ControlMessage { .. } = message {
            debug!("Received control message for emotion extraction");
        }
        Ok(false)
    }
}

/// Wrapper for EmotionExtractorNode
pub struct EmotionExtractorNodeWrapper(pub Arc<EmotionExtractorNode>);

#[async_trait]
impl StreamingNode for EmotionExtractorNodeWrapper {
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

/// Emotion extractor node factory
pub struct EmotionExtractorNodeFactory;

impl EmotionExtractorNodeFactory {
    pub fn new() -> Self { Self }
}

impl Default for EmotionExtractorNodeFactory {
    fn default() -> Self { Self::new() }
}

impl StreamingNodeFactory for EmotionExtractorNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> std::result::Result<Box<dyn StreamingNode>, Error> {
        let node = EmotionExtractorNode::from_params(node_id, params)
            .map_err(|e| Error::Execution(e.to_string()))?;
        Ok(Box::new(EmotionExtractorNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str { "EmotionExtractorNode" }
    fn capability_behavior(&self) -> CapabilityBehavior { CapabilityBehavior::Static }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }

    fn schema(&self) -> Option<remotemedia_core::nodes::schema::NodeSchema> {
        use remotemedia_core::nodes::schema::{
            LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType,
        };
        Some(
            NodeSchema::new("EmotionExtractorNode")
                .description(
                    "Extracts emotion direction vectors from labelled activations. \
                     Accepts RuntimeData::Tensor inputs with emotion labels in metadata. \
                     Computes mean-subtraction vectors against a neutral baseline, \
                     and emits L2-normalized direction vectors as RuntimeData::Tensor.",
                )
                .category("ml")
                .accepts([
                    RuntimeDataType::Tensor,
                    RuntimeDataType::Text,
                    RuntimeDataType::Json,
                ])
                .produces([RuntimeDataType::Tensor, RuntimeDataType::Json])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: true,
                    batch_aware: true,
                    supports_control: true,
                    latency_class: LatencyClass::Medium,
                })
                .config_schema_from::<EmotionExtractConfig>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let config = EmotionExtractConfig::default();
        let node = EmotionExtractorNode::new("test-extractor", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_factory() {
        let factory = EmotionExtractorNodeFactory::new();
        assert_eq!(factory.node_type(), "EmotionExtractorNode");
        assert!(factory.is_multi_output_streaming());
    }

    #[test]
    fn test_accumulator() {
        let mut acc = ActivationAccumulator::default();
        acc.add(&[1.0, 2.0, 3.0]).unwrap();
        acc.add(&[3.0, 4.0, 5.0]).unwrap();
        let mean = acc.mean().unwrap();
        assert_eq!(mean, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_tensor_to_f32() {
        let bytes: Vec<u8> = [1.0f32, 2.0f32]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();

        let tensor = RuntimeData::Tensor {
            data: bytes,
            shape: vec![2],
            dtype: 0,
            metadata: None,
        };

        let vec = EmotionExtractorNode::tensor_to_f32(&tensor).unwrap();
        assert!((vec[0] - 1.0).abs() < 1e-6);
        assert!((vec[1] - 2.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_accumulate_and_compute() {
        let config = EmotionExtractConfig::default();
        let node = EmotionExtractorNode::new("test", &config).unwrap();

        node.accumulate("neutral", vec![1.0, 1.0, 1.0]).await.unwrap();
        node.accumulate("neutral", vec![3.0, 3.0, 3.0]).await.unwrap();
        node.accumulate("happy", vec![5.0, 5.0, 5.0]).await.unwrap();
        node.accumulate("happy", vec![7.0, 7.0, 7.0]).await.unwrap();

        // happy_mean=[6,6,6] - neutral_mean=[2,2,2] = [4,4,4]
        let outputs = node.compute_vectors().await.unwrap();
        assert!(!outputs.is_empty());

        if let RuntimeData::Tensor { data, shape, dtype, .. } = &outputs[0] {
            assert_eq!(dtype, &0);
            assert_eq!(shape, &[3]);
            let vec: Vec<f32> = data.chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let expected = 4.0 / (48.0f32).sqrt();
            for &v in &vec {
                assert!((v - expected).abs() < 1e-5, "vector element {} != {}", v, expected);
            }
        } else {
            panic!("Expected tensor output");
        }
    }
}
