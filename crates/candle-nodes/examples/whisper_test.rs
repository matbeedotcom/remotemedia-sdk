//! Test WhisperNode with an audio file
//!
//! Run with: cargo run --example whisper_test --features whisper

use remotemedia_candle_nodes::whisper::{WhisperConfig, WhisperModel, WhisperNode};
use remotemedia_candle_nodes::DeviceSelector;
use remotemedia_core::data_compat::RuntimeData;
use remotemedia_core::nodes::streaming_node::AsyncStreamingNode;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup logging
    tracing_subscriber::fmt::init();

    // Path to audio file
    let audio_path = Path::new("audio.wav");
    if !audio_path.exists() {
        anyhow::bail!("audio.wav not found in project root");
    }

    println!("Loading audio from: {}", audio_path.display());

    // Read WAV file
    let mut reader = hound::WavReader::open(audio_path)?;
    let spec = reader.spec();
    
    println!("Audio format: {} Hz, {} channels, {} bits", 
             spec.sample_rate, spec.channels, spec.bits_per_sample);

    // Convert samples to f32
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => {
            reader.samples::<f32>()
                .filter_map(|s| s.ok())
                .collect()
        }
    };

    println!("Loaded {} samples ({:.2}s)", 
             samples.len(), 
             samples.len() as f32 / spec.sample_rate as f32);

    // Create WhisperNode config
    let config = WhisperConfig {
        model: WhisperModel::Tiny, // Use tiny for faster testing
        language: "en".to_string(),
        device: "cpu".to_string(), // Force CPU for compatibility
        ..Default::default()
    };

    println!("\nCreating WhisperNode with model: {:?}", config.model);
    println!("Device: {}", config.device);

    // Create node
    let node = WhisperNode::new("test-whisper", &config)?;

    // Initialize (downloads model if needed)
    println!("\nInitializing model (may download on first run)...");
    node.initialize().await?;
    println!("Model initialized!");

    // Create RuntimeData::Audio
    let audio_data = RuntimeData::Audio {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels as u32,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    // Process audio
    println!("\nTranscribing audio...");
    let start = std::time::Instant::now();
    let result = node.process(audio_data).await?;
    let elapsed = start.elapsed();

    // Print result
    match result {
        RuntimeData::Text(text) => {
            println!("\n=== Transcription Result ===");
            println!("{}", text);
            println!("============================");
        }
        RuntimeData::Json(json) => {
            println!("\n=== Transcription Result (JSON) ===");
            println!("{}", serde_json::to_string_pretty(&json)?);
            println!("===================================");
        }
        other => {
            println!("Unexpected result type: {:?}", other);
        }
    }

    println!("\nTranscription completed in {:.2}s", elapsed.as_secs_f32());

    Ok(())
}
