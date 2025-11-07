/// Silero VAD Streaming Node
///
/// High-accuracy voice activity detection using Silero VAD ONNX model.
/// Detects speech/silence in audio streams and outputs VAD events.
///
/// Features:
/// - Speech start/end detection
/// - Configurable thresholds and timing
/// - State management for streaming audio
/// - JSON output with VAD results
use crate::data::RuntimeData;
use crate::data::{AudioBuffer, TensorBuffer};
use crate::error::{Error, Result};
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

#[cfg(feature = "silero-vad")]
use ort::{
    execution_providers::CPUExecutionProvider,
    session::{Session, SessionOutputs},
    value::Tensor,
};

/// VAD state for speech detection
#[derive(Debug, Clone)]
struct VADState {
    /// Is speech currently active
    triggered: bool,
    /// Samples of silence accumulated
    temp_end_samples: usize,
    /// Total samples processed
    current_sample: usize,
    /// ONNX model state (combined, size: 2 * 128 = 256)
    state: Vec<f32>,
}

impl Default for VADState {
    fn default() -> Self {
        Self {
            triggered: false,
            temp_end_samples: 0,
            current_sample: 0,
            // Silero VAD uses combined state of size [2, 128]
            state: vec![0.0; 2 * 128],
        }
    }
}

/// Silero VAD Streaming Node
pub struct SileroVADNode {
    /// Speech probability threshold (0.0-1.0)
    threshold: f32,
    /// Expected sample rate (8000 or 16000)
    sampling_rate: u32,
    /// Minimum speech duration in ms to trigger
    min_speech_duration_ms: u32,
    /// Minimum silence duration in ms to end speech
    min_silence_duration_ms: u32,
    /// Padding before/after speech in ms
    speech_pad_ms: u32,

    #[cfg(feature = "silero-vad")]
    /// ONNX Runtime session (lazy-initialized with auto-download)
    session: OnceCell<Arc<Mutex<Session>>>,

    /// VAD state (one per session_id)
    states: Arc<Mutex<std::collections::HashMap<String, VADState>>>,
}

