//! `remotemedia serve` command - Start a pipeline server

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use std::path::PathBuf;
use std::sync::Arc;

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
    let _manifest_content = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read manifest: {:?}", args.manifest))?;

    let bind_addr = format!("{}:{}", args.host, args.port);

    tracing::info!(
        "Starting {:?} server on {} with pipeline {:?}",
        args.transport,
        bind_addr,
        args.manifest
    );

    let executor = Arc::new(
        remotemedia_core::transport::PipelineExecutor::new()
            .map_err(|e| anyhow::anyhow!("Failed to create executor: {}", e))?,
    );

    match args.transport {
        Transport::Grpc => {
            #[cfg(feature = "grpc")]
            {
                let mut builder = remotemedia_grpc::GrpcServerBuilder::new()
                    .bind(&bind_addr)
                    .executor(executor);

                if let Some(token) = args.auth_token {
                    builder = builder.auth_tokens(vec![token]).require_auth(true);
                }

                builder
                    .build()
                    .map_err(|e| anyhow::anyhow!("{}", e))?
                    .run()
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
            }
            #[cfg(not(feature = "grpc"))]
            {
                anyhow::bail!(
                    "gRPC transport not available. Rebuild with: cargo build --features grpc"
                );
            }
        }
        Transport::Http => {
            #[cfg(feature = "http")]
            {
                remotemedia_http::HttpServerBuilder::new()
                    .bind(&bind_addr)
                    .executor(executor)
                    .build()
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?
                    .run()
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
            }
            #[cfg(not(feature = "http"))]
            {
                anyhow::bail!(
                    "HTTP transport not available. Rebuild with: cargo build --features http"
                );
            }
        }
        Transport::Webrtc => {
            #[cfg(feature = "webrtc")]
            {
                remotemedia_webrtc::WebRtcSignalingServerBuilder::new()
                    .bind(&bind_addr)
                    .manifest_from_file(&args.manifest)
                    .map_err(|e| anyhow::anyhow!("{}", e))?
                    .executor(executor)
                    .build()
                    .map_err(|e| anyhow::anyhow!("{}", e))?
                    .run()
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
            }
            #[cfg(not(feature = "webrtc"))]
            {
                anyhow::bail!(
                    "WebRTC transport not available. Rebuild with: cargo build --features webrtc"
                );
            }
        }
    }

    Ok(())
}
