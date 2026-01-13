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
    AsyncStreamingNode,
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
///
/// Implements RuntimeDiscovered capability behavior when device is not explicitly
/// specified. This enables two-phase capability resolution:
/// - Phase 1: Use potential_capabilities() with broad range for early validation
/// - Phase 2: After initialize(), use actual_capabilities() with discovered device caps
pub struct MicInputStreamingNode {
    node_id: String,
    config: MicInputConfig,
    capture: Mutex<Option<AudioCapture>>,
    /// Discovered device capabilities (populated during initialize)
    discovered_capabilities: Mutex<Option<MediaCapabilities>>,
}

impl MicInputStreamingNode {
    pub fn new(node_id: String, config: MicInputConfig) -> Self {
        Self {
            node_id,
            config,
            capture: Mutex::new(None),
            discovered_capabilities: Mutex::new(None),
        }
    }

    /// Check if this node uses RuntimeDiscovered behavior
    /// (when device is not explicitly specified or is "default")
    fn is_runtime_discovered(&self) -> bool {
        match &self.config.device {
            None => true,
            Some(d) => d.to_lowercase() == "default",
        }
    }

    /// Get potential capabilities for Phase 1 validation (broad range)
    fn potential_caps() -> MediaCapabilities {
        MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
            channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        }))
    }

    /// Get configured capabilities when device is explicit
    fn configured_caps(config: &MicInputConfig) -> MediaCapabilities {
        MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(config.sample_rate)),
            channels: Some(ConstraintValue::Exact(config.channels as u32)),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        }))
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

        // Store discovered capabilities for Phase 2 validation
        // The actual device may have different capabilities than requested
        let actual_sample_rate = capture.config().sample_rate;
        let actual_channels = capture.config().channels;

        let discovered = MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(actual_sample_rate)),
            channels: Some(ConstraintValue::Exact(actual_channels as u32)),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        }));

        *self.discovered_capabilities.lock().await = Some(discovered);

        tracing::info!(
            "MicInput initialized: capturing from '{}' ({}Hz, {} ch)",
            capture.device_name(),
            actual_sample_rate,
            actual_channels
        );

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
                    timestamp_us: None,
                    arrival_ts_us: None,
                })
            }
            None => {
                // No audio available yet - return empty audio
                Ok(RuntimeData::Audio {
                    samples: vec![],
                    sample_rate: self.config.sample_rate,
                    channels: self.config.channels as u32,
                    stream_id: None,
                    timestamp_us: None,
                    arrival_ts_us: None,
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
                    timestamp_us: None,
                    arrival_ts_us: None,
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
                timestamp_us: None,
                arrival_ts_us: None,
            })?;
            chunks_sent += 1;
        }

        Ok(chunks_sent)
    }
}

// Implement StreamingNode directly for MicInputStreamingNode to support capability methods
#[async_trait::async_trait]
impl StreamingNode for MicInputStreamingNode {
    fn node_type(&self) -> &str {
        "MicInput"
    }

    fn node_id(&self) -> &str {
        &self.node_id
    }

