//! Speculative VAD Coordinator Node
//!
//! This node integrates speculative audio forwarding with actual VAD inference.
//! It coordinates between immediate forwarding (for low latency) and VAD decision
//! making (for false positive detection).
//!
//! Key behaviors:
//! 1. **Immediate forwarding**: Audio chunks are forwarded immediately without waiting for VAD
//! 2. **Parallel VAD inference**: Runs Silero VAD in parallel (doesn't block forwarding)
//! 3. **Speech segment tracking**: Tracks speech start/end and calculates segment duration
//! 4. **False positive detection**: Emits CancelSpeculation if segment < min_speech_duration_ms
//! 5. **Metrics tracking**: Records speculation acceptance rate for monitoring
//!
//! This node combines the responsibilities of SpeculativeVADGate and SileroVADNode
//! into a single coordinated flow, eliminating the need for manual integration.

use crate::data::{ControlMessageType, RuntimeData};
use crate::error::{Error, Result};
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use parking_lot::Mutex;

#[cfg(feature = "silero-vad")]
use crate::nodes::silero_vad::{SileroVADConfig, SileroVADNode};

/// Configuration for the Speculative VAD Coordinator
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct SpeculativeVADCoordinatorConfig {
    /// Speech probability threshold (0.0-1.0)
    #[schemars(range(min = 0.0, max = 1.0))]
    pub vad_threshold: f32,

    /// Expected sample rate (8000 or 16000)
    #[serde(alias = "samplingRate")]
    #[schemars(range(min = 8000, max = 16000))]
    pub sample_rate: u32,

    /// Minimum speech duration in ms to be considered valid speech
    /// Segments shorter than this are considered false positives
    #[serde(alias = "minSpeechDurationMs")]
    #[schemars(range(max = 5000))]
    pub min_speech_duration_ms: u32,

    /// Minimum silence duration in ms to end speech segment
    #[serde(alias = "minSilenceDurationMs")]
    #[schemars(range(max = 5000))]
    pub min_silence_duration_ms: u32,

    /// Lookback window in milliseconds (how much audio to buffer for cancellation)
    #[serde(alias = "lookbackMs")]
    #[schemars(range(max = 1000))]
    pub lookback_ms: u32,

    /// Padding before/after speech in ms
    #[serde(alias = "speechPadMs")]
    #[schemars(range(max = 500))]
    pub speech_pad_ms: u32,
}

impl Default for SpeculativeVADCoordinatorConfig {
    fn default() -> Self {
        Self {
            vad_threshold: 0.5,
            sample_rate: 16000,
            min_speech_duration_ms: 250,
            min_silence_duration_ms: 100,
            lookback_ms: 150,
            speech_pad_ms: 30,
        }
    }
}

/// Per-session state for the coordinator
#[derive(Debug)]
struct CoordinatorState {
    /// Ring buffer for audio samples (for potential cancellation)
    audio_buffer: VecDeque<f32>,

    /// Maximum buffer capacity
    buffer_capacity: usize,

    /// Sample index when current speech segment started (None if not in speech)
    speech_start_sample: Option<usize>,

    /// Total samples processed in this session
    current_sample: usize,

    /// Current segment counter for unique IDs
    segment_counter: u64,

    /// Speculation metrics
    speculations_accepted: u64,
    speculations_cancelled: u64,

    /// Is speech currently active (based on VAD threshold)
    speech_triggered: bool,

    /// Samples of silence accumulated (for min_silence detection)
    silence_samples: usize,
}

impl CoordinatorState {
    fn new(buffer_capacity: usize) -> Self {
        Self {
            audio_buffer: VecDeque::with_capacity(buffer_capacity),
            buffer_capacity,
            speech_start_sample: None,
            current_sample: 0,
            segment_counter: 0,
            speculations_accepted: 0,
            speculations_cancelled: 0,
            speech_triggered: false,
            silence_samples: 0,
        }
    }

    /// Calculate speculation acceptance rate (0.0-1.0)
    fn acceptance_rate(&self) -> f64 {
        let total = self.speculations_accepted + self.speculations_cancelled;
        if total == 0 {
            return 1.0;
        }
        self.speculations_accepted as f64 / total as f64
    }
}

/// Speculative VAD Coordinator Node
///
/// Integrates speculative forwarding with VAD inference:
/// 1. Forward audio immediately (speculative)
/// 2. Run VAD inference in parallel
/// 3. Track speech segments
/// 4. Emit CancelSpeculation on false positives (duration < min_speech_duration_ms)
pub struct SpeculativeVADCoordinator {
    config: SpeculativeVADCoordinatorConfig,

