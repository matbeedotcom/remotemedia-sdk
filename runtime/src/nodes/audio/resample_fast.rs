use crate::audio::buffer::{AudioBuffer, AudioData, AudioFormat};
use crate::error::{Error, Result};
use crate::nodes::audio::fast::FastAudioNode;
use rubato::{FftFixedIn, Resampler as RubatoResampler};

/// Quality settings for audio resampling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleQuality {
    Low,
    Medium,
    High,
}

impl ResampleQuality {
    fn chunk_size(&self) -> usize {
        match self {
            ResampleQuality::Low => 512,
            ResampleQuality::Medium => 1024,
            ResampleQuality::High => 2048,
        }
    }

    fn sinc_len(&self) -> usize {
        match self {
            ResampleQuality::Low => 64,
            ResampleQuality::Medium => 128,
            ResampleQuality::High => 256,
        }
    }
}

/// Fast path resampling node - no JSON overhead
pub struct FastResampleNode {
    resampler: FftFixedIn<f32>,
    target_rate: u32,
}

impl FastResampleNode {
    pub fn new(
        source_rate: u32,
        target_rate: u32,
        quality: ResampleQuality,
        channels: usize,
    ) -> Result<Self> {
        let chunk_size = quality.chunk_size();
        let sinc_len = quality.sinc_len();

        let resampler = FftFixedIn::new(
            source_rate as usize,
            target_rate as usize,
            chunk_size,
            sinc_len,
            channels,
        )
        .map_err(|e| Error::Execution(format!("Failed to create resampler: {}", e)))?;

        Ok(Self {
            resampler,
            target_rate,
        })
    }
}

impl FastAudioNode for FastResampleNode {
    fn node_type(&self) -> &str {
        "FastResampleNode"
    }

    fn process_audio(&mut self, input: AudioData) -> Result<AudioData> {
        // Get F32 buffer
        let samples = input
            .buffer
            .as_f32()
            .ok_or_else(|| Error::Execution("Resample requires F32 format".into()))?;

        let channels = input.channels;
        let frames_in = samples.len() / channels;

        // Deinterleave input channels for rubato
        let mut input_frames: Vec<Vec<f32>> = vec![Vec::with_capacity(frames_in); channels];
        for (i, &sample) in samples.iter().enumerate() {
            input_frames[i % channels].push(sample);
        }

        // Process with rubato (direct buffer access)
        let output_frames = self
            .resampler
            .process(&input_frames, None)
            .map_err(|e| Error::Execution(format!("Resampling failed: {}", e)))?;

        // Interleave output channels
        let frames_out = output_frames[0].len();
        let mut output_samples = Vec::with_capacity(frames_out * channels);
        for frame_idx in 0..frames_out {
            for channel_idx in 0..channels {
                output_samples.push(output_frames[channel_idx][frame_idx]);
            }
        }

        Ok(AudioData::new(
            AudioBuffer::new_f32(output_samples),
            self.target_rate,
            channels,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_resample_creation() {
        let node = FastResampleNode::new(44100, 16000, ResampleQuality::Medium, 2);
        assert!(node.is_ok());
    }

    #[test]
    fn test_fast_resample_process() {
        let mut node = FastResampleNode::new(44100, 16000, ResampleQuality::Low, 2).unwrap();

        // Generate 1 second of test audio (44100 samples per channel, 2 channels)
        let samples: Vec<f32> = (0..88200)
            .map(|i| {
                let t = (i / 2) as f32 / 44100.0;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5
            })
            .collect();

        let input = AudioData::new(AudioBuffer::new_f32(samples), 44100, 2);

        let result = node.process_audio(input);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output.sample_rate, 16000);
        assert_eq!(output.channels, 2);

        // Output should have approximately 32000 samples (16000 per channel * 2)
        let expected_samples = (16000 * 2) as f32;
        let actual_samples = output.buffer.len() as f32;
        let ratio = actual_samples / expected_samples;
        assert!(
            (ratio - 1.0).abs() < 0.1,
            "Expected ~{} samples, got {}",
            expected_samples,
            actual_samples
        );
    }
}
