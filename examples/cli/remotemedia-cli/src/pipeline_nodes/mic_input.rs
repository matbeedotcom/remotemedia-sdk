//! Microphone input source node
//!
//! Captures audio from the system microphone and outputs it as audio data
//! for pipeline processing. This is a source node (no input, produces output).
//!
//! # Configuration
//!
//! ```yaml
//! - id: mic
//!   node_type: MicInput
//!   params:
//!     sample_rate: 16000
//!     channels: 1
//!     device: "default"  # or specific device name/index
//!     chunk_size: 4000   # samples per chunk
//! ```
//!
//! # Output
//!
//! Produces `RuntimeData::Audio` chunks continuously until stopped.

use async_trait::async_trait;
use remotemedia_runtime_core::capabilities::{
    AudioConstraints, AudioSampleFormat, ConstraintValue, MediaCapabilities, MediaConstraints,
};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::executor::node_executor::{NodeContext, NodeExecutor};
use remotemedia_runtime_core::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use crate::audio::{CaptureConfig, DeviceSelector};

/// Configuration for MicInputNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicInputConfig {
    /// Sample rate in Hz
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    /// Number of audio channels (1 = mono, 2 = stereo)
    #[serde(default = "default_channels")]
    pub channels: u16,

    /// Audio device name, index, or "default"
    #[serde(default)]
    pub device: Option<String>,

    /// Audio host/backend (alsa, pulse, coreaudio, wasapi)
    #[serde(default)]
    pub host: Option<String>,

    /// Number of samples per output chunk
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,

    /// Buffer size in milliseconds (affects latency)
    #[serde(default = "default_buffer_ms")]
    pub buffer_ms: u32,
}

fn default_sample_rate() -> u32 {
    16000
}

fn default_channels() -> u16 {
    1
}

fn default_chunk_size() -> usize {
    4000
}

fn default_buffer_ms() -> u32 {
    20
}

impl Default for MicInputConfig {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            channels: default_channels(),
            device: None,
            host: None,
            chunk_size: default_chunk_size(),
            buffer_ms: default_buffer_ms(),
        }
    }
}

/// Handle for the audio capture thread
struct CaptureHandle {
    /// Receiver for audio samples from the capture thread
    receiver: mpsc::Receiver<Vec<f32>>,
    /// Channel to stop the capture thread
    stop_tx: mpsc::Sender<()>,
    /// Thread handle
    _thread: JoinHandle<()>,
}

/// Start audio capture on a dedicated thread
fn start_capture_thread(config: &MicInputConfig) -> Result<(CaptureHandle, String)> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let capture_config = CaptureConfig {
        device: config.device.as_ref().map(|s| DeviceSelector::parse(s)),
        host: config.host.clone(),
        sample_rate: config.sample_rate,
        channels: config.channels,
        buffer_size_ms: config.buffer_ms,
    };

    // Get host and device on main thread to capture device name
    let host = crate::audio::get_host(capture_config.host.as_deref()).map_err(|e| {
        remotemedia_runtime_core::Error::Execution(format!("Failed to get audio host: {}", e))
    })?;

    let device = match &capture_config.device {
        Some(selector) => crate::audio::find_input_device(selector, capture_config.host.as_deref())
            .map_err(|e| {
                remotemedia_runtime_core::Error::Execution(format!(
                    "Failed to find input device: {}",
                    e
                ))
            })?,
        None => host.default_input_device().ok_or_else(|| {
            remotemedia_runtime_core::Error::Execution(
                "No default input device available".to_string(),
            )
        })?,
    };

    let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
    let device_name_clone = device_name.clone();

    let sample_rate = config.sample_rate;
    let channels = config.channels;

    let (sample_tx, sample_rx) = mpsc::channel::<Vec<f32>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    let thread = thread::spawn(move || {
        // Build the stream on this thread (cpal::Stream is !Send)
        let supported_config = match device
            .supported_input_configs()
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

        let tx = sample_tx;
        let stream = match device.build_input_stream(
            &supported_config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(data.to_vec());
            },
            |err| {
                tracing::error!("Audio capture error: {}", err);
            },
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to build input stream: {}", e);
                return;
            }
        };

        if let Err(e) = stream.play() {
            tracing::error!("Failed to start input stream: {}", e);
            return;
        }

        tracing::debug!("Audio capture thread started");

        // Keep thread alive until stop signal
        loop {
            match stop_rx.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {
                    thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }

        tracing::debug!("Audio capture thread stopped");
    });

    let handle = CaptureHandle {
        receiver: sample_rx,
        stop_tx,
        _thread: thread,
    };

    Ok((handle, device_name))
}

/// Microphone input source node
///
/// Captures audio from the microphone and outputs it as RuntimeData::Audio.
/// This is a source node - it doesn't process input, it generates output.
pub struct MicInputNode {
    config: MicInputConfig,
    capture_handle: Option<CaptureHandle>,
    buffer: Vec<f32>,
    device_name: String,
}

// Safety: The capture thread owns the cpal::Stream.
// We only hold channels and a join handle, which are Send+Sync.
unsafe impl Send for MicInputNode {}
unsafe impl Sync for MicInputNode {}

impl MicInputNode {
    /// Create a new MicInputNode with default config
    pub fn new() -> Self {
        Self {
            config: MicInputConfig::default(),
            capture_handle: None,
            buffer: Vec::new(),
            device_name: String::new(),
        }
    }

