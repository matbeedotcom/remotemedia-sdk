//! Integration tests for SRT Ingest Gateway
//!
//! These tests verify the complete flow from API to event emission.

use std::time::Duration;

use remotemedia_health_analyzer::HealthEvent;
use remotemedia_ingest_srt::config::Config;
use remotemedia_ingest_srt::pipeline::PipelineTemplate;
use remotemedia_ingest_srt::session::{SessionConfig, SessionLimits, SessionManager};

/// Generate synthetic audio samples with specific characteristics
mod synthetic_audio {
    /// Generate silence (near-zero amplitude)
    pub fn generate_silence(num_samples: usize) -> Vec<f32> {
        vec![0.0001; num_samples] // Very low amplitude
    }

    /// Generate a sine wave at the given frequency
    pub fn generate_sine(sample_rate: u32, frequency: f32, duration_secs: f32) -> Vec<f32> {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5;
            samples.push(sample);
        }

        samples
    }

    /// Generate clipped audio (samples at ±1.0)
    pub fn generate_clipped(num_samples: usize, sample_rate: u32) -> Vec<f32> {
        let frequency = 440.0; // A4
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin() * 2.0; // Over-amplified
            samples.push(sample.clamp(-1.0, 1.0)); // Hard clip
        }

        samples
    }

    /// Generate low volume audio
    pub fn generate_low_volume(num_samples: usize, sample_rate: u32) -> Vec<f32> {
        let frequency = 440.0;
        let amplitude = 0.01; // Very low amplitude (-40 dB)

        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin() * amplitude;
            samples.push(sample);
        }

        samples
    }

    /// Generate channel imbalanced audio (stereo with one channel silent)
    pub fn generate_one_sided_stereo(num_samples: usize, sample_rate: u32) -> (Vec<f32>, Vec<f32>) {
        let frequency = 440.0;
        let mut left = Vec::with_capacity(num_samples);
        let mut right = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5;
            left.push(sample);
            right.push(0.0001); // Right channel nearly silent
        }

        (left, right)
    }

    /// Generate audio with intermittent dropouts
    pub fn generate_dropouts(sample_rate: u32, duration_secs: f32, dropout_duration_ms: u32) -> Vec<f32> {
        let frequency = 440.0;
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let dropout_samples = (sample_rate * dropout_duration_ms / 1000) as usize;

        let mut samples = Vec::with_capacity(num_samples);
        let mut is_dropout = false;
        let mut samples_since_last_toggle = 0usize;
        let toggle_interval = sample_rate as usize / 2; // Toggle every 500ms

        for i in 0..num_samples {
            samples_since_last_toggle += 1;

            if samples_since_last_toggle >= toggle_interval {
                is_dropout = !is_dropout;
                samples_since_last_toggle = 0;
            }

            let t = i as f32 / sample_rate as f32;
            let sample = if is_dropout && samples_since_last_toggle < dropout_samples {
                0.0001 // Dropout
            } else {
                (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5
            };

            samples.push(sample);
        }

        samples
    }

    /// Pack audio data in the format expected by the pipeline runner
    /// Format: sample_rate (4 bytes) + channels (4 bytes) + samples (f32...)
    pub fn pack_audio_chunk(samples: &[f32], sample_rate: u32, channels: u32) -> Vec<u8> {
        let mut data = Vec::with_capacity(8 + samples.len() * 4);

        // Add sample rate (4 bytes, little-endian)
        data.extend_from_slice(&sample_rate.to_le_bytes());

        // Add channels (4 bytes, little-endian)
        data.extend_from_slice(&channels.to_le_bytes());

        // Add samples (f32, little-endian)
        for sample in samples {
            data.extend_from_slice(&sample.to_le_bytes());
        }

        data
    }
}

