//! HTTP server binary entry point
//!
//! Starts the RemoteMedia HTTP/REST service with SSE streaming support.
//!
//! # Usage
//!
//! ```bash
//! # Start with defaults (localhost:8080)
//! cargo run -p remotemedia-http-server
//!
//! # Start with custom address
//! HTTP_BIND_ADDRESS="0.0.0.0:8080" cargo run -p remotemedia-http-server
//!
//! # With logging
//! RUST_LOG=debug cargo run -p remotemedia-http-server
//! ```
//!
//! # Environment Variables
//!
//! - `HTTP_BIND_ADDRESS`: Server bind address (default: `127.0.0.1:8080`)
//! - `RUST_LOG`: Logging level (default: `info`, options: `trace`, `debug`, `info`, `warn`, `error`)

use remotemedia_http::HttpServer;
use remotemedia_core::transport::PipelineExecutor;
use std::sync::Arc;
use tracing::{error, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Load configuration from environment
    let bind_address =
        std::env::var("HTTP_BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

    info!(
        version = env!("CARGO_PKG_VERSION"),
        bind_address = %bind_address,
        "RemoteMedia HTTP Server starting"
    );

    // Create tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name("remotemedia-http")
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        // Create pipeline executor (spec 026 migration)
        let executor = match PipelineExecutor::new() {
            Ok(executor) => Arc::new(executor),
            Err(e) => {
                error!("Failed to create pipeline executor: {}", e);
                return Err(Box::new(e) as Box<dyn std::error::Error>);
            }
        };

        // Create HTTP server
        let server = HttpServer::new(bind_address, executor).await.map_err(|e| {
            error!("Failed to create HTTP server: {}", e);
            e
        })?;

        info!("HTTP server ready - listening for connections");

        // Run server
        server.serve().await.map_err(|e| {
            error!("Server error: {}", e);
            e
        })?;

        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    info!("HTTP server shutdown complete");
    Ok(())
}
