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
use crate::error::{Error, Result};
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Silero VAD Node configuration
///
/// Configuration for the Silero VAD streaming node. Uses `#[serde(default)]` to allow
/// partial config, and `#[serde(alias)]` to accept both snake_case and camelCase.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct SileroVADConfig {
    /// Speech probability threshold (0.0-1.0)
    #[schemars(range(min = 0.0, max = 1.0))]
    pub threshold: f32,

    /// Expected sample rate (8000 or 16000)
    #[serde(alias = "samplingRate")]
    #[schemars(range(min = 8000, max = 16000))]
    pub sampling_rate: u32,

    /// Minimum speech duration in ms to trigger
    #[serde(alias = "minSpeechDurationMs")]
    #[schemars(range(max = 5000))]
    pub min_speech_duration_ms: u32,

    /// Minimum silence duration in ms to end speech
    #[serde(alias = "minSilenceDurationMs")]
    #[schemars(range(max = 5000))]
    pub min_silence_duration_ms: u32,

    /// Padding before/after speech in ms
    #[serde(alias = "speechPadMs")]
    #[schemars(range(max = 500))]
    pub speech_pad_ms: u32,
}

impl Default for SileroVADConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            sampling_rate: 16000,
            min_speech_duration_ms: 250,
            min_silence_duration_ms: 100,
            speech_pad_ms: 30,
        }
    }
}

#[cfg(feature = "silero-vad")]
use voice_activity_detector::VoiceActivityDetector;

/// VAD state for speech detection
#[derive(Debug)]
struct VADState {
    /// Is speech currently active
    triggered: bool,
    /// Samples of silence accumulated
    temp_end_samples: usize,
    /// Total samples processed
    current_sample: usize,
    /// Audio buffer for windowing (VAD needs fixed-size chunks)
    #[cfg(feature = "silero-vad")]
    audio_buffer: Vec<f32>,
    /// The voice activity detector instance
    #[cfg(feature = "silero-vad")]
    detector: VoiceActivityDetector,
}

#[cfg(feature = "silero-vad")]
impl VADState {
    fn new(sample_rate: u32) -> Result<Self> {
        let detector = VoiceActivityDetector::builder()
            .sample_rate(sample_rate)
            .chunk_size(512usize) // 512 samples for 16kHz
            .build()
            .map_err(|e| Error::Execution(format!("Failed to create VAD detector: {}", e)))?;
        
        Ok(Self {
            triggered: false,
            temp_end_samples: 0,
            current_sample: 0,
            audio_buffer: Vec::new(),
            detector,
        })
    }
}

/// Silero VAD Streaming Node
pub struct SileroVADNode {
    /// Speech probability threshold (0.0-1.0)
    threshold: f32,
    /// Expected sample rate (8000 or 16000)
    sampling_rate: u32,
    /// Minimum speech duration in ms to trigger
    #[allow(dead_code)]  // Reserved for speech duration filtering
    min_speech_duration_ms: u32,
    /// Minimum silence duration in ms to end speech
    min_silence_duration_ms: u32,
    /// Padding before/after speech in ms
    #[allow(dead_code)]  // Reserved for audio padding implementation
    speech_pad_ms: u32,

    /// VAD state (one per session_id)
    states: Arc<Mutex<std::collections::HashMap<String, VADState>>>,
}

