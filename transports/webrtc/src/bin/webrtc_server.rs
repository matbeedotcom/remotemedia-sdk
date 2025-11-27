//! WebRTC server binary entry point
//!
//! Starts the RemoteMedia WebRTC transport server for real-time media streaming.
//!
//! # Usage
//!
//! ```bash
//! # Start gRPC signaling server (default: 0.0.0.0:50051)
//! cargo run --bin webrtc_server --features grpc-signaling -- \
//!   --mode grpc \
//!   --grpc-address 0.0.0.0:50051 \
//!   --manifest ./examples/loopback.yaml
//!
//! # Start WebSocket client mode (connects to signaling server)
//! cargo run --bin webrtc_server -- \
//!   --mode websocket \
//!   --signaling-url ws://localhost:8080
//!
//! # Configure STUN/TURN servers
//! cargo run --bin webrtc_server -- \
//!   --stun-servers stun:stun.l.google.com:19302 \
//!   --max-peers 20
//! ```

use clap::Parser;
use remotemedia_webrtc::{
    ConfigOptions, DataChannelMode, TurnServerConfig, VideoCodec, VideoResolution, WebRtcTransport,
    WebRtcTransportConfig,
};
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

    /// Configuration preset: low_latency, high_quality, mobile_network
    #[arg(long, env = "WEBRTC_PRESET")]
    preset: Option<ConfigPreset>,

    /// Video codec: vp8, vp9, h264
    #[arg(long, default_value = "vp9", env = "WEBRTC_VIDEO_CODEC")]
    video_codec: VideoCodecArg,

    /// Data channel mode: reliable, unreliable
    #[arg(long, default_value = "reliable", env = "WEBRTC_DATA_CHANNEL_MODE")]
    data_channel_mode: DataChannelModeArg,

    /// TURN servers (format: turn:host:port:username:password, comma-separated)
    #[arg(long, value_delimiter = ',', env = "WEBRTC_TURN_SERVERS")]
    turn_servers: Vec<String>,

    /// Target bitrate in kbps
    #[arg(long, default_value_t = 2000, env = "WEBRTC_TARGET_BITRATE")]
    target_bitrate_kbps: u32,

    /// Maximum video resolution: 480p, 720p, 1080p
    #[arg(long, default_value = "720p", env = "WEBRTC_MAX_RESOLUTION")]
    max_resolution: VideoResolutionArg,

    /// Video framerate in fps
    #[arg(long, default_value_t = 30, env = "WEBRTC_VIDEO_FRAMERATE")]
    video_framerate_fps: u32,

    /// ICE connection timeout in seconds
    #[arg(long, default_value_t = 30, env = "WEBRTC_ICE_TIMEOUT")]
    ice_timeout_secs: u32,

    /// Enable adaptive bitrate
    #[arg(long, default_value_t = true, env = "WEBRTC_ADAPTIVE_BITRATE")]
    adaptive_bitrate: bool,

    /// Maximum reconnection attempts
    #[arg(long, default_value_t = 5, env = "WEBRTC_MAX_RECONNECT_RETRIES")]
    max_reconnect_retries: u32,

    /// Initial reconnection backoff in milliseconds
    #[arg(long, default_value_t = 1000, env = "WEBRTC_RECONNECT_BACKOFF_INITIAL")]
    reconnect_backoff_initial_ms: u64,

    /// Maximum reconnection backoff in milliseconds
    #[arg(long, default_value_t = 30000, env = "WEBRTC_RECONNECT_BACKOFF_MAX")]
    reconnect_backoff_max_ms: u64,

    /// RTCP report interval in milliseconds
    #[arg(long, default_value_t = 5000, env = "WEBRTC_RTCP_INTERVAL")]
    rtcp_interval_ms: u32,

    /// Enable quality metrics logging
    #[arg(long, default_value_t = false, env = "WEBRTC_METRICS_LOGGING")]
    enable_metrics_logging: bool,

    /// Quality metrics logging interval in seconds
    #[arg(long, default_value_t = 10, env = "WEBRTC_METRICS_INTERVAL")]
    metrics_interval_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ServerMode {
    /// gRPC signaling server mode
    Grpc,
    /// WebSocket signaling client mode
    Websocket,
}

/// Configuration preset for quick setup
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ConfigPreset {
    /// Optimized for real-time applications (minimal latency)
    LowLatency,
    /// Optimized for video/audio quality (higher bitrate, larger buffers)
    HighQuality,
    /// Optimized for cellular/unstable networks (larger buffers, lower bitrate)
    MobileNetwork,
}

/// Video codec CLI argument wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum VideoCodecArg {
    Vp8,
    Vp9,
    H264,
}