impl SileroVADNode {
    pub fn new(
        threshold: Option<f32>,
        sampling_rate: Option<u32>,
        min_speech_duration_ms: Option<u32>,
        min_silence_duration_ms: Option<u32>,
        speech_pad_ms: Option<u32>,
    ) -> Self {
        Self {
            threshold: threshold.unwrap_or(0.5),
            sampling_rate: sampling_rate.unwrap_or(16000),
            min_speech_duration_ms: min_speech_duration_ms.unwrap_or(250),
            min_silence_duration_ms: min_silence_duration_ms.unwrap_or(100),
            speech_pad_ms: speech_pad_ms.unwrap_or(30),
            #[cfg(feature = "silero-vad")]
            session: OnceCell::new(),
            states: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    #[cfg(feature = "silero-vad")]
    async fn get_or_init_session(&self) -> Result<&Arc<Mutex<Session>>> {
        self.session.get_or_try_init(|| async {
            tracing::info!("Initializing Silero VAD ONNX model");

            // Model path - use current directory for simplicity
            let model_path = std::path::Path::new("silero_vad.onnx");

            // Download model if it doesn't exist
            if !model_path.exists() {
                tracing::info!("Downloading Silero VAD model...");

                // Download from HuggingFace (direct download link)
                let model_url = "https://huggingface.co/onnx-community/silero-vad/resolve/main/onnx/model.onnx";

                let client = reqwest::Client::builder()
                    .user_agent("remotemedia-runtime/0.2.0")
                    .build()
                    .map_err(|e| Error::Execution(format!("Failed to create HTTP client: {}", e)))?;

                let response = client.get(model_url).send().await
                    .map_err(|e| Error::Execution(format!("Failed to download model: {}", e)))?;

                if !response.status().is_success() {
                    return Err(Error::Execution(format!("Failed to download model: HTTP {}", response.status())));
                }

                let bytes = response.bytes().await
                    .map_err(|e| Error::Execution(format!("Failed to read model bytes: {}", e)))?;

                tokio::fs::write(&model_path, &bytes).await
                    .map_err(|e| Error::Execution(format!("Failed to save model: {}", e)))?;

                tracing::info!("Silero VAD model downloaded successfully ({} bytes)", bytes.len());
            }

            // Create ONNX Runtime session
            let session = Session::builder()?
                .with_execution_providers([CPUExecutionProvider::default().build()])?
                .commit_from_file(&model_path)?;

            tracing::info!("Silero VAD model loaded successfully");

            Ok(Arc::new(Mutex::new(session)))
        }).await
    }

    #[cfg(feature = "silero-vad")]
    async fn run_vad(&self, audio: &[f32], state: &mut VADState) -> Result<f32> {
        // Lazy-initialize the session on first use
        let session_arc = self.get_or_init_session().await?;

        let mut session = session_arc.lock().await;

        // Prepare inputs for Silero VAD
        // Model expects: input [batch, samples], state [2, batch, 128], sr (scalar)

        let chunk_size = audio.len();
        tracing::debug!("VAD processing {} audio samples", chunk_size);
        tracing::trace!(
            "Audio data first 10 samples: {:?}",
            &audio[..audio.len().min(10)]
        );

        // Input tensor: [1, chunk_size]
        let input_tensor = Tensor::from_array(([1, chunk_size], audio.to_vec()))?;
        tracing::trace!("Created input tensor with shape [1, {}]", chunk_size);

        // State tensor: [2, 1, 128] - combined state
        tracing::trace!("State vector length: {}", state.state.len());
        let state_tensor = Tensor::from_array(([2, 1, 128], state.state.clone()))?;
        tracing::trace!("Created state tensor with shape [2, 1, 128]");

        // Sample rate tensor: scalar (shape [])
        // Create as a tuple of (shape, data) - use empty array for scalar
        let sr_tensor = Tensor::from_array(([0usize; 0], vec![self.sampling_rate as i64]))?;

        // Run inference
        let outputs: SessionOutputs = session.run(ort::inputs![
            "input" => input_tensor,
            "state" => state_tensor,
            "sr" => sr_tensor,
        ])?;

        // Extract outputs: output (speech probability), stateN (new state)
        let (_, output_data) = outputs["output"].try_extract_tensor::<f32>()?;
        let speech_prob = output_data[0];

        // Update state for next inference
        let (_, state_data) = outputs["stateN"].try_extract_tensor::<f32>()?;
        state.state.copy_from_slice(state_data);

        Ok(speech_prob)
    }

    fn resample_audio(&self, audio: &[f32], from_sr: u32, to_sr: u32) -> Vec<f32> {
        if from_sr == to_sr {
            return audio.to_vec();
        }

        // Simple linear interpolation resampling
        let ratio = from_sr as f32 / to_sr as f32;
        let new_len = (audio.len() as f32 / ratio) as usize;

        (0..new_len)
            .map(|i| {
                let pos = i as f32 * ratio;
                let idx = pos as usize;
                let frac = pos - idx as f32;

                if idx + 1 < audio.len() {
                    audio[idx] * (1.0 - frac) + audio[idx + 1] * frac
                } else {
                    audio[idx]
                }
            })
            .collect()
    }
}

#[async_trait]
impl AsyncStreamingNode for SileroVADNode {
    fn node_type(&self) -> &str {
        "SileroVADNode"
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData> {
        // Default implementation for non-streaming usage - just returns VAD event
        #[cfg(not(feature = "silero-vad"))]
        {
            return Err(Error::Execution(
                "SileroVADNode requires 'silero-vad' feature to be enabled".into(),
            ));
        }

        #[cfg(feature = "silero-vad")]
        {
            // This is a simplified version - use process_streaming for full functionality
            Err(Error::Execution(
                "SileroVADNode requires streaming mode - use process_streaming() instead".into(),
            ))
        }
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
        #[cfg(not(feature = "silero-vad"))]
        {
            return Err(Error::Execution(
                "SileroVADNode requires 'silero-vad' feature to be enabled".into(),
            ));
        }

        #[cfg(feature = "silero-vad")]
        {
            // Extract audio from RuntimeData
            let (audio_samples, audio_sample_rate, audio_channels) = match &data {
                RuntimeData::Audio {
                    samples,
                    sample_rate,
                    channels,
                } => (samples.clone(), *sample_rate, *channels),
                _ => {
                    return Err(Error::Execution(
                        "SileroVADNode requires audio input".into(),
                    ))
                }
            };

            // Samples are already f32 in RuntimeData
            let samples = audio_samples;

            // Resample if needed
            let resampled = if audio_sample_rate != self.sampling_rate {
                tracing::debug!(
                    "Resampling from {}Hz to {}Hz",
                    audio_sample_rate,
                    self.sampling_rate
                );
                self.resample_audio(&samples, audio_sample_rate, self.sampling_rate)
            } else {
                samples
            };

            // Convert to mono if stereo
            let mono: Vec<f32> = if audio_channels > 1 {
                resampled
                    .chunks(audio_channels as usize)
                    .map(|chunk| chunk.iter().sum::<f32>() / audio_channels as f32)
                    .collect()
            } else {
                resampled
            };

            // Get or create state for this session
            let session_key = session_id.clone().unwrap_or_else(|| "default".to_string());
            let mut states = self.states.lock().await;
            let state = states
                .entry(session_key.clone())
                .or_insert_with(VADState::default);

            // Run VAD
            let speech_prob = self.run_vad(&mono, state).await?;

            // Determine speech state transitions
            let mut is_speech_start = false;
            let mut is_speech_end = false;

            if speech_prob >= self.threshold {
                if !state.triggered {
                    is_speech_start = true;
                    state.triggered = true;
                    tracing::info!("Speech started (prob={:.3})", speech_prob);
                }
                state.temp_end_samples = 0;
            } else if state.triggered {
                state.temp_end_samples += mono.len();
                let silence_duration_ms =
                    (state.temp_end_samples as f32 / self.sampling_rate as f32 * 1000.0) as u32;

                if silence_duration_ms >= self.min_silence_duration_ms {
                    is_speech_end = true;
                    state.triggered = false;
                    state.temp_end_samples = 0;
                    tracing::info!("Speech ended (silence={}ms)", silence_duration_ms);
                }
            }

            state.current_sample += mono.len();

            // Create VAD result JSON
            let vad_result = serde_json::json!({
                "has_speech": speech_prob >= self.threshold,
                "speech_probability": speech_prob,
                "is_speech_start": is_speech_start,
                "is_speech_end": is_speech_end,
                "timestamp_ms": (state.current_sample as f32 / self.sampling_rate as f32 * 1000.0) as u64,
            });

            drop(states); // Release lock

            // Output 1: VAD JSON event
            let json_output = RuntimeData::Json(vad_result);
            callback(json_output)?;

            // Output 2: Pass through original audio (for audio_buffer to accumulate)
            callback(data)?;

            Ok(2)
        }
    }
}

impl SileroVADNode {
    /// Check if this node is stateful
    pub fn is_stateful(&self) -> bool {
        true
    }

    /// Reset the VAD state
    pub fn reset_state(&mut self) {
        tokio::task::block_in_place(|| {
            let mut states = self.states.blocking_lock();
            states.clear();
        });
        tracing::info!("VAD states reset");
    }
}