    /// Internal Silero VAD node for inference
    #[cfg(feature = "silero-vad")]
    vad_node: SileroVADNode,

    /// Per-session state
    sessions: Arc<Mutex<HashMap<String, CoordinatorState>>>,
}

impl SpeculativeVADCoordinator {
    /// Create a new coordinator with the given configuration
    pub fn with_config(config: SpeculativeVADCoordinatorConfig) -> Self {
        #[cfg(feature = "silero-vad")]
        let vad_node = SileroVADNode::with_config(SileroVADConfig {
            threshold: config.vad_threshold,
            sampling_rate: config.sample_rate,
            min_speech_duration_ms: config.min_speech_duration_ms,
            min_silence_duration_ms: config.min_silence_duration_ms,
            speech_pad_ms: config.speech_pad_ms,
        });

        Self {
            config,
            #[cfg(feature = "silero-vad")]
            vad_node,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new coordinator with default configuration
    pub fn new() -> Self {
        Self::with_config(SpeculativeVADCoordinatorConfig::default())
    }

    /// Ensure a session state exists, create it if missing.
    ///
    /// Returning a `MutexGuard` is avoided — the previous impl held it
    /// across `vad_node.process_streaming(...).await`, serializing every
    /// concurrent session on the lock for tens of milliseconds and
    /// making priority inversion very real. All locking is now scoped
    /// to narrow sync blocks.
    fn ensure_session_exists(&self, session_id: &str) {
        let mut sessions = self.sessions.lock();
        if !sessions.contains_key(session_id) {
            let samples_per_ms = self.config.sample_rate / 1000;
            let buffer_capacity = (self.config.lookback_ms * samples_per_ms) as usize;
            sessions.insert(session_id.to_string(), CoordinatorState::new(buffer_capacity));
        }
    }

    /// Get speculation acceptance rate for a session
    pub async fn get_acceptance_rate(&self, session_id: &str) -> f64 {
        let sessions = self.sessions.lock();
        sessions
            .get(session_id)
            .map(|s| s.acceptance_rate())
            .unwrap_or(1.0)
    }

    /// Clean up session state
    pub async fn terminate_session(&self, session_id: &str) {
        let mut sessions = self.sessions.lock();
        sessions.remove(session_id);
    }

    /// Get current timestamp in milliseconds
    fn current_timestamp_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_millis(0))
            .as_millis() as u64
    }
}

impl Default for SpeculativeVADCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsyncStreamingNode for SpeculativeVADCoordinator {
    fn node_type(&self) -> &str {
        "SpeculativeVADCoordinator"
    }

