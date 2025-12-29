//! Microphone capture using cpal with device selection support
//!
//! # Examples
//!
//! ```no_run
//! use remotemedia_cli::audio::{AudioCapture, CaptureConfig, DeviceSelector};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Capture from default device
//! let config = CaptureConfig::default();
//! let mut capture = AudioCapture::start(config)?;
//!
//! // Capture from specific device
//! let config = CaptureConfig {
//!     device: Some(DeviceSelector::Name("USB Microphone".into())),
//!     sample_rate: 48000,
//!     ..Default::default()
//! };
//! let mut capture = AudioCapture::start(config)?;
//!
//! // Receive audio samples
//! while let Some(samples) = capture.recv().await {
//!     // Process samples...
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Device;
use std::sync::mpsc;
use tokio::sync::broadcast;

use super::args::DeviceSelector;
use super::devices::{find_input_device, get_host};

/// Audio capture configuration
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Device selector (name, index, or default)
    pub device: Option<DeviceSelector>,
    /// Audio host/backend name (platform-specific)
    pub host: Option<String>,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// Buffer size in milliseconds
    pub buffer_size_ms: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            device: None,
            host: None,
            sample_rate: 48000,
            channels: 1,
            buffer_size_ms: 20,
        }
    }
}

impl CaptureConfig {
    /// Create config from AudioInputArgs
    pub fn from_args(args: &super::args::AudioInputArgs) -> Self {
        Self {
            device: args.input_device.as_ref().map(|s| DeviceSelector::parse(s)),
            host: args.audio_host.clone(),
            sample_rate: args.sample_rate,
            channels: args.channels,
            buffer_size_ms: args.buffer_ms,
        }
    }
}

/// Audio capture handle
pub struct AudioCapture {
    #[allow(dead_code)]
    stream: cpal::Stream,
    receiver: broadcast::Receiver<Vec<f32>>,
    device_name: String,
    config: CaptureConfig,
}

impl AudioCapture {
    /// Start capturing from the specified or default input device
    pub fn start(config: CaptureConfig) -> Result<Self> {
        let device = Self::get_device(&config)?;
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        
        tracing::info!(
            "Starting audio capture from '{}' ({}Hz, {} ch)",
            device_name,
            config.sample_rate,
            config.channels
        );

        let supported_config = device
            .supported_input_configs()?
            .find(|c| c.channels() == config.channels)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Device '{}' doesn't support {} channel(s). Try --channels 2 for stereo.",
                    device_name,
                    config.channels
                )
            })?
            .with_sample_rate(cpal::SampleRate(config.sample_rate));

        let (tx, rx) = broadcast::channel(100);

        let stream = device.build_input_stream(
            &supported_config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(data.to_vec());
            },
            |err| {
                tracing::error!("Audio capture error: {}", err);
            },
            None,
        )?;

        stream.play()?;

        Ok(Self {
            stream,
            receiver: rx,
            device_name,
            config,
        })
    }

    /// Get the device based on configuration
    fn get_device(config: &CaptureConfig) -> Result<Device> {
        match &config.device {
            Some(selector) => find_input_device(selector, config.host.as_deref()),
            None => {
                let host = get_host(config.host.as_deref())?;
                host.default_input_device()
                    .ok_or_else(|| anyhow::anyhow!("No default input device available"))
            }
        }
    }

    /// Receive the next audio buffer
    pub async fn recv(&mut self) -> Option<Vec<f32>> {
        self.receiver.recv().await.ok()
    }

    /// Get the device name being used
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Get the capture configuration
    pub fn config(&self) -> &CaptureConfig {
        &self.config
    }
}

/// Simple synchronous capture for one-shot recording
pub fn capture_sync(config: CaptureConfig, duration_ms: u32) -> Result<Vec<f32>> {
    let device = match &config.device {
        Some(selector) => find_input_device(selector, config.host.as_deref())?,
        None => {
            let host = get_host(config.host.as_deref())?;
            host.default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No default input device available"))?
        }
    };

    let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
    tracing::debug!("Sync capture from '{}' for {}ms", device_name, duration_ms);

    let supported_config = device
        .supported_input_configs()?
        .find(|c| c.channels() == config.channels)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Device '{}' doesn't support {} channel(s)",
                device_name,
                config.channels
            )
        })?
        .with_sample_rate(cpal::SampleRate(config.sample_rate));

    let (tx, rx) = mpsc::channel();
    let samples_needed = (config.sample_rate * duration_ms / 1000) as usize;
    let mut collected = Vec::with_capacity(samples_needed);

    let stream = device.build_input_stream(
        &supported_config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let _ = tx.send(data.to_vec());
        },
        |err| {
            tracing::error!("Audio capture error: {}", err);
        },
        None,
    )?;

    stream.play()?;

    while collected.len() < samples_needed {
        if let Ok(data) = rx.recv_timeout(std::time::Duration::from_secs(1)) {
            collected.extend(data);
        } else {
            break;
        }
    }

    collected.truncate(samples_needed);
    Ok(collected)
}

/// Capture audio using AudioInputArgs
pub fn capture_from_args(
    args: &super::args::AudioInputArgs,
    duration_ms: u32,
) -> Result<Vec<f32>> {
    let config = CaptureConfig::from_args(args);
    capture_sync(config, duration_ms)
}
