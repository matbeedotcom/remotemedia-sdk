//! RemoteMedia CLI library
//!
//! This library exposes the core utilities used by the RemoteMedia CLI,
//! enabling specialized executables that embed specific pipelines.
//!
//! # Modules
//!
//! - [`audio`] - Audio device management, capture, playback, and CLI arguments
//! - [`ffmpeg`] - FFmpeg-based media decoding
//! - [`io`] - Input/output handling (files, pipes, stdin/stdout)
//! - [`pipeline`] - Pipeline execution and session management
//! - [`output`] - Output formatting
//!
//! # Audio Device Example
//!
//! ```no_run
//! use remotemedia_cli::audio::{
//!     AudioDeviceArgs, AudioCapture, CaptureConfig,
//!     list_devices, print_device_list, DeviceSelector,
//! };
//!
//! # fn main() -> anyhow::Result<()> {
//! // List available devices
//! let devices = list_devices()?;
//! print_device_list(&devices);
//!
//! // Create capture config from CLI args or programmatically
//! let config = CaptureConfig {
//!     device: Some(DeviceSelector::Name("Microphone".into())),
//!     sample_rate: 16000,
//!     channels: 1,
//!     ..Default::default()
//! };
//!
//! let capture = AudioCapture::start(config)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Pipeline Execution Example
//!
//! ```no_run
//! use remotemedia_cli::{audio, io, pipeline, ffmpeg};
//! use remotemedia_core::data::RuntimeData;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Parse embedded pipeline YAML
//! # const PIPELINE_YAML: &str = "";
//! let manifest = pipeline::parse_manifest(PIPELINE_YAML)?;
//!
//! // Create runner and execute
//! let runner = pipeline::create_runner()?;
//! # let input = RuntimeData::Text(String::new());
//! let output = pipeline::execute_unary(&runner, std::sync::Arc::new(manifest), input).await?;
//! # Ok(())
//! # }
//! ```

pub mod audio;
pub mod ffmpeg;
pub mod io;
pub mod output;
pub mod pipeline;
pub mod pipeline_nodes;

// Re-export commonly used types from audio
pub use audio::{
    // CLI argument structs
    AudioDeviceArgs, AudioInputArgs, AudioOutputArgs,
    // Device selection
    DeviceSelector, SampleFormat,
    // Device enumeration
    AudioDevice, AudioConfig, AudioHostInfo, DeviceCapabilities,
    list_devices, list_devices_on_host, list_hosts,
    print_device_list, print_device_capabilities,
    default_input_device, default_output_device,
    find_input_device, find_output_device,
    get_device_capabilities, get_host,
    // Capture
    AudioCapture, CaptureConfig,
    capture_from_args, capture_sync,
    // Playback
    AudioPlayback, PlaybackConfig,
    play_from_args, play_sync,
    // WAV utilities
    is_wav, parse_wav, WavHeader,
};

// Re-export FFmpeg utilities
pub use ffmpeg::decode_audio_file;

// Re-export I/O utilities
pub use io::{
    detect_input_source, detect_output_sink,
    InputReader, InputSource,
    OutputSink, OutputWriter,
};

// Re-export output formatting
pub use output::{OutputFormat, Outputter};

// Re-export pipeline utilities
pub use pipeline::{
    create_runner, create_runner_with_cli_nodes, execute_unary,
    parse_manifest, StreamingSession,
};

// Re-export pipeline nodes for use in embedded pipelines
pub use pipeline_nodes::{
    // Node types
    MicInputNode, MicInputConfig,
    SpeakerOutputNode, SpeakerOutputConfig,
    SrtOutputNode, SrtOutputConfig,
    // Streaming registry and factories
    register_cli_nodes, create_cli_streaming_registry, get_cli_node_factories,
    MicInputNodeFactory, SpeakerOutputNodeFactory, SrtOutputNodeFactory,
};
