//! transcribe-srt - Self-contained audio-to-SRT subtitle transcription
//!
//! A specialized CLI tool that transcribes audio files to SRT subtitle format
//! using Whisper. The pipeline is embedded, requiring no external configuration.
//!
//! # Usage
//!
//! ```bash
//! # Basic usage
//! transcribe-srt input.wav -o subtitles.srt
//!
//! # Output to stdout
//! transcribe-srt input.wav -o -
//!
//! # With verbose logging
//! transcribe-srt -v input.wav -o subtitles.srt
//!
//! # Custom model (tiny for speed, large-v3-turbo for quality)
//! transcribe-srt --model quantized_tiny input.wav -o subtitles.srt
//! ```
//!
//! # Supported Formats
//!
//! - Input: WAV (recommended), raw PCM
//! - Output: SRT subtitle format

use anyhow::{Context, Result};
use clap::Parser;
use remotemedia_cli::{
    audio::{is_wav, parse_wav},
    io::{detect_input_source, detect_output_sink, InputReader, OutputWriter},
    pipeline,
};
use remotemedia_runtime_core::data::RuntimeData;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Embedded pipeline manifest - no external file needed
const PIPELINE_YAML: &str = r#"
version: v1
metadata:
  name: transcribe-srt
  description: Audio transcription to SRT subtitle format using Whisper

nodes:
  # Whisper speech-to-text
  # NOTE: Must use QUANTIZED model for word-level timestamps (DTW alignment)
  # Options with word timestamps: quantized_tiny, quantized_tiny_en, large-v3-turbo
  # Options without word timestamps: tiny, base, small, medium, large
  - id: whisper
    node_type: RustWhisperNode
    params:
      model_source: ${MODEL}
      language: ${LANGUAGE}
      n_threads: ${THREADS}
      accumulate_chunks: false

  # Convert Whisper JSON output to SRT format
  - id: srt
    node_type: SrtOutput
    params:
      include_numbers: true
      max_line_length: 42

connections:
  - from: whisper
    to: srt
"#;

/// Transcribe audio files to SRT subtitle format
#[derive(Parser)]
#[command(name = "transcribe-srt")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input audio file (WAV format recommended)
    pub input: PathBuf,

    /// Output SRT file path, or `-` for stdout
    #[arg(short = 'o', long, default_value = "-")]
    pub output: String,

    /// Whisper model to use
    #[arg(long, default_value = "large-v3-turbo", value_parser = parse_model)]
    pub model: String,

    /// Language code (e.g., en, es, fr, de, zh)
    #[arg(long, default_value = "en")]
    pub language: String,

    /// Number of threads for Whisper inference
    #[arg(long, default_value = "4")]
    pub threads: u32,

    /// Execution timeout
    #[arg(long, default_value = "600s", value_parser = parse_duration)]
    pub timeout: Duration,

    /// Increase verbosity (can be used multiple times: -v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress non-error output
    #[arg(short, long)]
    pub quiet: bool,
}

/// Supported Whisper models
const SUPPORTED_MODELS: &[&str] = &[
    "tiny",
    "base",
    "small",
    "medium",
    "large",
    "large-v3-turbo",
    "quantized_tiny",
    "quantized_tiny_en",
];

fn parse_model(s: &str) -> Result<String, String> {
    if SUPPORTED_MODELS.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Unknown model '{}'. Supported models: {}",
            s,
            SUPPORTED_MODELS.join(", ")
        ))
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.ends_with("ms") {
        let ms: u64 = s[..s.len() - 2]
            .parse()
            .map_err(|_| "Invalid milliseconds")?;
        Ok(Duration::from_millis(ms))
    } else if s.ends_with('s') {
        let secs: u64 = s[..s.len() - 1].parse().map_err(|_| "Invalid seconds")?;
        Ok(Duration::from_secs(secs))
    } else if s.ends_with('m') {
        let mins: u64 = s[..s.len() - 1].parse().map_err(|_| "Invalid minutes")?;
        Ok(Duration::from_secs(mins * 60))
    } else {
        let secs: u64 = s.parse().map_err(|_| "Invalid duration")?;
        Ok(Duration::from_secs(secs))
    }
}

