//! Stream Health Monitor Demo Binary
//!
//! A time-limited demo CLI tool for evaluating stream drift, freeze detection,
//! and health monitoring. Uses the pipeline-embed infrastructure with demo mode
//! enforcement (15 min/session, 3 sessions/day).
//!
//! # Usage
//!
//! ```bash
//! # Analyze a file
//! ./remotemedia-demo -i test.mp4
//!
//! # Stream from FFmpeg
//! ffmpeg -i rtmp://server/live/test -f wav -ar 16000 -ac 1 - | ./remotemedia-demo -i - --stream
//!
//! # Show demo limits
//! ./remotemedia-demo --show-limits
//!
//! # Activate a license
//! ./remotemedia-demo activate RMDA-XXXX-XXXX-XXXX
//! ```

// Include the generated pipeline and config
include!(concat!(env!("OUT_DIR"), "/embedded_pipeline.rs"));

mod banner;
mod demo_mode;
mod license;
mod summary;

// Use shared health analyzer types
use remotemedia_health_analyzer as events;
use remotemedia_health_analyzer::convert_json_to_health_events;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use remotemedia_cli::{
    audio::is_wav,
    ffmpeg,
    io::{detect_input_source, InputReader, InputSource},
    pipeline,
};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::{Manifest, NodeManifest};
use remotemedia_runtime_core::ingestion::{
    global_ingest_registry, IngestConfig, IngestStatus, AudioConfig,
};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Stream Health Monitor Demo
///
/// Analyze audio/video streams for drift, freezes, and health issues.
/// Demo mode: 15-minute sessions, 3 per day. Activate a license for unlimited use.
#[derive(Parser)]
#[command(name = BINARY_NAME)]
#[command(author, version)]
#[command(about = PIPELINE_DESCRIPTION)]
struct Args {
    /// Input source: file path, named pipe, or `-` for stdin
    #[arg(short = 'i', long, help = "Input file, pipe, or '-' for stdin")]
    input: Option<String>,

    /// Ingest from URI (file://, rtmp://, rtmps://)
    /// Uses the pluggable ingestion framework with auto-decoding
    #[arg(long, help = "Ingest URI (file://, rtmp://, rtmps://)")]
    ingest: Option<String>,

    /// Output destination for JSONL events [default: stdout]
    #[arg(short = 'o', long, default_value = "-", help = "Output file or '-' for stdout")]
    output: String,

    /// Enable streaming mode (continuous input/output)
    #[arg(long, help = "Run in streaming mode for real-time processing")]
    stream: bool,

    /// Drift alert threshold in milliseconds
    #[arg(long, default_value = "50", help = "Lead/drift threshold in ms")]
    lead_threshold: i64,

    /// Freeze detection threshold in milliseconds
    #[arg(long, default_value = "500", help = "Freeze duration threshold in ms")]
    freeze_threshold: u64,

    /// Health score emission interval in milliseconds
    #[arg(long, default_value = "1000", help = "Health score emit interval in ms")]
    health_interval: u64,

    /// Audio sample rate for input processing
    #[arg(short = 'r', long, default_value = "16000", help = "Audio sample rate (Hz)")]
    sample_rate: u32,

    /// Audio channels for input processing
    #[arg(short = 'c', long, default_value = "1", help = "Audio channels (1 or 2)")]
    channels: u32,

    /// Audio chunk size for streaming mode
    #[arg(long, default_value = "4000", help = "Chunk size in samples")]
    chunk_size: usize,

    /// Output raw JSONL without TUI elements
    #[arg(long, help = "Output only JSONL events, no banners")]
    json: bool,

    /// Show the embedded pipeline YAML and exit
    #[arg(long, help = "Display embedded pipeline manifest")]
    show_pipeline: bool,

    /// Show demo limits and exit
    #[arg(long, help = "Display demo mode limits")]
    show_limits: bool,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress non-error output
    #[arg(short, long)]
    quiet: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Activate a license key
    Activate {
        /// License key (RMDA-XXXX-XXXX-XXXX format)
        key: String,
    },
}

