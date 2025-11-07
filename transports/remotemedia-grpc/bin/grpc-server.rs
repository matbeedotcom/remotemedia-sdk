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

use remotemedia_grpc::{init_tracing, server::GrpcServer, ServiceConfig};
use remotemedia_runtime_core::transport::PipelineRunner;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up Ctrl+C handler at the very start
    // This ensures it's registered before any blocking operations
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_handler = Arc::clone(&shutdown_flag);

    ctrlc::set_handler(move || {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        eprintln!("\n🛑 [{}] Ctrl+C received! Setting shutdown flag...", timestamp);
        eprintln!("   [SIGNAL] Handler executing in thread: {:?}", std::thread::current().id());

        let was_already_set = shutdown_flag_handler.swap(true, Ordering::SeqCst);
        if was_already_set {
            eprintln!("   [SIGNAL] ⚠️  Shutdown already in progress, forcing immediate exit");
            std::process::exit(0);
        }

        eprintln!("   [SIGNAL] Shutdown flag set successfully");
        eprintln!("   [SIGNAL] Spawning watchdog thread for force-exit after 1s...");

        // Give it a brief moment, then force exit if graceful shutdown fails
        // Use 1 second timeout to ensure responsiveness even with blocking operations
        std::thread::spawn(move || {
            for i in 1..=10 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                eprintln!("   [WATCHDOG] Waiting for graceful shutdown... {}00ms", i);
            }
            eprintln!("⚠️  [WATCHDOG] Graceful shutdown timeout (1s), forcing exit");
            eprintln!("   [WATCHDOG] Likely blocked by: active gRPC requests, Python GIL, or node processing");
            std::process::exit(0);  // Exit with 0 for clean shutdown
        });
    })
    .expect("Failed to set Ctrl+C handler");

    // T036: Configure multi-threaded tokio runtime for concurrent client support
    // Use all available CPU cores for handling concurrent requests
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name("remotemedia-worker")
        .enable_all()
        .build()?;

    runtime.block_on(async_main(shutdown_flag))
}

async fn async_main(shutdown_flag: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
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

    // Create pipeline runner
    // PipelineRunner encapsulates the executor and all node registries
    let runner = PipelineRunner::new()?;
    info!("PipelineRunner initialized with all nodes");

    // TODO: Get node type information from PipelineRunner for logging
    // Currently PipelineRunner doesn't expose this information
    // The nodes are registered internally during PipelineRunner::new()

    let runner = Arc::new(runner);

    // Create and start server
    let server = GrpcServer::new(config, runner)?;

    info!("Server initialized, starting listener...");

    // Run server (will block until shutdown signal)
    match server.serve_with_shutdown_flag(shutdown_flag).await {
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
