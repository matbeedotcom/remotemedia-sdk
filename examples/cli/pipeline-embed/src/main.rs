//! Generic pipeline runner with embedded YAML
//!
//! Build with:
//!   PIPELINE_YAML=path/to/pipeline.yaml cargo build -p pipeline-embed --release
//!
//! The resulting binary can be renamed and distributed as a standalone tool.
//!
//! # Build-time Configuration
//!
//! Default CLI values can be configured at build time via environment variables
//! or in the pipeline YAML metadata. This allows creating pipelines that default
//! to streaming mode, specific sample rates, etc.
//!
//! ## Environment Variables
//!
//! ```bash
//! PIPELINE_YAML=pipeline.yaml \
//! PIPELINE_STREAM=true \
//! PIPELINE_MIC=true \
//! PIPELINE_SPEAKER=true \
//! PIPELINE_SAMPLE_RATE=16000 \
//! cargo build -p pipeline-embed --release
//! ```
//!
//! ## Pipeline Metadata
//!
//! ```yaml
//! version: v1
//! metadata:
//!   name: my-pipeline
//!   cli_defaults:
//!     stream: true
//!     mic: true
//!     speaker: true
//!     sample_rate: 16000
//!     channels: 1
//! ```
//!
//! # Examples
//!
//! ```bash
//! # Build a transcription tool
//! PIPELINE_YAML=pipelines/transcribe-srt.yaml cargo build -p pipeline-embed --release
//! cp target/release/pipeline-runner ./transcribe
//!
//! # Use it with file input
//! ./transcribe -i input.mp4 -o output.srt
//!
//! # Use with microphone (live transcription)
//! ./transcribe --mic --speaker -r 16000
//!
//! # List available audio devices
//! ./transcribe --list-devices
//!
//! # Use specific audio devices
//! ./transcribe --mic -D "USB Microphone" --speaker -O "DAC"
//! ```

// Include the generated pipeline YAML and defaults
include!(concat!(env!("OUT_DIR"), "/embedded_pipeline.rs"));

use anyhow::{Context, Result};

/// Generate the about text from embedded pipeline metadata
const fn get_about_text() -> &'static str {
    if PIPELINE_DESCRIPTION.is_empty() {
        PIPELINE_NAME
    } else {
        // Can't concatenate at const time, so we use the description if available
        PIPELINE_DESCRIPTION
    }
}
use clap::Parser;
use remotemedia_cli::{
    audio::{
        AudioCapture, AudioDeviceArgs, AudioPlayback,
        CaptureConfig, PlaybackConfig, DeviceSelector,
        is_wav, list_devices, parse_wav, print_device_list,
    },
    ffmpeg,
    io::{detect_input_source, detect_output_sink, InputReader, InputSource, OutputWriter},
    pipeline,
};
use remotemedia_runtime_core::data::RuntimeData;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Run an embedded pipeline with comprehensive audio device support
///
/// Default values may be configured at build time via environment variables
/// or pipeline metadata. Run with --show-defaults to see embedded defaults.
#[derive(Parser)]
#[command(name = BINARY_NAME)]
#[command(author, version)]
#[command(about = get_about_text())]
struct Args {
    /// Input source: file path, named pipe, or `-` for stdin
    #[arg(short = 'i', long, help = "Input file, pipe, or '-' for stdin")]
    pub input: Option<String>,

    /// Output destination: file path, named pipe, or `-` for stdout
    #[arg(short = 'o', long, default_value = "-", help = "Output file, pipe, or '-' for stdout")]
    pub output: String,

    // Audio device configuration (ffmpeg-style)
    #[command(flatten)]
    pub audio: AudioDeviceArgs,

    /// Audio chunk size in samples for streaming
    #[arg(long, help = "Audio chunk size in samples")]
    pub chunk_size: Option<usize>,

    /// Execution timeout
    #[arg(long, value_parser = parse_duration, help = "Execution timeout (e.g., 10s, 5m)")]
    pub timeout: Option<Duration>,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress non-error output
    #[arg(short, long)]
    pub quiet: bool,

    /// Show the embedded pipeline YAML and exit
    #[arg(long, help = "Display embedded pipeline manifest")]
    pub show_pipeline: bool,