/// Register RTMP plugin if the feature is enabled
fn register_ingest_plugins() {
    #[cfg(feature = "rtmp")]
    {
        use remotemedia_ingest_rtmp::RtmpIngestPlugin;
        let registry = global_ingest_registry();
        if let Err(e) = registry.register(Arc::new(RtmpIngestPlugin)) {
            tracing::warn!("Failed to register RTMP plugin: {}", e);
        } else {
            tracing::info!("Registered RTMP/RTMPS ingest plugin");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging to stderr
    let filter = if args.quiet {
        "error"
    } else {
        match args.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    };
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .init();

    // Register additional ingest plugins (e.g., RTMP if enabled)
    register_ingest_plugins();

    // Handle --show-pipeline
    if args.show_pipeline {
        println!("{}", PIPELINE_YAML);
        return Ok(());
    }

    // Handle --show-limits
    if args.show_limits {
        println!("Demo Mode Limits:");
        println!("================");
        println!();
        println!("  Session duration:  {} minutes", DemoConfig::SESSION_DURATION_SECS / 60);
        println!("  Sessions per day:  {}", DemoConfig::MAX_SESSIONS_PER_DAY);
        println!("  Warning before:    {} seconds", DemoConfig::WARNING_SECS);
        println!();
        println!("Activate a license to remove these limits:");
        println!("  ./remotemedia-demo activate RMDA-XXXX-XXXX-XXXX");
        println!();
        println!("Get a license at https://remotemedia.dev/license");
        return Ok(());
    }

    // Handle license activation subcommand
    if let Some(Command::Activate { key }) = args.command {
        return license::activate_license_command(&key);
    }

    // Validate input is provided for analysis
    if args.input.is_none() && args.ingest.is_none() {
        anyhow::bail!(
            "No input specified. Use -i <file> or --ingest <uri>.\n\
            Examples:\n  \
            ./remotemedia-demo -i audio.wav\n  \
            ./remotemedia-demo --ingest file:///path/to/audio.wav\n  \
            ./remotemedia-demo --ingest rtmp://server/live/stream\n  \
            ffmpeg -i input.mp4 -f wav -ar 16000 -ac 1 - | ./remotemedia-demo -i - --stream"
        );
    }

    // Check for conflicting options
    if args.input.is_some() && args.ingest.is_some() {
        anyhow::bail!("Cannot use both -i and --ingest. Choose one input method.");
    }

    // Load demo controller
    let mut demo = demo_mode::DemoController::load()
        .context("Failed to load demo state")?;

    // Check if we have a valid license
    if demo.has_valid_license() {
        // Licensed mode - no limits
        if !args.json {
            banner::show_licensed_banner();
        }
        if args.ingest.is_some() {
            run_licensed_ingest_session(&args).await
        } else {
            run_licensed_session(&args).await
        }
    } else {
        // Demo mode - enforce limits
        demo.check_can_start()?;
        demo.start_session()?;

        // Show demo banner
        if !args.json {
            banner::show_startup_banner(
                demo.sessions_remaining(),
                demo.time_remaining(),
            );
        }

        // Run with timer
        let result = if args.ingest.is_some() {
            run_demo_ingest_session(&args, &mut demo).await
        } else {
            run_demo_session(&args, &mut demo).await
        };

        // Generate and display summary
        let summary = demo.end_session(result.as_ref().map(|r| r.as_slice()).unwrap_or(&[]));
        
        if !args.json {
            summary.display();
        }
        
        // Save report
        if let Err(e) = summary.save_report(&summary::default_report_path()) {
            tracing::warn!("Failed to save report: {}", e);
        }

        result.map(|_| ())
    }
}

/// Run a demo session with time limit enforcement
async fn run_demo_session(
    args: &Args,
    demo: &mut demo_mode::DemoController,
) -> Result<Vec<events::HealthEvent>> {
    let session_duration = demo.time_remaining();
    let warning_time = Duration::from_secs(DemoConfig::WARNING_SECS);

    // Create shutdown channel for timer
    let (timer_shutdown_tx, mut timer_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn timer task
    let json_mode = args.json;
    tokio::spawn(async move {
        // Wait until warning time
        if session_duration > warning_time {
            tokio::time::sleep(session_duration - warning_time).await;
            if !json_mode {
                banner::show_warning(warning_time);
            }
            tokio::time::sleep(warning_time).await;
        } else {
            tokio::time::sleep(session_duration).await;
        }
        let _ = timer_shutdown_tx.send(());
    });

    // Spawn Ctrl+C handler
    let (ctrlc_tx, mut ctrlc_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = ctrlc_tx.send(());
    });

    // Setup pipeline and event emitter
    let (_emitter, collected_events) = run_health_analysis(args, &mut timer_shutdown_rx, &mut ctrlc_rx).await?;

    Ok(collected_events)
}

/// Run a session without time limits (licensed mode)
async fn run_licensed_session(args: &Args) -> Result<()> {
    // Setup Ctrl+C handler
    let (ctrlc_tx, mut ctrlc_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = ctrlc_tx.send(());
    });

    tracing::info!("Running in licensed mode (no time limit)");
    tracing::info!("Press Ctrl+C to stop");

    // Create a dummy timer that never fires
    let (_never_tx, mut never_rx) = tokio::sync::oneshot::channel::<()>();

    // Run health analysis
    let (_emitter, _collected_events) = run_health_analysis(args, &mut never_rx, &mut ctrlc_rx).await?;

    tracing::info!("Session ended");
    Ok(())
}

/// Run a demo session with time limit enforcement using ingestion framework
async fn run_demo_ingest_session(
    args: &Args,
    demo: &mut demo_mode::DemoController,
) -> Result<Vec<events::HealthEvent>> {
    let session_duration = demo.time_remaining();
    let warning_time = Duration::from_secs(DemoConfig::WARNING_SECS);

    // Create shutdown channel for timer
    let (timer_shutdown_tx, mut timer_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn timer task
    let json_mode = args.json;
    tokio::spawn(async move {
        // Wait until warning time
        if session_duration > warning_time {
            tokio::time::sleep(session_duration - warning_time).await;
            if !json_mode {
                banner::show_warning(warning_time);
            }
            tokio::time::sleep(warning_time).await;
        } else {
            tokio::time::sleep(session_duration).await;
        }
        let _ = timer_shutdown_tx.send(());
    });

    // Spawn Ctrl+C handler
    let (ctrlc_tx, mut ctrlc_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = ctrlc_tx.send(());
    });

    // Run ingest analysis
    let (_emitter, collected_events) = run_ingest_analysis(args, &mut timer_shutdown_rx, &mut ctrlc_rx).await?;

    Ok(collected_events)
}

/// Run a licensed session using ingestion framework (no time limits)
async fn run_licensed_ingest_session(args: &Args) -> Result<()> {
    // Setup Ctrl+C handler
    let (ctrlc_tx, mut ctrlc_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = ctrlc_tx.send(());
    });

    tracing::info!("Running ingest in licensed mode (no time limit)");
    tracing::info!("Press Ctrl+C to stop");

    // Create a dummy timer that never fires
    let (_never_tx, mut never_rx) = tokio::sync::oneshot::channel::<()>();

    // Run ingest analysis
    let (_emitter, _collected_events) = run_ingest_analysis(args, &mut never_rx, &mut ctrlc_rx).await?;

    tracing::info!("Session ended");
    Ok(())
}

