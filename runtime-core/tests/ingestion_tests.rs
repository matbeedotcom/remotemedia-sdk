//! Integration tests for the file ingestion plugin
//!
//! Run with:
//! ```sh
//! cargo test -p remotemedia-runtime-core --test ingestion_tests
//! ```

use std::io::Write;
use std::time::Duration;
use tempfile::NamedTempFile;

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::ingestion::{
    global_ingest_registry, AudioConfig, IngestConfig, IngestSource, IngestStatus,
    ReconnectConfig, TrackSelection,
};

/// Generate a simple WAV file with a sine wave for testing
fn generate_test_wav(duration_secs: f32, sample_rate: u32, channels: u16) -> NamedTempFile {
    let mut file = NamedTempFile::with_suffix(".wav").expect("Failed to create temp file");

    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let frequency = 440.0; // A4 note

    // Generate samples
    let mut samples: Vec<i16> = Vec::with_capacity(num_samples * channels as usize);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (f32::sin(2.0 * std::f32::consts::PI * frequency * t) * 16000.0) as i16;
        for _ in 0..channels {
            samples.push(sample);
        }
    }

    // Write WAV header
    let data_size = (samples.len() * 2) as u32;
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;

    // RIFF header
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_size).to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    // fmt chunk
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap(); // chunk size
    file.write_all(&1u16.to_le_bytes()).unwrap(); // PCM format
    file.write_all(&channels.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    file.write_all(&block_align.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap(); // bits per sample

    // data chunk
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();

    // Write samples
    for sample in samples {
        file.write_all(&sample.to_le_bytes()).unwrap();
    }

    file.flush().unwrap();
    file
}

/// T092: Integration test: ingest WAV file produces RuntimeData::Audio chunks
#[tokio::test]
async fn test_wav_file_produces_audio_chunks() {
    // Generate a 2-second test WAV file
    let wav_file = generate_test_wav(2.0, 16000, 1);
    let wav_path = wav_file.path().to_string_lossy().to_string();

    let registry = global_ingest_registry();

    let config = IngestConfig {
        url: wav_path.clone(),
        audio: Some(AudioConfig {
            sample_rate: 16000,
            channels: 1,
        }),
        video: None,
        track_selection: TrackSelection::FirstAudioVideo,
        reconnect: ReconnectConfig::disabled(),
        extra: None,
    };

    // Create source from registry
    let mut source = registry
        .create_from_uri(&config)
        .expect("Failed to create file source");

    assert_eq!(source.status(), IngestStatus::Idle);

    // Start ingestion
    let mut stream = source.start().await.expect("Failed to start");

    // Collect chunks
    let mut audio_chunks = Vec::new();
    let mut total_samples = 0usize;

    loop {
        match tokio::time::timeout(Duration::from_secs(5), stream.recv()).await {
            Ok(Some(RuntimeData::Audio {
                samples, stream_id, ..
            })) => {
                total_samples += samples.len();
                audio_chunks.push(samples.len());

                // Verify stream_id is set
                assert!(
                    stream_id.is_some(),
                    "Audio chunks should have stream_id set"
                );
                if let Some(id) = &stream_id {
                    assert!(
                        id.starts_with("audio:"),
                        "stream_id should start with 'audio:'"
                    );
                }
            }
            Ok(Some(other)) => {
                panic!("Expected Audio data, got {:?}", std::mem::discriminant(&other));
            }
            Ok(None) => break, // EOF
            Err(_) => panic!("Timeout waiting for audio chunks"),
        }
    }

    // Verify we got audio data
    assert!(!audio_chunks.is_empty(), "Should receive at least one audio chunk");
    println!(
        "✓ Received {} audio chunks, {} total samples",
        audio_chunks.len(),
        total_samples
    );

    // Verify approximate duration (2 seconds at 16kHz = ~32000 samples)
    // Allow some tolerance for chunking
    assert!(
        total_samples > 20000,
        "Should receive approximately 2 seconds of audio (got {} samples)",
        total_samples
    );

    source.stop().await.expect("Failed to stop");
    assert_eq!(source.status(), IngestStatus::Disconnected);
}

/// T092 variant: Test stereo WAV file
#[tokio::test]
async fn test_stereo_wav_produces_audio() {
    let wav_file = generate_test_wav(1.0, 44100, 2);
    let wav_path = wav_file.path().to_string_lossy().to_string();

    let registry = global_ingest_registry();

    let config = IngestConfig {
        url: wav_path,
        audio: Some(AudioConfig {
            sample_rate: 16000, // Request downsampled
            channels: 1,       // Request mono
        }),
        video: None,
        track_selection: TrackSelection::FirstAudioVideo,
        reconnect: ReconnectConfig::disabled(),
        extra: None,
    };

    let mut source = registry
        .create_from_uri(&config)
        .expect("Failed to create source");

    let mut stream = source.start().await.expect("Failed to start");

    let mut chunk_count = 0;
    loop {
        match tokio::time::timeout(Duration::from_secs(5), stream.recv()).await {
            Ok(Some(RuntimeData::Audio { .. })) => chunk_count += 1,
            Ok(Some(_)) => {} // Ignore non-audio
            Ok(None) => break,
            Err(_) => break, // Timeout
        }
    }

    assert!(chunk_count > 0, "Should receive audio chunks from stereo file");
    println!("✓ Received {} chunks from stereo WAV", chunk_count);

    source.stop().await.ok();
}

/// T093: Integration test: ingest MP4 file produces Audio (+ Video) chunks
///
/// Note: Current implementation focuses on audio. Video support is marked TODO.
#[tokio::test]
#[ignore = "Requires MP4 file - adjust path or run with --ignored"]
async fn test_mp4_file_produces_chunks() {
    // Use an existing MP4 file if available
    let mp4_path = "/home/acidhax/dev/personal/remotemedia-sdk/input.mp4";

    if !std::path::Path::new(mp4_path).exists() {
        println!("Skipping test - MP4 file not found at {}", mp4_path);
        return;
    }

    let registry = global_ingest_registry();

    let config = IngestConfig {
        url: mp4_path.to_string(),
        audio: Some(AudioConfig {
            sample_rate: 16000,
            channels: 1,
        }),
        video: None,
        track_selection: TrackSelection::All,
        reconnect: ReconnectConfig::disabled(),
        extra: None,
    };

    let mut source = registry
        .create_from_uri(&config)
        .expect("Failed to create source");

    let mut stream = source.start().await.expect("Failed to start");

    let mut audio_count = 0;
    let mut video_count = 0;

    // Receive chunks for up to 10 seconds of playback
    let timeout = tokio::time::Instant::now() + Duration::from_secs(10);

    while tokio::time::Instant::now() < timeout {
        match tokio::time::timeout(Duration::from_secs(2), stream.recv()).await {
            Ok(Some(data)) => match data {
                RuntimeData::Audio { stream_id, .. } => {
                    audio_count += 1;
                    if audio_count == 1 {
                        println!("First audio stream_id: {:?}", stream_id);
                    }
                }
                RuntimeData::Video { stream_id, .. } => {
                    video_count += 1;
                    if video_count == 1 {
                        println!("First video stream_id: {:?}", stream_id);
                    }
                }
                _ => {}
            },
            Ok(None) => break, // EOF
            Err(_) => break,   // Timeout
        }
    }

    println!(
        "✓ MP4 file produced {} audio chunks, {} video chunks",
        audio_count, video_count
    );

    assert!(audio_count > 0, "Should receive audio chunks from MP4");
    // Video support is TODO (T062)

    source.stop().await.ok();
}

/// Test file:// URL scheme
#[tokio::test]
async fn test_file_url_scheme() {
    let wav_file = generate_test_wav(0.5, 16000, 1);
    let file_url = format!("file://{}", wav_file.path().to_string_lossy());

    let registry = global_ingest_registry();

    let config = IngestConfig {
        url: file_url,
        audio: Some(AudioConfig {
            sample_rate: 16000,
            channels: 1,
        }),
        video: None,
        track_selection: TrackSelection::FirstAudioVideo,
        reconnect: ReconnectConfig::disabled(),
        extra: None,
    };

    let mut source = registry
        .create_from_uri(&config)
        .expect("Failed to create source from file:// URL");

    let mut stream = source.start().await.expect("Failed to start");

    let mut got_audio = false;
    while let Ok(Some(data)) =
        tokio::time::timeout(Duration::from_secs(5), stream.recv()).await
    {
        if matches!(data, RuntimeData::Audio { .. }) {
            got_audio = true;
            break;
        }
    }

    assert!(got_audio, "Should receive audio from file:// URL");
    source.stop().await.ok();
}

/// Test bare file path (no scheme)
#[tokio::test]
async fn test_bare_file_path() {
    let wav_file = generate_test_wav(0.5, 16000, 1);
    let bare_path = wav_file.path().to_string_lossy().to_string();

    let registry = global_ingest_registry();

    let config = IngestConfig::from_url(&bare_path);

    let mut source = registry
        .create_from_uri(&config)
        .expect("Failed to create source from bare path");

    let mut stream = source.start().await.expect("Failed to start");

    let mut got_audio = false;
    while let Ok(Some(data)) =
        tokio::time::timeout(Duration::from_secs(5), stream.recv()).await
    {
        if matches!(data, RuntimeData::Audio { .. }) {
            got_audio = true;
            break;
        }
    }

    assert!(got_audio, "Should receive audio from bare file path");
    source.stop().await.ok();
}

/// Test that non-existent file returns error
#[tokio::test]
async fn test_nonexistent_file_error() {
    let registry = global_ingest_registry();

    let config = IngestConfig::from_url("/nonexistent/path/to/file.wav");

    let result = registry.create_from_uri(&config);
    assert!(result.is_err(), "Should fail for non-existent file");

    if let Err(e) = result {
        let err_str = format!("{:?}", e);
        assert!(
            err_str.contains("not found") || err_str.contains("FileNotFound") || err_str.contains("exist"),
            "Error should indicate file not found: {}",
            err_str
        );
    }
}

/// Test registry lists file scheme
#[test]
fn test_registry_lists_file_scheme() {
    let registry = global_ingest_registry();
    let schemes = registry.list_schemes();

    assert!(
        schemes.contains(&"file".to_string()) || schemes.contains(&"".to_string()),
        "Registry should support file scheme"
    );
}

/// Test registry lists file plugin
#[test]
fn test_registry_lists_file_plugin() {
    let registry = global_ingest_registry();
    let plugins = registry.list_plugins();

    assert!(
        plugins.contains(&"file".to_string()),
        "Registry should have file plugin"
    );
}

/// Performance test: first chunk latency
#[tokio::test]
async fn test_first_chunk_latency() {
    let wav_file = generate_test_wav(5.0, 16000, 1);
    let wav_path = wav_file.path().to_string_lossy().to_string();

    let registry = global_ingest_registry();
    let config = IngestConfig::from_url(&wav_path);

    let start = std::time::Instant::now();

    let mut source = registry
        .create_from_uri(&config)
        .expect("Failed to create source");

    let mut stream = source.start().await.expect("Failed to start");

    // Wait for first chunk
    if let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_secs(5), stream.recv()).await
    {
        let latency = start.elapsed();
        println!("✓ First chunk latency: {:?}", latency);

        // SC-001: First chunk should arrive within 100ms for file ingest
        assert!(
            latency < Duration::from_millis(100),
            "First chunk latency {} ms exceeds 100ms target",
            latency.as_millis()
        );
    } else {
        panic!("Did not receive first chunk");
    }

    source.stop().await.ok();
}
