//! `remotemedia run` command - Execute a pipeline once (unary mode)

use anyhow::{Context, Result};
use clap::Args;
use remotemedia_core::data::RuntimeData;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::audio::{is_wav, parse_wav};
use crate::config::Config;
use crate::io::{detect_input_source, detect_output_sink, InputReader, InputSource, OutputWriter};
use crate::output::{OutputFormat, Outputter};
use crate::pipeline;

/// Arguments for the run command
#[derive(Args)]
pub struct RunArgs {
    /// Path to pipeline manifest (YAML or JSON)
    pub manifest: PathBuf,

    /// Input file path, named pipe, or `-` for stdin
    #[arg(short = 'i', long, help = "Input source: file path, named pipe (FIFO), or '-' for stdin")]
    pub input: Option<String>,

    /// Output file path, named pipe, or `-` for stdout
    #[arg(short = 'O', long, help = "Output destination: file path, named pipe (FIFO), or '-' for stdout")]
    pub output: Option<String>,

    /// Override node parameters (JSON string)
    #[arg(long)]
    pub params: Option<String>,

    /// Execution timeout
    #[arg(long, default_value = "300s", value_parser = parse_duration)]
    pub timeout: Duration,

    /// Input format hint (auto-detected if not specified)
    #[arg(long, value_enum, default_value = "auto")]
    pub input_format: InputFormat,
}

