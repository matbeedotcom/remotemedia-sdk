//! `remotemedia stream` command - Execute pipeline in streaming mode

use anyhow::{Context, Result};
use clap::Args;
use remotemedia_core::data::RuntimeData;
use std::path::PathBuf;
use std::sync::Arc;

use crate::audio::{is_wav, parse_wav};
use crate::config::Config;
use crate::io::{
    detect_input_source, detect_output_sink, InputReader, InputSource, OutputSink, OutputWriter,
};
use crate::output::OutputFormat;
use crate::pipeline;

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
    #[arg(long, default_value = "16000")]
    pub sample_rate: u32,

    /// Audio channels (1 or 2)
    #[arg(long, default_value = "1")]
    pub channels: u16,

    /// Chunk size in samples for streaming
    #[arg(long, default_value = "4000")]
    pub chunk_size: usize,
}

/// Convert RuntimeData to output bytes for streaming
fn format_output(data: &RuntimeData) -> Result<Vec<u8>> {
    match data {
        RuntimeData::Text(text) => {
            let mut bytes = text.as_bytes().to_vec();
            // Add newline for streaming text output
            if !bytes.ends_with(&[b'\n']) {
                bytes.push(b'\n');
            }
            Ok(bytes)
        }
        RuntimeData::Json(json) => {
            let mut bytes = serde_json::to_vec(json)?;
            bytes.push(b'\n');
            Ok(bytes)
        }
        RuntimeData::Binary(bytes) => Ok(bytes.clone()),
        RuntimeData::Audio { samples, .. } => {
            // Output as raw f32 PCM
            let mut bytes = Vec::with_capacity(samples.len() * 4);
            for &sample in samples {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            Ok(bytes)
        }
        _ => {
            // For other types, serialize as JSON
            let json = serde_json::to_value(data)?;
            let mut bytes = serde_json::to_vec(&json)?;
            bytes.push(b'\n');
            Ok(bytes)
        }
    }
}

pub async fn execute(args: StreamArgs, _config: &Config, _format: OutputFormat) -> Result<()> {
    // Load manifest
    let manifest_content = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read manifest: {:?}", args.manifest))?;

    // Parse manifest
    let manifest = pipeline::parse_manifest(&manifest_content)
        .with_context(|| format!("Failed to parse manifest: {:?}", args.manifest))?;

    tracing::info!(
        "Starting streaming pipeline '{}' (sample_rate={}, channels={}, chunk_size={})",
        manifest.metadata.name,
        args.sample_rate,
        args.channels,
        args.chunk_size
    );

    // Create pipeline runner
    let runner = pipeline::create_runner().context("Failed to create pipeline runner")?;

    // Create streaming session
    let manifest = Arc::new(manifest);
    let mut session = pipeline::StreamingSession::new(&runner, manifest.clone())
        .await
        .context("Failed to create streaming session")?;

    // Detect and prepare input source
    let mut input_reader: Option<InputReader> = if args.mic {
        tracing::info!("Using microphone input");
        // TODO: Initialize microphone capture using cpal
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
        let reader = InputReader::open(InputSource::Stdin)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open stdin: {}", e))?;
        Some(reader)
    };

    // Detect and prepare output sink
    let mut output_writer: Option<OutputWriter> = if args.speaker {
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
        // Default to stdout for streaming output
        let writer = OutputWriter::open(OutputSink::Stdout)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open stdout: {}", e))?;
        Some(writer)
    };

    // Set up signal handler for graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = shutdown_tx.send(());
    });

    // Main streaming loop
    let result = tokio::select! {
        _ = &mut shutdown_rx => {
            tracing::info!("Received shutdown signal");
            Ok(0usize)
        }
        result = stream_pipeline(&mut session, &mut input_reader, &mut output_writer, &args) => {
            result
        }
    };

    // Close session
    if let Err(e) = session.close().await {
        tracing::warn!("Error closing session: {}", e);
    }

    match result {
        Ok(bytes_processed) => {
            tracing::info!("Stream completed: processed {} bytes", bytes_processed);
            Ok(())
        }
        Err(e) => {
            // Check if it's a broken pipe (expected when downstream closes)
            if let Some(io_err) = e.downcast_ref::<crate::io::IoError>() {
                if matches!(io_err, crate::io::IoError::BrokenPipe { .. }) {
                    tracing::debug!("Output pipe closed by reader");
                    std::process::exit(141); // 128 + SIGPIPE (13)
                }
            }
            Err(e)
        }
    }
}

