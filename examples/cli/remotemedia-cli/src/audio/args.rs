//! Audio device CLI arguments - ffmpeg-inspired device selection
//!
//! # Device Selection Syntax
//!
//! Devices can be specified in several ways:
//!
//! - By name: `--input-device "Built-in Microphone"`
//! - By index: `--input-device 0` (first input device)
//! - Using default: omit the flag to use system default
//!
//! # Examples
//!
//! ```bash
//! # List available audio devices
//! remotemedia devices --list
//!
//! # Use specific input device by name
//! remotemedia stream pipeline.yaml --mic --input-device "USB Microphone"
//!
//! # Use specific output device by index
//! remotemedia stream pipeline.yaml --speaker --output-device 1
//!
//! # Full audio configuration (ffmpeg-style)
//! remotemedia stream pipeline.yaml \
//!     --mic --input-device hw:0 \
//!     --sample-rate 48000 \
//!     --channels 2 \
//!     --speaker --output-device "DAC"
//! ```

use clap::Args;
use serde::{Deserialize, Serialize};

/// Audio input device configuration (ffmpeg-inspired)
#[derive(Args, Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioInputArgs {
    /// Enable microphone/audio input capture
    #[arg(long, help = "Capture audio from input device")]
    pub mic: bool,

    /// Input audio device (name, index, or 'default')
    /// 
    /// Examples:
    ///   --input-device "Built-in Microphone"
    ///   --input-device 0
    ///   --input-device default
    ///   --input-device hw:0  (ALSA-style, Linux)
    #[arg(
        short = 'D',
        long = "input-device",
        alias = "audio-input",
        alias = "adev",
        value_name = "DEVICE",
        help = "Input device name, index, or 'default'"
    )]
    pub input_device: Option<String>,

    /// Audio host/backend to use (platform-specific)
    /// 
    /// Linux: alsa, pulse, jack
    /// macOS: coreaudio
    /// Windows: wasapi, asio
    #[arg(
        long = "audio-host",
        alias = "audio-backend",
        value_name = "HOST",
        help = "Audio host/backend (alsa, pulse, coreaudio, wasapi)"
    )]
    pub audio_host: Option<String>,

    /// Sample rate in Hz
    #[arg(
        short = 'r',
        long = "sample-rate",
        alias = "ar",
        value_name = "RATE",
        default_value = "48000",
        help = "Audio sample rate in Hz"
    )]
    pub sample_rate: u32,

    /// Number of audio channels
    #[arg(
        short = 'c',
        long = "channels",
        alias = "ac",
        value_name = "CHANNELS",
        default_value = "1",
        help = "Number of audio channels (1=mono, 2=stereo)"
    )]
    pub channels: u16,

    /// Audio buffer size in milliseconds
    #[arg(
        long = "buffer-ms",
        alias = "buffer-size",
        value_name = "MS",
        default_value = "20",
        help = "Audio buffer size in milliseconds"
    )]
    pub buffer_ms: u32,

    /// Audio sample format
    #[arg(
        long = "sample-format",
        alias = "format",
        value_name = "FORMAT",
        default_value = "f32",
        help = "Sample format: f32, i16, i32"
    )]
    pub sample_format: SampleFormat,
}

/// Audio output device configuration
#[derive(Args, Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioOutputArgs {
    /// Enable speaker/audio output playback
    #[arg(long, help = "Play audio through output device")]
    pub speaker: bool,

    /// Output audio device (name, index, or 'default')
    #[arg(
        short = 'O',
        long = "output-device",
        alias = "audio-output",
        alias = "odev",
        value_name = "DEVICE",
        help = "Output device name, index, or 'default'"
    )]
    pub output_device: Option<String>,

    /// Output sample rate (defaults to input sample rate if not specified)
    #[arg(
        long = "output-sample-rate",
        alias = "oar",
        value_name = "RATE",
        help = "Output sample rate in Hz (default: same as input)"
    )]
    pub output_sample_rate: Option<u32>,

    /// Output channels (defaults to input channels if not specified)
    #[arg(
        long = "output-channels",
        alias = "oac",
        value_name = "CHANNELS",
        help = "Output channels (default: same as input)"
    )]
    pub output_channels: Option<u16>,
}

/// Combined audio device configuration
#[derive(Args, Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioDeviceArgs {
    #[command(flatten)]
    pub input: AudioInputArgs,

    #[command(flatten)]
    pub output: AudioOutputArgs,

    /// List available audio devices and exit
    #[arg(
        long = "list-devices",
        alias = "list-audio",
        alias = "sources",
        help = "List available audio input/output devices"
    )]
    pub list_devices: bool,

    /// Show detailed device capabilities
    #[arg(
        long = "show-device-info",
        alias = "device-info",
        help = "Show detailed capabilities for selected devices"
    )]
    pub show_device_info: bool,
}