    /// Show the embedded default values and exit
    #[arg(long, help = "Display build-time configured defaults")]
    pub show_defaults: bool,

    /// Enable streaming mode (continuous input/output)
    #[arg(long, help = "Run in streaming mode for real-time processing")]
    pub stream: bool,

    /// Disable streaming mode (override embedded default)
    #[arg(long, help = "Disable streaming mode even if embedded default is enabled")]
    pub no_stream: bool,
}

/// Effective configuration after applying embedded defaults
struct EffectiveConfig {
    stream: bool,
    mic: bool,
    speaker: bool,
    sample_rate: u32,
    channels: u16,
    chunk_size: usize,
    timeout: Duration,
    input_device: Option<String>,
    output_device: Option<String>,
    audio_host: Option<String>,
    buffer_ms: u32,
}

impl EffectiveConfig {
    /// Build effective configuration from CLI args + embedded defaults
    fn from_args(args: &Args) -> Self {
        // Start with hardcoded fallback defaults
        let mut config = Self {
            stream: false,
            mic: false,
            speaker: false,
            sample_rate: 48000,
            channels: 1,
            chunk_size: 4000,
            timeout: Duration::from_secs(600),
            input_device: None,
            output_device: None,
            audio_host: None,
            buffer_ms: 20,
        };

        // Apply embedded defaults (from build.rs)
        if let Some(v) = PipelineDefaults::STREAM { config.stream = v; }
        if let Some(v) = PipelineDefaults::MIC { config.mic = v; }
        if let Some(v) = PipelineDefaults::SPEAKER { config.speaker = v; }
        if let Some(v) = PipelineDefaults::SAMPLE_RATE { config.sample_rate = v; }
        if let Some(v) = PipelineDefaults::CHANNELS { config.channels = v; }
        if let Some(v) = PipelineDefaults::CHUNK_SIZE { config.chunk_size = v; }
        if let Some(v) = PipelineDefaults::TIMEOUT_SECS { config.timeout = Duration::from_secs(v); }
        if let Some(v) = PipelineDefaults::INPUT_DEVICE { config.input_device = Some(v.to_string()); }
        if let Some(v) = PipelineDefaults::OUTPUT_DEVICE { config.output_device = Some(v.to_string()); }
        if let Some(v) = PipelineDefaults::AUDIO_HOST { config.audio_host = Some(v.to_string()); }
        if let Some(v) = PipelineDefaults::BUFFER_MS { config.buffer_ms = v; }

        // Apply CLI overrides
        if args.stream { config.stream = true; }
        if args.no_stream { config.stream = false; }
        if args.audio.input.mic { config.mic = true; }
        if args.audio.output.speaker { config.speaker = true; }
        
        // CLI audio settings override if provided (check if different from clap defaults)
        // These use the actual CLI values when provided
        if args.audio.input.sample_rate != 48000 {
            config.sample_rate = args.audio.input.sample_rate;
        }
        if args.audio.input.channels != 1 {
            config.channels = args.audio.input.channels;
        }
        if let Some(cs) = args.chunk_size {
            config.chunk_size = cs;
        }
        if let Some(t) = args.timeout {
            config.timeout = t;
        }
        if args.audio.input.buffer_ms != 20 {
            config.buffer_ms = args.audio.input.buffer_ms;
        }
        
        // Device selection from CLI always overrides
        if args.audio.input.input_device.is_some() {
            config.input_device = args.audio.input.input_device.clone();
        }
        if args.audio.output.output_device.is_some() {
            config.output_device = args.audio.output.output_device.clone();
        }
        if args.audio.input.audio_host.is_some() {
            config.audio_host = args.audio.input.audio_host.clone();
        }

        config
    }

    /// Check if audio input is enabled
    fn audio_input_enabled(&self) -> bool {
        self.mic
    }