#[tokio::test]
async fn test_session_creation_and_lifecycle() {
    let manager = SessionManager::new("test_secret".to_string(), 10);

    // Create a session
    let config = SessionConfig {
        pipeline: "test_pipeline".to_string(),
        webhook_url: None,
        audio_enabled: true,
        video_enabled: false,
        max_duration_seconds: 60,
    };

    let (session, _input_rx, _token) = manager
        .create_session(config, SessionLimits::default())
        .await
        .expect("Failed to create session");

    let session_id = session.id.clone();

    // Verify session exists
    let retrieved = manager.get_session(&session_id).await;
    assert!(retrieved.is_some());

    // Subscribe to events
    let mut event_rx = session.event_tx.subscribe();

    // Transition to connected
    session.set_connected().await.expect("Failed to set connected");

    // Transition to streaming
    session.set_streaming().await.expect("Failed to set streaming");

    // Should have received stream_started event
    let event = tokio::time::timeout(Duration::from_millis(100), event_rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Failed to receive event");

    assert!(matches!(event, HealthEvent::StreamStarted { .. }));
}

#[tokio::test]
async fn test_session_event_broadcast() {
    let manager = SessionManager::new("test_secret".to_string(), 10);

    let config = SessionConfig::default();
    let (session, _input_rx, _token) = manager
        .create_session(config, SessionLimits::default())
        .await
        .expect("Failed to create session");

    // Subscribe multiple receivers
    let mut rx1 = session.event_tx.subscribe();
    let mut rx2 = session.event_tx.subscribe();

    // Send an event
    let event = HealthEvent::silence(1000.0, -60.0, Some(session.id.clone()));
    session.event_tx.send(event.clone()).expect("Failed to send event");

    // Both receivers should get the event
    let received1 = rx1.try_recv().expect("Failed to receive on rx1");
    let received2 = rx2.try_recv().expect("Failed to receive on rx2");

    assert!(matches!(received1, HealthEvent::Silence { .. }));
    assert!(matches!(received2, HealthEvent::Silence { .. }));
}

#[tokio::test]
async fn test_max_sessions_limit() {
    let max_sessions = 2;
    let manager = SessionManager::new("test_secret".to_string(), max_sessions);

    // Create max sessions
    for _ in 0..max_sessions {
        let config = SessionConfig::default();
        manager
            .create_session(config, SessionLimits::default())
            .await
            .expect("Should create session");
    }

    // Next creation should fail
    let config = SessionConfig::default();
    let result = manager
        .create_session(config, SessionLimits::default())
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_pipeline_template_creation() {
    let template = PipelineTemplate::new(
        "test_audio".to_string(),
        "Test Audio Analysis Pipeline".to_string(), // This is the 'name' field
        r#"{"nodes": [{"id": "silence_detector", "type": "SilenceDetector"}, {"id": "clipping_detector", "type": "ClippingDetector"}]}"#.to_string(),
    )
    .with_description("Analyzes audio for quality issues".to_string());

    assert_eq!(template.id, "test_audio");
    assert_eq!(template.name, "Test Audio Analysis Pipeline");
    assert!(template.description.contains("audio"));
}

#[tokio::test]
async fn test_config_defaults() {
    let config = Config::default();

    assert_eq!(config.server.http_port, 8080);
    assert_eq!(config.server.srt_port, 9000);
    assert_eq!(config.limits.max_sessions, 100);
}

/// Test that audio analysis correctly identifies issues
mod audio_analysis_tests {
    use super::synthetic_audio::*;

    #[test]
    fn test_detect_silence() {
        let samples = generate_silence(16000); // 1 second at 16kHz

        // Calculate RMS
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        let rms = (sum / samples.len() as f32).sqrt();
        let db = 20.0 * rms.log10();

        // Should be detected as silence (< -40 dB)
        assert!(db < -40.0, "Expected silence, got {} dB", db);
    }

    #[test]
    fn test_detect_clipping() {
        let samples = generate_clipped(16000, 16000);

        // Count clipping samples
        let clipping_threshold = 0.95;
        let clipping_count = samples.iter().filter(|s| s.abs() > clipping_threshold).count();
        let clipping_ratio = clipping_count as f32 / samples.len() as f32;

        // Should have significant clipping
        assert!(clipping_ratio > 0.01, "Expected clipping, got ratio {}", clipping_ratio);
    }

    #[test]
    fn test_detect_low_volume() {
        let samples = generate_low_volume(16000, 16000);

        // Calculate RMS level
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        let rms = (sum / samples.len() as f32).sqrt();
        let db = 20.0 * rms.log10();

        // Should be low volume (between -60 and -35 dB)
        assert!(db < -35.0 && db > -60.0, "Expected low volume, got {} dB", db);
    }

    #[test]
    fn test_normal_audio_not_detected_as_issue() {
        let samples = generate_sine(16000, 440.0, 1.0);

        // Calculate RMS level
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        let rms = (sum / samples.len() as f32).sqrt();
        let db = 20.0 * rms.log10();

        // Normal audio should be above -35 dB
        assert!(db > -35.0, "Normal audio detected as low volume: {} dB", db);

        // Should not have clipping
        let clipping_count = samples.iter().filter(|s| s.abs() > 0.95).count();
        assert_eq!(clipping_count, 0, "Normal audio detected as clipping");
    }

    #[test]
    fn test_audio_chunk_packing() {
        let samples = vec![0.5f32, -0.5, 0.25, -0.25];
        let packed = pack_audio_chunk(&samples, 16000, 1);

        // Verify header
        assert_eq!(packed.len(), 8 + samples.len() * 4);

        // Verify sample rate
        let sample_rate = u32::from_le_bytes([packed[0], packed[1], packed[2], packed[3]]);
        assert_eq!(sample_rate, 16000);

        // Verify channels
        let channels = u32::from_le_bytes([packed[4], packed[5], packed[6], packed[7]]);
        assert_eq!(channels, 1);

        // Verify first sample
        let first_sample = f32::from_le_bytes([packed[8], packed[9], packed[10], packed[11]]);
        assert!((first_sample - 0.5).abs() < 0.0001);
    }
}

/// Test JWT token generation and validation
mod jwt_tests {
    use remotemedia_ingest_srt::jwt::JwtValidator;

    #[test]
    fn test_jwt_token_roundtrip() {
        let secret = "test_secret_key_12345";
        let validator = JwtValidator::new(secret.to_string());

        let session_id = "sess_test123";
        let token = validator.generate(session_id, 900).expect("Failed to generate token");

        // Validate the token
        let claims = validator.validate(&token).expect("Failed to validate token");
        assert_eq!(claims.session_id, session_id);
    }

    #[test]
    fn test_jwt_validation_for_session() {
        let secret = "test_secret_key_12345";
        let validator = JwtValidator::new(secret.to_string());

        let session_id = "sess_test456";
        let token = validator.generate(session_id, 900).expect("Failed to generate token");

        // Should succeed for matching session
        assert!(validator.validate_for_session(&token, session_id).is_ok());

        // Should fail for different session
        assert!(validator.validate_for_session(&token, "sess_different").is_err());
    }

    #[test]
    fn test_jwt_wrong_secret_fails() {
        let validator1 = JwtValidator::new("secret1".to_string());
        let validator2 = JwtValidator::new("secret2".to_string());

        let token = validator1.generate("test_session", 900).expect("Failed to generate token");

        // Validation with different secret should fail
        assert!(validator2.validate(&token).is_err());
    }
}

/// Test streamid parsing
mod streamid_tests {
    use remotemedia_ingest_srt::streamid::StreamIdParams;

    #[test]
    fn test_streamid_parse_full() {
        // Format: r=session_id, token=jwt, p=pipeline
        let streamid = "#!::r=session_123,token=jwt_token_here,p=test_pipeline";
        let params = StreamIdParams::parse(streamid).expect("Failed to parse");

        assert_eq!(params.session_id, "session_123");
        assert_eq!(params.pipeline, "test_pipeline");
        assert_eq!(params.token, "jwt_token_here");
    }

    #[test]
    fn test_streamid_roundtrip() {
        let params = StreamIdParams::new(
            "sess_abc".to_string(),
            "my_jwt_token".to_string(),
            "audio_quality".to_string(),
        );

        let streamid = params.to_streamid();
        let parsed = StreamIdParams::parse(&streamid).expect("Failed to parse");

        assert_eq!(parsed.session_id, params.session_id);
        assert_eq!(parsed.pipeline, params.pipeline);
        assert_eq!(parsed.token, params.token);
    }
}

/// Test metrics collection
mod metrics_tests {
    use remotemedia_ingest_srt::metrics::Metrics;

    #[test]
    fn test_metrics_session_tracking() {
        let metrics = Metrics::new();

        assert_eq!(metrics.active_session_count(), 0);

        metrics.session_created();
        metrics.session_created();
        assert_eq!(metrics.active_session_count(), 2);

        metrics.session_ended();
        assert_eq!(metrics.active_session_count(), 1);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.sessions_created, 2);
        assert_eq!(snapshot.sessions_ended, 1);
    }

    #[test]
    fn test_metrics_webhook_tracking() {
        let metrics = Metrics::new();

        metrics.webhook_attempted();
        metrics.webhook_succeeded();
        metrics.webhook_attempted();
        metrics.webhook_failed();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.webhook_attempts, 2);
        assert_eq!(snapshot.webhook_successes, 1);
        assert_eq!(snapshot.webhook_failures, 1);

        // Success rate should be 50%
        assert!((snapshot.webhook_success_rate() - 0.5).abs() < 0.01);
    }
}