    /// Returns the media capabilities for this node (spec 022).
    ///
    /// **Output capabilities:**
    /// - Audio: configurable sample rate and channels, f32 format
    ///
    /// This is a source node with no inputs. Output format is determined
    /// by the node configuration (sample_rate, channels params).
    ///
    /// Default output: 16kHz mono f32 audio (matches Whisper requirements).
    pub fn media_capabilities(config: &MicInputConfig) -> MediaCapabilities {
        MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(config.sample_rate)),
            channels: Some(ConstraintValue::Exact(config.channels as u32)),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        }))
    }

    /// Returns the default media capabilities (16kHz mono f32).
    pub fn default_media_capabilities() -> MediaCapabilities {
        Self::media_capabilities(&MicInputConfig::default())
    }

    /// Create from JSON params
    pub fn from_params(params: Value) -> Self {
        let config: MicInputConfig = serde_json::from_value(params).unwrap_or_default();
        Self {
            config,
            capture_handle: None,
            buffer: Vec::new(),
            device_name: String::new(),
        }
    }

    /// Create from CLI audio input args
    pub fn from_audio_args(args: &crate::audio::AudioInputArgs) -> Self {
        Self {
            config: MicInputConfig {
                sample_rate: args.sample_rate,
                channels: args.channels,
                device: args.input_device.clone(),
                host: args.audio_host.clone(),
                chunk_size: default_chunk_size(),
                buffer_ms: args.buffer_ms,
            },
            capture_handle: None,
            buffer: Vec::new(),
            device_name: String::new(),
        }
    }
}

impl Default for MicInputNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeExecutor for MicInputNode {
    async fn initialize(&mut self, ctx: &NodeContext) -> Result<()> {
        // Re-parse config from context params
        if let Ok(config) = serde_json::from_value::<MicInputConfig>(ctx.params.clone()) {
            self.config = config;
        }

        let (handle, device_name) = start_capture_thread(&self.config)?;
        self.capture_handle = Some(handle);
        self.device_name = device_name;
        self.buffer.clear();

        tracing::info!(
            "MicInputNode initialized: device='{}', {}Hz, {} ch, chunk_size={}",
            self.device_name,
            self.config.sample_rate,
            self.config.channels,
            self.config.chunk_size
        );

        Ok(())
    }

    async fn process(&mut self, _input: Value) -> Result<Vec<Value>> {
        // As a source node, we ignore input and generate audio output
        let handle = self.capture_handle.as_ref().ok_or_else(|| {
            remotemedia_runtime_core::Error::Execution(
                "Audio capture not initialized".to_string(),
            )
        })?;

        // Drain all available samples from the channel
        while let Ok(samples) = handle.receiver.try_recv() {
            self.buffer.extend(samples);
        }

        // Output chunks when we have enough samples
        let mut outputs = Vec::new();

        while self.buffer.len() >= self.config.chunk_size {
            let chunk: Vec<f32> = self.buffer.drain(..self.config.chunk_size).collect();

            let audio_data = RuntimeData::Audio {
                samples: chunk,
                sample_rate: self.config.sample_rate,
                channels: self.config.channels as u32,
                stream_id: None,
                timestamp_us: None,
                arrival_ts_us: None,
            };

            let value = serde_json::to_value(&audio_data)?;
            outputs.push(value);
        }

        Ok(outputs)
    }

    async fn cleanup(&mut self) -> Result<()> {
        tracing::info!("MicInputNode cleanup: stopping capture");

        // Signal the capture thread to stop
        if let Some(handle) = self.capture_handle.take() {
            let _ = handle.stop_tx.send(());
            // Thread will exit and join handle will be dropped
        }

        self.buffer.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = MicInputConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
        assert_eq!(config.chunk_size, 4000);
        assert!(config.device.is_none());
    }

    #[test]
    fn test_from_params() {
        let params = serde_json::json!({
            "sample_rate": 48000,
            "channels": 2,
            "device": "USB Mic",
            "chunk_size": 8000
        });

        let node = MicInputNode::from_params(params);
        assert_eq!(node.config.sample_rate, 48000);
        assert_eq!(node.config.channels, 2);
        assert_eq!(node.config.device, Some("USB Mic".to_string()));
        assert_eq!(node.config.chunk_size, 8000);
    }

    #[test]
    fn test_default_media_capabilities() {
        let caps = MicInputNode::default_media_capabilities();

        // Source node has no inputs
        assert!(caps.accepts_any());

        // Check output constraints (default 16kHz mono f32)
        let output = caps.default_output().expect("Should have default output");
        match output {
            MediaConstraints::Audio(audio) => {
                assert_eq!(audio.sample_rate, Some(ConstraintValue::Exact(16000)));
                assert_eq!(audio.channels, Some(ConstraintValue::Exact(1)));
                assert_eq!(
                    audio.format,
                    Some(ConstraintValue::Exact(AudioSampleFormat::F32))
                );
            }
            _ => panic!("Expected Audio output constraints"),
        }
    }

    #[test]
    fn test_custom_media_capabilities() {
        let config = MicInputConfig {
            sample_rate: 48000,
            channels: 2,
            ..Default::default()
        };
        let caps = MicInputNode::media_capabilities(&config);

        let output = caps.default_output().expect("Should have default output");
        match output {
            MediaConstraints::Audio(audio) => {
                assert_eq!(audio.sample_rate, Some(ConstraintValue::Exact(48000)));
                assert_eq!(audio.channels, Some(ConstraintValue::Exact(2)));
            }
            _ => panic!("Expected Audio output constraints"),
        }
    }
}
