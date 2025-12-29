//! Audio I/O using cpal with device selection and configuration
//!
//! This module provides comprehensive audio device management including:
//! - Device enumeration and selection (by name, index, or hardware ID)
//! - Microphone capture with streaming or one-shot modes
//! - Speaker playback with buffer management
//! - WAV file parsing
//! - CLI argument structs for ffmpeg-style device configuration
//!
//! # Device Selection
//!
//! Devices can be selected by:
//! - Name: `"Built-in Microphone"` or partial match `"USB"`
//! - Index: `0`, `1`, etc. (zero-based)
//! - ALSA hardware ID: `"hw:0,0"` (Linux only)
//! - Default: `"default"` or omit
//!
//! # Examples
//!
//! ```no_run
//! use remotemedia_cli::audio::{
//!     AudioCapture, AudioPlayback,
//!     CaptureConfig, PlaybackConfig,
//!     DeviceSelector, list_devices, print_device_list,
//! };
//!
//! # fn main() -> anyhow::Result<()> {
//! // List all available devices
//! let devices = list_devices()?;
//! print_device_list(&devices);
//!
//! // Capture from USB microphone
//! let config = CaptureConfig {
//!     device: Some(DeviceSelector::Name("USB".into())),
//!     sample_rate: 16000,
//!     channels: 1,
//!     ..Default::default()
//! };
//! let mut capture = AudioCapture::start(config)?;
//!
//! // Playback on default device
//! let playback = AudioPlayback::start(PlaybackConfig::default())?;
//! # Ok(())
//! # }
//! ```

pub mod args;
pub mod devices;
pub mod mic;
pub mod speaker;
pub mod wav;

// Re-export CLI argument structs
pub use args::{
    AudioDeviceArgs, AudioInputArgs, AudioOutputArgs,
    DeviceSelector, SampleFormat,
};

// Re-export device management
pub use devices::{
    AudioDevice, AudioConfig, AudioHostInfo, DeviceCapabilities,
    default_input_device, default_output_device,
    find_input_device, find_output_device,
    get_device_capabilities, get_host,
    list_devices, list_devices_on_host, list_hosts,
    print_device_capabilities, print_device_list,
};

// Re-export capture functionality
pub use mic::{
    AudioCapture, CaptureConfig,
    capture_from_args, capture_sync,
};

// Re-export playback functionality
pub use speaker::{
    AudioPlayback, PlaybackConfig,
    play_from_args, play_sync,
};

// Re-export WAV utilities
pub use wav::{
    WavHeader,
    is_wav, parse_wav,
    read_wav_header, read_wav_samples,
};
