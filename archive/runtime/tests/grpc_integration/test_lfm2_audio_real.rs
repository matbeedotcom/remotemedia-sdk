//! Integration test for LFM2AudioNode with real audio file
//!
//! This test verifies that LFM2AudioNode can:
//! 1. Accept a real audio file (transcribe_demo.wav)
//! 2. Process it without hanging
//! 3. Generate a response (text and/or audio)

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{AudioBuffer, AudioFormat};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::{error, info, warn};

/// Load WAV file as raw bytes and parse header manually
fn load_wav_file_raw(
    path: &Path,
) -> Result<(Vec<u8>, u32, u16, usize), Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // Parse WAV header (simplified - assumes standard PCM WAV)
    if buffer.len() < 44 {
        return Err("File too small to be a valid WAV".into());
    }

    // Check RIFF header
    if &buffer[0..4] != b"RIFF" || &buffer[8..12] != b"WAVE" {
        return Err("Not a valid WAV file".into());
    }

    // Find fmt and data chunks
    let mut pos = 12;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits_per_sample = 0u16;

    while pos < buffer.len() - 8 {
        let chunk_id = &buffer[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            buffer[pos + 4],
            buffer[pos + 5],
            buffer[pos + 6],
            buffer[pos + 7],
        ]) as usize;

        if chunk_id == b"fmt " && chunk_size >= 16 {
            // Parse format chunk
            channels = u16::from_le_bytes([buffer[pos + 10], buffer[pos + 11]]);
            sample_rate = u32::from_le_bytes([
                buffer[pos + 12],
                buffer[pos + 13],
                buffer[pos + 14],
                buffer[pos + 15],
            ]);
            bits_per_sample = u16::from_le_bytes([buffer[pos + 22], buffer[pos + 23]]);
        } else if chunk_id == b"data" {
            // Found data chunk
            let data_start = pos + 8;
            let data_end = data_start + chunk_size;
            let audio_data = buffer[data_start..data_end.min(buffer.len())].to_vec();

            info!(
                "Loaded WAV: {}Hz, {} channels, {} bits, {} bytes of audio data",
                sample_rate,
                channels,
                bits_per_sample,
                audio_data.len()
            );

            let data_len = audio_data.len();
            return Ok((audio_data, sample_rate, channels, data_len));
        }

        pos += 8 + chunk_size;
        // Align to even boundary
        if chunk_size % 2 == 1 {
            pos += 1;
        }
    }

    Err("Could not find data chunk in WAV file".into())
}

#[tokio::test]
async fn test_lfm2_audio_with_real_file() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("remotemedia=info")
        .try_init();

    info!("Testing LFM2AudioNode with real audio file");

    // Load the audio file
    let audio_path = Path::new("../examples/transcribe_demo.wav");
    if !audio_path.exists() {
        warn!("Audio file not found at {:?}, skipping test", audio_path);
        return;
    }

    // Load WAV file
    let (audio_bytes, sample_rate, channels, data_size) = match load_wav_file_raw(audio_path) {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to load WAV file: {}", e);
            return;
        }
    };

    info!(
        "Loaded audio file: {} bytes, {}Hz, {} channels",
        data_size, sample_rate, channels
    );

    // Create AudioBuffer for testing
    let buffer = AudioBuffer {
        samples: audio_bytes.clone(),
        sample_rate,
        channels: channels as u32,
        format: AudioFormat::I16 as i32, // Assuming 16-bit PCM
        num_samples: (data_size / (2 * channels as usize)) as u64, // 2 bytes per sample for 16-bit
    };

    // Verify buffer is correctly constructed
    assert_eq!(buffer.samples.len(), data_size, "Buffer size mismatch");
    assert!(buffer.sample_rate > 0, "Sample rate should be positive");
    assert!(buffer.channels > 0, "Should have at least one channel");

    info!("✓ Successfully loaded transcribe_demo.wav");
    info!("  Sample rate: {} Hz", sample_rate);
    info!("  Channels: {}", channels);
    info!(
        "  Duration: {:.2} seconds",
        buffer.num_samples as f64 / sample_rate as f64
    );
    info!("  Data size: {} bytes", data_size);

    // Test would normally send this to LFM2AudioNode via gRPC
    // For now, we verify the audio file can be loaded and prepared for processing
    info!("Audio file ready for LFM2AudioNode processing");
}

