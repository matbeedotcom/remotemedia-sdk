//! WebRTC server binary entry point
//!
//! Starts the RemoteMedia WebRTC transport server for real-time media streaming.
//!
//! # Usage
//!
//! ```bash
//! # Start with defaults (ws://localhost:8080 signaling, max 10 peers)
//! cargo run --bin webrtc_server
//!
//! # Start with custom signaling URL
//! WEBRTC_SIGNALING_URL="ws://0.0.0.0:8080" cargo run --bin webrtc_server
//!
//! # Configure peer limits
//! WEBRTC_MAX_PEERS=20 cargo run --bin webrtc_server
//!
//! # Configure STUN/TURN servers
//! WEBRTC_STUN_SERVERS="stun:stun.l.google.com:19302,stun:stun1.l.google.com:19302" \
//! cargo run --bin webrtc_server
//!
//! # Enable data channels
//! WEBRTC_ENABLE_DATA_CHANNEL=true cargo run --bin webrtc_server
//!
//! # Build with codecs
//! cargo run --bin webrtc_server --features codecs
//! ```
//!
//! # Environment Variables
//!
//! - `WEBRTC_SIGNALING_URL`: Signaling server WebSocket URL (default: `ws://localhost:8080`)
//! - `WEBRTC_STUN_SERVERS`: Comma-separated list of STUN servers (default: `stun:stun.l.google.com:19302`)
//! - `WEBRTC_TURN_SERVERS`: Comma-separated list of TURN servers (default: none)
//! - `WEBRTC_MAX_PEERS`: Maximum number of concurrent peer connections (default: `10`)
//! - `WEBRTC_ENABLE_DATA_CHANNEL`: Enable data channel support (default: `true`)
//! - `WEBRTC_JITTER_BUFFER_MS`: Jitter buffer size in milliseconds (default: `100`)
//! - `RUST_LOG`: Logging level (default: `info`, options: `trace`, `debug`, `info`, `warn`, `error`)

use remotemedia_webrtc::{TurnServerConfig, WebRtcTransport, WebRtcTransportConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up Ctrl+C handler at the very start
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_handler = Arc::clone(&shutdown_flag);

    ctrlc::set_handler(move || {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        eprintln!("\n🛑 [{}] Ctrl+C received! Initiating shutdown...", timestamp);

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

    runtime.block_on(async_main(shutdown_flag))
}

async fn async_main(shutdown_flag: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing (logging)
    init_tracing();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "RemoteMedia WebRTC Server starting"
    );

    // Load configuration from environment
    let config = load_config_from_env()?;

    // Log configuration
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

fn load_config_from_env() -> Result<WebRtcTransportConfig, Box<dyn std::error::Error>> {
    let signaling_url = std::env::var("WEBRTC_SIGNALING_URL")
        .unwrap_or_else(|_| "ws://localhost:8080".to_string());

    let max_peers = std::env::var("WEBRTC_MAX_PEERS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(10);

    let stun_servers = std::env::var("WEBRTC_STUN_SERVERS")
        .unwrap_or_else(|_| "stun:stun.l.google.com:19302".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    // TURN servers format: "turn:example.com:3478:username:credential,turns:example.com:5349:user2:cred2"
    let turn_servers = std::env::var("WEBRTC_TURN_SERVERS")
        .ok()
        .and_then(|v| {
            let servers: Vec<TurnServerConfig> = v
                .split(',')
                .filter_map(|s| {
                    let parts: Vec<&str> = s.trim().split(':').collect();
                    if parts.len() >= 4 {
                        // Format: turn://host:port:username:credential
                        Some(TurnServerConfig {
                            url: format!("{}://{}:{}", parts[0], parts[1], parts[2]),
                            username: parts[3].to_string(),
                            credential: parts.get(4).unwrap_or(&"").to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();
            if servers.is_empty() {
                None
            } else {
                Some(servers)
            }
        })
        .unwrap_or_default();

    let enable_data_channel = std::env::var("WEBRTC_ENABLE_DATA_CHANNEL")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);

    let jitter_buffer_size_ms = std::env::var("WEBRTC_JITTER_BUFFER_MS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(100);

    if stun_servers.is_empty() {
        warn!("No STUN servers configured - NAT traversal may fail");
    }

    let config = WebRtcTransportConfig {
        signaling_url,
        stun_servers,
        turn_servers,
        max_peers,
        enable_data_channel,
        jitter_buffer_size_ms,
        ..Default::default()
    };

    // Validate configuration
    config.validate()?;

    Ok(config)
}
