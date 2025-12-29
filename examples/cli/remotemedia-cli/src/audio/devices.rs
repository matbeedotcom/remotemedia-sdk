//! Audio device enumeration and selection
//!
//! Provides comprehensive device discovery and selection functionality,
//! supporting multiple audio hosts/backends and device selection by name or index.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host, SupportedStreamConfig};

use super::args::DeviceSelector;

/// Audio device information with extended metadata
#[derive(Debug, Clone)]
pub struct AudioDevice {
    /// Device name (as reported by the system)
    pub name: String,
    /// Zero-based index within device type (input or output)
    pub index: usize,
    /// Whether this device supports audio input (recording)
    pub is_input: bool,
    /// Whether this device supports audio output (playback)
    pub is_output: bool,
    /// Whether this is the system default for its type
    pub is_default: bool,
    /// Host/backend name (e.g., "ALSA", "CoreAudio", "WASAPI")
    pub host_name: String,
}

/// Device capabilities - supported configurations
#[derive(Debug, Clone)]
pub struct DeviceCapabilities {
    /// Device information
    pub device: AudioDevice,
    /// Supported sample rates
    pub sample_rates: Vec<u32>,
    /// Supported channel counts
    pub channels: Vec<u16>,
    /// Minimum buffer size (if known)
    pub min_buffer_size: Option<u32>,
    /// Maximum buffer size (if known)
    pub max_buffer_size: Option<u32>,
    /// Default configuration
    pub default_config: Option<AudioConfig>,
}

/// Audio configuration
#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: Option<u32>,
}

/// Audio host information
#[derive(Debug, Clone)]
pub struct AudioHostInfo {
    /// Host identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Whether this is the default host
    pub is_default: bool,
}

/// Get all available audio hosts/backends
pub fn list_hosts() -> Vec<AudioHostInfo> {
    let default_host_id = cpal::default_host().id();
    
    cpal::available_hosts()
        .into_iter()
        .map(|id| {
            let name = format!("{:?}", id);
            AudioHostInfo {
                id: format!("{:?}", id).to_lowercase(),
                name,
                is_default: id == default_host_id,
            }
        })
        .collect()
}

/// Get a specific audio host by name
pub fn get_host(name: Option<&str>) -> Result<Host> {
    match name {
        None => Ok(cpal::default_host()),
        Some(name) => {
            let name_lower = name.to_lowercase();
            
            for host_id in cpal::available_hosts() {
                let id_str = format!("{:?}", host_id).to_lowercase();
                if id_str.contains(&name_lower) {
                    return cpal::host_from_id(host_id)
                        .context(format!("Failed to initialize audio host: {}", name));
                }
            }
            
            anyhow::bail!(
                "Unknown audio host '{}'. Available hosts: {:?}",
                name,
                cpal::available_hosts()
            )
        }
    }
}

/// List available audio devices (both input and output)
pub fn list_devices() -> Result<Vec<AudioDevice>> {
    list_devices_on_host(None)
}

/// List available audio devices on a specific host
pub fn list_devices_on_host(host_name: Option<&str>) -> Result<Vec<AudioDevice>> {
    let host = get_host(host_name)?;
    let host_display = format!("{:?}", host.id());
    
    let default_input = host.default_input_device().and_then(|d| d.name().ok());
    let default_output = host.default_output_device().and_then(|d| d.name().ok());
    
    let mut devices = Vec::new();
    let mut input_idx = 0;
    let mut output_idx = 0;

    // Input devices
    for device in host.input_devices()? {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let is_default = default_input.as_ref() == Some(&name);
        
        devices.push(AudioDevice {
            name: name.clone(),
            index: input_idx,
            is_input: true,
            is_output: false,
            is_default,
            host_name: host_display.clone(),
        });
        input_idx += 1;
    }

    // Output devices
    for device in host.output_devices()? {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let is_default = default_output.as_ref() == Some(&name);
        
        // Check if we already have this device as input (it might be duplex)
        if let Some(existing) = devices.iter_mut().find(|d| d.name == name && d.is_input) {
            existing.is_output = true;
        } else {
            devices.push(AudioDevice {
                name,
                index: output_idx,
                is_input: false,
                is_output: true,
                is_default,
                host_name: host_display.clone(),
            });
        }
        output_idx += 1;
    }

    Ok(devices)
}