    /// Check if audio output is enabled
    fn audio_output_enabled(&self) -> bool {
        self.speaker
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.ends_with("ms") {
        s[..s.len() - 2].parse().map(Duration::from_millis).map_err(|_| "Invalid ms".into())
    } else if s.ends_with('s') {
        s[..s.len() - 1].parse().map(Duration::from_secs).map_err(|_| "Invalid s".into())
    } else if s.ends_with('m') {
        s[..s.len() - 1].parse::<u64>().map(|m| Duration::from_secs(m * 60)).map_err(|_| "Invalid m".into())
    } else {
        s.parse().map(Duration::from_secs).map_err(|_| "Invalid duration".into())
    }
}

fn show_defaults() {
    println!("Embedded Pipeline Defaults:");
    println!("===========================");
    println!();
    
    println!("Mode:");
    println!("  stream:       {}", PipelineDefaults::STREAM.map(|v| v.to_string()).unwrap_or_else(|| "false (fallback)".into()));
    println!();
    
    println!("Audio Input:");
    println!("  mic:          {}", PipelineDefaults::MIC.map(|v| v.to_string()).unwrap_or_else(|| "false (fallback)".into()));
    println!("  sample_rate:  {}", PipelineDefaults::SAMPLE_RATE.map(|v| format!("{} Hz", v)).unwrap_or_else(|| "48000 Hz (fallback)".into()));
    println!("  channels:     {}", PipelineDefaults::CHANNELS.map(|v| v.to_string()).unwrap_or_else(|| "1 (fallback)".into()));
    println!("  buffer_ms:    {}", PipelineDefaults::BUFFER_MS.map(|v| format!("{} ms", v)).unwrap_or_else(|| "20 ms (fallback)".into()));
    println!("  input_device: {}", PipelineDefaults::INPUT_DEVICE.unwrap_or("(system default)"));
    println!("  audio_host:   {}", PipelineDefaults::AUDIO_HOST.unwrap_or("(system default)"));
    println!();
    
    println!("Audio Output:");
    println!("  speaker:       {}", PipelineDefaults::SPEAKER.map(|v| v.to_string()).unwrap_or_else(|| "false (fallback)".into()));
    println!("  output_device: {}", PipelineDefaults::OUTPUT_DEVICE.unwrap_or("(system default)"));
    println!();
    
    println!("Processing:");
    println!("  chunk_size:   {}", PipelineDefaults::CHUNK_SIZE.map(|v| format!("{} samples", v)).unwrap_or_else(|| "4000 samples (fallback)".into()));
    println!("  timeout:      {}", PipelineDefaults::TIMEOUT_SECS.map(|v| format!("{} seconds", v)).unwrap_or_else(|| "600 seconds (fallback)".into()));
    println!();
    
    println!("Note: CLI arguments override these defaults.");
    println!("Use --no-stream to disable streaming when it's the default.");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle --list-devices early
    if args.audio.list_devices {
        let devices = list_devices().context("Failed to enumerate audio devices")?;
        print_device_list(&devices);
        return Ok(());
    }

    // Show pipeline and exit if requested
    if args.show_pipeline {
        println!("{}", PIPELINE_YAML);
        return Ok(());
    }

    // Show embedded defaults and exit if requested
    if args.show_defaults {
        show_defaults();
        return Ok(());
    }

    // Build effective configuration
    let config = EffectiveConfig::from_args(&args);

    // Setup logging to stderr
    let filter = if args.quiet { "error" } else {
        match args.verbose { 0 => "warn", 1 => "info", 2 => "debug", _ => "trace" }
    };
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .init();

    // Parse the embedded manifest
    let manifest = Arc::new(pipeline::parse_manifest(PIPELINE_YAML)?);
    tracing::info!("Running pipeline: {}", manifest.metadata.name);

    // Log effective configuration
    if config.stream || config.audio_input_enabled() {
        tracing::info!(
            "Streaming mode: sample_rate={}Hz, channels={}, chunk_size={}",
            config.sample_rate, config.channels, config.chunk_size
        );
    }

    // Determine mode: streaming vs unary
    if config.stream || config.audio_input_enabled() {
        run_streaming_mode(&args, &config, manifest).await
    } else {
        run_unary_mode(&args, &config, manifest).await
    }
}

/// Run pipeline in unary mode (single input -> single output)
async fn run_unary_mode(
    args: &Args,
    config: &EffectiveConfig,
    manifest: Arc<remotemedia_runtime_core::manifest::Manifest>,
) -> Result<()> {
    // Read input if provided
    let input_data = if let Some(input_path) = &args.input {
        let source = detect_input_source(input_path)
            .map_err(|e| anyhow::anyhow!("Invalid input '{}': {}", input_path, e))?;
        
        tracing::info!("Reading from: {:?}", source);
        
        let mut reader = InputReader::open(source.clone()).await
            .map_err(|e| anyhow::anyhow!("Failed to open input: {}", e))?;
        
        let data = reader.read_to_end().await
            .map_err(|e| anyhow::anyhow!("Failed to read input: {}", e))?;
        
        tracing::debug!("Read {} bytes", data.len());

        // Auto-detect format and decode
        if is_wav(&data) {
            tracing::info!("Detected WAV audio");
            let (samples, sample_rate, channels) = parse_wav(&data).context("Failed to parse WAV")?;
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels: channels as u32,
                stream_id: None,
            }
        } else if let InputSource::File(path) = &source {
            // Try FFmpeg for other file formats
            tracing::info!("Decoding with FFmpeg");
            let (samples, sample_rate, channels) = ffmpeg::decode_audio_file(path)?;
            RuntimeData::Audio { samples, sample_rate, channels, stream_id: None }
        } else if data.starts_with(b"{") || data.starts_with(b"[") {
            // JSON input
            tracing::info!("Detected JSON input");
            let json: serde_json::Value = serde_json::from_slice(&data)?;
            RuntimeData::Json(json)
        } else if let Ok(text) = String::from_utf8(data.clone()) {
            // Text input
            tracing::info!("Detected text input");
            RuntimeData::Text(text)
        } else {
            // Binary input
            tracing::info!("Using binary input");
            RuntimeData::Binary(data)
        }
    } else {
        // No input - use empty JSON
        RuntimeData::Json(serde_json::json!({}))
    };

