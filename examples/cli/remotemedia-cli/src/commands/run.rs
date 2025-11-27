//! `remotemedia run` command - Execute a pipeline once (unary mode)

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::time::Duration;

use crate::config::Config;
use crate::output::{OutputFormat, Outputter};

/// Arguments for the run command
#[derive(Args)]
pub struct RunArgs {
    /// Path to pipeline manifest (YAML or JSON)
    pub manifest: PathBuf,

    /// Input file path
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Output file path
    #[arg(short, long)]
    pub output: Option<PathBuf>,

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

    // Load input if provided
    let input_data = if let Some(input_path) = &args.input {
        if !input_path.exists() {
            std::process::exit(3); // Exit code 3 = input file not found
        }
        Some(std::fs::read(input_path).with_context(|| format!("Failed to read input: {:?}", input_path))?)
    } else {
        None
    };

    // TODO: Execute pipeline using runtime-core
    // For now, return a placeholder result
    tracing::info!("Executing pipeline with timeout {:?}", args.timeout);

    let result = serde_json::json!({
        "status": "success",
        "manifest": args.manifest.display().to_string(),
        "input_size": input_data.map(|d| d.len()).unwrap_or(0),
    });

    // Write output if path provided
    if let Some(output_path) = &args.output {
        let output_content = serde_json::to_string_pretty(&result)?;
        std::fs::write(output_path, &output_content)
            .with_context(|| format!("Failed to write output: {:?}", output_path))?;
        tracing::info!("Output written to {:?}", output_path);
    }

    outputter.output(&result)?;

    Ok(())
}
