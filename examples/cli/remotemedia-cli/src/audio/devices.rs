//! Audio device enumeration

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

/// Audio device information
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

/// List available audio devices
pub fn list_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    // Input devices
    for device in host.input_devices()? {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        devices.push(AudioDevice {
            name,
            is_input: true,
            is_output: false,
        });
    }

    // Output devices
    for device in host.output_devices()? {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        devices.push(AudioDevice {
            name,
            is_input: false,
            is_output: true,
        });
    }

    Ok(devices)
}

/// Get the default input device name
pub fn default_input_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_input_device()
        .and_then(|d| d.name().ok())
}

/// Get the default output device name
pub fn default_output_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_output_device()
        .and_then(|d| d.name().ok())
}