/// Input format for the pipeline
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum InputFormat {
    /// Auto-detect from file extension or content
    Auto,
    /// WAV audio file
    Wav,
    /// Raw PCM audio (requires --sample-rate and --channels)
    RawPcm,
    /// Plain text
    Text,
    /// JSON data
    Json,
    /// Binary data
    Binary,
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

/// Detect input format from file extension or content
fn detect_format(path: Option<&str>, data: &[u8]) -> InputFormat {
    // Check by file extension first
    if let Some(path) = path {
        let path_lower = path.to_lowercase();
        if path_lower.ends_with(".wav") {
            return InputFormat::Wav;
        } else if path_lower.ends_with(".txt") {
            return InputFormat::Text;
        } else if path_lower.ends_with(".json") {
            return InputFormat::Json;
        } else if path_lower.ends_with(".pcm") || path_lower.ends_with(".raw") {
            return InputFormat::RawPcm;
        }
    }

    // Check by content magic bytes
    if is_wav(data) {
        return InputFormat::Wav;
    }

    // Try to parse as JSON
    if let Ok(_) = serde_json::from_slice::<serde_json::Value>(data) {
        return InputFormat::Json;
    }

    // Check if it looks like UTF-8 text
    if std::str::from_utf8(data).is_ok() {
        return InputFormat::Text;
    }

    // Default to binary
    InputFormat::Binary
}

/// Convert input data to RuntimeData based on format
fn to_runtime_data(data: Vec<u8>, format: InputFormat, path: Option<&str>) -> Result<RuntimeData> {
    let format = match format {
        InputFormat::Auto => detect_format(path, &data),
        f => f,
    };

    match format {
        InputFormat::Wav => {
            let (samples, sample_rate, channels) = parse_wav(&data).with_context(|| {
                format!(
                    "Failed to parse WAV file ({} bytes, starts with: {:02x?})",
                    data.len(),
                    &data[..std::cmp::min(16, data.len())]
                )
            })?;
            tracing::info!(
                "Parsed WAV: {} samples, {}Hz, {} ch",
                samples.len(),
                sample_rate,
                channels
            );
            Ok(RuntimeData::Audio {
                samples,
                sample_rate,
                channels: channels as u32,
                stream_id: None,
                timestamp_us: None,
                arrival_ts_us: None,
            })
        }
        InputFormat::RawPcm => {
            // Assume 16kHz mono f32 for raw PCM (common for speech)
            // TODO: Add --sample-rate and --channels args for raw PCM
            let sample_rate = 16000u32;
            let channels = 1u32;

            // Try to interpret as f32 samples
            if data.len() % 4 == 0 {
                let samples: Vec<f32> = data
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                Ok(RuntimeData::Audio {
                    samples,
                    sample_rate,
                    channels,
                    stream_id: None,
                    timestamp_us: None,
                    arrival_ts_us: None,
                })
            } else {
                // Assume 16-bit signed PCM
                let samples: Vec<f32> = data
                    .chunks_exact(2)
                    .map(|chunk| {
                        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                        sample as f32 / 32768.0
                    })
                    .collect();
                Ok(RuntimeData::Audio {
                    samples,
                    sample_rate,
                    channels,
                    stream_id: None,
                    timestamp_us: None,
                    arrival_ts_us: None,
                })
            }
        }
        InputFormat::Text => {
            let text = String::from_utf8(data).context("Input is not valid UTF-8 text")?;
            Ok(RuntimeData::Text(text))
        }
        InputFormat::Json => {
            let json: serde_json::Value =
                serde_json::from_slice(&data).context("Failed to parse JSON input")?;
            Ok(RuntimeData::Json(json))
        }
        InputFormat::Binary => Ok(RuntimeData::Binary(data)),
        InputFormat::Auto => unreachable!("Auto should be resolved by detect_format"),
    }
}

/// Convert RuntimeData to output bytes
fn from_runtime_data(data: &RuntimeData) -> Result<Vec<u8>> {
    match data {
        RuntimeData::Text(text) => Ok(text.as_bytes().to_vec()),
        RuntimeData::Json(json) => {
            serde_json::to_vec_pretty(json).context("Failed to serialize JSON output")
        }
        RuntimeData::Binary(bytes) => Ok(bytes.clone()),
        RuntimeData::Audio {
            samples,
            sample_rate,
            channels,
            ..
        } => {
            // Output as raw f32 PCM
            tracing::info!(
                "Audio output: {} samples, {}Hz, {} channels",
                samples.len(),
                sample_rate,
                channels
            );
            let mut bytes = Vec::with_capacity(samples.len() * 4);
            for &sample in samples {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            Ok(bytes)
        }
        _ => {
            // For other types, serialize as JSON
            let json = serde_json::to_value(data).context("Failed to serialize output")?;
            serde_json::to_vec_pretty(&json).context("Failed to serialize output to JSON")
        }
    }
}

pub async fn execute(args: RunArgs, _config: &Config, format: OutputFormat) -> Result<()> {
    let outputter = Outputter::new(format);

    // Load manifest
    let manifest_content = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read manifest: {:?}", args.manifest))?;

    // Parse manifest
    let manifest = pipeline::parse_manifest(&manifest_content)
        .with_context(|| format!("Failed to parse manifest: {:?}", args.manifest))?;

    tracing::info!(
        "Loading pipeline '{}' from {:?}",
        manifest.metadata.name,
        args.manifest
    );

    // Create pipeline runner
    let runner = pipeline::create_runner().context("Failed to create pipeline runner")?;

    // Load input if provided
    let input_data = if let Some(input_path) = &args.input {
        let source = detect_input_source(input_path).map_err(|e| {
            anyhow::anyhow!("Failed to detect input source '{}': {}", input_path, e)
        })?;

        match &source {
            InputSource::Stdin => tracing::info!("Reading input from stdin"),
            InputSource::Pipe(p) => tracing::info!("Reading input from named pipe: {:?}", p),
            InputSource::File(p) => tracing::info!("Reading input from file: {:?}", p),
        }

        let mut reader = InputReader::open(source)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open input: {}", e))?;

        let data = reader
            .read_to_end()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read input: {}", e))?;

        tracing::debug!("Read {} bytes from input", data.len());

        // Convert to RuntimeData
        let runtime_data = to_runtime_data(data, args.input_format, Some(input_path))?;
        tracing::info!("Input data type: {}", runtime_data.data_type());
        Some(runtime_data)
    } else {
        None
    };

    // Execute pipeline with timeout
    tracing::info!("Executing pipeline with timeout {:?}", args.timeout);

    let manifest = Arc::new(manifest);
    let execution_result = if let Some(input) = input_data {
        tokio::time::timeout(args.timeout, async {
            pipeline::execute_unary(&runner, manifest.clone(), input).await
        })
        .await
        .context("Pipeline execution timed out")?
    } else {
        // Execute without input (for pipelines that don't need external input)
        tokio::time::timeout(args.timeout, async {
            pipeline::execute_unary(
                &runner,
                manifest.clone(),
                RuntimeData::Json(serde_json::json!({})),
            )
            .await
        })
        .await
        .context("Pipeline execution timed out")?
    };

    match execution_result {
        Ok(output) => {
            tracing::info!("Pipeline completed, output type: {}", output.data_type());

            // Write output if path provided
            if let Some(output_path) = &args.output {
                let sink = detect_output_sink(output_path).map_err(|e| {
                    anyhow::anyhow!("Failed to detect output sink '{}': {}", output_path, e)
                })?;

                let output_content = from_runtime_data(&output)?;
                let is_stdout = sink.is_stdout();

                match &sink {
                    crate::io::OutputSink::Stdout => tracing::info!("Writing output to stdout"),
                    crate::io::OutputSink::Pipe(p) => {
                        tracing::info!("Writing output to named pipe: {:?}", p)
                    }
                    crate::io::OutputSink::File(p) => {
                        tracing::info!("Writing output to file: {:?}", p)
                    }
                }

                let mut writer = OutputWriter::open(sink)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to open output: {}", e))?;

                writer
                    .write_all(&output_content)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to write output: {}", e))?;

                writer
                    .flush()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to flush output: {}", e))?;

                tracing::debug!("Wrote {} bytes to output", output_content.len());

                // Don't double-output to terminal if we already wrote to stdout
                if !is_stdout {
                    let result = serde_json::json!({
                        "status": "success",
                        "output_type": output.data_type(),
                        "output_size": output_content.len(),
                    });
                    outputter.output(&result)?;
                }
            } else {
                // Output to terminal
                match &output {
                    RuntimeData::Text(text) => {
                        println!("{}", text);
                    }
                    RuntimeData::Json(json) => {
                        outputter.output(json)?;
                    }
                    _ => {
                        let result = serde_json::json!({
                            "status": "success",
                            "output_type": output.data_type(),
                            "item_count": output.item_count(),
                        });
                        outputter.output(&result)?;
                    }
                }
            }
        }
        Err(e) => {
            let result = serde_json::json!({
                "status": "error",
                "error": e.to_string(),
            });
            outputter.output(&result)?;
            anyhow::bail!("Pipeline execution failed: {}", e);
        }
    }

    Ok(())
}
