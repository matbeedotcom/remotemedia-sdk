//! Generic pipeline runner with embedded YAML
//!
//! Build with:
//!   PIPELINE_YAML=path/to/pipeline.yaml cargo build -p pipeline-embed --release
//!
//! The resulting binary can be renamed and distributed as a standalone tool.
//!
//! # Examples
//!
//! ```bash
//! # Build a transcription tool
//! PIPELINE_YAML=pipelines/transcribe-srt.yaml cargo build -p pipeline-embed --release
//! cp target/release/pipeline-runner ./transcribe
//!
//! # Use it
//! ./transcribe -i input.mp4 -o output.srt
//! ```

// Include the generated pipeline YAML
include!(concat!(env!("OUT_DIR"), "/embedded_pipeline.rs"));

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

/// Run an embedded pipeline
#[derive(Parser)]
#[command(name = "pipeline-runner")]
#[command(author, version, about = "Embedded pipeline runner")]
struct Args {
    /// Input source: file path, named pipe, or `-` for stdin
    #[arg(short = 'i', long)]
    pub input: Option<String>,

    /// Output destination: file path, named pipe, or `-` for stdout
    #[arg(short = 'o', long, default_value = "-")]
    pub output: String,

    /// Execution timeout
    #[arg(long, default_value = "600s", value_parser = parse_duration)]
    pub timeout: Duration,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress non-error output
    #[arg(short, long)]
    pub quiet: bool,

    /// Show the embedded pipeline YAML and exit
    #[arg(long)]
    pub show_pipeline: bool,
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Show pipeline and exit if requested
    if args.show_pipeline {
        println!("{}", PIPELINE_YAML);
        return Ok(());
    }

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
    
    tracing::info!("Executing pipeline (timeout: {:?})", args.timeout);
    
    let output = tokio::time::timeout(args.timeout, 
        pipeline::execute_unary(&runner, manifest, input_data)
    ).await.context("Pipeline execution timed out")??;

    // Write output
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
