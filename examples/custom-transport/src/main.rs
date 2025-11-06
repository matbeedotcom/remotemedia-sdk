//! Custom transport example - Unary execution
//!
//! Demonstrates using runtime-core to create a custom transport
//! without any gRPC, FFI, or other transport dependencies.

use custom_transport_example::ConsoleTransport;
use remotemedia_runtime_core::transport::{PipelineTransport, TransportData};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    println!("=== Custom Transport Example (Unary) ===\n");

    // Create transport
    println!("Creating ConsoleTransport...");
    let transport = ConsoleTransport::new()?;

    // Create a simple manifest
    let manifest_json = r#"{
        "version": "v1",
        "nodes": [],
        "connections": []
    }"#;
    let manifest = Arc::new(Manifest::from_json(manifest_json)?);

    // Example 1: Text processing
    println!("\n--- Example 1: Text Processing ---");
    let text_input = TransportData::new(RuntimeData::Text("Hello, Custom Transport!".into()))
        .with_metadata("example".into(), "text_processing".into());

    let text_output = transport.execute(Arc::clone(&manifest), text_input).await?;
    match text_output.data {
        RuntimeData::Text(ref s) => println!("✓ Output: {}", s),
        _ => println!("✗ Unexpected output type"),
    }

    // Example 2: Audio processing
    println!("\n--- Example 2: Audio Processing ---");
    let audio_samples: Vec<f32> = (0..100)
        .map(|i| (i as f32 * 0.01).sin())
        .collect();

    let audio_input = TransportData::new(RuntimeData::Audio {
        samples: audio_samples.clone(),
        sample_rate: 16000,
        channels: 1,
    })
    .with_sequence(1);

    let audio_output = transport.execute(manifest, audio_input).await?;
    match audio_output.data {
        RuntimeData::Audio { samples, sample_rate, channels } => {
            println!("✓ Audio output:");
            println!("  - Samples: {} samples", samples.len());
            println!("  - Sample rate: {} Hz", sample_rate);
            println!("  - Channels: {}", channels);
        }
        _ => println!("✗ Unexpected output type"),
    }

    println!("\n=== Success! ===");
    println!("This example demonstrates:");
    println!("  ✓ Using runtime-core without any transport dependencies");
    println!("  ✓ Implementing PipelineTransport trait");
    println!("  ✓ Processing text and audio data");
    println!("  ✓ Custom transport in ~80 lines of code");

    Ok(())
}
