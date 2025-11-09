use crate::audio::{AudioBuffer, AudioFormat};
use crate::error::{Error, Result};
use crate::executor::node_executor::{NodeContext, NodeExecutor};
use crate::nodes::registry::NodeFactory;
use rubato::{FftFixedIn, Resampler as RubatoResampler};
use serde_json::Value;
use std::sync::Arc;

/// Quality settings for audio resampling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleQuality {
    Low,
    Medium,
    High,
}

impl ResampleQuality {
    /// Get rubato chunk size based on quality (affects latency vs quality tradeoff)
    fn chunk_size(&self) -> usize {
        match self {
            ResampleQuality::Low => 512,
            ResampleQuality::Medium => 1024,
            ResampleQuality::High => 2048,
        }
    }

    /// Get sinc length for interpolation quality
    fn sinc_len(&self) -> usize {
        match self {
            ResampleQuality::Low => 64,
            ResampleQuality::Medium => 128,
            ResampleQuality::High => 256,
        }
    }
}

/// Rust-native audio resampling node using rubato
pub struct RustResampleNode {
    resampler: Option<FftFixedIn<f32>>,
    source_rate: u32,
    target_rate: u32,
    quality: ResampleQuality,
    channels: usize,
}

impl RustResampleNode {
    pub fn new(
        source_rate: u32,
        target_rate: u32,
        quality: ResampleQuality,
        channels: usize,
    ) -> Self {
        Self {
            resampler: None,
            source_rate,
            target_rate,
            quality,
            channels,
        }
    }

    fn create_resampler(&self) -> Result<FftFixedIn<f32>> {
        let chunk_size = self.quality.chunk_size();
        let sinc_len = self.quality.sinc_len();

        FftFixedIn::new(
            self.source_rate as usize,
            self.target_rate as usize,
            chunk_size,
            sinc_len,
            self.channels,
        )
        .map_err(|e| Error::Execution(format!("Failed to create resampler: {}", e)))
    }
}

#[async_trait::async_trait]
impl NodeExecutor for RustResampleNode {
    async fn initialize(&mut self, _context: &NodeContext) -> Result<()> {
        self.resampler = Some(self.create_resampler()?);
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        let resampler = self
            .resampler
            .as_mut()
            .ok_or_else(|| Error::Execution("Resampler not initialized".into()))?;

        // Extract audio data from input
        let audio_data = input
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| Error::Execution("Missing audio data array".into()))?;

        let samples: Vec<f32> = audio_data
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        let sample_rate = input
            .get("sample_rate")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::Execution("Missing sample_rate".into()))?
            as u32;

        // Validate sample rate
        if sample_rate != self.source_rate {
            return Err(Error::Execution(format!(
                "Input sample rate {} doesn't match expected {}",
                sample_rate, self.source_rate
            )));
        }

        // Prepare input frames for rubato
        let frames_in = samples.len() / self.channels;
        let mut input_frames: Vec<Vec<f32>> = vec![Vec::with_capacity(frames_in); self.channels];
        for (i, sample) in samples.iter().enumerate() {
            input_frames[i % self.channels].push(*sample);
        }

        // Process with rubato
        let output_frames = resampler
            .process(&input_frames, None)
            .map_err(|e| Error::Execution(format!("Resampling failed: {}", e)))?;

        // Interleave output channels
        let frames_out = output_frames[0].len();
        let mut output_samples = Vec::with_capacity(frames_out * self.channels);
        for frame_idx in 0..frames_out {
            for channel in 0..self.channels {
                output_samples.push(output_frames[channel][frame_idx]);
            }
        }

        let output = serde_json::json!({
            "data": output_samples,
            "sample_rate": self.target_rate,
            "channels": self.channels
        });

        Ok(vec![output])
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.resampler = None;
        Ok(())
    }
}

/// Factory for creating RustResampleNode instances
pub struct ResampleNodeFactory {
    source_rate: u32,
    target_rate: u32,
    quality: ResampleQuality,
    channels: usize,
}

impl ResampleNodeFactory {
    pub fn new(
        source_rate: u32,
        target_rate: u32,
        quality: ResampleQuality,
        channels: usize,
    ) -> Self {
        Self {
            source_rate,
            target_rate,
            quality,
            channels,
        }
    }
}

impl NodeFactory for ResampleNodeFactory {
    fn create(&self, _params: Value) -> Result<Box<dyn NodeExecutor>> {
        Ok(Box::new(RustResampleNode::new(
            self.source_rate,
            self.target_rate,
            self.quality,
            self.channels,
        )))
    }

    fn node_type(&self) -> &str {
        "RustResampleNode"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resample_node_creation() {
        let node = RustResampleNode::new(16000, 48000, ResampleQuality::Medium, 1);
        assert_eq!(node.source_rate, 16000);
        assert_eq!(node.target_rate, 48000);
        assert_eq!(node.channels, 1);
    }

    #[tokio::test]
    async fn test_resample_quality_settings() {
        assert_eq!(ResampleQuality::Low.chunk_size(), 512);
        assert_eq!(ResampleQuality::Medium.chunk_size(), 1024);
        assert_eq!(ResampleQuality::High.chunk_size(), 2048);
    }

    #[tokio::test]
    async fn test_resample_node_initialize() {
        let mut node = RustResampleNode::new(16000, 48000, ResampleQuality::Medium, 1);
        let context = NodeContext {
            node_id: "test".to_string(),
            node_type: "resample".to_string(),
            params: serde_json::json!({}),
            metadata: std::collections::HashMap::new(),
        };

        let result = node.initialize(&context).await;
        assert!(result.is_ok());
        assert!(node.resampler.is_some());
    }

    #[tokio::test]
    async fn test_resample_factory() {
        let factory = ResampleNodeFactory::new(16000, 48000, ResampleQuality::High, 2);
        let node = factory.create(serde_json::json!({}));

        assert!(node.is_ok());
    }
}
