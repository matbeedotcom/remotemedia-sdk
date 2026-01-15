//! Test SileroVadNode with an audio file
//!
//! Run with: cargo run --example vad_test --features vad -p remotemedia-candle-nodes

use remotemedia_candle_nodes::{SileroVadNode, VadConfig, VadSampleRate, VadOutput};
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::streaming_node::AsyncStreamingNode;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Check for audio file
    let audio_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "audio.wav".to_string());

    println!("Loading audio from: {}", audio_path);

    // Load audio file
    let (samples, sample_rate, channels) = load_wav(&audio_path)?;
    let duration_secs = samples.len() as f32 / sample_rate as f32 / channels as f32;
    println!(
        "Audio: {} Hz, {} channels, {:.2}s",
        sample_rate, channels, duration_secs
    );

    // Determine VAD sample rate based on input
    let vad_sample_rate = if sample_rate >= 16000 {
        VadSampleRate::Sr16k
    } else {
        VadSampleRate::Sr8k
    };

    // Create VAD config
    let config = VadConfig {
        sample_rate: vad_sample_rate,
        threshold: 0.5,
        min_speech_duration_ms: 250,
        min_silence_duration_ms: 100,
        device: "cpu".to_string(),
        output_segments: true,
    };

    println!("\nCreating SileroVadNode");
    println!("Sample rate: {}", config.sample_rate);
    println!("Threshold: {}", config.threshold);

    let node = SileroVadNode::new("vad-test", &config)?;

    println!("\nInitializing model (may download on first run)...");
    let start = std::time::Instant::now();
    node.initialize().await?;
    println!("Model initialized in {:.2}s", start.elapsed().as_secs_f32());

    // Convert samples to mono f32
    let mono_samples: Vec<f32> = if channels > 1 {
        samples
            .chunks(channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    // Create RuntimeData::Audio
    let audio_data = RuntimeData::Audio {
        samples: mono_samples,
        sample_rate,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    println!("\nRunning VAD...");
    let start = std::time::Instant::now();
    let result = node.process(audio_data).await?;
    let elapsed = start.elapsed();

    // Parse result
    match result {
        RuntimeData::Json(json) => {
            let output: VadOutput = serde_json::from_value(json)?;
            println!("\n=== VAD Results ===");
            
            match output {
                VadOutput::Segments(segments) => {
                    if segments.is_empty() {
                        println!("No speech detected");
                    } else {
                        println!("Detected {} speech segment(s):\n", segments.len());
                        for (i, seg) in segments.iter().enumerate() {
                            println!(
                                "  Segment {}: {:.2}s - {:.2}s (prob: {:.2})",
                                i + 1,
                                seg.start_ms as f32 / 1000.0,
                                seg.end_ms as f32 / 1000.0,
                                seg.probability
                            );
                        }
                    }
                }
                VadOutput::Probability(prob) => {
                    println!("Average speech probability: {:.2}", prob);
                }
            }
            println!("===================");
        }
        other => {
            println!("Unexpected result type: {:?}", other);
        }
    }

    println!("\nVAD completed in {:.2}s", elapsed.as_secs_f32());

    Ok(())
}

/// Load a WAV file and return (samples, sample_rate, channels)
fn load_wav(path: &str) -> anyhow::Result<(Vec<f32>, u32, u32)> {
    let path = Path::new(path);
    if !path.exists() {
        anyhow::bail!("Audio file not found: {}", path.display());
    }

    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();

    println!(
        "Audio format: {} Hz, {} channels, {} bits",
        spec.sample_rate, spec.channels, spec.bits_per_sample
    );

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap() as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
    };

    Ok((samples, spec.sample_rate, spec.channels as u32))
}
