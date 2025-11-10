/// Audio Buffer Accumulator Node
///
/// Accumulates audio chunks during speech and releases complete utterances
/// when VAD detects silence. Works in conjunction with SileroVADNode.
///
/// Pipeline flow:
///   Microphone → [Audio] → AudioBufferAccumulator
///   Microphone → [Audio] → SileroVAD → [JSON] → AudioBufferAccumulator
///   AudioBufferAccumulator → [Audio] → LFM2Audio → Kokoro
///
/// The accumulator receives:
/// - Audio chunks (from microphone)
/// - VAD events (from SileroVAD)
///
/// When speech ends (is_speech_end=true), it outputs the accumulated audio buffer.
use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::grpc_service::generated::AudioBuffer as ProtoAudioBuffer;
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Buffer state for a single session
#[derive(Debug, Clone)]
struct BufferState {
    /// Accumulated audio samples (f32)
    accumulated_samples: Vec<f32>,
    /// Sample rate of accumulated audio
    sample_rate: u32,
    /// Number of channels
    channels: u32,
    /// Is speech currently active
    is_speaking: bool,
    /// Total chunks accumulated
    chunks_accumulated: usize,
}

impl Default for BufferState {
    fn default() -> Self {
        Self {
            accumulated_samples: Vec::new(),
            sample_rate: 24000,
            channels: 1,
            is_speaking: false,
            chunks_accumulated: 0,
        }
    }
}

/// Audio Buffer Accumulator Node
pub struct AudioBufferAccumulatorNode {
    /// Minimum buffer duration in ms before allowing output
    min_utterance_duration_ms: u32,

    /// Maximum buffer duration in ms (safety limit)
    max_utterance_duration_ms: u32,

    /// Buffer states per session
    states: Arc<Mutex<std::collections::HashMap<String, BufferState>>>,

    /// Pending audio chunks waiting for VAD event (per session)
    /// When we receive audio before VAD, we store it here
    pending_audio: Arc<Mutex<std::collections::HashMap<String, Vec<(Vec<f32>, u32, u32)>>>>,
}

