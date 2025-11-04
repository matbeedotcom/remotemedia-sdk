/// Streaming wrapper for FastResampleNode
///
/// Adapts the synchronous FastResampleNode (FastAudioNode trait) to work
/// in the async streaming pipeline (AsyncStreamingNode trait).

use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::{AsyncStreamingNode, audio::{FastResampleNode, FastAudioNode}};
use crate::audio::buffer::AudioData;
use crate::grpc_service::generated::{AudioBuffer, DataTypeHint};
use async_trait::async_trait;
use tokio::sync::Mutex;

pub struct ResampleStreamingNode {
    inner: Mutex<FastResampleNode>,
    target_rate: u32,
}

impl ResampleStreamingNode {
    pub fn new(inner: FastResampleNode, target_rate: u32) -> Self {
        Self {
            inner: Mutex::new(inner),
            target_rate,
        }
    }
}

#[async_trait]
impl AsyncStreamingNode for ResampleStreamingNode {
    fn node_type(&self) -> &str {
        "FastResampleNode"
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData> {
        // if not audio, passthrough
        if data.data_type() != DataTypeHint::Audio {
            return Ok(data);
        }
        // Extract audio buffer
        let audio_buf = match &data {
            RuntimeData::Audio(buf) => buf,
            _ => {
                return Err(Error::InvalidInput {
                    message: format!("Expected Audio, got {:?}", data.data_type()),
                    node_id: "FastResampleNode".into(),
                    context: "process".into(),
                });
            }
        };

        // Convert protobuf AudioBuffer to AudioData
        // Extract f32 samples from bytes
        let f32_samples: Vec<f32> = audio_buf
            .samples
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        // Lock and process - need to chunk input for FftFixedIn resampler
        let mut inner = self.inner.lock().await;

        // FftFixedIn requires fixed chunk sizes - split input into chunks
        let chunk_size = 1024; // Match Medium quality chunk size
        let total_samples = f32_samples.len();

        if total_samples <= chunk_size {
            // Small enough to process directly
            let audio_data = AudioData::new(
                crate::audio::buffer::AudioBuffer::new_f32(f32_samples),
                audio_buf.sample_rate,
                audio_buf.channels as usize,
            );

            let resampled = inner.process_audio(audio_data)?;
            drop(inner); // Release lock early

            // Convert f32 samples to bytes
            let f32_samples = resampled.buffer.as_f32()
                .ok_or_else(|| Error::Execution("Resampler output must be F32".into()))?;

            let num_samples = f32_samples.len() as u64;

            let bytes: Vec<u8> = f32_samples
                .iter()
                .flat_map(|&f| f.to_le_bytes())
                .collect();

            let output_buf = AudioBuffer {
                samples: bytes,
                sample_rate: resampled.sample_rate,
                channels: resampled.channels as u32,
                format: 1,
                num_samples,
            };

            return Ok(RuntimeData::Audio(output_buf));
        }

        // Large buffer - process in chunks
        tracing::info!("Resampling large buffer: {} samples in chunks of {}", total_samples, chunk_size);
        let mut all_output_samples = Vec::new();

        for chunk_start in (0..total_samples).step_by(chunk_size) {
            let chunk_end = (chunk_start + chunk_size).min(total_samples);
            let chunk_samples = &f32_samples[chunk_start..chunk_end];

            // Pad last chunk if needed
            let mut chunk_vec = chunk_samples.to_vec();
            if chunk_vec.len() < chunk_size {
                chunk_vec.resize(chunk_size, 0.0);
            }

            let chunk_data = AudioData::new(
                crate::audio::buffer::AudioBuffer::new_f32(chunk_vec),
                audio_buf.sample_rate,
                audio_buf.channels as usize,
            );

            let resampled_chunk = inner.process_audio(chunk_data)?;
            let chunk_out = resampled_chunk.buffer.as_f32()
                .ok_or_else(|| Error::Execution("Resampler output must be F32".into()))?;

            all_output_samples.extend_from_slice(chunk_out);
        }

        drop(inner); // Release lock

        let num_samples = all_output_samples.len() as u64;
        tracing::info!("Resampling complete: {} input samples -> {} output samples", total_samples, num_samples);

        // Convert f32 samples to bytes
        let bytes: Vec<u8> = all_output_samples
            .iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect();

        // Use stored target rate
        let target_rate = self.target_rate;

        // Convert back to RuntimeData
        let output_buf = AudioBuffer {
            samples: bytes,
            sample_rate: target_rate,
            channels: audio_buf.channels,
            format: 1, // AUDIO_FORMAT_F32 per protobuf enum
            num_samples,
        };

        Ok(RuntimeData::Audio(output_buf))
    }
}