/// Substitute template variables in pipeline YAML
fn substitute_pipeline_vars(yaml: &str, args: &Args) -> String {
    yaml.replace("${MODEL}", &args.model)
        .replace("${LANGUAGE}", &args.language)
        .replace("${THREADS}", &args.threads.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging
    let log_level = match args.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    let filter = if args.quiet { "error" } else { log_level };

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .init();

    // Read input audio
    let input_path = args.input.to_string_lossy().to_string();
    let source = detect_input_source(&input_path)
        .map_err(|e| anyhow::anyhow!("Failed to detect input source '{}': {}", input_path, e))?;

    tracing::info!("Reading audio from: {:?}", args.input);

    let mut reader = InputReader::open(source)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open input: {}", e))?;

    let data = reader
        .read_to_end()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read input: {}", e))?;

    tracing::debug!("Read {} bytes from input", data.len());

    // Parse as WAV audio
    let input_data = if is_wav(&data) {
        let (samples, sample_rate, channels) = parse_wav(&data).with_context(|| {
            format!(
                "Failed to parse WAV file ({} bytes, starts with: {:02x?})",
                data.len(),
                &data[..std::cmp::min(16, data.len())]
            )
        })?;
        tracing::info!(
            "Parsed WAV: {} samples, {}Hz, {} ch, {:.2}s duration",
            samples.len(),
            sample_rate,
            channels,
            samples.len() as f32 / sample_rate as f32 / channels as f32
        );
        RuntimeData::Audio {
            samples,
            sample_rate,
            channels: channels as u32,
            stream_id: None,
        }
    } else {
        anyhow::bail!(
            "Input file does not appear to be a WAV file. \
             Only WAV format is currently supported."
        );
    };

    // Build pipeline manifest with substituted variables
    let manifest_yaml = substitute_pipeline_vars(PIPELINE_YAML, &args);
    let manifest = pipeline::parse_manifest(&manifest_yaml)
        .context("Failed to parse embedded pipeline manifest")?;

    tracing::info!(
        "Using Whisper model '{}' with language '{}'",
        args.model,
        args.language
    );

    // Create pipeline runner
    let runner = pipeline::create_runner().context("Failed to create pipeline runner")?;

    // Execute pipeline with timeout
    tracing::info!(
        "Transcribing audio (timeout: {:?})...",
        args.timeout
    );

    let manifest = Arc::new(manifest);
    let output = tokio::time::timeout(args.timeout, async {
        pipeline::execute_unary(&runner, manifest.clone(), input_data).await
    })
    .await
    .context("Transcription timed out")??;

    // Extract SRT text from output
    let srt_content = match &output {
        RuntimeData::Text(text) => text.clone(),
        RuntimeData::Json(json) => {
            // If output is JSON, try to extract text field
            if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
                text.to_string()
            } else {
                serde_json::to_string_pretty(json)
                    .context("Failed to serialize JSON output")?
            }
        }
        other => {
            anyhow::bail!(
                "Unexpected output type: {}. Expected Text or JSON.",
                other.data_type()
            );
        }
    };

    // Write output
    let sink = detect_output_sink(&args.output)
        .map_err(|e| anyhow::anyhow!("Failed to detect output sink '{}': {}", args.output, e))?;

    let mut writer = OutputWriter::open(sink)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open output: {}", e))?;

    writer
        .write_all(srt_content.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write output: {}", e))?;

    writer
        .flush()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to flush output: {}", e))?;

    if args.output != "-" {
        tracing::info!("Wrote SRT subtitles to: {}", args.output);
    }

    Ok(())
}
