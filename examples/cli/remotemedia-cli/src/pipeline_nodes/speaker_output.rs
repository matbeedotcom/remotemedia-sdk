//! Speaker output sink node
//!
//! Plays audio data through the system speakers. This is a sink node
//! (consumes input, produces no output).
//!
//! # Configuration
//!
//! ```yaml
//! - id: speaker
//!   node_type: SpeakerOutput
//!   params:
//!     sample_rate: 16000
//!     channels: 1
//!     device: "default"  # or specific device name/index
//! ```
//!
//! # Input
//!
//! Accepts `RuntimeData::Audio` data and plays it through the speaker.

use async_trait::async_trait;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::executor::node_executor::{NodeContext, NodeExecutor};
use remotemedia_runtime_core::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::audio::{DeviceSelector, PlaybackConfig};

/// Configuration for SpeakerOutputNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerOutputConfig {
    /// Sample rate in Hz (should match input audio)
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    /// Number of audio channels (should match input audio)
    #[serde(default = "default_channels")]
    pub channels: u16,

    /// Audio device name, index, or "default"
    #[serde(default)]
    pub device: Option<String>,

    /// Audio host/backend (alsa, pulse, coreaudio, wasapi)
    #[serde(default)]
    pub host: Option<String>,
}

fn default_sample_rate() -> u32 {
    16000
}

fn default_channels() -> u16 {
    1
}

impl Default for SpeakerOutputConfig {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            channels: default_channels(),
            device: None,
            host: None,
        }
    }
}

/// Command sent to the playback thread
enum PlaybackCommand {
    Queue(Vec<f32>),
    Stop,
}

/// Handle for the audio playback thread
struct PlaybackHandle {
    /// Sender for playback commands
    sender: mpsc::Sender<PlaybackCommand>,
    /// Shared state for completion checking
    is_complete: Arc<AtomicBool>,
    /// Shared counter for samples played
    samples_played: Arc<AtomicUsize>,
    /// Thread handle
    _thread: JoinHandle<()>,
}

/// Start audio playback on a dedicated thread
fn start_playback_thread(config: &SpeakerOutputConfig) -> Result<(PlaybackHandle, String)> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let playback_config = PlaybackConfig {
        device: config.device.as_ref().map(|s| DeviceSelector::parse(s)),
        host: config.host.clone(),
        sample_rate: config.sample_rate,
        channels: config.channels,
    };

    // Get host and device on main thread to capture device name
    let host = crate::audio::get_host(playback_config.host.as_deref()).map_err(|e| {
        remotemedia_runtime_core::Error::Execution(format!("Failed to get audio host: {}", e))
    })?;

    let device = match &playback_config.device {
        Some(selector) => {
            crate::audio::find_output_device(selector, playback_config.host.as_deref()).map_err(
                |e| {
                    remotemedia_runtime_core::Error::Execution(format!(
                        "Failed to find output device: {}",
                        e
                    ))
                },
            )?
        }
        None => host.default_output_device().ok_or_else(|| {
            remotemedia_runtime_core::Error::Execution(
                "No default output device available".to_string(),
            )
        })?,
    };

    let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
    let device_name_clone = device_name.clone();

    let sample_rate = config.sample_rate;
    let channels = config.channels;

    let (cmd_tx, cmd_rx) = mpsc::channel::<PlaybackCommand>();
    let is_complete = Arc::new(AtomicBool::new(true));
    let is_complete_clone = is_complete.clone();
    let samples_played = Arc::new(AtomicUsize::new(0));
    let samples_played_clone = samples_played.clone();

    let thread = thread::spawn(move || {
        use std::sync::Mutex;

        // Build the stream on this thread (cpal::Stream is !Send)
        let supported_config = match device
            .supported_output_configs()
            .ok()
            .and_then(|mut configs| configs.find(|c| c.channels() == channels))
        {
            Some(c) => c.with_sample_rate(cpal::SampleRate(sample_rate)),
            None => {
                tracing::error!(
                    "Device '{}' doesn't support {} channel(s)",
                    device_name_clone,
                    channels
                );
                return;
            }
        };

        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let position = Arc::new(Mutex::new(0usize));
        let buffer_clone = buffer.clone();
        let position_clone = position.clone();
        let is_complete_inner = is_complete_clone.clone();
        let samples_played_inner = samples_played_clone.clone();

        let stream = match device.build_output_stream(
            &supported_config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let buf = buffer_clone.lock().unwrap();
                let mut pos = position_clone.lock().unwrap();

                for sample in data.iter_mut() {
                    if *pos < buf.len() {
                        *sample = buf[*pos];
                        *pos += 1;
                        samples_played_inner.fetch_add(1, Ordering::Relaxed);
                    } else {
                        *sample = 0.0;
                    }
                }

                // Update completion status
                is_complete_inner.store(*pos >= buf.len(), Ordering::Relaxed);
            },
            |err| {
                tracing::error!("Audio playback error: {}", err);
            },
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to build output stream: {}", e);
                return;
            }
        };

        if let Err(e) = stream.play() {
            tracing::error!("Failed to start output stream: {}", e);
            return;
        }

        tracing::debug!("Audio playback thread started");

        // Process commands
        loop {
            match cmd_rx.recv() {
                Ok(PlaybackCommand::Queue(samples)) => {
                    let mut buf = buffer.lock().unwrap();
                    buf.extend(samples);
                    is_complete_clone.store(false, Ordering::Relaxed);
                }
                Ok(PlaybackCommand::Stop) | Err(_) => break,
            }
        }

        tracing::debug!("Audio playback thread stopped");
    });

    let handle = PlaybackHandle {
        sender: cmd_tx,
        is_complete,
        samples_played,
        _thread: thread,
    };

    Ok((handle, device_name))
}