impl From<VideoCodecArg> for VideoCodec {
    fn from(arg: VideoCodecArg) -> Self {
        match arg {
            VideoCodecArg::Vp8 => VideoCodec::VP8,
            VideoCodecArg::Vp9 => VideoCodec::VP9,
            VideoCodecArg::H264 => VideoCodec::H264,
        }
    }
}

/// Data channel mode CLI argument wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum DataChannelModeArg {
    Reliable,
    Unreliable,
}

impl From<DataChannelModeArg> for DataChannelMode {
    fn from(arg: DataChannelModeArg) -> Self {
        match arg {
            DataChannelModeArg::Reliable => DataChannelMode::Reliable,
            DataChannelModeArg::Unreliable => DataChannelMode::Unreliable,
        }
    }
}

/// Video resolution CLI argument wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum VideoResolutionArg {
    #[value(name = "480p")]
    P480,
    #[value(name = "720p")]
    P720,
    #[value(name = "1080p")]
    P1080,
}

impl From<VideoResolutionArg> for VideoResolution {
    fn from(arg: VideoResolutionArg) -> Self {
        match arg {
            VideoResolutionArg::P480 => VideoResolution::P480,
            VideoResolutionArg::P720 => VideoResolution::P720,
            VideoResolutionArg::P1080 => VideoResolution::P1080,
        }
    }
}

/// Parse TURN server string (format: turn:host:port:username:password or turns:host:port:username:password)
fn parse_turn_server(s: &str) -> Result<TurnServerConfig, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 5 {
        return Err(format!(
            "Invalid TURN server format: '{}'. Expected: turn:host:port:username:password",
            s
        ));
    }

    let protocol = parts[0];
    if protocol != "turn" && protocol != "turns" {
        return Err(format!(
            "Invalid TURN protocol: '{}'. Expected 'turn' or 'turns'",
            protocol
        ));
    }

    let host = parts[1];
    let port = parts[2];
    let username = parts[3].to_string();
    // Password may contain colons, so join remaining parts
    let credential = parts[4..].join(":");

    Ok(TurnServerConfig {
        url: format!("{}:{}:{}", protocol, host, port),
        username,
        credential,
    })
}

/// Build WebRtcTransportConfig from CLI arguments
///
/// If a preset is specified, it starts with the preset defaults and then
/// applies any explicit CLI argument overrides.
fn build_config_from_args(args: &Args) -> Result<WebRtcTransportConfig, Box<dyn std::error::Error>> {
    // Start with preset or default config
    let mut config = match args.preset {
        Some(ConfigPreset::LowLatency) => {
            info!("Using low_latency preset");
            WebRtcTransportConfig::low_latency_preset(&args.signaling_url)
        }
        Some(ConfigPreset::HighQuality) => {
            info!("Using high_quality preset");
            WebRtcTransportConfig::high_quality_preset(&args.signaling_url)
        }
        Some(ConfigPreset::MobileNetwork) => {
            info!("Using mobile_network preset");
            WebRtcTransportConfig::mobile_network_preset(&args.signaling_url)
        }
        None => WebRtcTransportConfig {
            signaling_url: args.signaling_url.clone(),
            ..Default::default()
        },
    };

    // Apply explicit CLI overrides (these take precedence over presets)
    config.stun_servers = args.stun_servers.clone();
    config.max_peers = args.max_peers;
    config.enable_data_channel = args.enable_data_channel;
    config.jitter_buffer_size_ms = args.jitter_buffer_ms;
    config.video_codec = args.video_codec.into();
    config.data_channel_mode = args.data_channel_mode.into();
    config.rtcp_interval_ms = args.rtcp_interval_ms;

    // Parse and add TURN servers
    let mut turn_servers = Vec::new();
    for turn_str in &args.turn_servers {
        match parse_turn_server(turn_str) {
            Ok(turn_config) => {
                info!(
                    "Adding TURN server: {} (user: {})",
                    turn_config.url, turn_config.username
                );
                turn_servers.push(turn_config);
            }
            Err(e) => {
                return Err(format!("Failed to parse TURN server: {}", e).into());
            }
        }
    }
    if !turn_servers.is_empty() {
        config.turn_servers = turn_servers;
    }

    // Apply ConfigOptions overrides
    config.options = ConfigOptions {
        adaptive_bitrate_enabled: args.adaptive_bitrate,
        target_bitrate_kbps: args.target_bitrate_kbps,
        max_video_resolution: args.max_resolution.into(),
        video_framerate_fps: args.video_framerate_fps,
        ice_timeout_secs: args.ice_timeout_secs,
        max_reconnect_retries: args.max_reconnect_retries,
        reconnect_backoff_initial_ms: args.reconnect_backoff_initial_ms,
        reconnect_backoff_max_ms: args.reconnect_backoff_max_ms,
        reconnect_backoff_multiplier: 2.0, // Keep default multiplier
    };

    Ok(config)
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
            #[cfg(feature = "grpc-signaling")]
            {
                info!("Starting in gRPC signaling server mode");
                run_grpc_signaling_server(args, shutdown_flag).await?;
            }
            #[cfg(not(feature = "grpc-signaling"))]
            {
                return Err("gRPC signaling requested but feature not enabled. Build with --features grpc-signaling".into());
            }
        }
        ServerMode::Websocket => {
            info!("Starting in WebSocket signaling client mode");
            run_websocket_client_mode(args, shutdown_flag).await?;
        }
    }

    Ok(())
}

