use crate::audio::buffer::AudioData;
/// Streaming wrapper for FastResampleNode
///
/// Adapts the synchronous FastResampleNode (FastAudioNode trait) to work
/// in the async streaming pipeline (AsyncStreamingNode trait).
use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::{
    audio::{FastAudioNode, FastResampleNode, ResampleQuality},
    AsyncStreamingNode,
};
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
        if data.data_type() != "audio" {
            return Ok(data);
        }
        // Extract audio data
        let (f32_samples, input_sample_rate, input_channels) = match &data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                stream_id: _,
            } => (samples.clone(), *sample_rate, *channels),
            _ => {
                return Err(Error::InvalidInput {
                    message: format!("Expected Audio, got {:?}", data.data_type()),
                    node_id: "FastResampleNode".into(),
                    context: "process".into(),
                });
            }
        };

        // Lock and process - need to chunk input for FftFixedIn resampler
        let mut inner = self.inner.lock().await;

        // FftFixedIn requires fixed chunk sizes - split input into chunks
        let chunk_size = 1024; // Match Medium quality chunk size
        let total_samples = f32_samples.len();

        if total_samples <= chunk_size {
            // Small enough to process directly
            let audio_data = AudioData::new(
                crate::audio::buffer::AudioBuffer::new_f32(f32_samples),
                input_sample_rate,
                input_channels as usize,
            );

            let resampled = inner.process_audio(audio_data)?;
            drop(inner); // Release lock early

            // Convert f32 samples to bytes
            let f32_samples = resampled
                .buffer
                .as_f32()
                .ok_or_else(|| Error::Execution("Resampler output must be F32".into()))?;

            let _num_samples = f32_samples.len() as u64;  // TODO: Use for metadata

            let _bytes: Vec<u8> = f32_samples.iter().flat_map(|&f| f.to_le_bytes()).collect();  // TODO: Support byte format output

            // Return resampled audio as RuntimeData
            return Ok(RuntimeData::Audio {
                samples: f32_samples.to_vec(),
                sample_rate: resampled.sample_rate,
                channels: resampled.channels as u32,
                stream_id: None,
            });
        }

        // Large buffer - process in chunks
        tracing::info!(
            "Resampling large buffer: {} samples in chunks of {}",
            total_samples,
            chunk_size
        );
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
                input_sample_rate,
                input_channels as usize,
            );

            let resampled_chunk = inner.process_audio(chunk_data)?;
            let chunk_out = resampled_chunk
                .buffer
                .as_f32()
                .ok_or_else(|| Error::Execution("Resampler output must be F32".into()))?;

            all_output_samples.extend_from_slice(chunk_out);
        }

        drop(inner); // Release lock

        let num_samples = all_output_samples.len();
        tracing::info!(
            "Resampling complete: {} input samples -> {} output samples",
            total_samples,
            num_samples
        );

        // Use stored target rate
        let target_rate = self.target_rate;

        // Return resampled audio
        Ok(RuntimeData::Audio {
            samples: all_output_samples,
            sample_rate: target_rate,
            channels: input_channels,
            stream_id: None,
        })
    }
}

// =============================================================================
// AutoResampleStreamingNode - Lazy initialization with auto-configuration
// =============================================================================

/// Configuration for auto-resampling behavior
#[derive(Debug, Clone)]
pub struct AutoResampleConfig {
    /// Source sample rate (None = detect from first audio chunk)
    pub source_rate: Option<u32>,
    /// Target sample rate (None = use configured value or passthrough)
    pub target_rate: Option<u32>,
    /// Resampling quality
    pub quality: ResampleQuality,
    /// Number of channels (None = detect from first audio chunk)
    pub channels: Option<usize>,
}

impl Default for AutoResampleConfig {
    fn default() -> Self {
        Self {
            source_rate: None,
            target_rate: None,
            quality: ResampleQuality::Medium,
            channels: None,
        }
    }
}

/// Auto-configuring resample node with lazy initialization.
///
/// This node supports automatic sample rate detection and configuration:
/// - `source_rate`: If None, detected from first incoming audio chunk
/// - `target_rate`: If None, defaults to passthrough (no resampling)
///
/// The resampler is created lazily on the first `process()` call when we
/// have enough information to configure it.
///
/// # Capability Behavior
///
/// This node has `Adaptive` capability behavior:
/// - Input: Accepts any sample rate (8kHz-192kHz)
/// - Output: Adapts to downstream requirements during capability resolution
///
/// When used without explicit `target_rate`, the resolver's reverse pass
/// will set the output rate based on what downstream nodes require.
pub struct AutoResampleStreamingNode {
    node_id: String,
    config: AutoResampleConfig,
    /// Lazily initialized resampler
    inner: Mutex<Option<FastResampleNode>>,
    /// Resolved target rate (set during initialization or from config)
    resolved_target_rate: Mutex<Option<u32>>,
    /// Resolved source rate (detected from first chunk)
    resolved_source_rate: Mutex<Option<u32>>,
    /// Resolved channels (detected from first chunk)
    resolved_channels: Mutex<Option<usize>>,
}

