//! CLI node registry for streaming pipelines
//!
//! Provides factory implementations for CLI-specific nodes that can be
//! registered with the streaming node registry.

use crate::audio::{AudioCapture, AudioPlayback, CaptureConfig, DeviceSelector, PlaybackConfig};
use remotemedia_runtime_core::capabilities::{
    AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue, MediaCapabilities,
    MediaConstraints,
};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::nodes::streaming_node::{
    AsyncStreamingNode, AsyncNodeWrapper,
    StreamingNode, StreamingNodeFactory, StreamingNodeRegistry,
};
use remotemedia_runtime_core::nodes::streaming_registry::create_default_streaming_registry;
use remotemedia_runtime_core::Error;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::mic_input::MicInputConfig;
use super::speaker_output::SpeakerOutputConfig;
use super::srt_output::SrtOutputConfig;

// ============================================================================
// MicInput Streaming Node
// ============================================================================

/// Streaming node for microphone input that actually captures audio
struct MicInputStreamingNode {
    config: MicInputConfig,
    capture: Mutex<Option<AudioCapture>>,
}

impl MicInputStreamingNode {
    fn new(config: MicInputConfig) -> Self {
        Self {
            config,
            capture: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for MicInputStreamingNode {
    fn node_type(&self) -> &str {
        "MicInput"
    }

    async fn initialize(&self) -> Result<(), Error> {
        let capture_config = CaptureConfig {
            device: self.config.device.as_ref().map(|s| DeviceSelector::parse(s)),
            host: self.config.host.clone(),
            sample_rate: self.config.sample_rate,
            channels: self.config.channels,
            buffer_size_ms: self.config.buffer_ms,
        };

        let capture = AudioCapture::start(capture_config)
            .map_err(|e| Error::Execution(format!("Failed to start audio capture: {}", e)))?;

        tracing::info!("MicInput initialized: capturing from '{}'", capture.device_name());
        
        *self.capture.lock().await = Some(capture);
        Ok(())
    }

    async fn process(&self, _data: RuntimeData) -> Result<RuntimeData, Error> {
        // Get audio from capture
        let mut guard = self.capture.lock().await;
        let capture = guard.as_mut()
            .ok_or_else(|| Error::Execution("MicInput not initialized".into()))?;

        // Try to receive audio (non-blocking, return what's available)
        match capture.try_recv() {
            Some(samples) => {
                Ok(RuntimeData::Audio {
                    samples,
                    sample_rate: self.config.sample_rate,
                    channels: self.config.channels as u32,
                    stream_id: None,
                })
            }
            None => {
                // No audio available yet - return empty audio
                Ok(RuntimeData::Audio {
                    samples: vec![],
                    sample_rate: self.config.sample_rate,
                    channels: self.config.channels as u32,
                    stream_id: None,
                })
            }
        }
    }

    async fn process_streaming<F>(
        &self,
        _data: RuntimeData,
        _session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        // Continuous audio capture loop
        let mut guard = self.capture.lock().await;
        let capture = guard.as_mut()
            .ok_or_else(|| Error::Execution("MicInput not initialized".into()))?;

        let chunk_size = if self.config.chunk_size > 0 { self.config.chunk_size } else { 4000 };
        let mut buffer = Vec::with_capacity(chunk_size);
        let mut chunks_sent = 0;

        // Receive audio and chunk it
        while let Some(samples) = capture.recv().await {
            buffer.extend(samples);

            // Send complete chunks
            while buffer.len() >= chunk_size {
                let chunk: Vec<f32> = buffer.drain(..chunk_size).collect();
                callback(RuntimeData::Audio {
                    samples: chunk,
                    sample_rate: self.config.sample_rate,
                    channels: self.config.channels as u32,
                    stream_id: None,
                })?;
                chunks_sent += 1;
            }
        }

        // Send remaining samples if any
        if !buffer.is_empty() {
            callback(RuntimeData::Audio {
                samples: buffer,
                sample_rate: self.config.sample_rate,
                channels: self.config.channels as u32,
                stream_id: None,
            })?;
            chunks_sent += 1;
        }

        Ok(chunks_sent)
    }
}

/// Factory for creating MicInput streaming nodes
pub struct MicInputNodeFactory;

impl StreamingNodeFactory for MicInputNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: MicInputConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        Ok(Box::new(AsyncNodeWrapper(Arc::new(MicInputStreamingNode::new(config)))))
    }

    fn node_type(&self) -> &str {
        "MicInput"
    }

    fn media_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
        // Parse config to get sample rate and channels
        let config: MicInputConfig = serde_json::from_value(params.clone()).unwrap_or_default();

        // MicInput produces audio output based on configured sample_rate and channels
        Some(MediaCapabilities::with_output(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(config.sample_rate)),
                channels: Some(ConstraintValue::Exact(config.channels as u32)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        )))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        // MicInput capabilities are determined by its configuration parameters
        CapabilityBehavior::Configured
    }
}

// ============================================================================
// SpeakerOutput Streaming Node
// ============================================================================

/// Streaming node for speaker output that actually plays audio
struct SpeakerOutputStreamingNode {
    config: SpeakerOutputConfig,
    playback: Mutex<Option<AudioPlayback>>,
}

