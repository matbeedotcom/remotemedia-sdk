//! `remotemedia ui` command - Launch standalone web UI

use anyhow::Result;
use clap::Args;
use std::sync::Arc;

use crate::config::Config;

/// Arguments for the ui command
#[derive(Args)]
pub struct UiArgs {
    /// Remote server URL (grpc://, http://, ws://)
    #[arg(long)]
    pub server: Option<String>,

    /// UI server port
    #[arg(long, default_value = "3001")]
    pub port: u16,

    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
}

pub async fn execute(args: UiArgs, _config: &Config) -> Result<()> {
    let bind_addr = format!("{}:{}", args.host, args.port);

    // Create a local executor for pipeline execution
    let executor = Arc::new(
        remotemedia_core::transport::PipelineExecutor::new()
            .map_err(|e| anyhow::anyhow!("Failed to create executor: {}", e))?,
    );

    let mut builder = remotemedia_ui::UiServerBuilder::new()
        .bind(&bind_addr)
        .executor(executor);

    // If a remote server URL is provided, pass it as transport info
    if let Some(ref server_url) = args.server {
        let transport_type = if server_url.starts_with("grpc") {
            "grpc"
        } else if server_url.starts_with("ws") {
            "webrtc"
        } else {
            "http"
        };

        builder = builder.transport_info(remotemedia_ui::TransportInfo {
            transport_type: transport_type.to_string(),
            address: server_url.clone(),
        });
    }

    tracing::info!("Web UI available at http://{}", bind_addr);

    builder
        .build()
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .run()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}