    async fn initialize(&self) -> Result<(), Error> {
        <Self as AsyncStreamingNode>::initialize(self).await
    }

    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        <Self as AsyncStreamingNode>::process(self, data).await
    }

    async fn process_multi_async(
        &self,
        inputs: std::collections::HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
        if let Some((_name, data)) = inputs.into_iter().next() {
            self.process_async(data).await
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    fn is_multi_input(&self) -> bool {
        false
    }

    async fn process_streaming_async(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: Box<dyn FnMut(RuntimeData) -> Result<(), Error> + Send>,
    ) -> Result<usize, Error> {
        <Self as AsyncStreamingNode>::process_streaming(self, data, session_id, move |output| {
            callback(output)
        })
        .await
    }

    // Capability Resolution Methods (spec 023)

    fn media_capabilities(&self) -> Option<MediaCapabilities> {
        if self.is_runtime_discovered() {
            // For RuntimeDiscovered, return None here - use potential/actual instead
            None
        } else {
            Some(Self::configured_caps(&self.config))
        }
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        if self.is_runtime_discovered() {
            CapabilityBehavior::RuntimeDiscovered
        } else {
            CapabilityBehavior::Configured
        }
    }

    fn potential_capabilities(&self) -> Option<MediaCapabilities> {
        if self.is_runtime_discovered() {
            Some(Self::potential_caps())
        } else {
            Some(Self::configured_caps(&self.config))
        }
    }

    fn actual_capabilities(&self) -> Option<MediaCapabilities> {
        // Try to get discovered capabilities (set during initialize)
        // This is a sync method, so we can't await the lock
        // Use try_lock and fall back to configured caps
        if let Ok(guard) = self.discovered_capabilities.try_lock() {
            if let Some(ref caps) = *guard {
                return Some(caps.clone());
            }
        }
        // Fall back to configured capabilities
        Some(Self::configured_caps(&self.config))
    }
}

/// Factory for creating MicInput streaming nodes
pub struct MicInputNodeFactory;

impl StreamingNodeFactory for MicInputNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: MicInputConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        Ok(Box::new(MicInputStreamingNode::new(node_id, config)))
    }

    fn node_type(&self) -> &str {
        "MicInput"
    }

    fn media_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
        let config: MicInputConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        // Always return configured capabilities - this is used for resolution
        Some(MicInputStreamingNode::configured_caps(&config))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        // Return Configured as the default behavior
        // The node determines RuntimeDiscovered based on whether device is "default"
        CapabilityBehavior::Configured
    }

    fn potential_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
        // Return broad range for Phase 1 validation (used by RuntimeDiscovered nodes)
        Some(MicInputStreamingNode::potential_caps())
    }
}

// ============================================================================
// SpeakerOutput Streaming Node
// ============================================================================

/// Streaming node for speaker output that actually plays audio
///
/// Implements Passthrough capability behavior - it accepts whatever audio format
/// it receives from upstream and attempts to play it on the output device.
pub struct SpeakerOutputStreamingNode {
    node_id: String,
    config: SpeakerOutputConfig,
    playback: Mutex<Option<AudioPlayback>>,
    /// Capabilities inherited from upstream (set during resolution)
    inherited_capabilities: Mutex<Option<MediaCapabilities>>,
}

impl SpeakerOutputStreamingNode {
    pub fn new(node_id: String, config: SpeakerOutputConfig) -> Self {
        Self {
            node_id,
            config,
            playback: Mutex::new(None),
            inherited_capabilities: Mutex::new(None),
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

        tracing::info!(
            "SpeakerOutput initialized: playing to '{}' ({}Hz, {} ch)",
            playback.device_name(),
            playback.config().sample_rate,
            playback.config().channels
        );

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

// Implement StreamingNode directly to support capability methods
#[async_trait::async_trait]
impl StreamingNode for SpeakerOutputStreamingNode {
    fn node_type(&self) -> &str {
        "SpeakerOutput"
    }

    fn node_id(&self) -> &str {
        &self.node_id
    }

    async fn initialize(&self) -> Result<(), Error> {
        <Self as AsyncStreamingNode>::initialize(self).await
    }

    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        <Self as AsyncStreamingNode>::process(self, data).await
    }

    async fn process_multi_async(
        &self,
        inputs: std::collections::HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
        if let Some((_name, data)) = inputs.into_iter().next() {
            self.process_async(data).await
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    fn is_multi_input(&self) -> bool {
        false
    }

    // Capability Resolution Methods (spec 023)

    fn media_capabilities(&self) -> Option<MediaCapabilities> {
        // Passthrough node - capabilities are inherited from upstream
        // Return None to signal passthrough behavior
        None
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Passthrough
    }

    fn potential_capabilities(&self) -> Option<MediaCapabilities> {
        // Accept any audio format
        Some(MediaCapabilities::with_input(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
            channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        })))
    }

    fn actual_capabilities(&self) -> Option<MediaCapabilities> {
        // Return inherited capabilities if set, otherwise potential capabilities
        if let Ok(guard) = self.inherited_capabilities.try_lock() {
            if let Some(ref caps) = *guard {
                return Some(caps.clone());
            }
        }
        self.potential_capabilities()
    }
}

/// Factory for creating SpeakerOutput streaming nodes
pub struct SpeakerOutputNodeFactory;

impl StreamingNodeFactory for SpeakerOutputNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: SpeakerOutputConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        Ok(Box::new(SpeakerOutputStreamingNode::new(node_id, config)))
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