    // Execute pipeline
    let runner = pipeline::create_runner()?;
    
    tracing::info!("Executing pipeline (timeout: {:?})", config.timeout);
    
    let output = tokio::time::timeout(config.timeout, 
        pipeline::execute_unary(&runner, manifest, input_data)
    ).await.context("Pipeline execution timed out")??;

    // Handle audio output to speaker if requested
    if config.audio_output_enabled() {
        if let RuntimeData::Audio { samples, sample_rate, channels, .. } = &output {
            tracing::info!("Playing {} samples to speaker", samples.len());
            
            let playback_config = PlaybackConfig {
                device: config.output_device.as_ref().map(|s| DeviceSelector::parse(s)),
                host: config.audio_host.clone(),
                sample_rate: *sample_rate,
                channels: *channels as u16,
            };
            
            let playback = AudioPlayback::start(playback_config)?;
            playback.queue(samples);
            
            // Wait for playback to complete
            while !playback.is_complete() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    // Write output to file/stdout
    let output_bytes = match &output {
        RuntimeData::Text(t) => t.as_bytes().to_vec(),
        RuntimeData::Json(j) => serde_json::to_vec_pretty(j)?,
        RuntimeData::Binary(b) => b.clone(),
        RuntimeData::Audio { samples, .. } => {
            // Output raw f32 PCM
            samples.iter().flat_map(|s| s.to_le_bytes()).collect()
        }
        other => {
            // Serialize as JSON
            serde_json::to_vec_pretty(&serde_json::to_value(other)?)?
        }
    };

    let sink = detect_output_sink(&args.output)
        .map_err(|e| anyhow::anyhow!("Invalid output '{}': {}", args.output, e))?;
    
    let mut writer = OutputWriter::open(sink).await
        .map_err(|e| anyhow::anyhow!("Failed to open output: {}", e))?;
    
    writer.write_all(&output_bytes).await
        .map_err(|e| anyhow::anyhow!("Failed to write: {}", e))?;
    writer.flush().await
        .map_err(|e| anyhow::anyhow!("Failed to flush: {}", e))?;

    if args.output != "-" {
        tracing::info!("Wrote output to: {}", args.output);
    }

    Ok(())
}

/// Run pipeline in streaming mode (continuous audio capture/playback)
async fn run_streaming_mode(
    args: &Args,
    config: &EffectiveConfig,
    manifest: Arc<remotemedia_runtime_core::manifest::Manifest>,
) -> Result<()> {
    let runner = pipeline::create_runner()?;
    let mut session = pipeline::StreamingSession::new(&runner, manifest.clone())
        .await
        .context("Failed to create streaming session")?;

    // Start audio capture if mic is enabled
    let mut capture = if config.audio_input_enabled() {
        let capture_config = CaptureConfig {
            device: config.input_device.as_ref().map(|s| DeviceSelector::parse(s)),
            host: config.audio_host.clone(),
            sample_rate: config.sample_rate,
            channels: config.channels,
            buffer_size_ms: config.buffer_ms,
        };
        
        let cap = AudioCapture::start(capture_config)?;
        tracing::info!("Started audio capture from '{}'", cap.device_name());
        Some(cap)
    } else {
        None
    };

    // Start audio playback if speaker is enabled
    let playback = if config.audio_output_enabled() {
        let playback_config = PlaybackConfig {
            device: config.output_device.as_ref().map(|s| DeviceSelector::parse(s)),
            host: config.audio_host.clone(),
            sample_rate: config.sample_rate,
            channels: config.channels,
        };
        
        let pb = AudioPlayback::start(playback_config)?;
        tracing::info!("Started audio playback to '{}'", pb.device_name());
        Some(pb)
    } else {
        None
    };

    // Open output writer if not going to speaker
    let mut output_writer = if args.output != "-" || !config.audio_output_enabled() {
        let sink = detect_output_sink(&args.output)
            .map_err(|e| anyhow::anyhow!("Invalid output '{}': {}", args.output, e))?;
        Some(OutputWriter::open(sink).await?)
    } else {
        None
    };

    // Setup signal handler for graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = shutdown_tx.send(());
    });