impl SpeakerOutputStreamingNode {
    fn new(config: SpeakerOutputConfig) -> Self {
        Self {
            config,
            playback: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for SpeakerOutputStreamingNode {
    fn node_type(&self) -> &str {
        "SpeakerOutput"
    }

    async fn initialize(&self) -> Result<(), Error> {
        let playback_config = PlaybackConfig {
            device: self.config.device.as_ref().map(|s| DeviceSelector::parse(s)),
            host: self.config.host.clone(),
            sample_rate: self.config.sample_rate,
            channels: self.config.channels,
        };

        let playback = AudioPlayback::start(playback_config)
            .map_err(|e| Error::Execution(format!("Failed to start audio playback: {}", e)))?;

        tracing::info!("SpeakerOutput initialized: playing to '{}'", playback.device_name());
        
        *self.playback.lock().await = Some(playback);
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // Play audio to speaker
        if let RuntimeData::Audio { samples, sample_rate, .. } = &data {
            let mut guard = self.playback.lock().await;
            let playback = guard.as_mut()
                .ok_or_else(|| Error::Execution("SpeakerOutput not initialized".into()))?;

            // Resample if needed (simple case: just warn if mismatch)
            if *sample_rate != playback.config().sample_rate {
                tracing::warn!(
                    "Sample rate mismatch: input {} Hz, output {} Hz",
                    sample_rate,
                    playback.config().sample_rate
                );
            }

            playback.queue(samples);
            tracing::trace!("Played {} samples to speaker", samples.len());
        }

        // Sink node returns the input unchanged (for pipeline passthrough)
        Ok(data)
    }
}

/// Factory for creating SpeakerOutput streaming nodes
pub struct SpeakerOutputNodeFactory;

impl StreamingNodeFactory for SpeakerOutputNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: SpeakerOutputConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        Ok(Box::new(AsyncNodeWrapper(Arc::new(SpeakerOutputStreamingNode::new(config)))))
    }

    fn node_type(&self) -> &str {
        "SpeakerOutput"
    }

    fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
        // SpeakerOutput is a passthrough sink - it accepts whatever audio it receives
        // Capabilities are inherited from upstream during resolution
        None
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        // Output matches input (passthrough behavior)
        CapabilityBehavior::Passthrough
    }
}

// ============================================================================
// SrtOutput Streaming Node
// ============================================================================

use remotemedia_runtime_core::nodes::streaming_node::{SyncStreamingNode, SyncNodeWrapper};

/// Streaming node for SRT subtitle output
struct SrtOutputStreamingNode {
    #[allow(dead_code)]
    config: SrtOutputConfig,
    segment_counter: std::sync::atomic::AtomicUsize,
}

impl SrtOutputStreamingNode {
    fn new(config: SrtOutputConfig) -> Self {
        Self {
            config,
            segment_counter: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Convert seconds to SRT timecode format (HH:MM:SS,mmm)
    fn seconds_to_timecode(seconds: f64) -> String {
        let total_ms = (seconds * 1000.0).round() as u64;
        let ms = total_ms % 1000;
        let total_secs = total_ms / 1000;
        let secs = total_secs % 60;
        let total_mins = total_secs / 60;
        let mins = total_mins % 60;
        let hours = total_mins / 60;

        format!("{:02}:{:02}:{:02},{:03}", hours, mins, secs, ms)
    }

    /// Format a single segment as SRT
    fn format_segment(&self, start: f64, end: f64, text: &str) -> String {
        let counter = self.segment_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let start_tc = Self::seconds_to_timecode(start);
        let end_tc = Self::seconds_to_timecode(end);
        format!("{}\n{} --> {}\n{}\n\n", counter, start_tc, end_tc, text.trim())
    }
}

impl SyncStreamingNode for SrtOutputStreamingNode {
    fn node_type(&self) -> &str {
        "SrtOutput"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let mut srt_output = String::new();

        match data {
            RuntimeData::Json(ref json) => {
                // Check if this is a Whisper output with segments
                if let Some(segments) = json.get("segments").and_then(|s| s.as_array()) {
                    for segment in segments {
                        let start = segment.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let end = segment.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let text = segment.get("text").and_then(|v| v.as_str()).unwrap_or("");

                        if !text.trim().is_empty() {
                            srt_output.push_str(&self.format_segment(start, end, text));
                        }
                    }
                } else if let Some(text) = json.get("text").and_then(|t| t.as_str()) {
                    // Fallback: single text without segments
                    srt_output.push_str(&self.format_segment(0.0, 10.0, text));
                }
            }
            RuntimeData::Text(ref text) => {
                srt_output.push_str(&self.format_segment(0.0, 10.0, text));
            }
            _ => {}
        }

        Ok(RuntimeData::Text(srt_output))
    }
}

/// Factory for creating SrtOutput streaming nodes
pub struct SrtOutputNodeFactory;

impl StreamingNodeFactory for SrtOutputNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: SrtOutputConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        Ok(Box::new(SyncNodeWrapper(SrtOutputStreamingNode::new(config))))
    }

    fn node_type(&self) -> &str {
        "SrtOutput"
    }
}

// ============================================================================
// Registry Creation
// ============================================================================

/// Create a streaming registry with CLI nodes registered
///
/// This extends the default streaming registry with CLI-specific nodes:
/// - `MicInput` - Microphone audio capture source
/// - `SpeakerOutput` - Speaker audio playback sink
/// - `SrtOutput` - SRT subtitle format converter
pub fn create_cli_streaming_registry() -> StreamingNodeRegistry {
    let mut registry = create_default_streaming_registry();

    // Register CLI-specific nodes
    registry.register(Arc::new(MicInputNodeFactory));
    registry.register(Arc::new(SpeakerOutputNodeFactory));
    registry.register(Arc::new(SrtOutputNodeFactory));

    registry
}

/// Get CLI node factories for registration with a custom registry
pub fn get_cli_node_factories() -> Vec<Arc<dyn StreamingNodeFactory>> {
    vec![
        Arc::new(MicInputNodeFactory),
        Arc::new(SpeakerOutputNodeFactory),
        Arc::new(SrtOutputNodeFactory),
    ]
}
