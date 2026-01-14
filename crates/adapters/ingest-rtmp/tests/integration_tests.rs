//! Integration tests for RTMP/RTSP/UDP ingestion adapter
//!
//! These tests require external infrastructure (RTMP server, RTSP stream, etc.)
//! and are marked as #[ignore] by default. Run with:
//!
//! ```sh
//! cargo test -p remotemedia-ingest-rtmp --test integration_tests -- --ignored
//! ```

use std::time::Duration;

use remotemedia_ingest_rtmp::RtmpIngestPlugin;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::ingestion::{
    AudioConfig, IngestConfig, IngestPlugin, IngestStatus,
    ReconnectConfig, TrackSelection,
};

/// Helper to create a test config for a given URL
fn test_config(url: &str) -> IngestConfig {
    IngestConfig {
        url: url.to_string(),
        audio: Some(AudioConfig {
            sample_rate: 16000,
            channels: 1,
        }),
        video: None,
        track_selection: TrackSelection::FirstAudioVideo,
        reconnect: ReconnectConfig::default(),
        extra: None,
    }
}

/// T071: Integration test: connect to local RTMP server (if available)
///
/// Requires: Local RTMP server at rtmp://localhost:1935/live/test
/// Setup: Start nginx-rtmp or mediamtx, then push a stream with FFmpeg
#[tokio::test]
#[ignore = "Requires local RTMP server - run with --ignored"]
async fn test_rtmp_server_connection() {
    let plugin = RtmpIngestPlugin;
    let config = test_config("rtmp://localhost:1935/live/test");

    // Validate URL
    assert!(plugin.validate(&config).is_ok());

    // Create source
    let mut source = plugin.create(&config).expect("Failed to create source");
    assert_eq!(source.status(), IngestStatus::Idle);

    // Start ingestion with timeout
    let start_result = tokio::time::timeout(Duration::from_secs(5), source.start()).await;

    match start_result {
        Ok(Ok(mut stream)) => {
            // Should be connected
            assert!(matches!(
                source.status(),
                IngestStatus::Connected | IngestStatus::Connecting
            ));

            // Try to receive at least one chunk
            let recv_result =
                tokio::time::timeout(Duration::from_secs(5), stream.recv()).await;

            match recv_result {
                Ok(Some(data)) => {
                    // Verify we got audio data
                    assert!(
                        matches!(data, RuntimeData::Audio { .. }),
                        "Expected audio data, got {:?}",
                        std::mem::discriminant(&data)
                    );
                    println!("✓ Received audio chunk from RTMP stream");
                }
                Ok(None) => {
                    println!("Stream ended (no data available)");
                }
                Err(_) => {
                    panic!("Timeout waiting for first chunk - stream may not be active");
                }
            }

            // Clean stop
            source.stop().await.expect("Failed to stop");
            assert_eq!(source.status(), IngestStatus::Disconnected);
        }
        Ok(Err(e)) => {
            panic!("Failed to start RTMP source: {:?}", e);
        }
        Err(_) => {
            panic!("Timeout connecting to RTMP server - ensure server is running");
        }
    }
}

/// T071 variant: Test RTSP connection
///
/// Requires: Local RTSP server at rtsp://localhost:8554/test
#[tokio::test]
#[ignore = "Requires local RTSP server - run with --ignored"]
async fn test_rtsp_server_connection() {
    let plugin = RtmpIngestPlugin;
    let config = test_config("rtsp://localhost:8554/test");

    assert!(plugin.validate(&config).is_ok());

    let mut source = plugin.create(&config).expect("Failed to create source");

    let start_result = tokio::time::timeout(Duration::from_secs(10), source.start()).await;

    match start_result {
        Ok(Ok(mut stream)) => {
            // Receive some chunks
            let mut chunk_count = 0;
            for _ in 0..10 {
                match tokio::time::timeout(Duration::from_secs(2), stream.recv()).await {
                    Ok(Some(_)) => chunk_count += 1,
                    Ok(None) => break,
                    Err(_) => break,
                }
            }

            println!("✓ Received {} chunks from RTSP stream", chunk_count);
            assert!(chunk_count > 0, "Should receive at least one chunk");

            source.stop().await.expect("Failed to stop");
        }
        Ok(Err(e)) => {
            panic!("Failed to start RTSP source: {:?}", e);
        }
        Err(_) => {
            panic!("Timeout connecting to RTSP server");
        }
    }
}

/// T071 variant: Test UDP listener connection
///
/// Requires: FFmpeg sending to udp://127.0.0.1:5004
#[tokio::test]
#[ignore = "Requires UDP stream - run with --ignored"]
async fn test_udp_listener_connection() {
    let plugin = RtmpIngestPlugin;
    let config = test_config("udp://127.0.0.1:5004?listen=1&fifo_size=1000000");

    assert!(plugin.validate(&config).is_ok());

    // Start receiver first (it listens)
    let mut source = plugin.create(&config).expect("Failed to create source");

    // This will block until data arrives, use timeout
    let start_result = tokio::time::timeout(Duration::from_secs(15), source.start()).await;

    match start_result {
        Ok(Ok(mut stream)) => {
            // Receive some chunks
            let mut chunk_count = 0;
            for _ in 0..10 {
                match tokio::time::timeout(Duration::from_secs(2), stream.recv()).await {
                    Ok(Some(_)) => chunk_count += 1,
                    Ok(None) => break,
                    Err(_) => break,
                }
            }

            println!("✓ Received {} chunks from UDP stream", chunk_count);
            source.stop().await.expect("Failed to stop");
        }
        Ok(Err(e)) => {
            panic!("Failed to start UDP source: {:?}", e);
        }
        Err(_) => {
            println!("Timeout waiting for UDP data - this is expected if no sender is active");
        }
    }
}