/// Create the health monitoring manifest with configured thresholds
fn create_health_manifest(args: &Args) -> Manifest {
    // Parse the embedded YAML and override node params with CLI args
    let base_yaml = PIPELINE_YAML;
    let mut manifest: Manifest = serde_yaml::from_str(base_yaml)
        .unwrap_or_else(|_| {
            // Fallback to creating a minimal manifest
            Manifest {
                version: "v1".to_string(),
                metadata: remotemedia_runtime_core::manifest::ManifestMetadata {
                    name: "stream-health".to_string(),
                    description: Some("Stream health monitoring".to_string()),
                    created_at: None,
                    auto_negotiate: false,
                },
                nodes: vec![],
                connections: vec![],
            }
        });

    // Override or add HealthEmitterNode with CLI thresholds
    let health_node_params = serde_json::json!({
        "lead_threshold_ms": args.lead_threshold,
        "freeze_threshold_ms": args.freeze_threshold,
        "health_emit_interval_ms": args.health_interval,
    });

    // Find and update existing HealthEmitterNode, or add one
    let mut found = false;
    for node in &mut manifest.nodes {
        if node.node_type == "HealthEmitterNode" {
            node.params = health_node_params.clone();
            found = true;
            break;
        }
    }

    if !found {
        manifest.nodes.push(NodeManifest {
            id: "health".to_string(),
            node_type: "HealthEmitterNode".to_string(),
            params: health_node_params,
            is_streaming: true,
            ..Default::default()
        });
    }

    manifest
}

