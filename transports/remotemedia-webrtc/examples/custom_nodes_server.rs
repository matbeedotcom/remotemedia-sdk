//! Example: WebRTC server with custom node registration
//!
//! This example demonstrates how to register custom Python and Rust nodes
//! in a WebRTC transport server.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example custom_nodes_server --features grpc-signaling
//! ```

use remotemedia_runtime_core::transport::PipelineRunner;
use remotemedia_webrtc::custom_nodes::{create_custom_registry, PythonNodeFactory};
use remotemedia_webrtc::{WebRtcTransport, WebRtcTransportConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("=== Custom Node Registration Example ===");

    // Set up shutdown signal
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_handler = Arc::clone(&shutdown_flag);

    ctrlc::set_handler(move || {
        eprintln!("\n🛑 Shutdown signal received!");
        shutdown_flag_handler.store(true, Ordering::SeqCst);
    })?;

    // Example 1: Simple Python node registration
    info!("\n📝 Example 1: Simple Python Node Registration");
    {
        let runner = PipelineRunner::with_custom_registry(|| {
            create_custom_registry(&[
                ("WhisperASR", false),     // Single output per input
                ("GPT4TTS", true),         // Multi-output (streaming)
                ("CustomFilter", false),   // Custom processing
            ])
        })?;

        info!("✅ Created runner with 3 custom Python nodes");
        info!("   Available nodes: {:?}", [
            "WhisperASR",
            "GPT4TTS",
            "CustomFilter",
            "... plus all built-in nodes"
        ]);
    }

    // Example 2: Advanced registration with custom factories
    info!("\n📝 Example 2: Advanced Registration with Mixed Node Types");
    {
        use remotemedia_runtime_core::nodes::streaming_registry::create_default_streaming_registry;

        let runner = PipelineRunner::with_custom_registry(|| {
            let mut registry = create_default_streaming_registry();

            // Add Python nodes
            registry.register(Arc::new(PythonNodeFactory::new("MyASR", false)));
            registry.register(Arc::new(PythonNodeFactory::new("MyTTS", true)));

            // You can also add custom Rust nodes here:
            // registry.register(Arc::new(MyRustNodeFactory));

            info!("   Registered custom Python nodes: MyASR, MyTTS");
            registry
        })?;

        info!("✅ Created runner with mixed node types");
    }

    // Example 3: Production setup with WebRTC transport
    info!("\n📝 Example 3: Production WebRTC Server with Custom Nodes");
    {
        // Define custom nodes for this deployment
        let custom_nodes = vec![
            "WhisperLargeV3",      // High-quality ASR
            "KokoroTTS",           // Fast TTS
            "BackgroundNoise",     // Audio enhancement
        ];

        info!("   Custom nodes: {:?}", custom_nodes);

        // Create PipelineRunner with custom registry
        let runner = Arc::new(PipelineRunner::with_custom_registry(move || {
            let node_specs: Vec<(&str, bool)> = custom_nodes
                .iter()
                .map(|name| (name.as_str(), true))
                .collect();

            create_custom_registry(&node_specs)
        })?);

        info!("✅ Created PipelineRunner with custom nodes");

        // Configure WebRTC transport
        let config = WebRtcTransportConfig {
            signaling_url: "ws://localhost:8080".to_string(),
            stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
            max_peers: 10,
            enable_data_channel: true,
            jitter_buffer_size_ms: 100,
            ..Default::default()
        };

        info!("✅ WebRTC transport configured");
        info!("   Signaling: {}", config.signaling_url);
        info!("   Max peers: {}", config.max_peers);

        // Create transport
        let transport = WebRtcTransport::new(config)?;
        info!("✅ WebRTC transport created");

        // Start transport (in real app, this would connect to signaling server)
        info!("🚀 Transport ready to start");
        info!("   (Skipping actual start in example - requires signaling server)");

        // In production, you would:
        // transport.start().await?;
        //
        // Then handle incoming connections and create sessions:
        // let session = runner.create_stream_session(manifest).await?;
    }

    // Example 4: Command-line style registration
    info!("\n📝 Example 4: Command-Line Style Registration");
    {
        // Simulate parsing --custom-nodes flag
        let cli_nodes = vec!["Node1".to_string(), "Node2".to_string()];

        let runner = PipelineRunner::with_custom_registry(move || {
            let node_specs: Vec<(&str, bool)> = cli_nodes
                .iter()
                .map(|name| (name.as_str(), true))
                .collect();

            create_custom_registry(&node_specs)
        })?;

        info!("✅ Created runner from CLI args: {:?}", ["Node1", "Node2"]);
    }

    info!("\n✨ All examples completed successfully!");
    info!("\n📚 Next steps:");
    info!("   1. Implement your Python nodes in python-client/remotemedia/nodes/");
    info!("   2. Use --custom-nodes flag in webrtc_server");
    info!("   3. Reference nodes in your pipeline manifests");
    info!("\nSee docs/CUSTOM_NODE_REGISTRATION.md for details");

    Ok(())
}