/// Stream data through the pipeline
async fn stream_pipeline(
    session: &mut pipeline::StreamingSession,
    input: &mut Option<InputReader>,
    output: &mut Option<OutputWriter>,
    args: &StreamArgs,
) -> Result<usize> {
    let mut total_bytes = 0;

    // Check if we have a WAV file input that needs header parsing
    let mut is_wav_input = false;
    let mut wav_sample_rate = args.sample_rate;
    let mut wav_channels = args.channels as u32;

    if let Some(ref mut reader) = input {
        // Try to read WAV header first (peek at first 12 bytes)
        let mut header_buf = vec![0u8; 44]; // Standard WAV header size
        if reader.read(&mut header_buf).await.is_ok() && is_wav(&header_buf) {
            // Parse the full WAV file for now
            // TODO: Implement streaming WAV parsing
            let remaining = reader.read_to_end().await?;
            let mut full_data = header_buf;
            full_data.extend(remaining);

            let (samples, sample_rate, channels) =
                parse_wav(&full_data).context("Failed to parse WAV file")?;

            wav_sample_rate = sample_rate;
            wav_channels = channels as u32;
            is_wav_input = true;

            tracing::info!(
                "WAV input: {} samples, {}Hz, {} channels",
                samples.len(),
                sample_rate,
                channels
            );

            // Process WAV in chunks
            for chunk in samples.chunks(args.chunk_size) {
                let audio = RuntimeData::Audio {
                    samples: chunk.to_vec(),
                    sample_rate: wav_sample_rate,
                    channels: wav_channels,
                    stream_id: None,
                    timestamp_us: None,
                    arrival_ts_us: None,
                };

                session.send(audio).await?;
                total_bytes += chunk.len() * 4;

                // Try to receive any available output
                while let Ok(Some(output_data)) = session.recv().await {
                    if let Some(ref mut writer) = output {
                        let bytes = format_output(&output_data)?;
                        writer.write_all(&bytes).await?;
                        writer.flush().await?;
                    }
                }
            }
        } else {
            // Not a WAV file - stream as raw audio chunks
            // Put back the header bytes
            let mut combined = header_buf;

            // Read rest of input
            loop {
                let mut buf = vec![0u8; args.chunk_size * 4]; // f32 samples
                let n = reader.read(&mut buf).await?;

                if n == 0 {
                    // Process any remaining data from header
                    if !combined.is_empty() {
                        // Interpret as f32 PCM
                        let samples: Vec<f32> = combined
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();

                        if !samples.is_empty() {
                            let audio = RuntimeData::Audio {
                                samples,
                                sample_rate: args.sample_rate,
                                channels: args.channels as u32,
                                stream_id: None,
                                timestamp_us: None,
                                arrival_ts_us: None,
                            };
                            session.send(audio).await?;
                        }
                    }
                    break;
                }

                combined.extend_from_slice(&buf[..n]);
                total_bytes += n;

                // Process complete chunks
                let chunk_bytes = args.chunk_size * 4;
                while combined.len() >= chunk_bytes {
                    let chunk_data: Vec<u8> = combined.drain(..chunk_bytes).collect();
                    let samples: Vec<f32> = chunk_data
                        .chunks_exact(4)
                        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        .collect();

                    let audio = RuntimeData::Audio {
                        samples,
                        sample_rate: args.sample_rate,
                        channels: args.channels as u32,
                        stream_id: None,
                        timestamp_us: None,
                        arrival_ts_us: None,
                    };

                    session.send(audio).await?;

                    // Try to receive any available output
                    while let Ok(Some(output_data)) = session.recv().await {
                        if let Some(ref mut writer) = output {
                            let bytes = format_output(&output_data)?;
                            writer.write_all(&bytes).await?;
                            writer.flush().await?;
                        }
                    }
                }
            }
        }
    }

    // Drain remaining outputs
    loop {
        match session.recv().await {
            Ok(Some(output_data)) => {
                if let Some(ref mut writer) = output {
                    let bytes = format_output(&output_data)?;
                    writer.write_all(&bytes).await?;
                    writer.flush().await?;
                }
            }
            Ok(None) => break, // Session ended
            Err(e) => {
                tracing::warn!("Error receiving output: {}", e);
                break;
            }
        }
    }

    // Final flush
    if let Some(ref mut writer) = output {
        writer.flush().await?;
    }

    Ok(total_bytes)
}
