//! RemoteMedia CLI - Command-line interface for pipeline execution
//!
//! # Examples
//!
//! ```bash
//! # Run a pipeline
//! remotemedia run pipeline.yaml --input audio.wav --output result.json
//!
//! # Stream with microphone
//! remotemedia stream voice-assistant.yaml --mic --speaker
//!
//! # Start a server
//! remotemedia serve pipeline.yaml --port 50051 --transport grpc
//!
//! # Use named pipes for Unix pipeline integration
//! mkfifo /tmp/audio_in
//! remotemedia stream pipeline.yaml --input /tmp/audio_in
//!
//! # Use stdin/stdout shorthand
//! cat audio.wav | remotemedia run pipeline.yaml --input - --output -
//! ```

// Force-link node crates so inventory auto-registers node providers
#[cfg(feature = "candle")]
use remotemedia_candle_nodes as _;
#[cfg(feature = "python-nodes")]
use remotemedia_python_nodes as _;

mod audio;
mod commands;
mod config;
pub mod io;
pub mod pipeline;
pub mod pipeline_nodes;
mod output;
mod transports;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use commands::{models, nodes as nodes_cmd, pack, remote, run, serve, servers, stream, validate};

#[cfg(feature = "ui")]
use commands::ui;

/// RemoteMedia SDK command-line interface
#[derive(Parser)]
#[command(name = "remotemedia")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Increase verbosity (can be used multiple times)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress non-error output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Config file path
    #[arg(short, long, global = true, env = "REMOTEMEDIA_CONFIG")]
    config: Option<String>,

    /// Output format
    #[arg(short = 'o', long, global = true, default_value = "text")]
    output_format: output::OutputFormat,

    /// Python environment mode for multiprocess nodes.
    /// "system" uses the system Python (default).
    /// "managed" auto-creates venvs with dependencies from the manifest.
    /// "managed_with_python" also auto-downloads Python if not found.
    #[arg(long, global = true, env = "PYTHON_ENV_MODE", default_value = "system")]
    python_env: PythonEnvArg,

    /// Python version for managed environments (e.g. "3.11", "3.12")
    #[arg(long, global = true, env = "PYTHON_VERSION")]
    python_version: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum PythonEnvArg {
    /// Use system Python as-is (default, no env management)
    System,
    /// Auto-create venvs with node dependencies via uv or pip
    Managed,
    /// Same as managed, but also auto-downloads Python if not found
    ManagedWithPython,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a pipeline once (unary mode)
    Run(run::RunArgs),

    /// Execute a pipeline in streaming mode with real-time I/O
    Stream(stream::StreamArgs),

    /// Start a pipeline server
    Serve(serve::ServeArgs),

    /// Validate a pipeline manifest
    Validate(validate::ValidateArgs),

    /// Manage available node types
    Nodes {
        #[command(subcommand)]
        command: nodes_cmd::NodesCommand,
    },

    /// Execute pipeline on remote server
    Remote {
        #[command(subcommand)]
        command: remote::RemoteCommand,
    },

    /// Manage saved remote servers
    Servers {
        #[command(subcommand)]
        command: servers::ServersCommand,
    },

    /// Manage Candle ML model cache
    Models(models::ModelsArgs),

    /// Pack a pipeline into a self-contained Python wheel
    Pack(pack::PackArgs),

    /// Launch the web UI for pipeline interaction
    #[cfg(feature = "ui")]
    Ui(ui::UiArgs),
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

    let filter = if cli.quiet { "error" } else { log_level };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .init();

    // Load config
    let config = config::load_config(cli.config.as_deref())?;

    // Set Python env mode so MultiprocessConfig picks it up
    let env_mode = match cli.python_env {
        PythonEnvArg::System => "system",
        PythonEnvArg::Managed => "managed",
        PythonEnvArg::ManagedWithPython => "managed_with_python",
    };
    std::env::set_var("PYTHON_ENV_MODE", env_mode);
    if let Some(ref ver) = cli.python_version {
        std::env::set_var("PYTHON_VERSION", ver);
    }

    if !matches!(cli.python_env, PythonEnvArg::System) {
        tracing::info!("Python environment mode: {}", env_mode);
    }

    // Execute command
    let result = match cli.command {
        Commands::Run(args) => run::execute(args, &config, cli.output_format).await,
        Commands::Stream(args) => stream::execute(args, &config, cli.output_format).await,
        Commands::Serve(args) => serve::execute(args, &config).await,
        Commands::Validate(args) => validate::execute(args, cli.output_format).await,
        Commands::Nodes { command } => nodes_cmd::execute(command, cli.output_format).await,
        Commands::Remote { command } => remote::execute(command, &config, cli.output_format).await,
        Commands::Servers { command } => {
            servers::execute(command, &config, cli.output_format).await
        }
        Commands::Models(args) => models::run(args).await,
        Commands::Pack(args) => pack::execute(args).await,
        #[cfg(feature = "ui")]
        Commands::Ui(args) => ui::execute(args, &config).await,
    };

    // Handle exit codes
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            if !cli.quiet {
                eprintln!("Error: {}", e);
            }
            std::process::exit(1);
        }
    }
}