/// Speaker output sink node
///
/// Plays audio through the speaker. This is a sink node - it consumes
/// input but produces no output.
pub struct SpeakerOutputNode {
    config: SpeakerOutputConfig,
    playback_handle: Option<PlaybackHandle>,
    device_name: String,
}

// Safety: The playback thread owns the cpal::Stream.
// We only hold channels and atomic counters, which are Send+Sync.
unsafe impl Send for SpeakerOutputNode {}
unsafe impl Sync for SpeakerOutputNode {}

impl SpeakerOutputNode {
    /// Create a new SpeakerOutputNode with default config
    pub fn new() -> Self {
        Self {
            config: SpeakerOutputConfig::default(),
            playback_handle: None,
            device_name: String::new(),
        }
    }

    /// Create from JSON params
    pub fn from_params(params: Value) -> Self {
        let config: SpeakerOutputConfig = serde_json::from_value(params).unwrap_or_default();
        Self {
            config,
            playback_handle: None,
            device_name: String::new(),
        }
    }

    /// Create from CLI audio output args
    pub fn from_audio_args(
        args: &crate::audio::AudioOutputArgs,
        input_rate: u32,
        input_channels: u16,
    ) -> Self {
        Self {
            config: SpeakerOutputConfig {
                sample_rate: args.effective_sample_rate(input_rate),
                channels: args.effective_channels(input_channels),
                device: args.output_device.clone(),
                host: None,
            },
            playback_handle: None,
            device_name: String::new(),
        }
    }
}

impl Default for SpeakerOutputNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeExecutor for SpeakerOutputNode {
    async fn initialize(&mut self, ctx: &NodeContext) -> Result<()> {
        // Re-parse config from context params
        if let Ok(config) = serde_json::from_value::<SpeakerOutputConfig>(ctx.params.clone()) {
            self.config = config;
        }

        let (handle, device_name) = start_playback_thread(&self.config)?;
        self.playback_handle = Some(handle);
        self.device_name = device_name;

        tracing::info!(
            "SpeakerOutputNode initialized: device='{}', {}Hz, {} ch",
            self.device_name,
            self.config.sample_rate,
            self.config.channels
        );

        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        let handle = self.playback_handle.as_ref().ok_or_else(|| {
            remotemedia_runtime_core::Error::Execution(
                "Audio playback not initialized".to_string(),
            )
        })?;

        // Try to deserialize as RuntimeData::Audio
        let audio_data: RuntimeData = serde_json::from_value(input.clone())?;

        if let RuntimeData::Audio { samples, .. } = audio_data {
            handle
                .sender
                .send(PlaybackCommand::Queue(samples.clone()))
                .map_err(|e| {
                    remotemedia_runtime_core::Error::Execution(format!(
                        "Failed to queue audio: {}",
                        e
                    ))
                })?;

            tracing::trace!("Queued {} samples for playback", samples.len());
        } else {
            tracing::warn!("SpeakerOutputNode received non-audio input, ignoring");
        }

        // Sink node produces no output
        Ok(vec![])
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Wait for playback to finish
        if let Some(handle) = &self.playback_handle {
            let timeout = std::time::Duration::from_millis(500);
            let start = std::time::Instant::now();

            while !handle.is_complete.load(Ordering::Relaxed) && start.elapsed() < timeout {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            let samples = handle.samples_played.load(Ordering::Relaxed);
            tracing::info!("SpeakerOutputNode cleanup: played {} samples", samples);
        }

        // Signal the playback thread to stop
        if let Some(handle) = self.playback_handle.take() {
            let _ = handle.sender.send(PlaybackCommand::Stop);
            // Thread will exit and join handle will be dropped
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = SpeakerOutputConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
        assert!(config.device.is_none());
    }

    #[test]
    fn test_from_params() {
        let params = serde_json::json!({
            "sample_rate": 48000,
            "channels": 2,
            "device": "DAC"
        });

        let node = SpeakerOutputNode::from_params(params);
        assert_eq!(node.config.sample_rate, 48000);
        assert_eq!(node.config.channels, 2);
        assert_eq!(node.config.device, Some("DAC".to_string()));
    }
}