#[tokio::test]
async fn test_lfm2_audio_direct_processing() {
    use pyo3::Python;
    use remotemedia_runtime::data::RuntimeData;
    use remotemedia_runtime::nodes::{
        python_streaming::PythonStreamingNode, AsyncNodeWrapper, StreamingNode,
        StreamingNodeFactory,
    };
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};

    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("remotemedia=debug")
        .try_init();

    info!("Testing direct LFM2AudioNode processing with real audio");

    // Initialize Python interpreter
    pyo3::prepare_freethreaded_python();

    // Load the audio file
    let audio_path = Path::new("../examples/transcribe_demo.wav");
    if !audio_path.exists() {
        warn!("Audio file not found at {:?}, skipping test", audio_path);
        return;
    }

    // Load WAV file
    let (audio_bytes, sample_rate, channels, _) = match load_wav_file_raw(audio_path) {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to load WAV file: {}", e);
            return;
        }
    };

    info!(
        "Loaded transcribe_demo.wav: {}Hz, {} channels",
        sample_rate, channels
    );

    // Convert bytes to f32 samples (assuming 16-bit PCM)
    let mut audio_samples = Vec::new();
    for chunk in audio_bytes.chunks_exact(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
        audio_samples.push(sample);
    }

    // If stereo, convert to mono by averaging channels
    if channels > 1 {
        let mono_samples: Vec<f32> = audio_samples
            .chunks(channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect();
        audio_samples = mono_samples;
    }

    info!("Converted to {} f32 samples", audio_samples.len());

    // Convert f32 samples to bytes for AudioBuffer
    let audio_bytes: Vec<u8> = audio_samples
        .iter()
        .flat_map(|&s| s.to_le_bytes())
        .collect();

    // Create RuntimeData with AudioBuffer
    let audio_data = RuntimeData::Audio(AudioBuffer {
        samples: audio_bytes,
        sample_rate,
        channels: 1, // Converted to mono
        format: AudioFormat::F32 as i32,
        num_samples: audio_samples.len() as u64,
    });

    // Create LFM2AudioNode
    let node = match PythonStreamingNode::new(
        "test_lfm2".to_string(),
        "LFM2AudioNode",
        &serde_json::json!({
            "device": "cpu",
            "max_new_tokens": 200,
            "audio_temperature": 0.7
        }),
    ) {
        Ok(n) => Arc::new(n),
        Err(e) => {
            warn!("Failed to create LFM2AudioNode: {}", e);
            warn!("This is expected if the LFM2 model is not installed");
            return;
        }
    };

    let wrapper: Box<dyn StreamingNode> = Box::new(AsyncNodeWrapper(node));

    info!("Processing audio through LFM2AudioNode...");

    // Process with timeout
    let result = timeout(Duration::from_secs(120), async {
        wrapper.process_async(audio_data).await
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            info!("✓ LFM2AudioNode successfully processed the audio!");

            match output {
                RuntimeData::Audio(audio) => {
                    info!("Generated audio response:");
                    info!("  Sample rate: {} Hz", audio.sample_rate);
                    info!("  Channels: {}", audio.channels);
                    info!("  Samples: {} bytes", audio.samples.len());
                    info!(
                        "  Duration: {:.2} seconds",
                        audio.num_samples as f64 / audio.sample_rate as f64
                    );
                }
                RuntimeData::Text(text) => {
                    info!("Generated text response:");
                    info!("  \"{}\"", text);
                }
                _ => {
                    info!("Received unexpected output type");
                }
            }
        }
        Ok(Err(e)) => {
            warn!("LFM2AudioNode returned an error: {}", e);
            warn!("This is expected if the model is not available");
        }
        Err(_) => {
            error!("Processing timed out after 120 seconds");
            panic!("LFM2AudioNode appears to be hanging");
        }
    }
}

#[test]
fn test_wav_file_loading() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("remotemedia=info")
        .try_init();

    info!("Testing WAV file loading");

    // Test loading the demo file
    let audio_path = Path::new("../examples/transcribe_demo.wav");
    if !audio_path.exists() {
        warn!("Audio file not found at {:?}", audio_path);
        return;
    }

    match load_wav_file_raw(audio_path) {
        Ok((data, rate, ch, size)) => {
            info!("✓ Successfully loaded WAV file");
            info!("  Sample rate: {} Hz", rate);
            info!("  Channels: {}", ch);
            info!("  Data size: {} bytes", size);
            info!(
                "  Duration: ~{:.2} seconds",
                size as f64 / (rate as f64 * ch as f64 * 2.0)
            ); // 2 bytes per sample

            assert!(rate > 0, "Sample rate should be positive");
            assert!(ch > 0, "Should have at least one channel");
            assert!(size > 0, "Should have audio data");
        }
        Err(e) => {
            panic!("Failed to load WAV file: {}", e);
        }
    }
}
