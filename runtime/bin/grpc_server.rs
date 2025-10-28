//! gRPC server binary entry point
//!
//! Starts the RemoteMedia gRPC service for high-performance remote pipeline execution.
//!
//! # Usage
//!
//! ```bash
//! # Start with defaults (localhost:50051, auth required)
//! cargo run --bin grpc_server --features grpc-transport
//!
//! # Start with custom address
//! GRPC_BIND_ADDRESS="0.0.0.0:50051" cargo run --bin grpc_server --features grpc-transport
//!
//! # Start without authentication (dev mode)
//! GRPC_REQUIRE_AUTH=false cargo run --bin grpc_server --features grpc-transport
//!
//! # Start with API tokens
//! GRPC_AUTH_TOKENS="token1,token2" cargo run --bin grpc_server --features grpc-transport
//!
//! # Configure resource limits
//! GRPC_MAX_MEMORY_MB=200 GRPC_MAX_TIMEOUT_SEC=10 cargo run --bin grpc_server --features grpc-transport
//! ```
//!
//! # Environment Variables
//!
//! - `GRPC_BIND_ADDRESS`: Server bind address (default: `[::1]:50051`)
//! - `GRPC_AUTH_TOKENS`: Comma-separated list of valid API tokens
//! - `GRPC_REQUIRE_AUTH`: Enable/disable authentication (default: `true`)
//! - `GRPC_MAX_MEMORY_MB`: Maximum memory per execution in MB (default: `100`)
//! - `GRPC_MAX_TIMEOUT_SEC`: Maximum execution timeout in seconds (default: `5`)
//! - `GRPC_JSON_LOGGING`: Enable JSON structured logging (default: `true`)
//! - `RUST_LOG`: Logging level (default: `info`, options: `trace`, `debug`, `info`, `warn`, `error`)

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::{init_tracing, server::GrpcServer, ServiceConfig};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from environment
    let config = ServiceConfig::from_env();

    // Initialize tracing (logging)
    init_tracing(config.json_logging);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        protocol = "v1",
        "RemoteMedia gRPC Server starting"
    );

    // Log configuration
    info!(
        bind_address = %config.bind_address,
        auth_required = config.auth.require_auth,
        max_memory_mb = config.limits.max_memory_bytes / 1_000_000,
        max_timeout_sec = config.limits.max_timeout.as_secs(),
        json_logging = config.json_logging,
        "Configuration loaded"
    );

    // Create and start server
    let server = GrpcServer::new(config)?;

    info!("Server initialized, starting listener...");

    // Run server (will block until shutdown signal)
    match server.serve().await {
        Ok(_) => {
            info!("Server shut down gracefully");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Server error");
            Err(e)
        }
    }
}
