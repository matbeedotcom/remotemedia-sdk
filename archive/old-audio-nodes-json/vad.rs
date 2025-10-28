use crate::audio::{AudioBuffer, AudioFormat};
use crate::error::{Error, Result};
use crate::executor::node_executor::{NodeContext, NodeExecutor};
use crate::nodes::registry::NodeFactory;
use rustfft::{num_complex::Complex, FftPlanner};
use serde_json::Value;
use std::sync::Arc;

/// Voice Activity Detection node using energy-based FFT analysis
pub struct RustVADNode {
    sample_rate: u32,
    frame_duration_ms: u32,
    energy_threshold: f32,
    fft_planner: FftPlanner<f32>,
}

impl RustVADNode {
    pub fn new(sample_rate: u32, frame_duration_ms: u32, energy_threshold: f32) -> Self {
        Self {
            sample_rate,
            frame_duration_ms,
            energy_threshold,
            fft_planner: FftPlanner::new(),
        }
    }

    fn frame_size(&self) -> usize {
        ((self.sample_rate as f32 * self.frame_duration_ms as f32) / 1000.0) as usize
    }

    fn compute_frame_energy(&mut self, frame: &[f32]) -> f32 {
        let frame_size = frame.len();

        // Apply Hamming window
        let windowed: Vec<Complex<f32>> = frame
            .iter()
            .enumerate()
            .map(|(i, &sample)| {
                let window = 0.54
                    - 0.46
                        * (2.0 * std::f32::consts::PI * i as f32 / (frame_size - 1) as f32).cos();
                Complex::new(sample * window, 0.0)
            })
            .collect();

        // Compute FFT
        let mut buffer = windowed.clone();
        let fft = self.fft_planner.plan_fft_forward(frame_size);
        fft.process(&mut buffer);

        // Compute energy as sum of squared magnitudes (only need first half due to symmetry)
        let energy: f32 = buffer
            .iter()
            .take(frame_size / 2)
            .map(|c| c.norm_sqr())
            .sum();

        energy / frame_size as f32
    }
}

#[async_trait::async_trait]
impl NodeExecutor for RustVADNode {
    async fn initialize(&mut self, _context: &NodeContext) -> Result<()> {
        // No initialization needed, FFT planner is already set up
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
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
        if sample_rate != self.sample_rate {
            return Err(Error::Execution(format!(
                "Input sample rate {} doesn't match expected {}",
                sample_rate, self.sample_rate
            )));
        }

        let frame_size = self.frame_size();
        let num_frames = samples.len() / frame_size;

        // Process frames and detect voice activity
        let mut vad_results = Vec::with_capacity(num_frames);

        for frame_idx in 0..num_frames {
            let start = frame_idx * frame_size;
            let end = start + frame_size;

            if end <= samples.len() {
                let frame = &samples[start..end];
                let energy = self.compute_frame_energy(frame);
                let is_speech = energy > self.energy_threshold;
                vad_results.push(is_speech);
            }
        }

        // Create output with VAD results and passthrough audio
        let output = serde_json::json!({
            "data": samples,
            "sample_rate": sample_rate,
            "vad_results": vad_results,
            "is_speech": vad_results.iter().any(|&v| v)
        });

        Ok(vec![output])
    }

    async fn cleanup(&mut self) -> Result<()> {
        // No cleanup needed
        Ok(())
    }
}

/// Factory for creating RustVADNode instances
pub struct VADNodeFactory {
    sample_rate: u32,
    frame_duration_ms: u32,
    energy_threshold: f32,
}

impl VADNodeFactory {
    pub fn new(sample_rate: u32, frame_duration_ms: u32, energy_threshold: f32) -> Self {
        Self {
            sample_rate,
            frame_duration_ms,
            energy_threshold,
        }
    }
}

impl NodeFactory for VADNodeFactory {
    fn create(&self, _params: Value) -> Result<Box<dyn NodeExecutor>> {
        Ok(Box::new(RustVADNode::new(
            self.sample_rate,
            self.frame_duration_ms,
            self.energy_threshold,
        )))
    }

    fn node_type(&self) -> &str {
        "RustVADNode"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_vad_node_creation() {
        let node = RustVADNode::new(16000, 30, 0.01);
        assert_eq!(node.sample_rate, 16000);
        assert_eq!(node.frame_duration_ms, 30);
        assert_eq!(node.energy_threshold, 0.01);
    }

    #[tokio::test]
    async fn test_vad_frame_size() {
        let node = RustVADNode::new(16000, 30, 0.01);
        assert_eq!(node.frame_size(), 480); // 16000 * 0.03 = 480
    }

    #[tokio::test]
    async fn test_vad_node_initialize() {
        let mut node = RustVADNode::new(16000, 30, 0.01);
        let context = NodeContext {
            node_id: "test".to_string(),
            node_type: "vad".to_string(),
            params: serde_json::json!({}),
            metadata: std::collections::HashMap::new(),
        };

        let result = node.initialize(&context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_vad_factory() {
        let factory = VADNodeFactory::new(16000, 30, 0.01);
        let node = factory.create(serde_json::json!({}));

        assert!(node.is_ok());
    }

    #[tokio::test]
    async fn test_vad_energy_computation() {
        let mut node = RustVADNode::new(16000, 30, 0.01);

        // Create silent frame
        let silent_frame = vec![0.0f32; 480];
        let silent_energy = node.compute_frame_energy(&silent_frame);
        assert!(silent_energy < 0.0001);

        // Create noisy frame (sine wave)
        let noisy_frame: Vec<f32> = (0..480)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0).sin() * 0.5)
            .collect();
        let noisy_energy = node.compute_frame_energy(&noisy_frame);
        assert!(noisy_energy > 0.01);
    }
}
