//! Tests for pipeline capability validation (spec 022)
//!
//! These tests verify that the transcribe-srt pipeline can be validated
//! using the media capabilities system.

use remotemedia_runtime_core::capabilities::{
    validation::{validate_pipeline, CapabilityValidationResult},
    AudioConstraints, AudioSampleFormat, ConstraintValue, MediaCapabilities, MediaConstraints,
    TextConstraints,
};
use std::collections::HashMap;

/// Simulates the media capabilities for the transcribe-srt-mic-input pipeline:
/// MicInput (16kHz mono f32) -> RustWhisperNode (16kHz mono f32 -> JSON) -> SrtOutput (JSON -> SRT)
fn create_pipeline_capabilities() -> HashMap<String, MediaCapabilities> {
    let mut caps = HashMap::new();

    // MicInput: source node producing 16kHz mono f32 audio (matches pipeline config)
    caps.insert(
        "mic-input".to_string(),
        MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(16000)),
            channels: Some(ConstraintValue::Exact(1)),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        })),
    );

    // RustWhisperNode: accepts 16kHz mono f32, outputs JSON text
    caps.insert(
        "whisper".to_string(),
        MediaCapabilities::with_input_output(
            MediaConstraints::Audio(AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(16000)),
                channels: Some(ConstraintValue::Exact(1)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            }),
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("json".to_string())),
            }),
        ),
    );

    // SrtOutput: accepts JSON text, outputs SRT text
    caps.insert(
        "srt".to_string(),
        MediaCapabilities::with_input_output(
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("json".to_string())),
            }),
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("srt".to_string())),
            }),
        ),
    );

    caps
}

#[test]
fn test_transcribe_srt_pipeline_valid() {
    // Pipeline connections from transcribe-srt-mic-input.yaml:
    // mic-input -> whisper (implicit, mic-input is source)
    // whisper -> srt
    let connections = vec![
        ("mic-input".to_string(), "whisper".to_string()),
        ("whisper".to_string(), "srt".to_string()),
    ];

    let caps = create_pipeline_capabilities();
    let result = validate_pipeline(&caps, &connections);

    match result {
        CapabilityValidationResult::Valid => {
            // Expected: pipeline is valid
        }
        CapabilityValidationResult::Invalid(mismatches) => {
            panic!(
                "Pipeline should be valid, but got {} mismatches: {:?}",
                mismatches.len(),
                mismatches
            );
        }
    }
}

#[test]
fn test_pipeline_with_sample_rate_mismatch() {
    let mut caps = HashMap::new();

    // MicInput at 48kHz (mismatched with Whisper's 16kHz requirement)
    caps.insert(
        "mic-input".to_string(),
        MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(48000)), // Mismatch!
            channels: Some(ConstraintValue::Exact(1)),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        })),
    );

    // RustWhisperNode requires 16kHz
    caps.insert(
        "whisper".to_string(),
        MediaCapabilities::with_input_output(
            MediaConstraints::Audio(AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(16000)),
                channels: Some(ConstraintValue::Exact(1)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            }),
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("json".to_string())),
            }),
        ),
    );

    // SrtOutput (should still be valid)
    caps.insert(
        "srt".to_string(),
        MediaCapabilities::with_input_output(
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("json".to_string())),
            }),
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("srt".to_string())),
            }),
        ),
    );

    let connections = vec![
        ("mic-input".to_string(), "whisper".to_string()),
        ("whisper".to_string(), "srt".to_string()),
    ];

    let result = validate_pipeline(&caps, &connections);

    match result {
        CapabilityValidationResult::Valid => {
            panic!("Pipeline should have sample_rate mismatch, but was valid");
        }
        CapabilityValidationResult::Invalid(mismatches) => {
            assert!(!mismatches.is_empty(), "Should have at least one mismatch");

            // Find the sample_rate mismatch
            let has_sample_rate_mismatch = mismatches.iter().any(|m| {
                m.source_node == "mic-input"
                    && m.target_node == "whisper"
                    && m.constraint_name == "sample_rate"
            });

            assert!(
                has_sample_rate_mismatch,
                "Should have sample_rate mismatch between mic-input and whisper. Got: {:?}",
                mismatches
            );
        }
    }
}

