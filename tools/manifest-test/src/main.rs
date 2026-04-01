//! remotemedia-test-manifest — Universal manifest testing CLI
//!
//! Usage:
//!   remotemedia-test-manifest <MANIFEST_PATH>
//!   remotemedia-test-manifest pipeline.yaml --dry-run
//!   remotemedia-test-manifest pipeline.json --skip-ml --output-format json

// Link node crates so inventory auto-registration activates
use remotemedia_candle_nodes as _;
use remotemedia_python_nodes as _;

use anyhow::Result;
use clap::Parser;
use remotemedia_manifest_tester::tester::ManifestTester;
use remotemedia_manifest_tester::probes::ProbeSpec;
use remotemedia_manifest_tester::TestStatus;
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Universal manifest testing for RemoteMedia SDK pipelines
#[derive(Parser)]
#[command(name = "remotemedia-test-manifest")]
#[command(about = "Test a pipeline manifest end-to-end")]
#[command(version)]
struct Cli {
    /// Path to the pipeline manifest (YAML or JSON)
    manifest: PathBuf,

    /// Transport probes to run
    #[arg(short, long, default_value = "direct")]
    transport: Vec<TransportArg>,

    /// Timeout per probe in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,

    /// Output format
    #[arg(short, long, default_value = "text")]
    output_format: OutputFormat,

    /// Show test plan without executing
    #[arg(long)]
    dry_run: bool,

    /// Skip nodes requiring ML models (replace with passthrough stubs)
    #[arg(long)]
    skip_ml: bool,

    /// Input data: a WAV file path for audio pipelines, or raw text for text pipelines
    #[arg(short, long)]
    input: Option<String>,

    /// Chunk size for streaming audio input (samples per chunk)
    #[arg(long, default_value = "1024")]
    chunk_size: usize,

    /// Write pipeline audio output to file (raw f32le PCM) or "-" for stdout
    #[arg(long)]
    output: Option<String>,

    /// Increase verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum TransportArg {
    Direct,
    Grpc,
    Webrtc,
    Http,
    All,
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn transport_args_to_specs(args: &[TransportArg]) -> Vec<ProbeSpec> {
    let mut specs = Vec::new();
    for arg in args {
        match arg {
            TransportArg::Direct => specs.push(ProbeSpec::Direct),
            TransportArg::Grpc => specs.push(ProbeSpec::Grpc { port: None }),
            TransportArg::Webrtc => specs.push(ProbeSpec::WebRtc { signal_port: None }),
            TransportArg::Http => specs.push(ProbeSpec::Http { port: None }),
            TransportArg::All => {
                specs.push(ProbeSpec::Direct);
                specs.push(ProbeSpec::Grpc { port: None });
                specs.push(ProbeSpec::WebRtc { signal_port: None });
                specs.push(ProbeSpec::Http { port: None });
            }
        }
    }
    if specs.is_empty() {
        specs.push(ProbeSpec::Direct);
    }
    specs
}

fn main() -> Result<()> {
    // Use a manually-built runtime so that when main() returns, the runtime
    // is dropped first (cleaning up async tasks) and then all remaining Rust
    // destructors run — including ProcessManager::drop which kills child
    // processes.  The previous `#[tokio::main]` + `std::process::exit()`
    // approach skipped all destructors, leaving Python children orphaned.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let code = rt.block_on(async_main())?;
    // Drop the runtime explicitly so spawned tasks are cancelled.
    drop(rt);
    std::process::exit(code);
}

async fn async_main() -> Result<i32> {
    let cli = Cli::parse();

    // Setup logging
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| log_level.into()))
        .init();

    // Load custom test data if provided
    let custom_data = if let Some(input) = &cli.input {
        use remotemedia_core::data::RuntimeData;
        use remotemedia_manifest_tester::synthetic_data::SyntheticDataFactory;

        let path = std::path::Path::new(input);
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("wav")) && path.exists() {
            // WAV file → chunked audio
            let chunks = SyntheticDataFactory::load_wav_chunked(path, cli.chunk_size)
                .map_err(|e| anyhow::anyhow!("Failed to load input audio: {e}"))?;
            Some(chunks)
        } else {
            // Raw text input
            Some(vec![RuntimeData::Text(input.clone())])
        }
    } else {
        None
    };

    // Build and run tester
    let specs = transport_args_to_specs(&cli.transport);
    let output_collector = cli.output.as_ref().map(|_| {
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<remotemedia_core::data::RuntimeData>::new()))
    });

    let mut tester = ManifestTester::test(&cli.manifest)
        .with_probes(&specs)
        .with_timeout(Duration::from_secs(cli.timeout))
        .skip_ml(cli.skip_ml)
        .dry_run(cli.dry_run);

    if let Some(data) = custom_data {
        tester = tester.with_test_data(data);
    }
    if let Some(ref collector) = output_collector {
        tester = tester.collect_outputs(collector.clone());
    }

    let report = tester.run().await;

    // Write collected output if requested
    if let (Some(output_path), Some(collector)) = (&cli.output, &output_collector) {
        use std::io::Write;
        let outputs = collector.lock().unwrap();

        // Separate text and audio outputs
        let mut all_text: Vec<String> = Vec::new();
        let mut all_samples: Vec<f32> = Vec::new();
        let mut sample_rate = 24000u32;
        for data in outputs.iter() {
            match data {
                remotemedia_core::data::RuntimeData::Text(text) => {
                    all_text.push(text.clone());
                }
                remotemedia_core::data::RuntimeData::Audio { samples, sample_rate: sr, .. } => {
                    all_samples.extend_from_slice(samples);
                    sample_rate = *sr;
                }
                _ => {}
            }
        }

        if !all_text.is_empty() {
            let joined = all_text.join("\n");
            if output_path == "-" {
                std::io::stdout().write_all(joined.as_bytes()).ok();
                std::io::stdout().write_all(b"\n").ok();
            } else {
                std::fs::write(output_path, &joined)
                    .map_err(|e| anyhow::anyhow!("Failed to write output: {e}"))?;
                eprintln!("Wrote {} text output(s) to {}", all_text.len(), output_path);
            }
        } else if !all_samples.is_empty() {
            let raw_bytes: Vec<u8> = all_samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            if output_path == "-" {
                std::io::stdout().write_all(&raw_bytes).ok();
            } else {
                std::fs::write(output_path, &raw_bytes)
                    .map_err(|e| anyhow::anyhow!("Failed to write output: {e}"))?;
                eprintln!(
                    "Wrote {} samples ({:.2}s) of audio to {} (f32le, {}Hz, mono)",
                    all_samples.len(),
                    all_samples.len() as f32 / sample_rate as f32,
                    output_path,
                    sample_rate
                );
            }
        }
    }

    // Output results (use stderr when piping audio to stdout)
    let use_stderr = cli.output.as_deref() == Some("-");
    match cli.output_format {
        OutputFormat::Text => {
            if use_stderr { eprintln!("{report}"); } else { println!("{report}"); }
        }
        OutputFormat::Json => {
            if use_stderr { eprintln!("{}", report.to_json()); } else { println!("{}", report.to_json()); }
        }
    }

    // Exit code
    let code = match report.overall_status {
        TestStatus::Pass => 0,
        TestStatus::Fail => 1,
        TestStatus::Partial => 1,
        TestStatus::Skipped => {
            if report.errors.is_empty() {
                3 // All skipped (prerequisites or dry-run)
            } else {
                2 // Parse/validation error
            }
        }
    };

    Ok(code)
}