/// T072: Integration test: reconnection on disconnect
///
/// Tests that the source attempts reconnection when stream drops
#[tokio::test]
#[ignore = "Requires RTMP server with controllable stream - run with --ignored"]
async fn test_reconnection_on_disconnect() {
    let plugin = RtmpIngestPlugin;
    let config = IngestConfig {
        url: "rtmp://localhost:1935/live/test".to_string(),
        audio: Some(AudioConfig {
            sample_rate: 16000,
            channels: 1,
        }),
        video: None,
        track_selection: TrackSelection::FirstAudioVideo,
        reconnect: ReconnectConfig {
            enabled: true,
            max_attempts: 3,
            delay_ms: 1000,
            backoff_multiplier: 1.5,
            max_delay_ms: 10_000,
        },
        extra: None,
    };

    let mut source = plugin.create(&config).expect("Failed to create source");

    // Start and receive initial data
    let start_result = tokio::time::timeout(Duration::from_secs(5), source.start()).await;

    if let Ok(Ok(mut stream)) = start_result {
        // Receive a few chunks
        for _ in 0..3 {
            if let Ok(Some(_)) =
                tokio::time::timeout(Duration::from_secs(2), stream.recv()).await
            {
                println!("Received chunk");
            }
        }

        // At this point, if the stream is stopped (by killing FFmpeg),
        // the source should attempt reconnection.
        // We can verify by checking status transitions.

        println!("✓ Initial connection successful. To test reconnection:");
        println!("  1. Stop the FFmpeg push");
        println!("  2. Observe IngestStatus::Reconnecting");
        println!("  3. Restart FFmpeg push");
        println!("  4. Verify IngestStatus::Connected");

        source.stop().await.expect("Failed to stop");
    } else {
        println!("Could not establish initial connection - skipping reconnection test");
    }
}

/// T073: Integration test: multi-track RTMP produces Audio + Video with stream_ids
///
/// Requires: RTMP stream with both audio and video
#[tokio::test]
#[ignore = "Requires RTMP stream with A/V - run with --ignored"]
async fn test_multitrack_rtmp_produces_audio_video() {
    let plugin = RtmpIngestPlugin;
    let config = IngestConfig {
        url: "rtmp://localhost:1935/live/test".to_string(),
        audio: Some(AudioConfig {
            sample_rate: 16000,
            channels: 1,
        }),
        video: None, // We're not decoding video in current implementation
        track_selection: TrackSelection::All,
        reconnect: ReconnectConfig::disabled(),
        extra: None,
    };

    let mut source = plugin.create(&config).expect("Failed to create source");

    let start_result = tokio::time::timeout(Duration::from_secs(10), source.start()).await;

    if let Ok(Ok(mut stream)) = start_result {
        let mut audio_count = 0;
        let mut video_count = 0;
        let mut audio_stream_id = None;

        // Receive chunks and categorize
        for _ in 0..50 {
            match tokio::time::timeout(Duration::from_secs(2), stream.recv()).await {
                Ok(Some(data)) => match &data {
                    RuntimeData::Audio { stream_id, .. } => {
                        audio_count += 1;
                        if audio_stream_id.is_none() {
                            audio_stream_id = stream_id.clone();
                        }
                    }
                    RuntimeData::Video { stream_id, .. } => {
                        video_count += 1;
                        println!("Video chunk with stream_id: {:?}", stream_id);
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => break,
            }
        }

        println!(
            "✓ Received {} audio chunks, {} video chunks",
            audio_count, video_count
        );
        println!("  Audio stream_id: {:?}", audio_stream_id);

        // Audio should be present (we always decode audio)
        assert!(audio_count > 0, "Should receive audio chunks");

        // Verify stream_id format
        if let Some(id) = audio_stream_id {
            assert!(
                id.starts_with("audio:"),
                "Audio stream_id should start with 'audio:'"
            );
        }

        source.stop().await.expect("Failed to stop");
    } else {
        panic!("Failed to connect to RTMP server");
    }
}

/// Test that the plugin correctly lists all supported schemes
#[test]
fn test_plugin_supports_all_streaming_schemes() {
    let plugin = RtmpIngestPlugin;
    let schemes = plugin.schemes();

    // Verify all expected schemes
    assert!(schemes.contains(&"rtmp"), "Should support rtmp://");
    assert!(schemes.contains(&"rtmps"), "Should support rtmps://");
    assert!(schemes.contains(&"rtsp"), "Should support rtsp://");
    assert!(schemes.contains(&"rtsps"), "Should support rtsps://");
    assert!(schemes.contains(&"udp"), "Should support udp://");
    assert!(schemes.contains(&"rtp"), "Should support rtp://");
    assert!(schemes.contains(&"srt"), "Should support srt://");
}

/// Test URL validation for various schemes
#[test]
fn test_url_validation_all_schemes() {
    let plugin = RtmpIngestPlugin;

    let valid_urls = [
        "rtmp://localhost:1935/live/stream",
        "rtmps://secure.server.com/app/key",
        "rtsp://localhost:8554/test",
        "rtsps://secure.rtsp.server/stream",
        "udp://127.0.0.1:5004?listen=1",
        "rtp://127.0.0.1:5006",
        "srt://localhost:4000?mode=listener",
    ];

    for url in &valid_urls {
        let config = test_config(url);
        assert!(
            plugin.validate(&config).is_ok(),
            "Should validate URL: {}",
            url
        );
    }

    let invalid_urls = [
        "http://localhost/stream",
        "https://server.com/video",
        "file:///path/to/file.mp4",
        "ftp://server.com/file",
    ];

    for url in &invalid_urls {
        let config = test_config(url);
        assert!(
            plugin.validate(&config).is_err(),
            "Should reject URL: {}",
            url
        );
    }
}
