//! remotemedia-test-manifest — Universal manifest testing CLI
//!
//! Usage:
//!   remotemedia-test-manifest <MANIFEST_PATH>
//!   remotemedia-test-manifest pipeline.yaml --dry-run
//!   remotemedia-test-manifest pipeline.json --skip-ml --output-format json

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

    /// Path to input audio WAV file (overrides synthetic data generation)
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Chunk size for streaming audio input (samples per chunk)
    #[arg(long, default_value = "1024")]
    chunk_size: usize,

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

#[tokio::main]
async fn main() -> Result<()> {
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
    let custom_data = if let Some(input_path) = &cli.input {
        use remotemedia_manifest_tester::synthetic_data::SyntheticDataFactory;
        let chunks = SyntheticDataFactory::load_wav_chunked(input_path, cli.chunk_size)
            .map_err(|e| anyhow::anyhow!("Failed to load input audio: {e}"))?;
        Some(chunks)
    } else {
        None
    };

    // Build and run tester
    let specs = transport_args_to_specs(&cli.transport);
    let mut tester = ManifestTester::test(&cli.manifest)
        .with_probes(&specs)
        .with_timeout(Duration::from_secs(cli.timeout))
        .skip_ml(cli.skip_ml)
        .dry_run(cli.dry_run);

    if let Some(data) = custom_data {
        tester = tester.with_test_data(data);
    }

    let report = tester.run().await;

    // Output results
    match cli.output_format {
        OutputFormat::Text => {
            println!("{report}");
        }
        OutputFormat::Json => {
            println!("{}", report.to_json());
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

    std::process::exit(code);
}