impl AudioBufferAccumulatorNode {
    pub fn new(
        min_utterance_duration_ms: Option<u32>,
        max_utterance_duration_ms: Option<u32>,
    ) -> Self {
        Self {
            min_utterance_duration_ms: min_utterance_duration_ms.unwrap_or(250),
            max_utterance_duration_ms: max_utterance_duration_ms.unwrap_or(30000),
            states: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pending_audio: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    fn convert_audio_to_f32(&self, audio_buf: &ProtoAudioBuffer) -> Result<Vec<f32>> {
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
            2 => {
                // AUDIO_FORMAT_I16
                let sample_count = audio_buf.samples.len() / 2;
                Ok((0..sample_count)
                    .map(|i| {
                        let offset = i * 2;
                        let i16_val = i16::from_le_bytes([
                            audio_buf.samples[offset],
                            audio_buf.samples[offset + 1],
                        ]);
                        i16_val as f32 / 32768.0
                    })
                    .collect())
            }
            _ => Err(Error::Execution(format!(
                "Unsupported audio format: {} (expected 1=F32 or 2=I16)",
                audio_buf.format
            ))),
        }
    }

    fn convert_f32_to_bytes(&self, samples: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for &sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    async fn handle_audio_chunk(
        &self,
        audio_buf: &ProtoAudioBuffer,
        session_id: &str,
        states: &mut std::collections::HashMap<String, BufferState>,
        pending: &mut std::collections::HashMap<String, Vec<(Vec<f32>, u32, u32)>>,
    ) -> Result<Option<RuntimeData>> {
        let samples = self.convert_audio_to_f32(audio_buf)?;

        // Get or create state for this session
        let state = states
            .entry(session_id.to_string())
            .or_insert_with(BufferState::default);

        // Check if we're currently speaking (have received speech_start)
        if state.is_speaking {
            // Accumulate this chunk
            state.accumulated_samples.extend_from_slice(&samples);
            state.sample_rate = audio_buf.sample_rate;
            state.channels = audio_buf.channels;
            state.chunks_accumulated += 1;

            tracing::debug!(
                "[AudioBuffer] Session {}: Accumulated chunk {} ({} total samples)",
                session_id,
                state.chunks_accumulated,
                state.accumulated_samples.len()
            );

            // Check if we've hit the maximum duration (safety limit)
            let duration_ms =
                (state.accumulated_samples.len() as f32 / state.sample_rate as f32 * 1000.0) as u32;
            if duration_ms >= self.max_utterance_duration_ms {
                tracing::warn!(
                    "[AudioBuffer] Session {}: Hit max duration {}ms, forcing output",
                    session_id,
                    duration_ms
                );

                // Force output
                return self.flush_buffer(state, session_id);
            }

            Ok(None)
        } else {
            // Not speaking yet, buffer this audio in pending for speech padding
            tracing::trace!(
                "[AudioBuffer] Session {}: Buffering audio chunk in pending (not speaking yet)",
                session_id
            );

            let pending_vec = pending
                .entry(session_id.to_string())
                .or_insert_with(Vec::new);

            pending_vec.push((samples, audio_buf.sample_rate, audio_buf.channels));

            // Keep only last 20 chunks for speech padding (~640ms at 16kHz/512 samples)
            // This prevents accumulating minutes of silence between utterances
            let max_padding_chunks = 20;
            if pending_vec.len() > max_padding_chunks {
                pending_vec.drain(0..(pending_vec.len() - max_padding_chunks));
            }

            Ok(None)
        }
    }

    async fn handle_vad_event(
        &self,
        vad_json: &serde_json::Value,
        session_id: &str,
        states: &mut std::collections::HashMap<String, BufferState>,
        pending: &mut std::collections::HashMap<String, Vec<(Vec<f32>, u32, u32)>>,
    ) -> Result<Option<RuntimeData>> {
        let is_speech_start = vad_json
            .get("is_speech_start")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_speech_end = vad_json
            .get("is_speech_end")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        tracing::debug!(
            "[AudioBuffer] Session {}: VAD event - start={}, end={}",
            session_id,
            is_speech_start,
            is_speech_end
        );

        if is_speech_start {
            // Start accumulating audio
            let state = states
                .entry(session_id.to_string())
                .or_insert_with(BufferState::default);
            state.is_speaking = true;
            state.accumulated_samples.clear();
            state.chunks_accumulated = 0;

            // Add pending chunks that contain the beginning of speech
            if let Some(pending_chunks) = pending.remove(session_id) {
                tracing::info!(
                    "[AudioBuffer] Session {}: Speech started, adding {} pending chunks to buffer",
                    session_id,
                    pending_chunks.len()
                );

                for (samples, sr, ch) in pending_chunks {
                    state.accumulated_samples.extend_from_slice(&samples);
                    state.sample_rate = sr;
                    state.channels = ch;
                    state.chunks_accumulated += 1;
                }
            } else {
                tracing::info!(
                    "[AudioBuffer] Session {}: Speech started, no pending chunks",
                    session_id
                );
            }

            Ok(None)
        } else if is_speech_end {
            // Output accumulated buffer
            if let Some(state) = states.get_mut(session_id) {
                if state.is_speaking {
                    tracing::info!(
                        "[AudioBuffer] Session {}: Speech ended, outputting {} samples ({} chunks)",
                        session_id,
                        state.accumulated_samples.len(),
                        state.chunks_accumulated
                    );

                    let result = self.flush_buffer(state, session_id);

                    // Clear any pending buffer to start fresh for next utterance
                    if let Some(old_pending) = pending.remove(session_id) {
                        tracing::info!(
                            "[AudioBuffer] Session {}: Cleared {} pending chunks after speech end",
                            session_id,
                            old_pending.len()
                        );
                    }

                    return result;
                }
            }

            Ok(None)
        } else {
            // No action needed for intermediate VAD frames
            Ok(None)
        }
    }

    fn flush_buffer(
        &self,
        state: &mut BufferState,
        session_id: &str,
    ) -> Result<Option<RuntimeData>> {
        if state.accumulated_samples.is_empty() {
            tracing::debug!("[AudioBuffer] Session {}: No samples to flush", session_id);
            state.is_speaking = false;
            return Ok(None);
        }

        // Check minimum duration
        let duration_ms =
            (state.accumulated_samples.len() as f32 / state.sample_rate as f32 * 1000.0) as u32;
        if duration_ms < self.min_utterance_duration_ms {
            tracing::debug!(
                "[AudioBuffer] Session {}: Utterance too short ({}ms < {}ms), discarding",
                session_id,
                duration_ms,
                self.min_utterance_duration_ms
            );
            state.accumulated_samples.clear();
            state.is_speaking = false;
            state.chunks_accumulated = 0;
            return Ok(None);
        }

        // Convert accumulated samples to bytes
        let sample_bytes = self.convert_f32_to_bytes(&state.accumulated_samples);
        let num_samples = state.accumulated_samples.len() as u64;

        tracing::info!(
            "[AudioBuffer] Session {}: Flushing buffer - {}ms, {} samples",
            session_id,
            duration_ms,
            num_samples
        );

        // Create audio output
        let audio_output = ProtoAudioBuffer {
            samples: sample_bytes,
            sample_rate: state.sample_rate,
            channels: state.channels,
            format: 1, // F32
            num_samples,
        };

        // Clear buffer
        state.accumulated_samples.clear();
        state.is_speaking = false;
        state.chunks_accumulated = 0;

        Ok(Some(RuntimeData::Audio(audio_output)))
    }
}

#[async_trait]
impl AsyncStreamingNode for AudioBufferAccumulatorNode {
    fn node_type(&self) -> &str {
        "AudioBufferAccumulatorNode"
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData> {
        // Simplified non-streaming version - not recommended
        // Use process_streaming for full buffering functionality
        Err(Error::Execution(
            "AudioBufferAccumulatorNode requires streaming mode - use process_streaming() instead"
                .into(),
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
        let session_key = session_id.clone().unwrap_or_else(|| "default".to_string());

        let mut states = self.states.lock().await;
        let mut pending = self.pending_audio.lock().await;

        let output = match &data {
            RuntimeData::Audio(audio_buf) => {
                // Handle audio chunk
                self.handle_audio_chunk(audio_buf, &session_key, &mut states, &mut pending)
                    .await?
            }
            RuntimeData::Json(json_value) => {
                // Handle VAD event
                self.handle_vad_event(json_value, &session_key, &mut states, &mut pending)
                    .await?
            }
            _ => {
                tracing::warn!("[AudioBuffer] Received unexpected data type");
                None
            }
        };

        drop(states);
        drop(pending);

        // Output accumulated buffer if speech ended
        if let Some(output_data) = output {
            callback(output_data)?;
            Ok(1)
        } else {
            Ok(0)
        }
    }
}

impl AudioBufferAccumulatorNode {
    /// Initialize the audio buffer accumulator
    pub async fn initialize(&mut self) -> Result<()> {
        tracing::info!(
            "[AudioBufferAccumulator] Initialized (min={}ms, max={}ms)",
            self.min_utterance_duration_ms,
            self.max_utterance_duration_ms
        );
        Ok(())
    }

    /// Check if this node is stateful
    pub fn is_stateful(&self) -> bool {
        true
    }

    /// Reset the buffer state
    pub fn reset_state(&mut self) {
        tokio::task::block_in_place(|| {
            let mut states = self.states.blocking_lock();
            states.clear();
            let mut pending = self.pending_audio.blocking_lock();
            pending.clear();
        });
        tracing::info!("[AudioBufferAccumulator] States reset");
    }
}
