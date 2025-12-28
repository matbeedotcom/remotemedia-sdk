//! `remotemedia stream` command - Execute pipeline in streaming mode

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use crate::config::Config;
use crate::io::{
    detect_input_source, detect_output_sink, InputReader, InputSource, OutputSink, OutputWriter,
};
use crate::output::OutputFormat;

/// Arguments for the stream command
#[derive(Args)]
pub struct StreamArgs {
    /// Path to pipeline manifest (YAML or JSON)
    pub manifest: PathBuf,

    /// Use microphone as input
    #[arg(long)]
    pub mic: bool,

    /// Play audio output to speaker
    #[arg(long)]
    pub speaker: bool,

    /// Input file, named pipe, or `-` for stdin (streamed)
    #[arg(short = 'i', long, help = "Input source: file path, named pipe (FIFO), or '-' for stdin")]
    pub input: Option<String>,

    /// Output file, named pipe, or `-` for stdout (appended)
    #[arg(short = 'O', long, help = "Output destination: file path, named pipe (FIFO), or '-' for stdout")]
    pub output: Option<String>,

    /// Audio sample rate
    #[arg(long, default_value = "48000")]
    pub sample_rate: u32,

    /// Audio channels (1 or 2)
    #[arg(long, default_value = "1")]
    pub channels: u16,
}

pub async fn execute(args: StreamArgs, _config: &Config, _format: OutputFormat) -> Result<()> {
    // Load manifest
    let manifest_content = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read manifest: {:?}", args.manifest))?;

    // Parse manifest
    let _manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Invalid manifest: {}", e))?;

    tracing::info!(
        "Starting stream pipeline from {:?} (sample_rate={}, channels={})",
        args.manifest,
        args.sample_rate,
        args.channels
    );

    // Detect and prepare input source
    let input_reader: Option<InputReader> = if args.mic {
        tracing::info!("Using microphone input");
        // TODO: Initialize microphone capture using cpal
        // This would use crate::audio::mic::capture()
        None
    } else if let Some(input_path) = &args.input {
        let source = detect_input_source(input_path).map_err(|e| {
            anyhow::anyhow!("Failed to detect input source '{}': {}", input_path, e)
        })?;

        match &source {
            InputSource::Stdin => tracing::info!("Streaming from stdin"),
            InputSource::Pipe(p) => tracing::info!("Streaming from named pipe: {:?}", p),
            InputSource::File(p) => tracing::info!("Streaming from file: {:?}", p),
        }

        let reader = InputReader::open(source)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open input: {}", e))?;
        Some(reader)
    } else {
        tracing::info!("No input specified, waiting for data on stdin");
        // Default to stdin if no input specified and no mic
        let reader = InputReader::open(InputSource::Stdin)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open stdin: {}", e))?;
        Some(reader)
    };

    // Detect and prepare output sink
    let output_writer: Option<OutputWriter> = if args.speaker {
        tracing::info!("Audio output to speaker enabled");
        // TODO: Initialize speaker output using cpal
        None
    } else if let Some(output_path) = &args.output {
        let sink = detect_output_sink(output_path).map_err(|e| {
            anyhow::anyhow!("Failed to detect output sink '{}': {}", output_path, e)
        })?;

        match &sink {
            OutputSink::Stdout => tracing::info!("Streaming output to stdout"),
            OutputSink::Pipe(p) => tracing::info!("Streaming output to named pipe: {:?}", p),
            OutputSink::File(p) => tracing::info!("Streaming output to file: {:?}", p),
        }

        let writer = OutputWriter::open(sink)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open output: {}", e))?;
        Some(writer)
    } else {
        None
    };

    // TODO: Execute streaming pipeline using runtime-core
    // For now, just demonstrate the streaming I/O structure

    // Set up signal handler for graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = shutdown_tx.send(());
    });

    // Main streaming loop
    tokio::select! {
        _ = &mut shutdown_rx => {
            tracing::info!("Received shutdown signal");
        }
        result = stream_data(input_reader, output_writer) => {
            match result {
                Ok(bytes_processed) => {
                    tracing::info!("Processed {} bytes", bytes_processed);
                }
                Err(e) => {
                    // Check if it's a broken pipe (expected when downstream closes)
                    if let Some(io_err) = e.downcast_ref::<crate::io::IoError>() {
                        if matches!(io_err, crate::io::IoError::BrokenPipe { .. }) {
                            tracing::debug!("Output pipe closed by reader");
                            // Exit with SIGPIPE exit code (128 + 13 = 141)
                            std::process::exit(141);
                        }
                    }
                    return Err(e);
                }
            }
        }
    }

    tracing::info!("Stream completed");

    // Exit code 0 for normal completion
    Ok(())
}

/// Stream data from input to output
async fn stream_data(
    mut input: Option<InputReader>,
    mut output: Option<OutputWriter>,
) -> Result<usize> {
    let mut total_bytes = 0;
    let mut buf = vec![0u8; 8192]; // 8KB buffer for streaming

    if let Some(ref mut reader) = input {
        loop {
            let n = reader
                .read(&mut buf)
                .await
                .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;

            if n == 0 {
                // End of stream
                tracing::debug!("End of input stream");
                break;
            }

            total_bytes += n;

            // Write to output if available
            if let Some(ref mut writer) = output {
                writer
                    .write_all(&buf[..n])
                    .await
                    .map_err(|e| anyhow::anyhow!("Write error: {}", e))?;
            }

            // TODO: Process data through pipeline here
            // For now, just pass through
        }

        // Flush output
        if let Some(ref mut writer) = output {
            writer
                .flush()
                .await
                .map_err(|e| anyhow::anyhow!("Flush error: {}", e))?;
        }
    }

    Ok(total_bytes)
}