impl SileroVADNode {
    /// Create a new SileroVADNode with the given configuration
    pub fn with_config(config: SileroVADConfig) -> Self {
        Self {
            threshold: config.threshold,
            sampling_rate: config.sampling_rate,
            min_speech_duration_ms: config.min_speech_duration_ms,
            min_silence_duration_ms: config.min_silence_duration_ms,
            speech_pad_ms: config.speech_pad_ms,
            states: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Create a new SileroVADNode with optional parameters (legacy API)
    pub fn new(
        threshold: Option<f32>,
        sampling_rate: Option<u32>,
        min_speech_duration_ms: Option<u32>,
        min_silence_duration_ms: Option<u32>,
        speech_pad_ms: Option<u32>,
    ) -> Self {
        Self::with_config(SileroVADConfig {
            threshold: threshold.unwrap_or(0.5),
            sampling_rate: sampling_rate.unwrap_or(16000),
            min_speech_duration_ms: min_speech_duration_ms.unwrap_or(250),
            min_silence_duration_ms: min_silence_duration_ms.unwrap_or(100),
            speech_pad_ms: speech_pad_ms.unwrap_or(30),
        })
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

    async fn process(&self, _data: RuntimeData) -> Result<RuntimeData> {
        // Default implementation for non-streaming usage - not supported
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
            let _ = (data, session_id, callback);
            return Err(Error::Execution(
                "SileroVADNode requires 'silero-vad' feature to be enabled".into(),
            ));
        }

        #[cfg(feature = "silero-vad")]
        {
            // Extract audio from RuntimeData - pass through non-audio data
            let (audio_samples, audio_sample_rate, audio_channels) = match &data {
                RuntimeData::Audio {
                    samples,
                    sample_rate,
                    channels,
                    ..
                } => (samples.clone(), *sample_rate, *channels),
                _ => {
                    // Pass through non-audio data (e.g., video frames)
                    callback(data)?;
                    return Ok(1);
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

            // Handle empty audio (e.g., from resampler buffering)
            if mono.is_empty() {
                tracing::debug!("VAD received empty audio, skipping");
                return Ok(0);
            }

            // Get or create state for this session
            let session_key = session_id.clone().unwrap_or_else(|| "default".to_string());
            let mut states = self.states.lock().await;
            let state = if !states.contains_key(&session_key) {
                tracing::info!("Creating new VAD state for session: {}", session_key);
                
                // Emit progress events for initialization
                crate::nodes::progress::emit_progress(crate::nodes::progress::ProgressEvent {
                    node_type: "SileroVADNode".to_string(),
                    node_id: None,
                    event_type: crate::nodes::progress::ProgressEventType::LoadingStarted,
                    message: "Initializing Silero VAD".to_string(),
                    progress_pct: Some(0.0),
                    details: None,
                });
                
                let new_state = VADState::new(self.sampling_rate)?;
                states.insert(session_key.clone(), new_state);
                
                crate::nodes::progress::emit_init_complete("SileroVADNode", None);
                
                states.get_mut(&session_key).unwrap()
            } else {
                states.get_mut(&session_key).unwrap()
            };

            // Add incoming audio to buffer
            state.audio_buffer.extend_from_slice(&mono);
            
            // Determine chunk size based on sample rate
            // voice_activity_detector uses 512 samples for 16kHz, 256 for 8kHz
            let chunk_size = if self.sampling_rate >= 16000 { 512 } else { 256 };
            
            let mut output_count = 0;
            let mut last_speech_prob = 0.0f32;
            let mut any_is_speech_start = false;
            let mut any_is_speech_end = false;

            // Process complete chunks
            while state.audio_buffer.len() >= chunk_size {
                let chunk: Vec<f32> = state.audio_buffer.drain(..chunk_size).collect();
                
                // Run VAD on this chunk
                let speech_prob = state.detector.predict(chunk.iter().copied());
                last_speech_prob = speech_prob;
                
                tracing::trace!("VAD chunk processed: prob={:.3}", speech_prob);

                // Determine speech state transitions
                if speech_prob >= self.threshold {
                    if !state.triggered {
                        any_is_speech_start = true;
                        state.triggered = true;
                        tracing::info!("Speech started (prob={:.3})", speech_prob);
                    }
                    state.temp_end_samples = 0;
                } else if state.triggered {
                    state.temp_end_samples += chunk_size;
                    let silence_duration_ms =
                        (state.temp_end_samples as f32 / self.sampling_rate as f32 * 1000.0) as u32;

                    if silence_duration_ms >= self.min_silence_duration_ms {
                        any_is_speech_end = true;
                        state.triggered = false;
                        state.temp_end_samples = 0;
                        tracing::info!("Speech ended (silence={}ms)", silence_duration_ms);
                    }
                }

                state.current_sample += chunk_size;
            }

            // If we processed any chunks, output the result
            if state.current_sample > 0 || !mono.is_empty() {
                // Create VAD result JSON with aggregate results
                let vad_result = serde_json::json!({
                    "has_speech": last_speech_prob >= self.threshold,
                    "speech_probability": last_speech_prob,
                    "is_speech_start": any_is_speech_start,
                    "is_speech_end": any_is_speech_end,
                    "timestamp_ms": (state.current_sample as f32 / self.sampling_rate as f32 * 1000.0) as u64,
                });

                drop(states); // Release lock before callbacks

                // Output 1: VAD JSON event
                let json_output = RuntimeData::Json(vad_result);
                callback(json_output)?;
                output_count += 1;

                // Output 2: Pass through original audio (for audio_buffer to accumulate)
                callback(data)?;
                output_count += 1;
            } else {
                drop(states);
            }

            Ok(output_count)
        }
    }

    /// Process control messages for flow control
    ///
    /// Handles:
    /// - CancelSpeculation: Log but no action (VAD doesn't buffer)
    /// - Other messages: Ignore
    async fn process_control_message(
        &self,
        message: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<bool> {
        match message {
            RuntimeData::ControlMessage {
                message_type,
                segment_id,
                ..
            } => {
                use crate::data::ControlMessageType;

                match message_type {
                    ControlMessageType::CancelSpeculation { .. } => {
                        // VAD doesn't buffer data, so cancellation is a no-op
                        // Just log for debugging
                        tracing::debug!(
                            "VAD received cancellation for segment {:?}, no action needed",
                            segment_id
                        );
                        Ok(true)
                    }
                    ControlMessageType::BatchHint { .. } => {
                        // VAD processes samples immediately, batching not applicable
                        Ok(false)
                    }
                    ControlMessageType::DeadlineWarning { .. } => {
                        // VAD is already optimized for low latency
                        Ok(false)
                    }
                }
            }
            _ => Ok(false), // Not a control message
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
