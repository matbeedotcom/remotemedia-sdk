//! WebRTC server binary entry point
//!
//! Starts the RemoteMedia WebRTC transport server for real-time media streaming.
//!
//! # Usage
//!
//! ```bash
//! # Start gRPC signaling server (default: 0.0.0.0:50051)
//! cargo run -p remotemedia-webrtc-server -- \
//!   --mode grpc \
//!   --grpc-address 0.0.0.0:50051 \
//!   --manifest ./examples/loopback.yaml
//!
//! # Start WebSocket client mode (connects to signaling server)
//! cargo run -p remotemedia-webrtc-server -- \
//!   --mode websocket \
//!   --signaling-url ws://localhost:8080
//!
//! # Configure STUN/TURN servers
//! cargo run -p remotemedia-webrtc-server -- \
//!   --stun-servers stun:stun.l.google.com:19302 \
//!   --max-peers 20
//! ```

use clap::Parser;
use remotemedia_webrtc::{WebRtcTransport, WebRtcTransportConfig};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// RemoteMedia WebRTC Server
///
/// Real-time media streaming server with WebRTC transport.
/// Supports both gRPC signaling (server mode) and WebSocket signaling (client mode).
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server mode: 'grpc' for signaling server, 'websocket' for signaling client
    #[arg(short, long, default_value = "websocket", env = "WEBRTC_MODE")]
    mode: ServerMode,

    /// gRPC signaling server address (gRPC mode only)
    #[arg(long, default_value = "0.0.0.0:50051", env = "GRPC_SIGNALING_ADDRESS")]
    grpc_address: String,

    /// Pipeline manifest path (gRPC mode only)
    #[arg(
        long,
        default_value = "./examples/loopback.yaml",
        env = "WEBRTC_PIPELINE_MANIFEST"
    )]
    manifest: PathBuf,

    /// WebSocket signaling URL (WebSocket mode only)
    #[arg(
        long,
        default_value = "ws://localhost:8080",
        env = "WEBRTC_SIGNALING_URL"
    )]
    signaling_url: String,

    /// STUN servers (comma-separated)
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "stun:stun.l.google.com:19302"
    )]
    stun_servers: Vec<String>,

    /// Maximum concurrent peer connections
    #[arg(long, default_value_t = 10, env = "WEBRTC_MAX_PEERS")]
    max_peers: u32,

    /// Enable data channel support
    #[arg(long, default_value_t = true, env = "WEBRTC_ENABLE_DATA_CHANNEL")]
    enable_data_channel: bool,

    /// Jitter buffer size in milliseconds
    #[arg(long, default_value_t = 100, env = "WEBRTC_JITTER_BUFFER_MS")]
    jitter_buffer_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ServerMode {
    /// gRPC signaling server mode
    Grpc,
    /// WebSocket signaling client mode
    Websocket,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Set up Ctrl+C handler at the very start
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_handler = Arc::clone(&shutdown_flag);

    ctrlc::set_handler(move || {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        eprintln!(
            "\n🛑 [{}] Ctrl+C received! Initiating shutdown...",
            timestamp
        );

        let was_already_set = shutdown_flag_handler.swap(true, Ordering::SeqCst);
        if was_already_set {
            eprintln!("   [SIGNAL] ⚠️  Shutdown already in progress, forcing immediate exit");
            std::process::exit(0);
        }

        eprintln!("   [SIGNAL] Shutdown flag set successfully");

        // Give it a moment for graceful shutdown
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(3));
            eprintln!("⚠️  [WATCHDOG] Graceful shutdown timeout (3s), forcing exit");
            std::process::exit(0);
        });
    })
    .expect("Failed to set Ctrl+C handler");

    // Create multi-threaded tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name("webrtc-worker")
        .enable_all()
        .build()?;

    runtime.block_on(async_main(args, shutdown_flag))
}

async fn async_main(
    args: Args,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing (logging)
    init_tracing();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        mode = ?args.mode,
        "RemoteMedia WebRTC Server starting"
    );

    match args.mode {
        ServerMode::Grpc => {
            info!("Starting in gRPC signaling server mode");
            run_grpc_signaling_server(args, shutdown_flag).await?;
        }
        ServerMode::Websocket => {
            info!("Starting in WebSocket signaling client mode");
            run_websocket_client_mode(args, shutdown_flag).await?;
        }
    }

    Ok(())
}

