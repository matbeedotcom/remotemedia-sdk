use crate::data::AudioBuffer as ProtoAudioBuffer;
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
use crate::nodes::SyncStreamingNode;
use parking_lot::Mutex;
use std::sync::Arc;

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

    #[allow(dead_code)]  // Reserved for byte-format audio output support
    fn convert_f32_to_bytes(&self, samples: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for &sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    fn handle_audio_chunk(
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

    fn handle_vad_event(
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

        // Get accumulated samples
        let samples = state.accumulated_samples.clone();
        let num_samples = samples.len();

        tracing::info!(
            "[AudioBuffer] Session {}: Flushing buffer - {}ms, {} samples",
            session_id,
            duration_ms,
            num_samples
        );

        // Save format info before clearing
        let sample_rate = state.sample_rate;
        let channels = state.channels;

        // Clear buffer
        state.accumulated_samples.clear();
        state.is_speaking = false;
        state.chunks_accumulated = 0;

        Ok(Some(RuntimeData::Audio {
            samples: samples.into(),
            sample_rate,
            channels,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata: None,
        }))
    }
}

// Phase A-Wave 3: migrated to `SyncStreamingNode`. Previously held two
// `tokio::sync::Mutex` locks across `handle_audio_chunk(...).await` /
// `handle_vad_event(...).await` — but those helpers never actually
// awaited anything internally, so the async annotations and `.await`s
// were vestigial. The lock-across-await hazard is gone with
// `parking_lot::Mutex` + sync helpers. The collect-then-fire
// discipline is preserved: all state mutation happens under the two
// locks, which are dropped before the callback fires.
impl SyncStreamingNode for AudioBufferAccumulatorNode {
    fn node_type(&self) -> &str {
        "AudioBufferAccumulatorNode"
    }

    fn process(&self, _data: RuntimeData) -> Result<RuntimeData> {
        Err(Error::Execution(
            "AudioBufferAccumulatorNode requires streaming mode - \
             callers must use process_streaming() (the router does this \
             automatically when the factory declares is_multi_output_streaming=true)"
                .into(),
        ))
    }

    fn process_streaming(
        &self,
        data: RuntimeData,
        session_id: Option<&str>,
        callback: &mut dyn FnMut(RuntimeData) -> Result<()>,
    ) -> Result<usize> {
        let session_key = session_id.unwrap_or("default").to_string();

        // Collect output under the lock, release, then fire callback.
        // Both locks are `parking_lot::Mutex`; uncontended fast path
        // is a single CAS. No `.await` anywhere in this method; the
        // old async annotations were pure overhead.
        let output: Option<RuntimeData> = {
            let mut states = self.states.lock();
            let mut pending = self.pending_audio.lock();

            match &data {
                RuntimeData::Audio {
                    samples,
                    sample_rate,
                    channels,
                    ..
                } => {
                    // Convert to the legacy ProtoAudioBuffer shape the
                    // helpers expect. One heap op per call — unchanged
                    // from the previous implementation.
                    let audio_buf = crate::data::AudioBuffer {
                        samples: samples.iter().flat_map(|f| f.to_le_bytes()).collect(),
                        sample_rate: *sample_rate,
                        channels: *channels,
                        format: 1, // F32
                        num_samples: samples.len() as u64,
                    };
                    self.handle_audio_chunk(&audio_buf, &session_key, &mut states, &mut pending)?
                }
                RuntimeData::Json(json_value) => {
                    self.handle_vad_event(json_value, &session_key, &mut states, &mut pending)?
                }
                _ => {
                    tracing::warn!("[AudioBuffer] Received unexpected data type");
                    None
                }
            }
        };

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

    /// Reset the buffer state.
    ///
    /// Previously required `tokio::task::block_in_place` +
    /// `blocking_lock` because the state was behind a
    /// `tokio::sync::Mutex`. After A-Wave 3 those are
    /// `parking_lot::Mutex` — plain sync `.lock()` works from any
    /// thread.
    pub fn reset_state(&mut self) {
        self.states.lock().clear();
        self.pending_audio.lock().clear();
        tracing::info!("[AudioBufferAccumulator] States reset");
    }
}