#[cfg(feature = "grpc-signaling")]
async fn run_grpc_signaling_server(
    args: Args,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    use remotemedia_runtime_core::manifest::Manifest;
    use remotemedia_runtime_core::transport::PipelineRunner;
    use remotemedia_webrtc::signaling::grpc::WebRtcSignalingService;
    use std::sync::Arc;
    use tonic::transport::Server;

    // Build configuration from CLI arguments (supports presets)
    let config = build_config_from_args(&args)?;

    info!(
        grpc_address = %args.grpc_address,
        manifest_path = ?args.manifest,
        preset = ?args.preset,
        stun_servers = ?config.stun_servers,
        turn_servers = config.turn_servers.len(),
        max_peers = config.max_peers,
        video_codec = ?config.video_codec,
        data_channel_mode = ?config.data_channel_mode,
        jitter_buffer_ms = config.jitter_buffer_size_ms,
        target_bitrate = config.options.target_bitrate_kbps,
        max_resolution = ?config.options.max_video_resolution,
        adaptive_bitrate = config.options.adaptive_bitrate_enabled,
        ice_timeout = config.options.ice_timeout_secs,
        max_reconnect_retries = config.options.max_reconnect_retries,
        "gRPC signaling server configuration"
    );

    // Load pipeline manifest from file
    let manifest_json = std::fs::read_to_string(&args.manifest)?;
    let manifest: Manifest = serde_json::from_str(&manifest_json)?;
    let manifest = Arc::new(manifest);
    info!("Loaded pipeline manifest: {:?}", args.manifest);

    // Create PipelineRunner
    let runner = Arc::new(PipelineRunner::new()?);
    info!("PipelineRunner initialized");

    // Validate configuration
    config.validate()?;
    let config = Arc::new(config);
    info!("WebRTC transport configuration validated and loaded");

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

    // Parse address
    let addr: std::net::SocketAddr = args.grpc_address.parse()?;

    // Start gRPC server with shutdown
    let shutdown_future = async move {
        while !shutdown_flag.load(Ordering::SeqCst) {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        info!("Shutdown signal received, stopping gRPC server...");
    };

    Server::builder()
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
    // Build configuration from CLI arguments (supports presets)
    let config = build_config_from_args(&args)?;

    // Validate configuration
    config.validate()?;

    info!(
        signaling_url = %config.signaling_url,
        preset = ?args.preset,
        max_peers = config.max_peers,
        stun_servers = config.stun_servers.len(),
        turn_servers = config.turn_servers.len(),
        enable_data_channel = config.enable_data_channel,
        video_codec = ?config.video_codec,
        data_channel_mode = ?config.data_channel_mode,
        jitter_buffer_ms = config.jitter_buffer_size_ms,
        target_bitrate = config.options.target_bitrate_kbps,
        max_resolution = ?config.options.max_video_resolution,
        adaptive_bitrate = config.options.adaptive_bitrate_enabled,
        ice_timeout = config.options.ice_timeout_secs,
        max_reconnect_retries = config.options.max_reconnect_retries,
        "Configuration loaded"
    );

    // Create and start WebRTC transport
    let transport = WebRtcTransport::new(config)?;
    info!("WebRTC transport created");

    // Start transport (connects to signaling server)
    transport.start().await?;
    info!("WebRTC transport started and connected to signaling server");

    // Optionally start quality metrics logging
    if args.enable_metrics_logging {
        let metrics_interval = args.metrics_interval_secs;
        let shutdown_flag_metrics = Arc::clone(&shutdown_flag);
        tokio::spawn(async move {
            info!(
                "Quality metrics logging enabled (interval: {}s)",
                metrics_interval
            );
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(metrics_interval));
            loop {
                interval.tick().await;
                if shutdown_flag_metrics.load(Ordering::SeqCst) {
                    break;
                }
                // TODO: Integrate with actual quality metrics from transport
                // For now, log a placeholder - actual metrics would come from
                // ConnectionQualityMetrics attached to each peer connection
                info!(
                    event = "quality_metrics",
                    message = "Quality metrics collection running (integrate with transport.get_metrics())"
                );
            }
        });
    }

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
