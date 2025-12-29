//! Speaker playback using cpal with device selection support
//!
//! # Examples
//!
//! ```no_run
//! use remotemedia_cli::audio::{AudioPlayback, PlaybackConfig, DeviceSelector};
//!
//! # fn example() -> anyhow::Result<()> {
//! // Playback on default device
//! let config = PlaybackConfig::default();
//! let playback = AudioPlayback::start(config)?;
//!
//! // Playback on specific device
//! let config = PlaybackConfig {
//!     device: Some(DeviceSelector::Name("DAC".into())),
//!     sample_rate: 48000,
//!     ..Default::default()
//! };
//! let playback = AudioPlayback::start(config)?;
//!
//! // Queue and play samples
//! playback.queue(&samples);
//!
//! // Wait for completion
//! while !playback.is_complete() {
//!     std::thread::sleep(std::time::Duration::from_millis(10));
//! }
//! # Ok(())
//! # }
//! # let samples: Vec<f32> = vec![];
//! ```

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Device;
use std::sync::{Arc, Mutex};

use super::args::DeviceSelector;
use super::devices::{find_output_device, get_host};

/// Audio playback configuration
#[derive(Debug, Clone)]
pub struct PlaybackConfig {
    /// Device selector (name, index, or default)
    pub device: Option<DeviceSelector>,
    /// Audio host/backend name (platform-specific)
    pub host: Option<String>,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            device: None,
            host: None,
            sample_rate: 48000,
            channels: 1,
        }
    }
}

impl PlaybackConfig {
    /// Create config from AudioOutputArgs with fallback sample rate/channels
    pub fn from_args(args: &super::args::AudioOutputArgs, input_rate: u32, input_channels: u16) -> Self {
        Self {
            device: args.output_device.as_ref().map(|s| DeviceSelector::parse(s)),
            host: None, // Output uses same host as input by default
            sample_rate: args.effective_sample_rate(input_rate),
            channels: args.effective_channels(input_channels),
        }
    }
}

/// Audio playback handle (Send + Sync safe)
/// 
/// The actual cpal::Stream runs on a dedicated thread, and this handle
/// communicates via channels. This makes it safe to use in async contexts.
pub struct AudioPlayback {
    /// Shared buffer for samples to play
    buffer: Arc<Mutex<Vec<f32>>>,
    /// Current playback position
    position: Arc<Mutex<usize>>,
    /// Device name for logging
    device_name: String,
    /// Original config
    config: PlaybackConfig,
    /// Handle to stop the playback thread
    _stop_tx: std::sync::mpsc::Sender<()>,
}

// Ensure AudioPlayback is Send + Sync
unsafe impl Send for AudioPlayback {}
unsafe impl Sync for AudioPlayback {}

impl AudioPlayback {
    /// Start playback on the specified or default output device
    pub fn start(config: PlaybackConfig) -> Result<Self> {
        let device = Self::get_device(&config)?;
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        
        tracing::info!(
            "Starting audio playback on '{}' ({}Hz, {} ch)",
            device_name,
            config.sample_rate,
            config.channels
        );

        let supported_config = device
            .supported_output_configs()?
            .find(|c| c.channels() == config.channels)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Device '{}' doesn't support {} channel(s). Try --output-channels 2 for stereo.",
                    device_name,
                    config.channels
                )
            })?
            .with_sample_rate(cpal::SampleRate(config.sample_rate));

        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let position = Arc::new(Mutex::new(0usize));
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();

        let buffer_clone = buffer.clone();
        let position_clone = position.clone();
        let device_name_clone = device_name.clone();

        // Spawn a dedicated thread to own the cpal::Stream (which is !Send)
        std::thread::spawn(move || {
            let stream = match device.build_output_stream(
                &supported_config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let buf = buffer_clone.lock().unwrap();
                    let mut pos = position_clone.lock().unwrap();

                    for sample in data.iter_mut() {
                        if *pos < buf.len() {
                            *sample = buf[*pos];
                            *pos += 1;
                        } else {
                            *sample = 0.0;
                        }
                    }
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
                tracing::error!("Failed to start stream: {}", e);
                return;
            }

            tracing::debug!("Audio playback thread started for '{}'", device_name_clone);

            // Keep the thread alive until stop signal
            let _ = stop_rx.recv();

            tracing::debug!("Audio playback thread stopping for '{}'", device_name_clone);
            // Stream is dropped here, stopping playback
        });

        Ok(Self {
            buffer,
            position,
            device_name,
            config,
            _stop_tx: stop_tx,
        })
    }

    /// Get the device based on configuration
    fn get_device(config: &PlaybackConfig) -> Result<Device> {
        match &config.device {
            Some(selector) => find_output_device(selector, config.host.as_deref()),
            None => {
                let host = get_host(config.host.as_deref())?;
                host.default_output_device()
                    .ok_or_else(|| anyhow::anyhow!("No default output device available"))
            }
        }
    }

    /// Queue audio samples for playback
    pub fn queue(&self, samples: &[f32]) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend_from_slice(samples);
    }

    /// Clear the playback buffer
    pub fn clear(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        let mut position = self.position.lock().unwrap();
        buffer.clear();
        *position = 0;
    }

    /// Check if playback is complete
    pub fn is_complete(&self) -> bool {
        let buffer = self.buffer.lock().unwrap();
        let position = self.position.lock().unwrap();
        *position >= buffer.len()
    }

    /// Get current playback position in samples
    pub fn position(&self) -> usize {
        *self.position.lock().unwrap()
    }

    /// Get total samples queued
    pub fn total_samples(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }

    /// Get the device name being used
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Get the playback configuration
    pub fn config(&self) -> &PlaybackConfig {
        &self.config
    }
}

/// Play audio samples synchronously
pub fn play_sync(samples: &[f32], config: PlaybackConfig) -> Result<()> {
    let playback = AudioPlayback::start(config)?;
    playback.queue(samples);

    // Wait for playback to complete
    while !playback.is_complete() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Small delay to ensure final samples are played
    std::thread::sleep(std::time::Duration::from_millis(50));

    Ok(())
}

/// Play audio samples using AudioOutputArgs
pub fn play_from_args(
    samples: &[f32],
    args: &super::args::AudioOutputArgs,
    sample_rate: u32,
    channels: u16,
) -> Result<()> {
    let config = PlaybackConfig::from_args(args, sample_rate, channels);
    play_sync(samples, config)
}
