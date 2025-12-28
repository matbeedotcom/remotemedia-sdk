//! `remotemedia run` command - Execute a pipeline once (unary mode)

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::time::Duration;

use crate::config::Config;
use crate::io::{detect_input_source, detect_output_sink, InputReader, InputSource, OutputWriter};
use crate::output::{OutputFormat, Outputter};

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
    #[arg(long, default_value = "30s", value_parser = parse_duration)]
    pub timeout: Duration,
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

pub async fn execute(args: RunArgs, _config: &Config, format: OutputFormat) -> Result<()> {
    let outputter = Outputter::new(format);

    // Load manifest
    let manifest_content = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read manifest: {:?}", args.manifest))?;

    // Parse manifest
    let manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Invalid manifest: {}", e))?;

    // Validate manifest has required fields
    if manifest.get("version").is_none() {
        anyhow::bail!("Invalid manifest: missing 'version' field");
    }

    tracing::info!("Loading pipeline from {:?}", args.manifest);

    // Load input if provided using the new I/O abstraction
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
        Some(data)
    } else {
        None
    };

    // TODO: Execute pipeline using runtime-core
    // For now, return a placeholder result
    tracing::info!("Executing pipeline with timeout {:?}", args.timeout);

    let result = serde_json::json!({
        "status": "success",
        "manifest": args.manifest.display().to_string(),
        "input_size": input_data.as_ref().map(|d| d.len()).unwrap_or(0),
    });

    // Write output if path provided using the new I/O abstraction
    if let Some(output_path) = &args.output {
        let sink = detect_output_sink(output_path).map_err(|e| {
            anyhow::anyhow!("Failed to detect output sink '{}': {}", output_path, e)
        })?;

        let output_content = serde_json::to_string_pretty(&result)?;

        // If outputting to stdout, skip the normal outputter
        let is_stdout = sink.is_stdout();

        match &sink {
            crate::io::OutputSink::Stdout => tracing::info!("Writing output to stdout"),
            crate::io::OutputSink::Pipe(p) => {
                tracing::info!("Writing output to named pipe: {:?}", p)
            }
            crate::io::OutputSink::File(p) => tracing::info!("Writing output to file: {:?}", p),
        }

        let mut writer = OutputWriter::open(sink)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open output: {}", e))?;

        writer
            .write_all(output_content.as_bytes())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write output: {}", e))?;

        writer
            .flush()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to flush output: {}", e))?;

        tracing::debug!("Wrote {} bytes to output", output_content.len());

        // Don't double-output to terminal if we already wrote to stdout
        if !is_stdout {
            outputter.output(&result)?;
        }
    } else {
        outputter.output(&result)?;
    }

    Ok(())
}
