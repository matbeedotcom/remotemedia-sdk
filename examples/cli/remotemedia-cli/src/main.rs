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
//! ```

mod audio;
mod commands;
mod config;
mod output;
mod transports;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use commands::{nodes, remote, run, serve, servers, stream, validate};

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

    #[command(subcommand)]
    command: Commands,
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
        command: nodes::NodesCommand,
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

    // Execute command
    let result = match cli.command {
        Commands::Run(args) => run::execute(args, &config, cli.output_format).await,
        Commands::Stream(args) => stream::execute(args, &config, cli.output_format).await,
        Commands::Serve(args) => serve::execute(args, &config).await,
        Commands::Validate(args) => validate::execute(args, cli.output_format).await,
        Commands::Nodes { command } => nodes::execute(command, cli.output_format).await,
        Commands::Remote { command } => remote::execute(command, &config, cli.output_format).await,
        Commands::Servers { command } => {
            servers::execute(command, &config, cli.output_format).await
        }
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