/// Read audio input based on source type (file, stdin, named pipe)
async fn read_audio_input(args: &Args) -> Result<AudioInput> {
    let input_path = args.input.as_ref().unwrap();
    let source = detect_input_source(input_path)
        .map_err(|e| anyhow::anyhow!("Invalid input '{}': {}", input_path, e))?;

    tracing::info!("Reading from: {:?}", source);

    match &source {
        InputSource::Stdin | InputSource::Pipe(_) if args.stream => {
            // Streaming mode - return reader for chunk-by-chunk processing
            let reader = InputReader::open(source.clone()).await
                .map_err(|e| anyhow::anyhow!("Failed to open input: {}", e))?;
            Ok(AudioInput::Streaming {
                reader,
                sample_rate: args.sample_rate,
                channels: args.channels,
            })
        }
        _ => {
            // Unary mode - read all data at once
            let mut reader = InputReader::open(source.clone()).await
                .map_err(|e| anyhow::anyhow!("Failed to open input: {}", e))?;

            let data = reader.read_to_end().await
                .map_err(|e| anyhow::anyhow!("Failed to read input: {}", e))?;

            tracing::debug!("Read {} bytes", data.len());

            // Decode audio
            let (samples, sample_rate, channels): (Vec<f32>, u32, u32) = if is_wav(&data) {
                tracing::info!("Detected WAV audio");
                let (s, sr, ch) = remotemedia_cli::audio::parse_wav(&data).context("Failed to parse WAV")?;
                (s, sr, ch as u32)
            } else if let InputSource::File(path) = &source {
                // Try FFmpeg for other file formats
                tracing::info!("Decoding with FFmpeg");
                ffmpeg::decode_audio_file(path)?
            } else {
                // Assume raw PCM f32 for stdin/pipe in non-streaming mode
                let samples: Vec<f32> = data.chunks(4)
                    .filter_map(|chunk| {
                        if chunk.len() == 4 {
                            Some(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        } else {
                            None
                        }
                    })
                    .collect();
                (samples, args.sample_rate, args.channels)
            };

            Ok(AudioInput::Unary {
                samples,
                sample_rate,
                channels: channels as u32,
            })
        }
    }
}

/// Audio input types for different processing modes
enum AudioInput {
    /// Complete audio loaded in memory
    Unary {
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u32,
    },
    /// Streaming input reader
    Streaming {
        reader: InputReader,
        sample_rate: u32,
        channels: u32,
    },
}

/// Run the health analysis pipeline
async fn run_health_analysis(
    args: &Args,
    timer_shutdown: &mut tokio::sync::oneshot::Receiver<()>,
    ctrlc_shutdown: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<(events::EventEmitter, Vec<events::HealthEvent>)> {
    // Setup output writer
    let writer: Box<dyn std::io::Write + Send> = if args.output == "-" {
        Box::new(std::io::stdout())
    } else {
        Box::new(std::fs::File::create(&args.output)
            .context("Failed to create output file")?)
    };
    let mut emitter = events::EventEmitter::new(writer);

    // Create pipeline runner
    let runner = pipeline::create_runner_with_cli_nodes().await?;
    let manifest = Arc::new(create_health_manifest(args));

    // Read input
    let audio_input = read_audio_input(args).await?;

    match audio_input {
        AudioInput::Unary { samples, sample_rate, channels } => {
            run_unary_analysis(
                &runner,
                manifest,
                samples,
                sample_rate,
                channels,
                args.chunk_size,
                &mut emitter,
                timer_shutdown,
                ctrlc_shutdown,
            ).await?;
        }
        AudioInput::Streaming { mut reader, sample_rate, channels } => {
            run_streaming_analysis(
                &runner,
                manifest,
                &mut reader,
                sample_rate,
                channels,
                args.chunk_size,
                &mut emitter,
                timer_shutdown,
                ctrlc_shutdown,
            ).await?;
        }
    }

    let collected = emitter.into_events();
    let new_emitter = if args.output == "-" {
        events::EventEmitter::stdout()
    } else {
        events::EventEmitter::new(Box::new(std::io::stdout()))
    };

    Ok((new_emitter, collected))
}

/// Run health analysis in unary mode (complete audio in memory)
async fn run_unary_analysis(
    runner: &remotemedia_runtime_core::transport::PipelineExecutor,
    manifest: Arc<Manifest>,
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u32,
    chunk_size: usize,
    emitter: &mut events::EventEmitter,
    timer_shutdown: &mut tokio::sync::oneshot::Receiver<()>,
    ctrlc_shutdown: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    // Create streaming session
    let mut session = pipeline::StreamingSession::new(runner, manifest)
        .await
        .context("Failed to create streaming session")?;

    // Track timing
    let start_time = Instant::now();
    let mut sample_offset: u64 = 0;

    // Process audio in chunks
    for chunk in samples.chunks(chunk_size) {
        // Check for shutdown
        if timer_shutdown.try_recv().is_ok() {
            tracing::info!("Demo session timeout");
            break;
        }
        if ctrlc_shutdown.try_recv().is_ok() {
            tracing::info!("Received Ctrl+C");
            break;
        }

        // Calculate timestamps
        let timestamp_us = (sample_offset * 1_000_000) / sample_rate as u64;
        let arrival_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        // Create audio data
        let audio = RuntimeData::Audio {
            samples: chunk.to_vec(),
            sample_rate,
            channels,
            stream_id: Some("audio".to_string()),
            timestamp_us: Some(timestamp_us),
            arrival_ts_us: Some(arrival_us),
        };

        // Send to pipeline
        if let Err(e) = session.send(audio).await {
            tracing::error!("Failed to send audio to pipeline: {}", e);
            break;
        }

        // Process available outputs (non-blocking)
        while let Ok(Some(output)) = session.try_recv() {
            process_health_output(output, emitter)?;
        }

        // Small yield to allow output processing
        tokio::task::yield_now().await;

        sample_offset += chunk.len() as u64;
    }

    // Drain remaining outputs with a timeout
    let drain_timeout = std::time::Duration::from_millis(500);
    while let Ok(Some(output)) = session.recv_timeout(drain_timeout).await {
        process_health_output(output, emitter)?;
    }

    // Close session
    if let Err(e) = session.close().await {
        tracing::warn!("Error closing session: {}", e);
    }

    tracing::info!("Processed {} samples in {:?}", sample_offset, start_time.elapsed());
    Ok(())
}

/// Run health analysis in streaming mode (continuous input)
async fn run_streaming_analysis(
    runner: &remotemedia_runtime_core::transport::PipelineExecutor,
    manifest: Arc<Manifest>,
    reader: &mut InputReader,
    sample_rate: u32,
    channels: u32,
    chunk_size: usize,
    emitter: &mut events::EventEmitter,
    timer_shutdown: &mut tokio::sync::oneshot::Receiver<()>,
    ctrlc_shutdown: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    // Create streaming session
    let mut session = pipeline::StreamingSession::new(runner, manifest)
        .await
        .context("Failed to create streaming session")?;

    // Buffer for accumulating samples
    let mut sample_buffer: Vec<f32> = Vec::with_capacity(chunk_size * 2);
    let mut byte_buffer = vec![0u8; chunk_size * 4]; // 4 bytes per f32 sample
    let mut sample_offset: u64 = 0;
    let start_time = Instant::now();

    tracing::info!("Streaming mode started (press Ctrl+C to stop)");

    loop {
        tokio::select! {
            biased;

            _ = &mut *timer_shutdown => {
                tracing::info!("Demo session timeout");
                break;
            }

            _ = &mut *ctrlc_shutdown => {
                tracing::info!("Received Ctrl+C");
                break;
            }

            // Read from input
            read_result = reader.read(&mut byte_buffer) => {
                match read_result {
                    Ok(0) => {
                        // EOF
                        tracing::info!("End of input stream");
                        break;
                    }
                    Ok(n) => {
                        // Convert bytes to f32 samples
                        for chunk in byte_buffer[..n].chunks(4) {
                            if chunk.len() == 4 {
                                let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                                sample_buffer.push(sample);
                            }
                        }

                        // Process when we have enough samples
                        while sample_buffer.len() >= chunk_size {
                            let chunk: Vec<f32> = sample_buffer.drain(..chunk_size).collect();

                            // Calculate timestamps
                            let timestamp_us = (sample_offset * 1_000_000) / sample_rate as u64;
                            let arrival_us = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_micros() as u64)
                                .unwrap_or(0);

                            // Create audio data
                            let audio = RuntimeData::Audio {
                                samples: chunk,
                                sample_rate,
                                channels,
                                stream_id: Some("audio".to_string()),
                                timestamp_us: Some(timestamp_us),
                                arrival_ts_us: Some(arrival_us),
                            };

                            // Send to pipeline
                            if let Err(e) = session.send(audio).await {
                                tracing::error!("Failed to send audio to pipeline: {}", e);
                                break;
                            }

                            // Process outputs
                            while let Ok(Some(output)) = session.recv().await {
                                process_health_output(output, emitter)?;
                            }

                            sample_offset += chunk_size as u64;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Read error: {}", e);
                        break;
                    }
                }
            }
        }
    }

    // Process remaining samples in buffer
    if !sample_buffer.is_empty() {
        let timestamp_us = (sample_offset * 1_000_000) / sample_rate as u64;
        let arrival_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        let audio = RuntimeData::Audio {
            samples: sample_buffer,
            sample_rate,
            channels,
            stream_id: Some("audio".to_string()),
            timestamp_us: Some(timestamp_us),
            arrival_ts_us: Some(arrival_us),
        };

        let _ = session.send(audio).await;
    }

    // Drain remaining outputs
    while let Ok(Some(output)) = session.recv().await {
        process_health_output(output, emitter)?;
    }

    // Close session
    if let Err(e) = session.close().await {
        tracing::warn!("Error closing session: {}", e);
    }

    tracing::info!("Processed {} samples in {:?}", sample_offset, start_time.elapsed());
    Ok(())
}

/// Process health output from the pipeline and emit events
fn process_health_output(output: RuntimeData, emitter: &mut events::EventEmitter) -> Result<()> {
    match output {
        RuntimeData::Json(json) => {
            tracing::debug!("Received JSON output: {}", json);
            // Convert JSON health events to HealthEvent enum
            if let Some(events) = convert_json_to_health_events(&json) {
                for event in events {
                    tracing::debug!("Emitting event: {:?}", event);
                    if let Err(e) = emitter.emit(event) {
                        tracing::warn!("Failed to emit event: {}", e);
                    }
                }
            } else {
                tracing::debug!("No events converted from JSON");
            }
        }
        _ => {
            // Ignore non-JSON outputs (e.g., audio passthrough)
            tracing::trace!("Ignoring non-JSON output");
        }
    }
    Ok(())
}

// ============================================================================
// Ingestion Framework Mode
// ============================================================================

/// Run health analysis using the pluggable ingestion framework
async fn run_ingest_analysis(
    args: &Args,
    timer_shutdown: &mut tokio::sync::oneshot::Receiver<()>,
    ctrlc_shutdown: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<(events::EventEmitter, Vec<events::HealthEvent>)> {
    let ingest_uri = args.ingest.as_ref().expect("--ingest should be set");

    // Setup output writer
    let writer: Box<dyn std::io::Write + Send> = if args.output == "-" {
        Box::new(std::io::stdout())
    } else {
        Box::new(std::fs::File::create(&args.output)
            .context("Failed to create output file")?)
    };
    let mut emitter = events::EventEmitter::new(writer);

    // Create ingest configuration with the URI
    let config = IngestConfig::from_url(ingest_uri)
        .with_audio(AudioConfig {
            sample_rate: args.sample_rate,
            channels: args.channels as u16,
        });

    // Get the global registry and create the ingest source
    let registry = global_ingest_registry();

    let mut source = registry.create_from_uri(&config)
        .map_err(|e| anyhow::anyhow!("Failed to create ingest source: {}", e))?;

    // Start the ingest
    let mut stream = source.start().await
        .map_err(|e| anyhow::anyhow!("Failed to start ingest: {}", e))?;

    tracing::info!("Ingesting from: {}", ingest_uri);
    tracing::info!("Metadata: {:?}", stream.metadata());

    // Create pipeline runner
    let runner = pipeline::create_runner_with_cli_nodes().await?;
    let manifest = Arc::new(create_health_manifest(args));

    // Create streaming session
    let mut session = pipeline::StreamingSession::new(&runner, manifest)
        .await
        .context("Failed to create streaming session")?;

    let start_time = Instant::now();
    let mut chunk_count: u64 = 0;

    tracing::info!("Ingest mode started (press Ctrl+C to stop)");

    // Main ingest loop
    loop {
        tokio::select! {
            biased;

            _ = &mut *timer_shutdown => {
                tracing::info!("Demo session timeout");
                break;
            }

            _ = &mut *ctrlc_shutdown => {
                tracing::info!("Received Ctrl+C");
                break;
            }

            // Receive from ingest stream
            data = stream.recv() => {
                match data {
                    Some(runtime_data) => {
                        // Only process audio data
                        if let RuntimeData::Audio { .. } = &runtime_data {
                            // Send to pipeline
                            if let Err(e) = session.send(runtime_data).await {
                                tracing::error!("Failed to send to pipeline: {}", e);
                                break;
                            }

                            // Process available outputs
                            while let Ok(Some(output)) = session.try_recv() {
                                process_health_output(output, &mut emitter)?;
                            }

                            chunk_count += 1;
                        } else {
                            // Log non-audio data (e.g., video)
                            tracing::trace!("Skipping non-audio data: {:?}", std::mem::discriminant(&runtime_data));
                        }
                    }
                    None => {
                        // End of stream
                        tracing::info!("End of ingest stream");
                        break;
                    }
                }
            }
        }

        // Check ingest source status
        if source.status() == IngestStatus::Disconnected {
            tracing::warn!("Ingest source disconnected");
            break;
        }
    }

    // Stop the ingest source
    if let Err(e) = source.stop().await {
        tracing::warn!("Error stopping ingest source: {}", e);
    }

    // Drain remaining pipeline outputs
    let drain_timeout = std::time::Duration::from_millis(500);
    while let Ok(Some(output)) = session.recv_timeout(drain_timeout).await {
        process_health_output(output, &mut emitter)?;
    }

    // Close session
    if let Err(e) = session.close().await {
        tracing::warn!("Error closing session: {}", e);
    }

    tracing::info!("Processed {} chunks in {:?}", chunk_count, start_time.elapsed());

    let collected = emitter.into_events();
    let new_emitter = if args.output == "-" {
        events::EventEmitter::stdout()
    } else {
        events::EventEmitter::new(Box::new(std::io::stdout()))
    };

    Ok((new_emitter, collected))
}
