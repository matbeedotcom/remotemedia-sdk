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

    /// Enable embedded web UI
    #[arg(long)]
    pub ui: bool,

    /// Web UI port (when --ui is enabled)
    #[arg(long, default_value = "3001")]
    pub ui_port: u16,
}

pub async fn execute(args: ServeArgs, _config: &Config) -> Result<()> {
    // Load and validate manifest
    let manifest_content = std::fs::read_to_string(&args.manifest)
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

    // Start embedded web UI if requested
    #[cfg(feature = "ui")]
    if args.ui {
        let ui_bind = format!("{}:{}", args.host, args.ui_port);
        let transport_type = format!("{:?}", args.transport).to_lowercase();
        let address = format!("{}:{}", args.host, args.port);

        // Parse manifest for the UI
        let manifest: remotemedia_core::manifest::Manifest =
            match args.manifest.extension().and_then(|e| e.to_str()) {
                Some("yaml" | "yml") => serde_json::from_value(
                    serde_yaml::from_str::<serde_json::Value>(&manifest_content)?,
                )?,
                _ => serde_json::from_str(&manifest_content)?,
            };

        let ui_executor = executor.clone();
        tokio::spawn(async move {
            if let Err(e) = remotemedia_ui::UiServerBuilder::new()
                .bind(&ui_bind)
                .executor(ui_executor)
                .manifest(Arc::new(manifest))
                .transport_info(remotemedia_ui::TransportInfo {
                    transport_type,
                    address,
                })
                .build()
                .expect("Failed to build UI server")
                .run()
                .await
            {
                tracing::error!("UI server error: {}", e);
            }
        });
        tracing::info!("Web UI available at http://{}:{}", args.host, args.ui_port);
    }
    #[cfg(not(feature = "ui"))]
    if args.ui {
        anyhow::bail!("Web UI not available. Rebuild with: cargo build --features ui");
    }

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
