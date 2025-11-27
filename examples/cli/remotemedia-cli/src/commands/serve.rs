//! `remotemedia serve` command - Start a pipeline server

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use std::path::PathBuf;

use crate::config::Config;

/// Transport type for the server
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Transport {
    /// gRPC transport (default)
    Grpc,
    /// HTTP REST transport
    Http,
    /// WebRTC transport
    Webrtc,
}

/// Arguments for the serve command
#[derive(Args)]
pub struct ServeArgs {
    /// Path to pipeline manifest (YAML or JSON)
    pub manifest: PathBuf,

    /// Server port
    #[arg(long, default_value = "8080")]
    pub port: u16,

    /// Bind address
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Transport type
    #[arg(long, default_value = "grpc")]
    pub transport: Transport,

    /// Required authentication token
    #[arg(long)]
    pub auth_token: Option<String>,

    /// Maximum concurrent sessions
    #[arg(long, default_value = "100")]
    pub max_sessions: u32,
}

pub async fn execute(args: ServeArgs, _config: &Config) -> Result<()> {
    // Load and validate manifest
    let manifest_content = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read manifest: {:?}", args.manifest))?;

    let _manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Invalid manifest: {}", e))?;

    tracing::info!(
        "Starting {:?} server on {}:{} with pipeline {:?}",
        args.transport,
        args.host,
        args.port,
        args.manifest
    );

    // Attempt to bind to the port
    let addr = format!("{}:{}", args.host, args.port);
    let listener = match std::net::TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to {}: {}", addr, e);
            std::process::exit(2); // Exit code 2 = port in use
        }
    };
    drop(listener); // Release for the actual server

    tracing::info!("Max sessions: {}", args.max_sessions);

    if args.auth_token.is_some() {
        tracing::info!("Authentication enabled");
    }

    // Set up shutdown handler
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Received shutdown signal");
        let _ = shutdown_tx.send(());
    });

    // Start the appropriate server
    match args.transport {
        Transport::Grpc => {
            tracing::info!("Starting gRPC server...");
            // TODO: Use remotemedia-grpc transport
        }
        Transport::Http => {
            tracing::info!("Starting HTTP server...");
            // TODO: Use remotemedia-http transport
        }
        Transport::Webrtc => {
            tracing::info!("Starting WebRTC server...");
            // TODO: Use remotemedia-webrtc transport
        }
    }

    // Wait for shutdown
    shutdown_rx.await.ok();
    tracing::info!("Server shutting down gracefully");

    Ok(())
}
