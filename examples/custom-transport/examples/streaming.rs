//! Custom transport streaming example
//!
//! Demonstrates using StreamSession for continuous data streaming
//! through a custom transport.

use custom_transport_example::ConsoleTransport;
use remotemedia_runtime_core::transport::{PipelineTransport, StreamSession, TransportData};
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

    println!("=== Custom Transport Example (Streaming) ===\n");

    // Create transport
    let transport = ConsoleTransport::new()?;

    // Create manifest
    let manifest_json = r#"{
        "version": "v1",
        "nodes": [],
        "connections": []
    }"#;
    let manifest = Arc::new(Manifest::from_json(manifest_json)?);

    // Create streaming session
    println!("Creating streaming session...");
    let mut session = transport.stream(manifest).await?;
    println!("✓ Session created: {}\n", session.session_id());

    // Send multiple chunks
    println!("Streaming 5 audio chunks...");
    for i in 1..=5 {
        // Generate audio chunk (simple sine wave)
        let samples: Vec<f32> = (0..160)  // 10ms at 16kHz
            .map(|j| ((i * 100 + j) as f32 * 0.01).sin())
            .collect();

        let chunk = TransportData::new(RuntimeData::Audio {
            samples,
            sample_rate: 16000,
            channels: 1,
        })
        .with_sequence(i)
        .with_metadata("chunk_id".into(), format!("chunk_{}", i));

        println!("  → Sending chunk {}...", i);
        session.send_input(chunk).await?;

        // Receive output
        if let Some(output) = session.recv_output().await? {
            match output.data {
                RuntimeData::Audio { samples, .. } => {
                    println!("  ← Received {} samples", samples.len());
                }
                _ => println!("  ← Unexpected output type"),
            }
        }
    }

    // Send text data
    println!("\nSending text message...");
    let text_data = TransportData::new(RuntimeData::Text("End of stream".into()))
        .with_sequence(6);

    session.send_input(text_data).await?;

    if let Some(output) = session.recv_output().await? {
        match output.data {
            RuntimeData::Text(ref s) => println!("  ← Received: {}", s),
            _ => println!("  ← Unexpected output type"),
        }
    }

    // Close session
    println!("\nClosing session...");
    session.close().await?;
    println!("✓ Session closed");
    println!("✓ Active: {}", session.is_active());

    println!("\n=== Success! ===");
    println!("Demonstrated:");
    println!("  ✓ Creating streaming session");
    println!("  ✓ Sending multiple chunks with sequence numbers");
    println!("  ✓ Receiving outputs continuously");
    println!("  ✓ Session lifecycle management");
    println!("  ✓ All without transport dependencies!");

    Ok(())
}
