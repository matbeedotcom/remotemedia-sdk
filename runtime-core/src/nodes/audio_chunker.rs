//! Audio Chunker Node
//!
//! This node splits incoming audio into fixed-size chunks. It's useful for:
//! - Feeding audio to models that require specific chunk sizes (e.g., Silero VAD needs 512 samples)
//! - Rate limiting or batching audio processing
//! - Buffering partial chunks until enough samples are available
//!
//! Features:
//! - Configurable chunk size
//! - Maintains state per session to handle partial chunks
//! - Streaming output - emits chunks as they become available

use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::AsyncStreamingNode;
use crate::data::AudioBuffer as ProtoAudioBuffer;
use async_trait::async_trait;
use tracing::info;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Chunker state for a session
#[derive(Debug, Clone)]
struct ChunkerState {
    /// Buffered samples waiting to form a complete chunk
    buffer: Vec<f32>,
    /// Sample rate of buffered audio
    sample_rate: u32,
    /// Number of channels in buffered audio
    channels: u32,
    /// Audio format (1 = F32, per protobuf AudioFormat enum)
    format: i32,
}

impl Default for ChunkerState {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            sample_rate: 16000,
            channels: 1,
            format: 1, // AUDIO_FORMAT_F32
        }
    }
}

/// Audio Chunker Node
pub struct AudioChunkerNode {
    /// Target chunk size in samples
    chunk_size: usize,
    /// State per session (buffers partial chunks)
    states: Arc<Mutex<std::collections::HashMap<String, ChunkerState>>>,
}

impl AudioChunkerNode {
    pub fn new(chunk_size: Option<usize>) -> Self {
        Self {
            chunk_size: chunk_size.unwrap_or(512),
            states: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Convert ProtoAudioBuffer samples to f32 vector
    fn samples_to_f32(&self, audio_buf: &ProtoAudioBuffer) -> Result<Vec<f32>> {
        match audio_buf.format {
            1 => {
                // AUDIO_FORMAT_F32
                let sample_count = audio_buf.samples.len() / 4;
                Ok((0..sample_count)
                    .map(|i| {
                        let offset = i * 4;
                        f32::from_le_bytes([
                            audio_buf.samples[offset],
                            audio_buf.samples[offset + 1],
                            audio_buf.samples[offset + 2],
                            audio_buf.samples[offset + 3],
                        ])
                    })
                    .collect())
            }
            _ => Err(Error::Execution(format!("AudioChunkerNode only supports F32 audio format (received format {})", audio_buf.format))),
        }
    }

    /// Convert f32 samples back to ProtoAudioBuffer
    fn f32_to_audio_buffer(&self, samples: &[f32], state: &ChunkerState) -> ProtoAudioBuffer {
        let mut sample_bytes = Vec::with_capacity(samples.len() * 4);
        for &sample in samples {
            sample_bytes.extend_from_slice(&sample.to_le_bytes());
        }

        ProtoAudioBuffer {
            samples: sample_bytes,
            sample_rate: state.sample_rate,
            channels: state.channels,
            format: state.format,
            num_samples: samples.len() as u64,
        }
    }
}

#[async_trait]
impl AsyncStreamingNode for AudioChunkerNode {
    fn node_type(&self) -> &str {
        "AudioChunkerNode"
    }

    async fn process(&self, _data: RuntimeData) -> Result<RuntimeData> {
        Err(Error::Execution(
            "AudioChunkerNode requires streaming mode - use process_streaming() instead".into()
        ))
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize>
    where
        F: FnMut(RuntimeData) -> Result<()> + Send,
    {
        let (input_samples, input_sample_rate, input_channels) = match data {
            RuntimeData::Audio { ref samples, sample_rate, channels } => {
                (samples.clone(), sample_rate, channels)
            }
            _ => return Err(Error::Execution("AudioChunkerNode requires audio input".into())),
        };

        // Get or create state for this session
        let session_key = session_id.unwrap_or_else(|| "default".to_string());
        let mut states = self.states.lock().await;
        let state = states.entry(session_key).or_insert_with(ChunkerState::default);

        // Update state with current audio format
        state.sample_rate = input_sample_rate;
        state.channels = input_channels;
        state.format = 1; // F32

        // Add new samples to buffer (already f32)
        state.buffer.extend_from_slice(&input_samples);

        let mut output_count = 0;

        // Emit chunks while we have enough samples
        while state.buffer.len() >= self.chunk_size {
            // Extract one chunk
            let chunk: Vec<f32> = state.buffer.drain(..self.chunk_size).collect();

            // Emit chunk with current state's format
            callback(RuntimeData::Audio {
                samples: chunk,
                sample_rate: state.sample_rate,
                channels: state.channels,
            })?;
            output_count += 1;
        }

        tracing::debug!(
            "AudioChunker: processed {} samples, emitted {} chunks, {} samples buffered",
            input_samples.len(),
            output_count,
            state.buffer.len()
        );

        Ok(output_count)
    }
}