/// Get the default input device name
pub fn default_input_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_input_device().and_then(|d| d.name().ok())
}

/// Get the default output device name
pub fn default_output_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_output_device().and_then(|d| d.name().ok())
}

/// Find an input device by selector
pub fn find_input_device(selector: &DeviceSelector, host_name: Option<&str>) -> Result<Device> {
    let host = get_host(host_name)?;
    
    match selector {
        DeviceSelector::Default => {
            host.default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No default input device available"))
        }
        DeviceSelector::Name(name) => {
            for device in host.input_devices()? {
                if let Ok(device_name) = device.name() {
                    if device_name.eq_ignore_ascii_case(name) || device_name.contains(name) {
                        return Ok(device);
                    }
                }
            }
            anyhow::bail!("Input device '{}' not found", name)
        }
        DeviceSelector::Index(idx) => {
            host.input_devices()?
                .nth(*idx)
                .ok_or_else(|| anyhow::anyhow!("Input device index {} out of range", idx))
        }
        DeviceSelector::AlsaHw { card, device } => {
            // ALSA-style selector: look for device containing card/device number
            let pattern = format!("hw:{},{}", card, device);
            let alt_pattern = format!("card {}", card);
            
            for dev in host.input_devices()? {
                if let Ok(name) = dev.name() {
                    if name.contains(&pattern) || name.contains(&alt_pattern) {
                        return Ok(dev);
                    }
                }
            }
            anyhow::bail!("ALSA input device hw:{},{} not found", card, device)
        }
    }
}

/// Find an output device by selector
pub fn find_output_device(selector: &DeviceSelector, host_name: Option<&str>) -> Result<Device> {
    let host = get_host(host_name)?;
    
    match selector {
        DeviceSelector::Default => {
            host.default_output_device()
                .ok_or_else(|| anyhow::anyhow!("No default output device available"))
        }
        DeviceSelector::Name(name) => {
            for device in host.output_devices()? {
                if let Ok(device_name) = device.name() {
                    if device_name.eq_ignore_ascii_case(name) || device_name.contains(name) {
                        return Ok(device);
                    }
                }
            }
            anyhow::bail!("Output device '{}' not found", name)
        }
        DeviceSelector::Index(idx) => {
            host.output_devices()?
                .nth(*idx)
                .ok_or_else(|| anyhow::anyhow!("Output device index {} out of range", idx))
        }
        DeviceSelector::AlsaHw { card, device } => {
            let pattern = format!("hw:{},{}", card, device);
            let alt_pattern = format!("card {}", card);
            
            for dev in host.output_devices()? {
                if let Ok(name) = dev.name() {
                    if name.contains(&pattern) || name.contains(&alt_pattern) {
                        return Ok(dev);
                    }
                }
            }
            anyhow::bail!("ALSA output device hw:{},{} not found", card, device)
        }
    }
}

/// Get device capabilities
pub fn get_device_capabilities(device: &Device, is_input: bool) -> Result<DeviceCapabilities> {
    let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
    let host_name = format!("{:?}", cpal::default_host().id());
    
    let configs: Vec<SupportedStreamConfig> = if is_input {
        device
            .supported_input_configs()?
            .map(|c| c.with_max_sample_rate())
            .collect()
    } else {
        device
            .supported_output_configs()?
            .map(|c| c.with_max_sample_rate())
            .collect()
    };

    // Collect unique sample rates and channel counts
    let mut sample_rates: Vec<u32> = configs
        .iter()
        .map(|c| c.sample_rate().0)
        .collect();
    sample_rates.sort();
    sample_rates.dedup();

    let mut channels: Vec<u16> = configs.iter().map(|c| c.channels()).collect();
    channels.sort();
    channels.dedup();

    // Get default config
    let default_config = if is_input {
        device.default_input_config().ok()
    } else {
        device.default_output_config().ok()
    }
    .map(|c| AudioConfig {
        sample_rate: c.sample_rate().0,
        channels: c.channels(),
        buffer_size: None,
    });

    Ok(DeviceCapabilities {
        device: AudioDevice {
            name,
            index: 0, // Will be set by caller if needed
            is_input,
            is_output: !is_input,
            is_default: false,
            host_name,
        },
        sample_rates,
        channels,
        min_buffer_size: None, // cpal doesn't expose this directly
        max_buffer_size: None,
        default_config,
    })
}

