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

use remotemedia_runtime::executor::Executor;
use remotemedia_runtime::grpc_service::{init_tracing, server::GrpcServer, ServiceConfig};
use std::sync::Arc;
use tracing::{error, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // T036: Configure multi-threaded tokio runtime for concurrent client support
    // Use all available CPU cores for handling concurrent requests
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name("remotemedia-worker")
        .enable_all()
        .build()?;

    runtime.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
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

    // Create executor for pipeline execution
    let executor = Arc::new(Executor::new());
    info!("Pipeline executor initialized");

    // Create and start server
    let server = GrpcServer::new(config, executor)?;

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