/// Audio sample format
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleFormat {
    /// 32-bit floating point (-1.0 to 1.0)
    #[default]
    F32,
    /// 16-bit signed integer
    I16,
    /// 32-bit signed integer
    I32,
}

impl std::fmt::Display for SampleFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SampleFormat::F32 => write!(f, "f32"),
            SampleFormat::I16 => write!(f, "i16"),
            SampleFormat::I32 => write!(f, "i32"),
        }
    }
}

impl std::str::FromStr for SampleFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "f32" | "float32" | "float" => Ok(SampleFormat::F32),
            "i16" | "s16" | "int16" | "pcm_s16le" => Ok(SampleFormat::I16),
            "i32" | "s32" | "int32" | "pcm_s32le" => Ok(SampleFormat::I32),
            _ => Err(format!(
                "Unknown sample format '{}'. Valid formats: f32, i16, i32",
                s
            )),
        }
    }
}

impl AudioInputArgs {
    /// Check if audio input is requested
    pub fn is_enabled(&self) -> bool {
        self.mic || self.input_device.is_some()
    }

    /// Get effective sample rate
    pub fn effective_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get effective channel count
    pub fn effective_channels(&self) -> u16 {
        self.channels
    }
}

impl AudioOutputArgs {
    /// Check if audio output is requested
    pub fn is_enabled(&self) -> bool {
        self.speaker || self.output_device.is_some()
    }

    /// Get effective output sample rate (falls back to given input rate)
    pub fn effective_sample_rate(&self, input_rate: u32) -> u32 {
        self.output_sample_rate.unwrap_or(input_rate)
    }

    /// Get effective output channel count (falls back to given input channels)
    pub fn effective_channels(&self, input_channels: u16) -> u16 {
        self.output_channels.unwrap_or(input_channels)
    }
}

impl AudioDeviceArgs {
    /// Check if any audio functionality is requested
    pub fn has_audio(&self) -> bool {
        self.input.is_enabled() || self.output.is_enabled()
    }

    /// Check if device listing was requested
    pub fn should_list_devices(&self) -> bool {
        self.list_devices
    }
}

/// Device selector - parsed from CLI device argument
#[derive(Debug, Clone)]
pub enum DeviceSelector {
    /// Use system default device
    Default,
    /// Select by exact name
    Name(String),
    /// Select by index (0-based)
    Index(usize),
    /// ALSA-style hardware specifier (hw:X,Y)
    AlsaHw { card: u32, device: u32 },
}

impl DeviceSelector {
    /// Parse a device selector from a string
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        
        // Handle explicit default
        if s.eq_ignore_ascii_case("default") || s.is_empty() {
            return DeviceSelector::Default;
        }

        // Try parsing as index
        if let Ok(idx) = s.parse::<usize>() {
            return DeviceSelector::Index(idx);
        }

        // Try parsing ALSA-style hw:X,Y or hw:X
        if s.starts_with("hw:") {
            let parts: Vec<&str> = s[3..].split(',').collect();
            if let Some(card) = parts.first().and_then(|p| p.parse::<u32>().ok()) {
                let device = parts.get(1).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
                return DeviceSelector::AlsaHw { card, device };
            }
        }

        // Treat as device name
        DeviceSelector::Name(s.to_string())
    }
}

impl std::fmt::Display for DeviceSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceSelector::Default => write!(f, "default"),
            DeviceSelector::Name(name) => write!(f, "{}", name),
            DeviceSelector::Index(idx) => write!(f, "{}", idx),
            DeviceSelector::AlsaHw { card, device } => write!(f, "hw:{},{}", card, device),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_selector_parse() {
        assert!(matches!(DeviceSelector::parse("default"), DeviceSelector::Default));
        assert!(matches!(DeviceSelector::parse(""), DeviceSelector::Default));
        assert!(matches!(DeviceSelector::parse("0"), DeviceSelector::Index(0)));
        assert!(matches!(DeviceSelector::parse("5"), DeviceSelector::Index(5)));
        assert!(matches!(DeviceSelector::parse("hw:0"), DeviceSelector::AlsaHw { card: 0, device: 0 }));
        assert!(matches!(DeviceSelector::parse("hw:1,2"), DeviceSelector::AlsaHw { card: 1, device: 2 }));
        
        if let DeviceSelector::Name(name) = DeviceSelector::parse("USB Microphone") {
            assert_eq!(name, "USB Microphone");
        } else {
            panic!("Expected Name variant");
        }
    }

    #[test]
    fn test_sample_format_parse() {
        assert_eq!("f32".parse::<SampleFormat>().unwrap(), SampleFormat::F32);
        assert_eq!("i16".parse::<SampleFormat>().unwrap(), SampleFormat::I16);
        assert_eq!("pcm_s16le".parse::<SampleFormat>().unwrap(), SampleFormat::I16);
        assert!("invalid".parse::<SampleFormat>().is_err());
    }
}