async fn run_grpc_signaling_server(
    args: Args,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
    use remotemedia_core::manifest::Manifest;
    use remotemedia_core::transport::PipelineExecutor;
    use remotemedia_webrtc::signaling::grpc::WebRtcSignalingService;
    use std::sync::Arc;
    use tonic::transport::Server;
    use tower_http::cors::{Any, CorsLayer};

    info!(
        grpc_address = %args.grpc_address,
        manifest_path = ?args.manifest,
        stun_servers = ?args.stun_servers,
        max_peers = args.max_peers,
        "gRPC signaling server configuration (with gRPC-Web support)"
    );

    // Load pipeline manifest from file
    let manifest_json = std::fs::read_to_string(&args.manifest)?;
    let manifest: Manifest = serde_json::from_str(&manifest_json)?;
    let manifest = Arc::new(manifest);
    info!("Loaded pipeline manifest: {:?}", args.manifest);

    // Create PipelineExecutor
    let runner = Arc::new(PipelineExecutor::new()?);
    info!("PipelineExecutor initialized");

    // Build WebRTC transport configuration from arguments
    let config = Arc::new(WebRtcTransportConfig {
        signaling_url: args.signaling_url.clone(),
        stun_servers: args.stun_servers.clone(),
        turn_servers: vec![], // TODO: Add TURN server parsing
        max_peers: args.max_peers,
        enable_data_channel: args.enable_data_channel,
        jitter_buffer_size_ms: args.jitter_buffer_ms,
        ..Default::default()
    });
    info!("WebRTC transport configuration loaded");

    // Create gRPC signaling service with config, runner, and manifest
    let signaling_service = WebRtcSignalingService::new(
        Arc::clone(&config),
        Arc::clone(&runner),
        Arc::clone(&manifest),
    );

    // Register virtual server peer
    let (server_tx, mut _server_rx) = tokio::sync::mpsc::channel(128);
    signaling_service.register_server_peer(server_tx).await;

    let signaling_server = signaling_service.into_server();

    info!("gRPC signaling server listening on {}", args.grpc_address);
    info!("gRPC-Web enabled for browser clients");

    // Parse address
    let addr: std::net::SocketAddr = args.grpc_address.parse()?;

    // Configure CORS for gRPC-Web browser clients
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers([AUTHORIZATION, ACCEPT, CONTENT_TYPE, "x-grpc-web".parse().unwrap(), "grpc-timeout".parse().unwrap()])
        .allow_methods(Any)
        .expose_headers(["grpc-status".parse().unwrap(), "grpc-message".parse().unwrap()]);

    // Start gRPC server with shutdown
    let shutdown_future = async move {
        while !shutdown_flag.load(Ordering::SeqCst) {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        info!("Shutdown signal received, stopping gRPC server...");
    };

    // Build server with gRPC-Web layer and CORS
    Server::builder()
        .accept_http1(true) // Required for gRPC-Web
        .layer(cors)
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(signaling_server)
        .serve_with_shutdown(addr, shutdown_future)
        .await?;

    info!("gRPC signaling server shut down gracefully");
    Ok(())
}

async fn run_websocket_client_mode(
    args: Args,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Build configuration from arguments
    let config = WebRtcTransportConfig {
        signaling_url: args.signaling_url.clone(),
        stun_servers: args.stun_servers.clone(),
        turn_servers: vec![], // TODO: Add TURN server parsing
        max_peers: args.max_peers,
        enable_data_channel: args.enable_data_channel,
        jitter_buffer_size_ms: args.jitter_buffer_ms,
        ..Default::default()
    };

    // Validate and log configuration
    config.validate()?;

    info!(
        signaling_url = %config.signaling_url,
        max_peers = config.max_peers,
        stun_servers = config.stun_servers.len(),
        turn_servers = config.turn_servers.len(),
        enable_data_channel = config.enable_data_channel,
        jitter_buffer_ms = config.jitter_buffer_size_ms,
        "Configuration loaded"
    );

    // Create and start WebRTC transport
    let transport = WebRtcTransport::new(config)?;
    info!("WebRTC transport created");

    // Start transport (connects to signaling server)
    transport.start().await?;
    info!("WebRTC transport started and connected to signaling server");

    // Keep server running until shutdown signal
    info!("Server running. Press Ctrl+C to shutdown.");

    while !shutdown_flag.load(Ordering::SeqCst) {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    info!("Shutdown signal received, cleaning up...");

    // Graceful shutdown
    transport.shutdown().await?;
    info!("WebRTC transport shut down gracefully");

    Ok(())
}

fn init_tracing() {
    // Initialize tracing with EnvFilter for RUST_LOG support
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