impl AutoResampleStreamingNode {
    pub fn new(node_id: String, config: AutoResampleConfig) -> Self {
        let resolved_target = config.target_rate;
        let resolved_source = config.source_rate;
        let resolved_channels = config.channels;

        Self {
            node_id,
            config,
            inner: Mutex::new(None),
            resolved_target_rate: Mutex::new(resolved_target),
            resolved_source_rate: Mutex::new(resolved_source),
            resolved_channels: Mutex::new(resolved_channels),
        }
    }

    /// Set the target sample rate (called during capability resolution).
    pub async fn set_target_rate(&self, rate: u32) {
        *self.resolved_target_rate.lock().await = Some(rate);
    }

    /// Get the resolved target rate.
    pub async fn get_target_rate(&self) -> Option<u32> {
        *self.resolved_target_rate.lock().await
    }

    /// Initialize the resampler with detected/configured rates.
    async fn ensure_initialized(
        &self,
        source_rate: u32,
        target_rate: u32,
        channels: usize,
    ) -> Result<()> {
        let mut inner = self.inner.lock().await;
        if inner.is_some() {
            return Ok(()); // Already initialized
        }

        // Store resolved values
        *self.resolved_source_rate.lock().await = Some(source_rate);
        *self.resolved_channels.lock().await = Some(channels);

        // If source and target are the same, we can skip resampling
        if source_rate == target_rate {
            tracing::info!(
                "[{}] No resampling needed (source == target = {}Hz)",
                self.node_id,
                source_rate
            );
            return Ok(());
        }

        tracing::info!(
            "[{}] Creating resampler: {}Hz -> {}Hz, {} channels, quality: {:?}",
            self.node_id,
            source_rate,
            target_rate,
            channels,
            self.config.quality
        );

        let resampler = FastResampleNode::new(
            source_rate,
            target_rate,
            self.config.quality,
            channels,
        )?;

        *inner = Some(resampler);
        Ok(())
    }
}

#[async_trait]
impl AsyncStreamingNode for AutoResampleStreamingNode {
    fn node_type(&self) -> &str {
        "FastResampleNode"
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData> {
        // Passthrough non-audio data
        if data.data_type() != "audio" {
            return Ok(data);
        }

        // Extract audio data
        let (f32_samples, input_sample_rate, input_channels) = match &data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                stream_id: _,
            } => (samples.clone(), *sample_rate, *channels as usize),
            _ => {
                return Err(Error::InvalidInput {
                    message: format!("Expected Audio, got {:?}", data.data_type()),
                    node_id: self.node_id.clone(),
                    context: "process".into(),
                });
            }
        };

        // Determine source rate (from config or incoming data)
        let source_rate = self.config.source_rate.unwrap_or(input_sample_rate);

        // Determine target rate (from config, resolved value, or passthrough)
        let target_rate = {
            let resolved = *self.resolved_target_rate.lock().await;
            resolved.or(self.config.target_rate).unwrap_or(input_sample_rate)
        };

        // Determine channels (from config or incoming data)
        let channels = self.config.channels.unwrap_or(input_channels);

        // Ensure resampler is initialized
        self.ensure_initialized(source_rate, target_rate, channels).await?;

        // If no resampling needed, passthrough
        if source_rate == target_rate {
            return Ok(RuntimeData::Audio {
                samples: f32_samples,
                sample_rate: target_rate,
                channels: channels as u32,
                stream_id: None,
            });
        }

        // Process through resampler
        let mut inner_guard = self.inner.lock().await;
        let inner = inner_guard.as_mut().ok_or_else(|| Error::Execution(
            format!("[{}] Resampler not initialized", self.node_id)
        ))?;

        let chunk_size = 1024;
        let total_samples = f32_samples.len();

        if total_samples <= chunk_size {
            // Small buffer - process directly
            let audio_data = AudioData::new(
                crate::audio::buffer::AudioBuffer::new_f32(f32_samples),
                input_sample_rate,
                channels,
            );

            let resampled = inner.process_audio(audio_data)?;
            let f32_samples = resampled
                .buffer
                .as_f32()
                .ok_or_else(|| Error::Execution("Resampler output must be F32".into()))?;

            return Ok(RuntimeData::Audio {
                samples: f32_samples.to_vec(),
                sample_rate: resampled.sample_rate,
                channels: resampled.channels as u32,
                stream_id: None,
            });
        }

        // Large buffer - process in chunks
        let mut all_output_samples = Vec::new();

        for chunk_start in (0..total_samples).step_by(chunk_size) {
            let chunk_end = (chunk_start + chunk_size).min(total_samples);
            let chunk_samples = &f32_samples[chunk_start..chunk_end];

            let mut chunk_vec = chunk_samples.to_vec();
            if chunk_vec.len() < chunk_size {
                chunk_vec.resize(chunk_size, 0.0);
            }

            let chunk_data = AudioData::new(
                crate::audio::buffer::AudioBuffer::new_f32(chunk_vec),
                input_sample_rate,
                channels,
            );

            let resampled_chunk = inner.process_audio(chunk_data)?;
            let chunk_out = resampled_chunk
                .buffer
                .as_f32()
                .ok_or_else(|| Error::Execution("Resampler output must be F32".into()))?;

            all_output_samples.extend_from_slice(chunk_out);
        }

        Ok(RuntimeData::Audio {
            samples: all_output_samples,
            sample_rate: target_rate,
            channels: channels as u32,
            stream_id: None,
        })
    }
}