    async fn process(&self, _data: RuntimeData) -> Result<RuntimeData> {
        Err(Error::Execution(
            "SpeculativeVADCoordinator requires streaming mode - use process_streaming() instead".into(),
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
        let session_id = session_id.unwrap_or_else(|| "default".to_string());

        // Extract audio from RuntimeData
        let (samples, sample_rate, channels) = match &data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                ..
            } => (samples.clone(), *sample_rate, *channels),
            _ => {
                return Err(Error::Execution(
                    "SpeculativeVADCoordinator requires audio input".into(),
                ))
            }
        };

        let mut output_count = 0;

        // **Step 1: Forward audio IMMEDIATELY (speculative)**
        // This is the key to low latency - audio goes out without waiting for VAD
        let audio_output = RuntimeData::Audio {
            samples: samples.clone(),
            sample_rate,
            channels,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata: None,
        };
        callback(audio_output)?;
        output_count += 1;

        // Ensure session state exists. We deliberately do NOT hold the
        // sessions lock across the VAD .await below — with parking_lot
        // that would deadlock any concurrent caller for this session_id
        // for the duration of VAD inference (tens of ms). Instead we:
        //   Scope A:  buffer incoming samples, release lock
        //   (no lock) run VAD inference
        //   Scope B:  apply VAD result to state
        self.ensure_session_exists(&session_id);

        // **Step 2: Buffer audio for potential cancellation** (Scope A)
        {
            let mut sessions = self.sessions.lock();
            let state = sessions.get_mut(&session_id).unwrap();
            for &sample in samples.iter() {
                if state.audio_buffer.len() >= state.buffer_capacity {
                    state.audio_buffer.pop_front();
                }
                state.audio_buffer.push_back(sample);
            }
        } // lock released before VAD .await

        // **Step 3: Run VAD inference** (no lock held)
        #[cfg(feature = "silero-vad")]
        let vad_result = {
            // Collect VAD events from the internal VAD node
            // Use std::sync::Mutex since callback is sync
            let vad_events: Arc<std::sync::Mutex<Vec<serde_json::Value>>> = 
                Arc::new(std::sync::Mutex::new(Vec::new()));
            let vad_events_clone = vad_events.clone();

            let vad_callback = move |vad_data: RuntimeData| {
                if let RuntimeData::Json(json) = vad_data {
                    if let Ok(mut events) = vad_events_clone.lock() {
                        events.push(json);
                    }
                }
                Ok(())
            };

            // Run VAD on the audio
            let _ = self.vad_node.process_streaming(
                data.clone(),
                Some(format!("{}_vad", session_id)),
                vad_callback,
            ).await;

            // Extract the VAD result
            let events = vad_events.lock().ok();
            events.and_then(|e| e.first().cloned())
        };

        #[cfg(not(feature = "silero-vad"))]
        let vad_result: Option<serde_json::Value> = None;

        // **Step 4: Process VAD result and track speech segments** (Scope B).
        // Re-acquire the lock for the state update, collect the outputs
        // to emit into a local Vec, then release the lock BEFORE firing
        // user callbacks. This keeps the per-session lock narrow and
        // prevents user-supplied callbacks from blocking other sessions.
        let mut pending_outputs: Vec<RuntimeData> = Vec::new();
        if let Some(vad_json) = vad_result {
            let has_speech = vad_json
                .get("has_speech")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let is_speech_start = vad_json
                .get("is_speech_start")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let is_speech_end = vad_json
                .get("is_speech_end")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let speech_probability = vad_json
                .get("speech_probability")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;

            {
                let mut sessions = self.sessions.lock();
                let state = sessions.get_mut(&session_id).unwrap();

                // Track speech start
                if is_speech_start {
                    state.speech_start_sample = Some(state.current_sample);
                    state.speech_triggered = true;
                    state.silence_samples = 0;
                    tracing::debug!(
                        session_id = %session_id,
                        sample = state.current_sample,
                        "Speech started (speculative segment begins)"
                    );
                }

                // Update silence tracking
                if !has_speech && state.speech_triggered {
                    state.silence_samples += samples.len();
                } else if has_speech {
                    state.silence_samples = 0;
                }

                // Track speech end
                if is_speech_end {
                    if let Some(start_sample) = state.speech_start_sample.take() {
                        let duration_samples = state.current_sample - start_sample;
                        let duration_ms = (duration_samples as f32
                            / self.config.sample_rate as f32
                            * 1000.0) as u32;

                        tracing::debug!(
                            session_id = %session_id,
                            duration_ms = duration_ms,
                            min_required_ms = self.config.min_speech_duration_ms,
                            "Speech ended, checking duration"
                        );

                        // **Step 5: Determine if this is a false positive**
                        if duration_ms < self.config.min_speech_duration_ms {
                            // FALSE POSITIVE - segment too short, queue cancellation
                            let segment_id =
                                format!("{}_{}", session_id, state.segment_counter);
                            state.segment_counter += 1;

                            let cancel_msg = RuntimeData::ControlMessage {
                                message_type: ControlMessageType::CancelSpeculation {
                                    from_timestamp: start_sample as u64,
                                    to_timestamp: state.current_sample as u64,
                                },
                                segment_id: Some(segment_id.clone()),
                                timestamp_ms: Self::current_timestamp_ms(),
                                metadata: serde_json::json!({
                                    "reason": "speech_too_short",
                                    "duration_ms": duration_ms,
                                    "min_required_ms": self.config.min_speech_duration_ms,
                                    "vad_confidence": speech_probability,
                                }),
                            };
                            pending_outputs.push(cancel_msg);

                            state.speculations_cancelled += 1;
                            tracing::info!(
                                session_id = %session_id,
                                segment_id = %segment_id,
                                duration_ms = duration_ms,
                                acceptance_rate = state.acceptance_rate() * 100.0,
                                "Speculation cancelled (false positive)"
                            );
                        } else {
                            // CONFIRMED SPEECH - speculation accepted
                            state.speculations_accepted += 1;
                            state.audio_buffer.clear();

                            tracing::info!(
                                session_id = %session_id,
                                duration_ms = duration_ms,
                                acceptance_rate = state.acceptance_rate() * 100.0,
                                "Speculation accepted (confirmed speech)"
                            );
                        }
                    }

                    state.speech_triggered = false;
                    state.silence_samples = 0;
                }

                state.current_sample += samples.len();
            } // sessions lock released here — callbacks fire unlocked

            // Always emit the VAD JSON for downstream nodes that want it
            pending_outputs.push(RuntimeData::Json(vad_json));
        } else {
            // No VAD result — just advance the sample counter.
            let mut sessions = self.sessions.lock();
            if let Some(state) = sessions.get_mut(&session_id) {
                state.current_sample += samples.len();
            }
        }

        // Fire queued callbacks with the lock released.
        for output in pending_outputs {
            callback(output)?;
            output_count += 1;
        }

        Ok(output_count)
    }