/// Print device list in a formatted table
pub fn print_device_list(devices: &[AudioDevice]) {
    println!("\nAvailable Audio Devices:");
    println!("{}", "=".repeat(80));
    
    // Input devices
    let inputs: Vec<_> = devices.iter().filter(|d| d.is_input).collect();
    if !inputs.is_empty() {
        println!("\n📥 Input Devices (Recording):");
        println!("{:-<80}", "");
        for device in inputs {
            let default_marker = if device.is_default { " [DEFAULT]" } else { "" };
            let duplex_marker = if device.is_output { " (duplex)" } else { "" };
            println!(
                "  [{:2}] {}{}{}\n       Host: {}",
                device.index, device.name, default_marker, duplex_marker, device.host_name
            );
        }
    }

    // Output devices
    let outputs: Vec<_> = devices.iter().filter(|d| d.is_output && !d.is_input).collect();
    if !outputs.is_empty() {
        println!("\n🔊 Output Devices (Playback):");
        println!("{:-<80}", "");
        for device in outputs {
            let default_marker = if device.is_default { " [DEFAULT]" } else { "" };
            println!(
                "  [{:2}] {}{}\n       Host: {}",
                device.index, device.name, default_marker, device.host_name
            );
        }
    }

    // Duplex devices (both input and output)
    let duplex: Vec<_> = devices.iter().filter(|d| d.is_input && d.is_output).collect();
    if !duplex.is_empty() {
        println!("\n🔄 Duplex Devices (Input & Output):");
        println!("{:-<80}", "");
        for device in duplex {
            let default_marker = if device.is_default { " [DEFAULT]" } else { "" };
            println!(
                "  [{:2}] {}{}\n       Host: {}",
                device.index, device.name, default_marker, device.host_name
            );
        }
    }

    println!();
}

/// Print device capabilities
pub fn print_device_capabilities(caps: &DeviceCapabilities) {
    println!("\nDevice: {}", caps.device.name);
    println!("{}", "=".repeat(60));
    println!("  Host:         {}", caps.device.host_name);
    println!(
        "  Type:         {}",
        match (caps.device.is_input, caps.device.is_output) {
            (true, true) => "Duplex (Input & Output)",
            (true, false) => "Input Only",
            (false, true) => "Output Only",
            _ => "Unknown",
        }
    );
    
    if let Some(default) = &caps.default_config {
        println!("\n  Default Configuration:");
        println!("    Sample Rate: {} Hz", default.sample_rate);
        println!("    Channels:    {}", default.channels);
    }

    if !caps.sample_rates.is_empty() {
        println!("\n  Supported Sample Rates:");
        println!("    {:?}", caps.sample_rates);
    }

    if !caps.channels.is_empty() {
        println!("\n  Supported Channels:");
        println!("    {:?}", caps.channels);
    }

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_hosts() {
        let hosts = list_hosts();
        assert!(!hosts.is_empty(), "Should have at least one audio host");
        
        // At least one should be default
        assert!(hosts.iter().any(|h| h.is_default));
    }

    #[test]
    fn test_list_devices() {
        // This might fail in CI without audio hardware
        if let Ok(devices) = list_devices() {
            // If we have any devices, verify structure
            for device in &devices {
                assert!(!device.name.is_empty());
                assert!(device.is_input || device.is_output);
            }
        }
    }
}