    tracing::info!("Streaming started (press Ctrl+C to stop)");

    let sample_rate = config.sample_rate;
    let channels = config.channels as u32;
    let chunk_size = config.chunk_size;

    // Main streaming loop
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                tracing::info!("Received shutdown signal");
                break;
            }
            
            // Receive audio from microphone
            audio_result = async {
                if let Some(ref mut cap) = capture {
                    cap.recv().await
                } else {
                    // If no mic, just sleep forever
                    std::future::pending::<Option<Vec<f32>>>().await
                }
            } => {
                if let Some(samples) = audio_result {
                    // Send audio chunks to pipeline
                    for chunk in samples.chunks(chunk_size) {
                        let audio = RuntimeData::Audio {
                            samples: chunk.to_vec(),
                            sample_rate,
                            channels,
                            stream_id: None,
                        };
                        
                        if let Err(e) = session.send(audio).await {
                            tracing::error!("Failed to send audio to pipeline: {}", e);
                            break;
                        }
                    }
                    
                    // Process outputs
                    while let Ok(Some(output)) = session.recv().await {
                        handle_streaming_output(&output, &playback, &mut output_writer).await?;
                    }
                }
            }
        }
    }

    // Drain remaining outputs
    while let Ok(Some(output)) = session.recv().await {
        handle_streaming_output(&output, &playback, &mut output_writer).await?;
    }

    // Cleanup
    if let Err(e) = session.close().await {
        tracing::warn!("Error closing session: {}", e);
    }

    tracing::info!("Streaming stopped");
    Ok(())
}

/// Handle a single streaming output
async fn handle_streaming_output(
    output: &RuntimeData,
    playback: &Option<AudioPlayback>,
    writer: &mut Option<OutputWriter>,
) -> Result<()> {
    match output {
        RuntimeData::Audio { samples, .. } => {
            // Send to speaker if enabled
            if let Some(pb) = playback {
                pb.queue(samples);
            }
        }
        RuntimeData::Text(text) => {
            // Write text to output
            if let Some(w) = writer {
                let mut bytes = text.as_bytes().to_vec();
                if !bytes.ends_with(&[b'\n']) {
                    bytes.push(b'\n');
                }
                w.write_all(&bytes).await?;
                w.flush().await?;
            } else {
                print!("{}", text);
            }
        }
        RuntimeData::Json(json) => {
            // Write JSON to output
            if let Some(w) = writer {
                let mut bytes = serde_json::to_vec(json)?;
                bytes.push(b'\n');
                w.write_all(&bytes).await?;
                w.flush().await?;
            } else {
                println!("{}", serde_json::to_string(json)?);
            }
        }
        _ => {
            // Log other output types
            tracing::debug!("Pipeline output: {:?}", output);
        }
    }
    Ok(())
}