#[test]
fn test_pipeline_with_channel_mismatch() {
    let mut caps = HashMap::new();

    // MicInput with stereo (2 channels) - mismatched with Whisper's mono requirement
    caps.insert(
        "mic-input".to_string(),
        MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(16000)),
            channels: Some(ConstraintValue::Exact(2)), // Stereo - mismatch!
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        })),
    );

    // RustWhisperNode requires mono
    caps.insert(
        "whisper".to_string(),
        MediaCapabilities::with_input_output(
            MediaConstraints::Audio(AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(16000)),
                channels: Some(ConstraintValue::Exact(1)), // Mono required
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            }),
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("json".to_string())),
            }),
        ),
    );

    let connections = vec![("mic-input".to_string(), "whisper".to_string())];

    let result = validate_pipeline(&caps, &connections);

    match result {
        CapabilityValidationResult::Valid => {
            panic!("Pipeline should have channel mismatch, but was valid");
        }
        CapabilityValidationResult::Invalid(mismatches) => {
            let has_channel_mismatch = mismatches.iter().any(|m| {
                m.source_node == "mic-input"
                    && m.target_node == "whisper"
                    && m.constraint_name == "channels"
            });

            assert!(
                has_channel_mismatch,
                "Should have channel mismatch. Got: {:?}",
                mismatches
            );
        }
    }
}

#[test]
fn test_pipeline_with_media_type_mismatch() {
    let mut caps = HashMap::new();

    // MicInput produces audio
    caps.insert(
        "mic-input".to_string(),
        MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(16000)),
            channels: Some(ConstraintValue::Exact(1)),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        })),
    );

    // SrtOutput expects Text, not Audio - media type mismatch!
    caps.insert(
        "srt".to_string(),
        MediaCapabilities::with_input(MediaConstraints::Text(TextConstraints {
            encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
            format: Some(ConstraintValue::Exact("json".to_string())),
        })),
    );

    // Direct connection mic-input -> srt (skipping whisper) should fail
    let connections = vec![("mic-input".to_string(), "srt".to_string())];

    let result = validate_pipeline(&caps, &connections);

    match result {
        CapabilityValidationResult::Valid => {
            panic!("Pipeline should have media type mismatch (audio vs text), but was valid");
        }
        CapabilityValidationResult::Invalid(mismatches) => {
            let has_type_mismatch = mismatches.iter().any(|m| {
                m.source_node == "mic-input"
                    && m.target_node == "srt"
                    && m.constraint_name == "media_type"
            });

            assert!(
                has_type_mismatch,
                "Should have media type mismatch. Got: {:?}",
                mismatches
            );
        }
    }
}

#[test]
fn test_pipeline_with_flexible_sample_rate() {
    let mut caps = HashMap::new();

    // MicInput with flexible sample rate range
    caps.insert(
        "mic-input".to_string(),
        MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Range {
                min: 8000,
                max: 48000,
            }),
            channels: Some(ConstraintValue::Exact(1)),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        })),
    );

    // Whisper requires exactly 16kHz (which is in the range)
    caps.insert(
        "whisper".to_string(),
        MediaCapabilities::with_input(MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(16000)),
            channels: Some(ConstraintValue::Exact(1)),
            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
        })),
    );

    let connections = vec![("mic-input".to_string(), "whisper".to_string())];

    let result = validate_pipeline(&caps, &connections);

    // Should be valid because 16kHz is within the 8000-48000 range
    match result {
        CapabilityValidationResult::Valid => {
            // Expected
        }
        CapabilityValidationResult::Invalid(mismatches) => {
            panic!(
                "Pipeline should be valid (16kHz in 8-48kHz range), got mismatches: {:?}",
                mismatches
            );
        }
    }
}
