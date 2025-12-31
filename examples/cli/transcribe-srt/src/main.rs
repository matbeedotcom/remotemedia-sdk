//! transcribe-srt - Self-contained audio-to-SRT subtitle transcription
//!
//! A thin wrapper around the remotemedia CLI with an embedded pipeline.
//! Supports all the same I/O options: files, named pipes, and stdin/stdout.
//!
//! # Usage
//!
//! ```bash
//! # File input/output
//! transcribe-srt -i input.mp4 -o subtitles.srt
//!
//! # Pipe from ffmpeg, output to stdout
//! ffmpeg -i video.mp4 -f wav -ar 16000 -ac 1 - | transcribe-srt -i - -o -
//!
//! # Full pipeline: extract audio, transcribe, mux subtitles back
//! ffmpeg -i input.mp4 -f wav -ar 16000 -ac 1 - 2>/dev/null | \
//!   transcribe-srt -i - -o - | \
//!   ffmpeg -y -i input.mp4 -f srt -i pipe:0 -map 0:v -map 0:a -map 1:0 -c:v copy -c:a copy -c:s mov_text output.mp4
//! ```

use anyhow::{Context, Result};
use clap::Parser;
use remotemedia_cli::{
    audio::{is_wav, parse_wav},
    ffmpeg,
    io::{detect_input_source, detect_output_sink, InputReader, InputSource, OutputWriter},
    pipeline,
};
use remotemedia_runtime_core::data::RuntimeData;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Pipeline YAML embedded from the shared pipelines directory at compile time
const PIPELINE_YAML: &str = include_str!("../../pipelines/transcribe-srt.yaml");

/// Transcribe audio/video to SRT subtitles
#[derive(Parser)]
#[command(name = "transcribe-srt")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input source: file path, named pipe, or `-` for stdin
    #[arg(short = 'i', long)]
    pub input: String,

    /// Output destination: file path, named pipe, or `-` for stdout
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

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress non-error output
    #[arg(short, long)]
    pub quiet: bool,
}

const SUPPORTED_MODELS: &[&str] = &[
    "tiny", "base", "small", "medium", "large",
    "large-v3-turbo", "quantized_tiny", "quantized_tiny_en",
];

fn parse_model(s: &str) -> Result<String, String> {
    if SUPPORTED_MODELS.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!("Unknown model '{}'. Supported: {}", s, SUPPORTED_MODELS.join(", ")))
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

/// Substitute template variables in pipeline YAML
/// Supports ${VAR:-default} syntax from the shared YAML file
fn substitute_pipeline_vars(yaml: &str, args: &Args) -> String {
    yaml
        // Replace ${VAR:-default} patterns with CLI args
        .replace("${MODEL:-large-v3-turbo}", &args.model)
        .replace("${LANGUAGE:-en}", &args.language)
        .replace("${THREADS:-4}", &args.threads.to_string())
        // Also support simple ${VAR} patterns for compatibility
        .replace("${MODEL}", &args.model)
        .replace("${LANGUAGE}", &args.language)
        .replace("${THREADS}", &args.threads.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging to stderr (keep stdout clean for piping)
    let filter = if args.quiet { "error" } else {
        match args.verbose { 0 => "warn", 1 => "info", 2 => "debug", _ => "trace" }
    };
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .init();

    // Read input using CLI's I/O infrastructure
    let source = detect_input_source(&args.input)
        .map_err(|e| anyhow::anyhow!("Invalid input '{}': {}", args.input, e))?;
    
    tracing::info!("Reading from: {:?}", source);
    
    let mut reader = InputReader::open(source.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open input: {}", e))?;
    
    let data = reader.read_to_end().await
        .map_err(|e| anyhow::anyhow!("Failed to read input: {}", e))?;
    
    tracing::debug!("Read {} bytes", data.len());

    // Decode audio - WAV for stdin (from ffmpeg), or use FFmpeg for files
    let (samples, sample_rate, channels) = if is_wav(&data) {
        tracing::info!("Parsing WAV audio");
        let (s, sr, ch) = parse_wav(&data).context("Failed to parse WAV")?;
        (s, sr, ch as u32)
    } else if let InputSource::File(path) = &source {
        tracing::info!("Decoding with FFmpeg");
        ffmpeg::decode_audio_file(path)?
    } else {
        anyhow::bail!("Stdin input must be WAV format (use: ffmpeg -f wav -ar 16000 -ac 1 -)");
    };

    let duration = samples.len() as f32 / sample_rate as f32 / channels as f32;
    tracing::info!("Audio: {} samples, {}Hz, {}ch, {:.1}s", samples.len(), sample_rate, channels, duration);

    // Build and execute pipeline
    let manifest_yaml = substitute_pipeline_vars(PIPELINE_YAML, &args);
    let manifest = Arc::new(pipeline::parse_manifest(&manifest_yaml)?);
    let runner = pipeline::create_runner()?;

    tracing::info!("Transcribing with model '{}', language '{}'", args.model, args.language);

    let input_data = RuntimeData::Audio { samples, sample_rate, channels, stream_id: None, timestamp_us: None, arrival_ts_us: None };
    
    let output = tokio::time::timeout(args.timeout, 
        pipeline::execute_unary(&runner, manifest, input_data)
    ).await.context("Timeout")??;

    // Extract SRT content
    let srt = match &output {
        RuntimeData::Text(t) => t.clone(),
        RuntimeData::Json(j) => j.get("text").and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| serde_json::to_string_pretty(j).unwrap_or_default()),
        _ => anyhow::bail!("Unexpected output type: {}", output.data_type()),
    };

    // Write output using CLI's I/O infrastructure
    let sink = detect_output_sink(&args.output)
        .map_err(|e| anyhow::anyhow!("Invalid output '{}': {}", args.output, e))?;
    
    let mut writer = OutputWriter::open(sink).await
        .map_err(|e| anyhow::anyhow!("Failed to open output: {}", e))?;
    
    writer.write_all(srt.as_bytes()).await
        .map_err(|e| anyhow::anyhow!("Failed to write: {}", e))?;
    writer.flush().await
        .map_err(|e| anyhow::anyhow!("Failed to flush: {}", e))?;

    if args.output != "-" {
        tracing::info!("Wrote SRT to: {}", args.output);
    }

    Ok(())
}
