//! `remotemedia remote stream` - Stream to/from remote server

use anyhow::Result;
use clap::Args;

use crate::config::Config;
use crate::output::OutputFormat;

#[derive(Args)]
pub struct RemoteStreamArgs {
    /// Server URL
    #[arg(long, env = "REMOTEMEDIA_DEFAULT_SERVER")]
    pub server: Option<String>,

    /// Named pipeline on server
    #[arg(long)]
    pub pipeline: String,

    /// Use microphone
    #[arg(long)]
    pub mic: bool,

    /// Use speaker
    #[arg(long)]
    pub speaker: bool,
}

pub async fn execute(args: RemoteStreamArgs, config: &Config, _format: OutputFormat) -> Result<()> {
    // Determine server
    let server = args
        .server
        .or_else(|| config.default_server.clone())
        .ok_or_else(|| anyhow::anyhow!("No server specified. Use --server or set a default server."))?;

    // Validate URL scheme
    if !server.starts_with("grpc://")
        && !server.starts_with("http://")
        && !server.starts_with("https://")
        && !server.starts_with("ws://")
        && !server.starts_with("wss://")
    {
        anyhow::bail!("Invalid server URL scheme");
    }

    tracing::info!(
        "Streaming to remote server: {} pipeline: {}",
        server,
        args.pipeline
    );

    if args.mic {
        tracing::info!("Microphone input enabled");
    }

    if args.speaker {
        tracing::info!("Speaker output enabled");
    }

    // TODO: Connect to remote server and stream
    // For now, simulate a connection attempt

    eprintln!("Connection to {} failed: server not available", server);
    std::process::exit(1);
}
