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
    let mut executor = Executor::new();
    
    // Register built-in test nodes (PassThrough, Echo, Calculator, Add, Multiply)
    let builtin_registry = remotemedia_runtime::nodes::create_builtin_registry();
    executor.add_system_registry(Arc::new(builtin_registry));
    info!("Built-in test nodes registered (PassThrough, Echo, CalculatorNode, AddNode, MultiplyNode)");
    
    // Register audio processing nodes (resample, VAD, format converter)
    let audio_registry = remotemedia_runtime::nodes::audio::create_audio_registry();
    executor.add_audio_registry(Arc::new(audio_registry));
    info!("Audio processing nodes registered (RustResampleNode, RustVADNode, RustFormatConverterNode)");
    
    // Get all node types from all registries for version info
    let node_types = executor.list_all_node_types();
    info!(
        node_count = node_types.len(),
        nodes = ?node_types,
        "Available node types"
    );
    
    // Update config to include node types
    let mut config = config;
    config.version = remotemedia_runtime::grpc_service::version::VersionManager::from_node_types(node_types);
    
    let executor = Arc::new(executor);
    info!("Pipeline executor initialized with all nodes");

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