    async fn process_control_message(
        &self,
        message: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<bool> {
        // Coordinator doesn't need to handle incoming control messages
        // It's the one that generates them
        match message {
            RuntimeData::ControlMessage { .. } => Ok(false),
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_immediate_forwarding() {
        let coordinator = SpeculativeVADCoordinator::new();

        let audio = RuntimeData::Audio {
            samples: vec![0.1, 0.2, 0.3].into(),
            sample_rate: 16000,
            channels: 1,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata: None,
        };

        let mut outputs = Vec::new();
        let callback = |data: RuntimeData| {
            outputs.push(data);
            Ok(())
        };

        let result = coordinator
            .process_streaming(audio, Some("test_session".to_string()), callback)
            .await;

        assert!(result.is_ok());
        
        // Should at minimum emit the audio immediately
        assert!(!outputs.is_empty());
        
        // First output should be audio (immediate forwarding)
        match &outputs[0] {
            RuntimeData::Audio { samples, .. } => {
                assert_eq!(samples.len(), 3);
            }
            _ => panic!("First output should be audio"),
        }
    }

    #[tokio::test]
    async fn test_acceptance_rate_initial() {
        let coordinator = SpeculativeVADCoordinator::new();

        // Initially should be 1.0 (no data)
        let rate = coordinator.get_acceptance_rate("test").await;
        assert_eq!(rate, 1.0);
    }

    #[tokio::test]
    async fn test_session_isolation() {
        let coordinator = Arc::new(SpeculativeVADCoordinator::new());

        // Process audio from two different sessions
        for session_num in 0..2 {
            let coordinator_clone = coordinator.clone();
            let session_id = format!("session_{}", session_num);

            let audio = RuntimeData::Audio {
                samples: vec![session_num as f32; 100].into(),
                sample_rate: 16000,
                channels: 1,
                stream_id: None,
                timestamp_us: None,
                arrival_ts_us: None,
                metadata: None,
            };

            let callback = |_: RuntimeData| Ok(());
            let result = coordinator_clone
                .process_streaming(audio, Some(session_id.clone()), callback)
                .await;

            assert!(result.is_ok());
        }

        // Both sessions should have independent state
        let rate_0 = coordinator.get_acceptance_rate("session_0").await;
        let rate_1 = coordinator.get_acceptance_rate("session_1").await;

        assert_eq!(rate_0, 1.0);
        assert_eq!(rate_1, 1.0);
    }

    #[tokio::test]
    async fn test_session_cleanup() {
        let coordinator = SpeculativeVADCoordinator::new();

        // Create a session by processing some audio
        let audio = RuntimeData::Audio {
            samples: vec![0.1; 100].into(),
            sample_rate: 16000,
            channels: 1,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata: None,
        };

        let callback = |_: RuntimeData| Ok(());
        let _ = coordinator
            .process_streaming(audio, Some("to_cleanup".to_string()), callback)
            .await;

        // Session should exist
        {
            let sessions = coordinator.sessions.lock();
            assert!(sessions.contains_key("to_cleanup"));
        }

        // Terminate session
        coordinator.terminate_session("to_cleanup").await;

        // Session should be gone
        {
            let sessions = coordinator.sessions.lock();
            assert!(!sessions.contains_key("to_cleanup"));
        }
    }

    #[tokio::test]
    async fn test_config_customization() {
        let config = SpeculativeVADCoordinatorConfig {
            vad_threshold: 0.7,
            sample_rate: 8000,
            min_speech_duration_ms: 500,
            min_silence_duration_ms: 200,
            lookback_ms: 300,
            speech_pad_ms: 50,
        };

        let coordinator = SpeculativeVADCoordinator::with_config(config.clone());
        
        assert_eq!(coordinator.config.vad_threshold, 0.7);
        assert_eq!(coordinator.config.sample_rate, 8000);
        assert_eq!(coordinator.config.min_speech_duration_ms, 500);
    }
}
