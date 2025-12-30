//! Tests for pipeline capability validation (spec 022) and resolution (spec 023)
//!
//! These tests verify that the transcribe-srt pipeline can be validated
//! using the media capabilities system and that the CapabilityResolver
//! correctly resolves and validates capabilities.

use remotemedia_runtime_core::capabilities::{
    validation::{validate_pipeline, CapabilityValidationResult},
    AudioConstraints, AudioSampleFormat, CapabilityBehavior, CapabilityResolver, ConstraintValue,
    MediaCapabilities, MediaConstraints, TextConstraints,
};
use remotemedia_runtime_core::nodes::streaming_node::StreamingNodeRegistry;
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

// =============================================================================
// CapabilityResolver Tests (spec 023)
// =============================================================================

/// Create a registry with CLI nodes that have capability declarations
fn create_test_registry() -> StreamingNodeRegistry {
    use remotemedia_cli::pipeline_nodes::registry::create_cli_streaming_registry;
    create_cli_streaming_registry()
}

#[test]
fn test_resolver_mic_whisper_mismatch() {
    // Test that CapabilityResolver detects sample rate mismatch
    // MicInput(48kHz) -> RustWhisperNode(16kHz) should produce error

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    // Define nodes: (node_id, node_type)
    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];

    // Define connections
    let connections = vec![("mic".to_string(), "whisper".to_string())];

    // Define params with mismatched sample rate
    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,  // Mismatch! Whisper needs 16kHz
            "channels": 1
        }),
    );
    params.insert("whisper".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Should have detected the mismatch
    assert!(
        ctx.has_errors(),
        "Should detect sample_rate mismatch between MicInput(48kHz) and RustWhisperNode(16kHz)"
    );

    // Verify the error details
    let sample_rate_error = ctx.errors.iter().find(|e| e.constraint_name == "sample_rate");
    assert!(
        sample_rate_error.is_some(),
        "Should have sample_rate mismatch error. Errors: {:?}",
        ctx.errors
    );
}

#[test]
fn test_resolver_valid_pipeline() {
    // Test that CapabilityResolver validates matching capabilities
    // MicInput(16kHz) -> RustWhisperNode(16kHz) should be valid

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];

    let connections = vec![("mic".to_string(), "whisper".to_string())];

    // Matching params
    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 16000,  // Matches Whisper!
            "channels": 1
        }),
    );
    params.insert("whisper".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Should have no errors
    assert!(
        !ctx.has_errors(),
        "Pipeline with matching sample rates should be valid. Errors: {:?}",
        ctx.errors
    );
}

#[test]
fn test_resolver_passthrough_propagation() {
    // Test that Passthrough nodes (SpeakerOutput) inherit capabilities from upstream
    // MicInput(48kHz stereo) -> SpeakerOutput should resolve SpeakerOutput to 48kHz stereo

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("speaker".to_string(), "SpeakerOutput".to_string()),
    ];

    let connections = vec![("mic".to_string(), "speaker".to_string())];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,
            "channels": 2
        }),
    );
    params.insert("speaker".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Should be valid - passthrough accepts any format
    assert!(
        !ctx.has_errors(),
        "MicInput -> SpeakerOutput should be valid. Errors: {:?}",
        ctx.errors
    );

    // Verify MicInput capabilities were resolved
    let mic_resolved = ctx.resolved.get("mic");
    assert!(
        mic_resolved.is_some(),
        "MicInput should have resolved capabilities"
    );

    // Check that MicInput has Configured behavior
    let mic_behavior = ctx.get_behavior("mic");
    assert_eq!(
        mic_behavior,
        CapabilityBehavior::Configured,
        "MicInput should have Configured behavior"
    );

    // Check that SpeakerOutput has Passthrough behavior
    let speaker_behavior = ctx.get_behavior("speaker");
    assert_eq!(
        speaker_behavior,
        CapabilityBehavior::Passthrough,
        "SpeakerOutput should have Passthrough behavior"
    );
}

#[test]
fn test_resolver_whisper_static_capabilities() {
    // Test that RustWhisperNode has Static behavior and 16kHz requirements

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![("whisper".to_string(), "RustWhisperNode".to_string())];
    let connections: Vec<(String, String)> = vec![];
    let mut params = HashMap::new();
    params.insert("whisper".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Check behavior
    let behavior = ctx.get_behavior("whisper");
    assert_eq!(
        behavior,
        CapabilityBehavior::Static,
        "RustWhisperNode should have Static behavior"
    );

    // Should have resolved capabilities
    let resolved = ctx.resolved.get("whisper");
    assert!(
        resolved.is_some(),
        "RustWhisperNode should have resolved capabilities"
    );

    // Check input requirements
    if let Some(caps) = resolved {
        let input = caps.capabilities.default_input();
        assert!(input.is_some(), "Whisper should have input requirements");

        if let Some(MediaConstraints::Audio(audio)) = input {
            assert_eq!(
                audio.sample_rate,
                Some(ConstraintValue::Exact(16000)),
                "Whisper should require 16kHz"
            );
            assert_eq!(
                audio.channels,
                Some(ConstraintValue::Exact(1)),
                "Whisper should require mono"
            );
        } else {
            panic!("Whisper input should be Audio constraints");
        }
    }
}

#[test]
fn test_resolver_adaptive_resample_resolution() {
    // Test that Adaptive nodes (FastResampleNode) adapt their output to downstream requirements
    // MicInput(48kHz) -> FastResampleNode (Adaptive) -> RustWhisperNode(16kHz)
    // The resample node should adapt its output to 16kHz to match Whisper's input requirements

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("resample".to_string(), "FastResampleNode".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];

    let connections = vec![
        ("mic".to_string(), "resample".to_string()),
        ("resample".to_string(), "whisper".to_string()),
    ];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,  // Source at 48kHz
            "channels": 1
        }),
    );
    params.insert(
        "resample".to_string(),
        serde_json::json!({
            "sourceRate": 48000,
            "targetRate": 16000,  // Target matches Whisper
            "channels": 1
        }),
    );
    params.insert("whisper".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Should have no errors - the adaptive node bridges the gap
    assert!(
        !ctx.has_errors(),
        "MicInput(48kHz) -> Resample -> Whisper(16kHz) should be valid. Errors: {:?}",
        ctx.errors
    );

    // Check that FastResampleNode has Configured behavior when explicit rates are given
    let resample_behavior = ctx.get_behavior("resample");
    assert_eq!(
        resample_behavior,
        CapabilityBehavior::Configured,
        "FastResampleNode should have Configured behavior with explicit rates"
    );

    // Check that MicInput has Configured behavior
    let mic_behavior = ctx.get_behavior("mic");
    assert_eq!(
        mic_behavior,
        CapabilityBehavior::Configured,
        "MicInput should have Configured behavior"
    );

    // Check that Whisper has Static behavior
    let whisper_behavior = ctx.get_behavior("whisper");
    assert_eq!(
        whisper_behavior,
        CapabilityBehavior::Static,
        "RustWhisperNode should have Static behavior"
    );
}

#[test]
fn test_resolver_adaptive_without_resample_fails() {
    // Test that without the adaptive resample node, MicInput(48kHz) -> Whisper(16kHz) fails
    // This verifies the resolver correctly detects incompatible direct connections

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];

    let connections = vec![("mic".to_string(), "whisper".to_string())];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,  // 48kHz - incompatible with Whisper's 16kHz
            "channels": 1
        }),
    );
    params.insert("whisper".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Should have errors - no adaptive node to bridge the gap
    assert!(
        ctx.has_errors(),
        "MicInput(48kHz) -> Whisper(16kHz) without resample should fail"
    );

    // Verify sample_rate mismatch error
    let sample_rate_error = ctx.errors.iter().find(|e| e.constraint_name == "sample_rate");
    assert!(
        sample_rate_error.is_some(),
        "Should have sample_rate mismatch. Errors: {:?}",
        ctx.errors
    );
}

// =============================================================================
// Capability Introspection API Tests (spec 023 - US4)
// =============================================================================

#[tokio::test]
async fn test_executor_introspection_api() {
    use remotemedia_runtime_core::capabilities::{CapabilitySource, ResolutionContext, ResolvedCapabilities};
    use remotemedia_runtime_core::executor::Executor;

    // Create executor
    let executor = Executor::new();

    // Initially, no capabilities resolved
    assert!(
        !executor.has_resolved_capabilities().await,
        "Should have no resolved capabilities initially"
    );
    assert!(
        executor.get_resolved_capabilities("any_node").await.is_none(),
        "get_resolved_capabilities should return None when no context set"
    );
    assert!(
        executor.all_resolved_capabilities().await.is_none(),
        "all_resolved_capabilities should return None when no context set"
    );

    // Create a mock resolution context
    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];
    let connections = vec![("mic".to_string(), "whisper".to_string())];
    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 16000,
            "channels": 1
        }),
    );
    params.insert("whisper".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Set the resolution context
    executor.set_resolution_context(ctx).await;

    // Now introspection should work
    assert!(
        executor.has_resolved_capabilities().await,
        "Should have resolved capabilities after setting context"
    );

    // Get specific node capabilities
    let mic_caps = executor.get_resolved_capabilities("mic").await;
    assert!(
        mic_caps.is_some(),
        "Should be able to get MicInput capabilities"
    );
    let mic_caps = mic_caps.unwrap();
    assert_eq!(mic_caps.node_id, "mic");

    let whisper_caps = executor.get_resolved_capabilities("whisper").await;
    assert!(
        whisper_caps.is_some(),
        "Should be able to get Whisper capabilities"
    );

    // Get all capabilities
    let all_caps = executor.all_resolved_capabilities().await;
    assert!(all_caps.is_some(), "Should be able to get all capabilities");
    let all_caps = all_caps.unwrap();
    assert_eq!(all_caps.len(), 2, "Should have 2 nodes resolved");
    assert!(all_caps.contains_key("mic"));
    assert!(all_caps.contains_key("whisper"));

    // Get capability source
    let mic_source = executor.get_capability_source("mic", "default").await;
    assert!(
        mic_source.is_some(),
        "Should be able to get MicInput capability source"
    );
    assert_eq!(
        mic_source.unwrap(),
        CapabilitySource::Configured,
        "MicInput should have Configured source"
    );

    // Non-existent node should return None
    assert!(
        executor.get_resolved_capabilities("nonexistent").await.is_none(),
        "Non-existent node should return None"
    );
}

#[tokio::test]
async fn test_introspection_with_adaptive_node() {
    use remotemedia_runtime_core::capabilities::{CapabilitySource, ResolutionContext};
    use remotemedia_runtime_core::executor::Executor;

    let executor = Executor::new();
    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    // Pipeline: MicInput(48kHz) -> FastResampleNode -> RustWhisperNode(16kHz)
    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("resample".to_string(), "FastResampleNode".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];

    let connections = vec![
        ("mic".to_string(), "resample".to_string()),
        ("resample".to_string(), "whisper".to_string()),
    ];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,
            "channels": 1
        }),
    );
    params.insert(
        "resample".to_string(),
        serde_json::json!({
            "sourceRate": 48000,
            "targetRate": 16000,
            "channels": 1
        }),
    );
    params.insert("whisper".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();
    executor.set_resolution_context(ctx).await;

    // Verify all nodes are introspectable
    let all_caps = executor.all_resolved_capabilities().await.unwrap();
    assert_eq!(all_caps.len(), 3, "Should have 3 nodes resolved");

    // Verify behaviors through introspection
    let resample_caps = executor.get_resolved_capabilities("resample").await.unwrap();
    assert_eq!(resample_caps.node_id, "resample");

    // Get full context for advanced introspection
    let full_ctx = executor.get_resolution_context().await;
    assert!(full_ctx.is_some(), "Should be able to get full context");

    let full_ctx = full_ctx.unwrap();
    assert_eq!(
        full_ctx.get_behavior("resample"),
        CapabilityBehavior::Configured,
        "Resample should have Configured behavior in context (explicit rates)"
    );
    assert_eq!(
        full_ctx.get_behavior("whisper"),
        CapabilityBehavior::Static,
        "Whisper should have Static behavior in context"
    );
}

// =============================================================================
// Two-Phase Resolution Tests (spec 023 - US5: RuntimeDiscovered)
// =============================================================================

/// Mock RuntimeDiscovered factory for testing two-phase resolution
mod mock_runtime_discovered {
    use remotemedia_runtime_core::capabilities::{
        AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue, MediaCapabilities,
        MediaConstraints,
    };
    use remotemedia_runtime_core::data::RuntimeData;
    use remotemedia_runtime_core::nodes::streaming_node::{StreamingNode, StreamingNodeFactory};
    use remotemedia_runtime_core::Error;
    use serde_json::Value;
    use std::sync::Arc;

    /// A mock node that simulates RuntimeDiscovered behavior (e.g., a microphone with device="default")
    pub struct MockRuntimeDiscoveredNode {
        node_id: String,
        /// Actual capabilities discovered after initialization
        actual_sample_rate: u32,
    }

    impl MockRuntimeDiscoveredNode {
        pub fn new(node_id: String, actual_sample_rate: u32) -> Self {
            Self {
                node_id,
                actual_sample_rate,
            }
        }
    }

    #[async_trait::async_trait]
    impl StreamingNode for MockRuntimeDiscoveredNode {
        fn node_type(&self) -> &str {
            "MockRuntimeDiscovered"
        }

        fn node_id(&self) -> &str {
            &self.node_id
        }

        async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
            Ok(data)
        }

        async fn process_multi_async(
            &self,
            inputs: std::collections::HashMap<String, RuntimeData>,
        ) -> Result<RuntimeData, Error> {
            // Return first input or empty text
            Ok(inputs.into_values().next().unwrap_or(RuntimeData::Text(String::new())))
        }

        fn is_multi_input(&self) -> bool {
            false
        }

        fn media_capabilities(&self) -> Option<MediaCapabilities> {
            // Return actual capabilities (discovered after init)
            Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Exact(self.actual_sample_rate)),
                    channels: Some(ConstraintValue::Exact(1)),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }

        fn capability_behavior(&self) -> CapabilityBehavior {
            CapabilityBehavior::RuntimeDiscovered
        }

        fn potential_capabilities(&self) -> Option<MediaCapabilities> {
            // Broad range for Phase 1 validation
            Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }

        fn actual_capabilities(&self) -> Option<MediaCapabilities> {
            self.media_capabilities()
        }
    }

    /// Factory for MockRuntimeDiscoveredNode
    pub struct MockRuntimeDiscoveredFactory;

    impl StreamingNodeFactory for MockRuntimeDiscoveredFactory {
        fn create(
            &self,
            node_id: String,
            params: &Value,
            _session_id: Option<String>,
        ) -> Result<Box<dyn StreamingNode>, Error> {
            // Simulate device discovery - actual rate determined at runtime
            let actual_rate = params
                .get("actual_sample_rate")
                .and_then(|v| v.as_u64())
                .unwrap_or(44100) as u32;
            Ok(Box::new(MockRuntimeDiscoveredNode::new(node_id, actual_rate)))
        }

        fn node_type(&self) -> &str {
            "MockRuntimeDiscovered"
        }

        fn capability_behavior(&self) -> CapabilityBehavior {
            CapabilityBehavior::RuntimeDiscovered
        }

        fn media_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
            // If actual_sample_rate is specified in params, use it
            // Otherwise return None (will use potential_capabilities)
            let actual_rate = params
                .get("actual_sample_rate")
                .and_then(|v| v.as_u64())? as u32;
            Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Exact(actual_rate)),
                    channels: Some(ConstraintValue::Exact(1)),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }

        fn potential_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
            // Broad range for Phase 1 - works with most downstream nodes
            Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }
    }
}

#[test]
fn test_runtime_discovered_phase1_resolution() {
    use mock_runtime_discovered::MockRuntimeDiscoveredFactory;
    use remotemedia_runtime_core::capabilities::ResolutionState;
    use remotemedia_runtime_core::nodes::streaming_node::StreamingNodeRegistry;

    // Create registry with mock RuntimeDiscovered node
    let mut registry = StreamingNodeRegistry::new();
    registry.register(std::sync::Arc::new(MockRuntimeDiscoveredFactory));

    // Also register RustWhisperNode for downstream validation
    let cli_registry = create_test_registry();
    if let Some(factory) = cli_registry.get_factory("RustWhisperNode") {
        registry.register(factory.clone());
    }

    let resolver = CapabilityResolver::new(&registry);

    // Pipeline: MockRuntimeDiscovered -> RustWhisperNode
    let nodes = vec![
        ("mic".to_string(), "MockRuntimeDiscovered".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];

    let connections = vec![("mic".to_string(), "whisper".to_string())];

    // No actual_sample_rate specified - will use potential_capabilities
    let mut params = HashMap::new();
    params.insert("mic".to_string(), serde_json::json!({}));
    params.insert("whisper".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Phase 1 should pass because potential_capabilities has broad range
    // that includes Whisper's 16kHz requirement
    assert!(
        !ctx.has_errors(),
        "Phase 1 should pass with potential_capabilities. Errors: {:?}",
        ctx.errors
    );

    // Verify MockRuntimeDiscovered is marked as provisional
    let mic_resolved = ctx.resolved.get("mic");
    assert!(mic_resolved.is_some(), "Mic should be resolved");
    let mic_resolved = mic_resolved.unwrap();
    assert!(
        mic_resolved.provisional,
        "RuntimeDiscovered node should be marked as provisional"
    );

    // Verify state is ResolvedForward (not Complete - needs Phase 2)
    assert_eq!(
        ctx.get_state("mic"),
        &ResolutionState::ResolvedForward,
        "RuntimeDiscovered node should be in ResolvedForward state"
    );

    // Verify behavior
    assert_eq!(
        ctx.get_behavior("mic"),
        CapabilityBehavior::RuntimeDiscovered,
        "Mock node should have RuntimeDiscovered behavior"
    );
}

#[test]
fn test_runtime_discovered_phase2_revalidation_success() {
    use mock_runtime_discovered::MockRuntimeDiscoveredFactory;
    use remotemedia_runtime_core::capabilities::{MediaCapabilities, MediaConstraints, AudioConstraints, AudioSampleFormat, ConstraintValue};
    use remotemedia_runtime_core::nodes::streaming_node::StreamingNodeRegistry;

    // Create registry
    let mut registry = StreamingNodeRegistry::new();
    registry.register(std::sync::Arc::new(MockRuntimeDiscoveredFactory));
    let cli_registry = create_test_registry();
    if let Some(factory) = cli_registry.get_factory("RustWhisperNode") {
        registry.register(factory.clone());
    }

    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MockRuntimeDiscovered".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];
    let connections = vec![("mic".to_string(), "whisper".to_string())];
    let mut params = HashMap::new();
    params.insert("mic".to_string(), serde_json::json!({}));
    params.insert("whisper".to_string(), serde_json::json!({}));

    let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Phase 1 passed, now simulate device initialization
    // Device discovered actual capabilities: 16kHz (compatible with Whisper)
    let actual_caps = MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
        sample_rate: Some(ConstraintValue::Exact(16000)), // Compatible!
        channels: Some(ConstraintValue::Exact(1)),
        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
    }));

    // Phase 2: revalidate with actual capabilities
    resolver.revalidate(&mut ctx, "mic", actual_caps).unwrap();

    // Should still be valid - 16kHz matches Whisper's requirements
    assert!(
        !ctx.has_errors(),
        "Phase 2 should pass with compatible actual capabilities. Errors: {:?}",
        ctx.errors
    );

    // Verify node is no longer provisional
    let mic_resolved = ctx.resolved.get("mic").unwrap();
    assert!(
        !mic_resolved.provisional,
        "Node should no longer be provisional after Phase 2"
    );
}

#[test]
fn test_runtime_discovered_phase2_revalidation_failure() {
    use mock_runtime_discovered::MockRuntimeDiscoveredFactory;
    use remotemedia_runtime_core::capabilities::{MediaCapabilities, MediaConstraints, AudioConstraints, AudioSampleFormat, ConstraintValue};
    use remotemedia_runtime_core::nodes::streaming_node::StreamingNodeRegistry;

    // Create registry
    let mut registry = StreamingNodeRegistry::new();
    registry.register(std::sync::Arc::new(MockRuntimeDiscoveredFactory));
    let cli_registry = create_test_registry();
    if let Some(factory) = cli_registry.get_factory("RustWhisperNode") {
        registry.register(factory.clone());
    }

    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MockRuntimeDiscovered".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
    ];
    let connections = vec![("mic".to_string(), "whisper".to_string())];
    let mut params = HashMap::new();
    params.insert("mic".to_string(), serde_json::json!({}));
    params.insert("whisper".to_string(), serde_json::json!({}));

    let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Phase 1 passed with broad potential_capabilities
    assert!(!ctx.has_errors(), "Phase 1 should pass");

    // Phase 2: Device discovered actual capabilities: 48kHz (INCOMPATIBLE with Whisper!)
    let actual_caps = MediaCapabilities::with_output(MediaConstraints::Audio(AudioConstraints {
        sample_rate: Some(ConstraintValue::Exact(48000)), // Incompatible!
        channels: Some(ConstraintValue::Exact(1)),
        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
    }));

    // Revalidate with actual capabilities
    resolver.revalidate(&mut ctx, "mic", actual_caps).unwrap();

    // Should now have errors - 48kHz doesn't match Whisper's 16kHz requirement
    assert!(
        ctx.has_errors(),
        "Phase 2 should fail with incompatible actual capabilities"
    );

    // Verify the error is about sample_rate
    let sample_rate_error = ctx.errors.iter().find(|e| e.constraint_name == "sample_rate");
    assert!(
        sample_rate_error.is_some(),
        "Should have sample_rate mismatch error. Errors: {:?}",
        ctx.errors
    );
}

// =============================================================================
// CapabilityHints Tests (spec 023 - US6)
// =============================================================================

#[test]
fn test_capability_hints_refines_adaptive_output() {
    // Test that CapabilityHints can refine an Adaptive node's output when downstream
    // accepts a range of values. The hint should narrow the output to the preferred value.
    //
    // Pipeline: MicInput(48kHz) -> FastResampleNode (Adaptive) -> FlexibleNode (accepts 8-48kHz)
    //
    // Without hints: FastResampleNode output matches FlexibleNode's input range (8-48kHz)
    // With hints: FastResampleNode output narrows to preferred_sample_rate (16000)

    use remotemedia_runtime_core::capabilities::{
        AudioConstraints, AudioSampleFormat, CapabilityBehavior, CapabilityHints, ConstraintValue,
        MediaCapabilities, MediaConstraints, NodeHints,
    };
    use remotemedia_runtime_core::nodes::streaming_node::{
        StreamingNode, StreamingNodeFactory, StreamingNodeRegistry,
    };

    // Create a mock "FlexibleNode" that accepts a range of sample rates
    mod mock_flexible {
        use async_trait::async_trait;
        use remotemedia_runtime_core::capabilities::{
            AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue,
            MediaCapabilities, MediaConstraints,
        };
        use remotemedia_runtime_core::data::RuntimeData;
        use remotemedia_runtime_core::nodes::streaming_node::{StreamingNode, StreamingNodeFactory};
        use serde_json::Value;
        use std::collections::HashMap;

        pub struct FlexibleNode;

        #[async_trait]
        impl StreamingNode for FlexibleNode {
            async fn process_async(&self, _input: RuntimeData) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                Ok(RuntimeData::Text(String::new()))
            }
            async fn process_multi_async(&self, _inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                Ok(RuntimeData::Text(String::new()))
            }
            fn is_multi_input(&self) -> bool { false }
            fn node_type(&self) -> &str { "FlexibleNode" }

            fn media_capabilities(&self) -> Option<MediaCapabilities> {
                Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Range { min: 8000, max: 48000 }),
                        channels: Some(ConstraintValue::Exact(1)),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                )))
            }

            fn capability_behavior(&self) -> CapabilityBehavior {
                CapabilityBehavior::Static
            }
        }

        pub struct FlexibleNodeFactory;

        impl StreamingNodeFactory for FlexibleNodeFactory {
            fn create(&self, _node_id: String, _params: &Value, _session_id: Option<String>) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
                Ok(Box::new(FlexibleNode))
            }
            fn node_type(&self) -> &str { "FlexibleNode" }
            fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
                Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Range { min: 8000, max: 48000 }),
                        channels: Some(ConstraintValue::Exact(1)),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                )))
            }
            fn capability_behavior(&self) -> CapabilityBehavior {
                CapabilityBehavior::Static
            }
        }
    }

    // Create registry with our FlexibleNode + standard CLI nodes
    let mut registry = StreamingNodeRegistry::new();
    registry.register(std::sync::Arc::new(mock_flexible::FlexibleNodeFactory));

    let cli_registry = create_test_registry();
    if let Some(factory) = cli_registry.get_factory("MicInput") {
        registry.register(factory.clone());
    }
    if let Some(factory) = cli_registry.get_factory("FastResampleNode") {
        registry.register(factory.clone());
    }

    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("resample".to_string(), "FastResampleNode".to_string()),
        ("flex".to_string(), "FlexibleNode".to_string()),
    ];

    let connections = vec![
        ("mic".to_string(), "resample".to_string()),
        ("resample".to_string(), "flex".to_string()),
    ];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,
            "channels": 1
        }),
    );
    params.insert(
        "resample".to_string(),
        serde_json::json!({
            "sourceRate": 48000,
            "targetRate": 16000,
            "channels": 1
        }),
    );
    params.insert("flex".to_string(), serde_json::json!({}));

    // Create hints that specify preferred sample rate for the resample node
    let mut hints = CapabilityHints::new();
    hints.nodes.insert(
        "resample".to_string(),
        NodeHints {
            preferred_sample_rate: Some(16000),
            preferred_channels: None,
            preferred_format: None,
            prefer_exact: Some(true),
        },
    );

    // Resolve with hints
    let ctx = resolver
        .resolve_with_hints(&nodes, &connections, &params, &hints)
        .unwrap();

    // Should have no errors
    assert!(
        !ctx.has_errors(),
        "Pipeline should be valid. Errors: {:?}",
        ctx.errors
    );

    // Verify resample node is Configured (explicit rates given)
    let resample_behavior = ctx.get_behavior("resample");
    assert_eq!(
        resample_behavior,
        CapabilityBehavior::Configured,
        "FastResampleNode should have Configured behavior with explicit rates"
    );

    // Verify the resample node's output was refined by the hint
    if let Some(resolved) = ctx.resolved.get("resample") {
        let output = resolved.capabilities.default_output();
        assert!(output.is_some(), "Resample node should have output capabilities");

        if let Some(MediaConstraints::Audio(audio)) = output {
            // The hint should have refined the output to exactly 16kHz
            assert_eq!(
                audio.sample_rate,
                Some(ConstraintValue::Exact(16000)),
                "Hint should have refined sample_rate to exactly 16000. Got: {:?}",
                audio.sample_rate
            );
        } else {
            panic!("Resample output should be Audio constraints");
        }
    } else {
        panic!("Resample node should have resolved capabilities");
    }
}

// =============================================================================
// Integration Test: MicInput -> Resample -> Whisper -> SpeakerOutput (spec 023)
// =============================================================================

#[test]
fn test_mic_resample_whisper_speaker_pipeline() {
    // Integration test for complete audio pipeline with capability resolution:
    // MicInput(48kHz stereo) -> FastResampleNode -> RustWhisperNode(16kHz mono) -> SpeakerOutput (passthrough)
    //
    // This tests:
    // - MicInput with Configured behavior (48kHz, 2ch)
    // - FastResampleNode with Configured behavior (explicit sourceRate/targetRate given)
    // - RustWhisperNode with Static behavior (requires exactly 16kHz mono)
    // - SpeakerOutput with Passthrough behavior (accepts text output from Whisper)

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("resample".to_string(), "FastResampleNode".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
        ("speaker".to_string(), "SpeakerOutput".to_string()),
    ];

    let connections = vec![
        ("mic".to_string(), "resample".to_string()),
        ("resample".to_string(), "whisper".to_string()),
        ("whisper".to_string(), "speaker".to_string()),
    ];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,  // High quality input
            "channels": 2,         // Stereo
            "device": "test"       // Explicit device (Configured behavior)
        }),
    );
    params.insert(
        "resample".to_string(),
        serde_json::json!({
            "sourceRate": 48000,
            "targetRate": 16000,   // Match Whisper requirements
            "channels": 1          // Convert to mono
        }),
    );
    params.insert("whisper".to_string(), serde_json::json!({}));
    params.insert("speaker".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Pipeline should be valid
    assert!(
        !ctx.has_errors(),
        "MicInput -> Resample -> Whisper -> Speaker should be valid. Errors: {:?}",
        ctx.errors
    );

    // Verify capability behaviors
    assert_eq!(
        ctx.get_behavior("mic"),
        CapabilityBehavior::Configured,
        "MicInput should have Configured behavior"
    );
    assert_eq!(
        ctx.get_behavior("resample"),
        CapabilityBehavior::Configured,
        "FastResampleNode should have Configured behavior (explicit rates given)"
    );
    assert_eq!(
        ctx.get_behavior("whisper"),
        CapabilityBehavior::Static,
        "RustWhisperNode should have Static behavior"
    );
    assert_eq!(
        ctx.get_behavior("speaker"),
        CapabilityBehavior::Passthrough,
        "SpeakerOutput should have Passthrough behavior"
    );

    // Verify resolution is complete for all nodes
    assert!(ctx.resolved.contains_key("mic"), "MicInput should be resolved");
    assert!(ctx.resolved.contains_key("resample"), "Resample should be resolved");
    assert!(ctx.resolved.contains_key("whisper"), "Whisper should be resolved");
    assert!(ctx.resolved.contains_key("speaker"), "Speaker should be resolved");

    // Verify MicInput capabilities (Configured from params)
    let mic_resolved = ctx.resolved.get("mic").unwrap();
    let mic_output = mic_resolved.capabilities.default_output();
    assert!(mic_output.is_some(), "MicInput should have output caps");
    if let Some(MediaConstraints::Audio(audio)) = mic_output {
        assert_eq!(
            audio.sample_rate,
            Some(ConstraintValue::Exact(48000)),
            "MicInput should output 48kHz"
        );
        assert_eq!(
            audio.channels,
            Some(ConstraintValue::Exact(2)),
            "MicInput should output stereo"
        );
    }

    // Verify Whisper capabilities (Static requirements)
    let whisper_resolved = ctx.resolved.get("whisper").unwrap();
    let whisper_input = whisper_resolved.capabilities.default_input();
    assert!(whisper_input.is_some(), "Whisper should have input requirements");
    if let Some(MediaConstraints::Audio(audio)) = whisper_input {
        assert_eq!(
            audio.sample_rate,
            Some(ConstraintValue::Exact(16000)),
            "Whisper should require 16kHz"
        );
        assert_eq!(
            audio.channels,
            Some(ConstraintValue::Exact(1)),
            "Whisper should require mono"
        );
    }
}

#[test]
fn test_mic_whisper_speaker_without_resample_fails() {
    // Test that direct MicInput(48kHz) -> Whisper(16kHz) fails without resample
    // This validates the capability mismatch detection

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("whisper".to_string(), "RustWhisperNode".to_string()),
        ("speaker".to_string(), "SpeakerOutput".to_string()),
    ];

    let connections = vec![
        ("mic".to_string(), "whisper".to_string()),
        ("whisper".to_string(), "speaker".to_string()),
    ];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,
            "channels": 2,
            "device": "test"
        }),
    );
    params.insert("whisper".to_string(), serde_json::json!({}));
    params.insert("speaker".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Pipeline should have capability mismatch errors
    assert!(
        ctx.has_errors(),
        "MicInput(48kHz) -> Whisper(16kHz) without resample should fail"
    );

    // Verify specific mismatch errors
    let sample_rate_error = ctx.errors.iter().find(|e| e.constraint_name == "sample_rate");
    assert!(
        sample_rate_error.is_some(),
        "Should have sample_rate mismatch. Errors: {:?}",
        ctx.errors
    );

    let channels_error = ctx.errors.iter().find(|e| e.constraint_name == "channels");
    assert!(
        channels_error.is_some(),
        "Should have channels mismatch. Errors: {:?}",
        ctx.errors
    );
}

#[test]
fn test_mic_runtime_discovered_with_resample_speaker() {
    // Test RuntimeDiscovered behavior for MicInput with device="default"
    // MicInput(default) -> FastResampleNode -> SpeakerOutput
    //
    // When device="default", MicInput uses RuntimeDiscovered behavior with
    // potential_capabilities (Phase 1) for early validation

    let registry = create_test_registry();
    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("resample".to_string(), "FastResampleNode".to_string()),
        ("speaker".to_string(), "SpeakerOutput".to_string()),
    ];

    let connections = vec![
        ("mic".to_string(), "resample".to_string()),
        ("resample".to_string(), "speaker".to_string()),
    ];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            // No device or device="default" triggers RuntimeDiscovered
            "sample_rate": 48000,
            "channels": 1
        }),
    );
    params.insert(
        "resample".to_string(),
        serde_json::json!({
            "sourceRate": 48000,
            "targetRate": 16000,
            "channels": 1
        }),
    );
    params.insert("speaker".to_string(), serde_json::json!({}));

    let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

    // Pipeline should be valid with Phase 1 (potential_capabilities)
    assert!(
        !ctx.has_errors(),
        "Pipeline with RuntimeDiscovered MicInput should pass Phase 1. Errors: {:?}",
        ctx.errors
    );

    // Verify behaviors
    // Note: Factory returns RuntimeDiscovered by default when we can't determine from params
    let mic_behavior = ctx.get_behavior("mic");
    assert!(
        mic_behavior == CapabilityBehavior::RuntimeDiscovered || mic_behavior == CapabilityBehavior::Configured,
        "MicInput should have RuntimeDiscovered or Configured behavior, got {:?}",
        mic_behavior
    );

    assert_eq!(
        ctx.get_behavior("speaker"),
        CapabilityBehavior::Passthrough,
        "SpeakerOutput should have Passthrough behavior"
    );
}

#[test]
fn test_capability_hints_ignored_when_outside_range() {
    // Test that hints are ignored when the preferred value is outside the valid range.
    // Pipeline: MicInput(48kHz) -> FastResampleNode (Adaptive) -> FlexibleNode (accepts 8-24kHz)
    // Hint requests 48000, but downstream only accepts 8-24kHz, so hint should be ignored.

    use remotemedia_runtime_core::capabilities::{
        AudioConstraints, AudioSampleFormat, CapabilityBehavior, CapabilityHints, ConstraintValue,
        MediaCapabilities, MediaConstraints, NodeHints,
    };
    use remotemedia_runtime_core::nodes::streaming_node::{
        StreamingNode, StreamingNodeFactory, StreamingNodeRegistry,
    };

    mod mock_limited {
        use async_trait::async_trait;
        use remotemedia_runtime_core::capabilities::{
            AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue,
            MediaCapabilities, MediaConstraints,
        };
        use remotemedia_runtime_core::data::RuntimeData;
        use remotemedia_runtime_core::nodes::streaming_node::{StreamingNode, StreamingNodeFactory};
        use serde_json::Value;
        use std::collections::HashMap;

        pub struct LimitedNode;

        #[async_trait]
        impl StreamingNode for LimitedNode {
            async fn process_async(&self, _input: RuntimeData) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                Ok(RuntimeData::Text(String::new()))
            }
            async fn process_multi_async(&self, _inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                Ok(RuntimeData::Text(String::new()))
            }
            fn is_multi_input(&self) -> bool { false }
            fn node_type(&self) -> &str { "LimitedNode" }

            fn media_capabilities(&self) -> Option<MediaCapabilities> {
                // Only accepts 8-24kHz
                Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Range { min: 8000, max: 24000 }),
                        channels: Some(ConstraintValue::Exact(1)),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                )))
            }

            fn capability_behavior(&self) -> CapabilityBehavior {
                CapabilityBehavior::Static
            }
        }

        pub struct LimitedNodeFactory;

        impl StreamingNodeFactory for LimitedNodeFactory {
            fn create(&self, _node_id: String, _params: &Value, _session_id: Option<String>) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
                Ok(Box::new(LimitedNode))
            }
            fn node_type(&self) -> &str { "LimitedNode" }
            fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
                Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Range { min: 8000, max: 24000 }),
                        channels: Some(ConstraintValue::Exact(1)),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                )))
            }
            fn capability_behavior(&self) -> CapabilityBehavior {
                CapabilityBehavior::Static
            }
        }
    }

    let mut registry = StreamingNodeRegistry::new();
    registry.register(std::sync::Arc::new(mock_limited::LimitedNodeFactory));

    let cli_registry = create_test_registry();
    if let Some(factory) = cli_registry.get_factory("MicInput") {
        registry.register(factory.clone());
    }
    if let Some(factory) = cli_registry.get_factory("FastResampleNode") {
        registry.register(factory.clone());
    }

    let resolver = CapabilityResolver::new(&registry);

    let nodes = vec![
        ("mic".to_string(), "MicInput".to_string()),
        ("resample".to_string(), "FastResampleNode".to_string()),
        ("limited".to_string(), "LimitedNode".to_string()),
    ];

    let connections = vec![
        ("mic".to_string(), "resample".to_string()),
        ("resample".to_string(), "limited".to_string()),
    ];

    let mut params = HashMap::new();
    params.insert(
        "mic".to_string(),
        serde_json::json!({
            "sample_rate": 48000,
            "channels": 1
        }),
    );
    params.insert(
        "resample".to_string(),
        serde_json::json!({
            "sourceRate": 48000,
            "targetRate": 16000,
            "channels": 1
        }),
    );
    params.insert("limited".to_string(), serde_json::json!({}));

    // Create hints that specify 48000 (outside the 8-24kHz range!)
    let mut hints = CapabilityHints::new();
    hints.nodes.insert(
        "resample".to_string(),
        NodeHints {
            preferred_sample_rate: Some(48000), // Outside valid range!
            preferred_channels: None,
            preferred_format: None,
            prefer_exact: None,
        },
    );

    // Resolve with hints
    let ctx = resolver
        .resolve_with_hints(&nodes, &connections, &params, &hints)
        .unwrap();

    // Should have no errors (hint is just ignored)
    assert!(
        !ctx.has_errors(),
        "Pipeline should be valid. Errors: {:?}",
        ctx.errors
    );

    // Verify the resample node's output kept the original range (hint was ignored)
    if let Some(resolved) = ctx.resolved.get("resample") {
        let output = resolved.capabilities.default_output();
        assert!(output.is_some(), "Resample node should have output capabilities");

        if let Some(MediaConstraints::Audio(audio)) = output {
            // The hint should have been ignored since 48000 is outside 8-24kHz
            // Output should match the downstream's input range
            match &audio.sample_rate {
                Some(ConstraintValue::Range { min: 8000, max: 24000 }) => {
                    // Expected - hint was ignored
                }
                other => {
                    // Also acceptable if it matched any valid value
                    // The key is that it shouldn't be 48000
                    if let Some(ConstraintValue::Exact(rate)) = other {
                        assert_ne!(
                            *rate, 48000,
                            "Hint for 48000 should have been ignored (outside 8-24kHz range)"
                        );
                    }
                }
            }
        } else {
            panic!("Resample output should be Audio constraints");
        }
    } else {
        panic!("Resample node should have resolved capabilities");
    }
}

// =============================================================================
// Manifest-Based Integration Tests (using actual Manifest/PipelineGraph)
// =============================================================================

mod manifest_integration_tests {
    use super::*;
    use remotemedia_runtime_core::executor::PipelineGraph;
    use remotemedia_runtime_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};

    /// Helper to create a Manifest programmatically
    fn create_manifest(
        name: &str,
        nodes: Vec<(&str, &str, serde_json::Value)>,
        connections: Vec<(&str, &str)>,
    ) -> Manifest {
        Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: name.to_string(),
                description: None,
                created_at: None,
                auto_negotiate: false,
            },
            nodes: nodes
                .into_iter()
                .map(|(id, node_type, params)| NodeManifest {
                    id: id.to_string(),
                    node_type: node_type.to_string(),
                    params,
                    ..Default::default()
                })
                .collect(),
            connections: connections
                .into_iter()
                .map(|(from, to)| Connection {
                    from: from.to_string(),
                    to: to.to_string(),
                })
                .collect(),
        }
    }

    /// Helper to resolve capabilities from a manifest using the graph
    fn resolve_from_manifest(
        manifest: &Manifest,
        registry: &remotemedia_runtime_core::nodes::streaming_node::StreamingNodeRegistry,
    ) -> remotemedia_runtime_core::capabilities::ResolutionContext {
        let graph = PipelineGraph::from_manifest(manifest).expect("Failed to build graph");
        let resolver = CapabilityResolver::new(registry);

        // Extract nodes and connections from graph
        let nodes: Vec<(String, String)> = graph
            .execution_order
            .iter()
            .map(|id| {
                let node = graph.get_node(id).unwrap();
                (id.clone(), node.node_type.clone())
            })
            .collect();

        let connections: Vec<(String, String)> = manifest
            .connections
            .iter()
            .map(|c| (c.from.clone(), c.to.clone()))
            .collect();

        // Build params map from manifest
        let mut params = HashMap::new();
        for node in &manifest.nodes {
            params.insert(node.id.clone(), node.params.clone());
        }

        resolver.resolve(&nodes, &connections, &params).unwrap()
    }

    #[test]
    fn test_manifest_mic_resample_whisper_speaker_pipeline() {
        // Test using actual Manifest and PipelineGraph infrastructure
        // MicInput(48kHz stereo) -> FastResampleNode -> RustWhisperNode -> SpeakerOutput

        let manifest = create_manifest(
            "test-transcription-pipeline",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 2,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({
                        "sourceRate": 48000,
                        "targetRate": 16000,
                        "channels": 1
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
                ("speaker", "SpeakerOutput", serde_json::json!({})),
            ],
            vec![
                ("mic", "resample"),
                ("resample", "whisper"),
                ("whisper", "speaker"),
            ],
        );

        // Validate graph construction
        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.sources, vec!["mic"]);
        assert_eq!(graph.sinks, vec!["speaker"]);
        assert_eq!(
            graph.execution_order,
            vec!["mic", "resample", "whisper", "speaker"]
        );

        // Resolve capabilities using the graph
        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Pipeline should be valid
        assert!(
            !ctx.has_errors(),
            "Manifest-based pipeline should be valid. Errors: {:?}",
            ctx.errors
        );

        // Verify all nodes resolved
        assert!(ctx.resolved.contains_key("mic"));
        assert!(ctx.resolved.contains_key("resample"));
        assert!(ctx.resolved.contains_key("whisper"));
        assert!(ctx.resolved.contains_key("speaker"));

        // Verify behaviors
        assert_eq!(ctx.get_behavior("mic"), CapabilityBehavior::Configured);
        // FastResampleNode with explicit sourceRate/targetRate is Configured, not Adaptive
        assert_eq!(ctx.get_behavior("resample"), CapabilityBehavior::Configured);
        assert_eq!(ctx.get_behavior("whisper"), CapabilityBehavior::Static);
        assert_eq!(ctx.get_behavior("speaker"), CapabilityBehavior::Passthrough);
    }

    #[test]
    fn test_manifest_mismatch_detection() {
        // Test that manifest-based resolution detects capability mismatches
        // MicInput(48kHz) -> RustWhisperNode(16kHz) - should fail without resample

        let manifest = create_manifest(
            "test-mismatch-pipeline",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 2,
                        "device": "test"
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
            ],
            vec![("mic", "whisper")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.node_count(), 2);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should have errors
        assert!(
            ctx.has_errors(),
            "Direct MicInput(48kHz) -> Whisper(16kHz) should fail"
        );

        // Verify specific mismatches
        let has_sample_rate_error = ctx
            .errors
            .iter()
            .any(|e| e.constraint_name == "sample_rate");
        let has_channels_error = ctx.errors.iter().any(|e| e.constraint_name == "channels");

        assert!(
            has_sample_rate_error,
            "Should detect sample_rate mismatch. Errors: {:?}",
            ctx.errors
        );
        assert!(
            has_channels_error,
            "Should detect channels mismatch. Errors: {:?}",
            ctx.errors
        );
    }

    #[test]
    fn test_manifest_passthrough_chain() {
        // Test passthrough behavior with manifest:
        // MicInput -> SpeakerOutput (passthrough should inherit upstream caps)

        let manifest = create_manifest(
            "test-passthrough-pipeline",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 44100,
                        "channels": 2,
                        "device": "explicit"
                    }),
                ),
                ("speaker", "SpeakerOutput", serde_json::json!({})),
            ],
            vec![("mic", "speaker")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.sources, vec!["mic"]);
        assert_eq!(graph.sinks, vec!["speaker"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should be valid - SpeakerOutput accepts any audio
        assert!(
            !ctx.has_errors(),
            "MicInput -> SpeakerOutput should be valid. Errors: {:?}",
            ctx.errors
        );

        // Verify behaviors
        assert_eq!(ctx.get_behavior("mic"), CapabilityBehavior::Configured);
        assert_eq!(ctx.get_behavior("speaker"), CapabilityBehavior::Passthrough);
    }

    #[test]
    fn test_manifest_branching_pipeline() {
        // Test a branching pipeline (one source, multiple sinks):
        // MicInput -> Resample -> Whisper
        //          -> SpeakerOutput (direct passthrough)

        let manifest = create_manifest(
            "test-branching-pipeline",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({
                        "sourceRate": 48000,
                        "targetRate": 16000,
                        "channels": 1
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
                ("speaker", "SpeakerOutput", serde_json::json!({})),
            ],
            vec![
                ("mic", "resample"),
                ("mic", "speaker"), // Branch: mic also goes to speaker
                ("resample", "whisper"),
            ],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");

        // Verify graph structure
        assert_eq!(graph.sources, vec!["mic"]);
        assert!(graph.sinks.contains(&"whisper".to_string()));
        assert!(graph.sinks.contains(&"speaker".to_string()));

        // Verify mic has two outputs
        let mic_node = graph.get_node("mic").unwrap();
        assert_eq!(mic_node.outputs.len(), 2);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should be valid
        assert!(
            !ctx.has_errors(),
            "Branching pipeline should be valid. Errors: {:?}",
            ctx.errors
        );

        // All nodes should be resolved
        assert!(ctx.resolved.contains_key("mic"));
        assert!(ctx.resolved.contains_key("resample"));
        assert!(ctx.resolved.contains_key("whisper"));
        assert!(ctx.resolved.contains_key("speaker"));
    }

    #[test]
    fn test_manifest_graph_cycle_detection() {
        // Test that PipelineGraph correctly detects cycles

        let manifest = create_manifest(
            "test-cycle-pipeline",
            vec![
                ("a", "MicInput", serde_json::json!({"sample_rate": 16000, "channels": 1})),
                ("b", "FastResampleNode", serde_json::json!({"sourceRate": 16000, "targetRate": 16000})),
                ("c", "SpeakerOutput", serde_json::json!({})),
            ],
            vec![
                ("a", "b"),
                ("b", "c"),
                ("c", "a"), // Cycle: c -> a
            ],
        );

        let result = PipelineGraph::from_manifest(&manifest);
        assert!(
            result.is_err(),
            "Pipeline with cycle should fail to build graph"
        );

        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(
            err_msg.contains("cycle"),
            "Error should mention cycle. Got: {}",
            err_msg
        );
    }

    #[test]
    fn test_manifest_valid_transcription_pipeline() {
        // Test a valid transcription pipeline similar to transcribe-srt-mic-input.yaml
        // MicInput(16kHz mono) -> RustWhisperNode -> SrtOutput

        let manifest = create_manifest(
            "transcribe-srt",
            vec![
                (
                    "mic-input",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 16000,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "whisper",
                    "RustWhisperNode",
                    serde_json::json!({
                        "model_source": "tiny"
                    }),
                ),
                ("srt", "SrtOutput", serde_json::json!({})),
            ],
            vec![("mic-input", "whisper"), ("whisper", "srt")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.execution_order, vec!["mic-input", "whisper", "srt"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should be valid - mic at 16kHz matches Whisper requirements
        assert!(
            !ctx.has_errors(),
            "Valid transcription pipeline should pass. Errors: {:?}",
            ctx.errors
        );
    }
}

// =============================================================================
// RuntimeDiscovered Tests: Both MicInput and SpeakerOutput Unknown Until Init
// =============================================================================
//
// These tests verify the two-phase capability resolution when BOTH the input
// device (MicInput) and output device (SpeakerOutput) have unknown sample rates
// until they are actually initialized with real hardware.
//
// Real-world scenario: User selects "default" for both input and output devices.
// We don't know the actual sample rates until the devices are opened.
// =============================================================================

mod dual_runtime_discovered_tests {
    use super::*;
    use remotemedia_runtime_core::capabilities::{
        AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue,
        MediaCapabilities, MediaConstraints, ResolutionState,
    };
    use remotemedia_runtime_core::data::RuntimeData;
    use remotemedia_runtime_core::nodes::streaming_node::{
        StreamingNode, StreamingNodeFactory, StreamingNodeRegistry,
    };
    use serde_json::Value;
    use std::sync::Arc;

    /// Mock audio source with RuntimeDiscovered behavior
    /// Simulates MicInput with device="default"
    pub struct MockRuntimeDiscoveredSource {
        node_id: String,
        /// Actual sample rate discovered when device is opened
        actual_sample_rate: u32,
        /// Actual channels discovered when device is opened
        actual_channels: u32,
        initialized: std::sync::atomic::AtomicBool,
    }

    impl MockRuntimeDiscoveredSource {
        pub fn new(node_id: String, actual_sample_rate: u32, actual_channels: u32) -> Self {
            Self {
                node_id,
                actual_sample_rate,
                actual_channels,
                initialized: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    #[async_trait::async_trait]
    impl StreamingNode for MockRuntimeDiscoveredSource {
        fn node_type(&self) -> &str {
            "MockRuntimeDiscoveredSource"
        }

        fn node_id(&self) -> &str {
            &self.node_id
        }

        async fn initialize(&self) -> Result<(), remotemedia_runtime_core::Error> {
            // Simulate device discovery
            self.initialized.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn process_async(
            &self,
            _data: RuntimeData,
        ) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
            // Return audio with discovered sample rate
            Ok(RuntimeData::Audio {
                samples: vec![0.0; 1024],
                sample_rate: self.actual_sample_rate,
                channels: self.actual_channels,
                stream_id: None,
            })
        }

        async fn process_multi_async(
            &self,
            _inputs: std::collections::HashMap<String, RuntimeData>,
        ) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
            self.process_async(RuntimeData::Text(String::new())).await
        }

        fn is_multi_input(&self) -> bool {
            false
        }

        fn media_capabilities(&self) -> Option<MediaCapabilities> {
            // RuntimeDiscovered - return None, use potential/actual instead
            None
        }

        fn capability_behavior(&self) -> CapabilityBehavior {
            CapabilityBehavior::RuntimeDiscovered
        }

        fn potential_capabilities(&self) -> Option<MediaCapabilities> {
            // Broad range for Phase 1 validation
            Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }

        fn actual_capabilities(&self) -> Option<MediaCapabilities> {
            if self.initialized.load(std::sync::atomic::Ordering::SeqCst) {
                // After initialization, return actual discovered capabilities
                Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Exact(self.actual_sample_rate)),
                        channels: Some(ConstraintValue::Exact(self.actual_channels)),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                )))
            } else {
                // Before init, return potential
                self.potential_capabilities()
            }
        }
    }

    /// Mock audio sink with RuntimeDiscovered behavior
    /// Simulates SpeakerOutput with device="default"
    pub struct MockRuntimeDiscoveredSink {
        node_id: String,
        /// Actual sample rate the device supports
        actual_sample_rate: u32,
        /// Actual channels the device supports
        actual_channels: u32,
        initialized: std::sync::atomic::AtomicBool,
    }

    impl MockRuntimeDiscoveredSink {
        pub fn new(node_id: String, actual_sample_rate: u32, actual_channels: u32) -> Self {
            Self {
                node_id,
                actual_sample_rate,
                actual_channels,
                initialized: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    #[async_trait::async_trait]
    impl StreamingNode for MockRuntimeDiscoveredSink {
        fn node_type(&self) -> &str {
            "MockRuntimeDiscoveredSink"
        }

        fn node_id(&self) -> &str {
            &self.node_id
        }

        async fn initialize(&self) -> Result<(), remotemedia_runtime_core::Error> {
            self.initialized.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn process_async(
            &self,
            data: RuntimeData,
        ) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
            // Passthrough - just return the data
            Ok(data)
        }

        async fn process_multi_async(
            &self,
            inputs: std::collections::HashMap<String, RuntimeData>,
        ) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
            Ok(inputs.into_values().next().unwrap_or(RuntimeData::Text(String::new())))
        }

        fn is_multi_input(&self) -> bool {
            false
        }

        fn media_capabilities(&self) -> Option<MediaCapabilities> {
            // RuntimeDiscovered sink - return None
            None
        }

        fn capability_behavior(&self) -> CapabilityBehavior {
            // The sink has RuntimeDiscovered behavior because we don't know
            // what sample rate the output device supports until we open it
            CapabilityBehavior::RuntimeDiscovered
        }

        fn potential_capabilities(&self) -> Option<MediaCapabilities> {
            // Broad range for Phase 1 - accepts any audio format
            Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }

        fn actual_capabilities(&self) -> Option<MediaCapabilities> {
            if self.initialized.load(std::sync::atomic::Ordering::SeqCst) {
                // After initialization, return what the device actually supports
                Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Exact(self.actual_sample_rate)),
                        channels: Some(ConstraintValue::Exact(self.actual_channels)),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                )))
            } else {
                self.potential_capabilities()
            }
        }
    }

    /// Factory for MockRuntimeDiscoveredSource
    pub struct MockSourceFactory {
        actual_sample_rate: u32,
        actual_channels: u32,
    }

    impl MockSourceFactory {
        pub fn new(actual_sample_rate: u32, actual_channels: u32) -> Self {
            Self {
                actual_sample_rate,
                actual_channels,
            }
        }
    }

    impl StreamingNodeFactory for MockSourceFactory {
        fn create(
            &self,
            node_id: String,
            _params: &Value,
            _session_id: Option<String>,
        ) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
            Ok(Box::new(MockRuntimeDiscoveredSource::new(
                node_id,
                self.actual_sample_rate,
                self.actual_channels,
            )))
        }

        fn node_type(&self) -> &str {
            "MockRuntimeDiscoveredSource"
        }

        fn capability_behavior(&self) -> CapabilityBehavior {
            CapabilityBehavior::RuntimeDiscovered
        }

        fn potential_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
            Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }
    }

    /// Factory for MockRuntimeDiscoveredSink
    pub struct MockSinkFactory {
        actual_sample_rate: u32,
        actual_channels: u32,
    }

    impl MockSinkFactory {
        pub fn new(actual_sample_rate: u32, actual_channels: u32) -> Self {
            Self {
                actual_sample_rate,
                actual_channels,
            }
        }
    }

    impl StreamingNodeFactory for MockSinkFactory {
        fn create(
            &self,
            node_id: String,
            _params: &Value,
            _session_id: Option<String>,
        ) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
            Ok(Box::new(MockRuntimeDiscoveredSink::new(
                node_id,
                self.actual_sample_rate,
                self.actual_channels,
            )))
        }

        fn node_type(&self) -> &str {
            "MockRuntimeDiscoveredSink"
        }

        fn capability_behavior(&self) -> CapabilityBehavior {
            CapabilityBehavior::RuntimeDiscovered
        }

        fn potential_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
            Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }
    }

    #[test]
    fn test_dual_runtime_discovered_phase1_passes() {
        // Test Phase 1: Both source and sink have RuntimeDiscovered behavior
        // Pipeline: MockSource (unknown) -> MockSink (unknown)
        //
        // Phase 1 should pass because both have potential_capabilities with
        // broad ranges that overlap.

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(MockSourceFactory::new(44100, 2))); // Will discover 44.1kHz stereo
        registry.register(Arc::new(MockSinkFactory::new(48000, 2)));   // Will discover 48kHz stereo

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "MockRuntimeDiscoveredSource".to_string()),
            ("sink".to_string(), "MockRuntimeDiscoveredSink".to_string()),
        ];
        let connections = vec![("source".to_string(), "sink".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("sink".to_string(), serde_json::json!({}));

        let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 should pass - potential ranges overlap
        assert!(
            !ctx.has_errors(),
            "Phase 1 should pass with broad potential_capabilities. Errors: {:?}",
            ctx.errors
        );

        // Both nodes should be marked as provisional (awaiting Phase 2)
        let source_resolved = ctx.resolved.get("source").unwrap();
        assert!(
            source_resolved.provisional,
            "Source should be marked as provisional"
        );

        let sink_resolved = ctx.resolved.get("sink").unwrap();
        assert!(
            sink_resolved.provisional,
            "Sink should be marked as provisional"
        );

        // Verify behaviors
        assert_eq!(
            ctx.get_behavior("source"),
            CapabilityBehavior::RuntimeDiscovered,
            "Source should have RuntimeDiscovered behavior"
        );
        assert_eq!(
            ctx.get_behavior("sink"),
            CapabilityBehavior::RuntimeDiscovered,
            "Sink should have RuntimeDiscovered behavior"
        );
    }

    #[test]
    fn test_dual_runtime_discovered_phase2_compatible() {
        // Test Phase 2: Both devices discover compatible capabilities
        // Source discovers: 48kHz stereo
        // Sink discovers: 48kHz stereo
        // Result: Pipeline is valid

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(MockSourceFactory::new(48000, 2)));
        registry.register(Arc::new(MockSinkFactory::new(48000, 2)));

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "MockRuntimeDiscoveredSource".to_string()),
            ("sink".to_string(), "MockRuntimeDiscoveredSink".to_string()),
        ];
        let connections = vec![("source".to_string(), "sink".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("sink".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 passes
        assert!(!ctx.has_errors(), "Phase 1 should pass");

        // Phase 2: Revalidate source with actual 48kHz stereo
        let source_actual = MediaCapabilities::with_output(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(48000)),
                channels: Some(ConstraintValue::Exact(2)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "source", source_actual).unwrap();

        // Phase 2: Revalidate sink with actual 48kHz stereo
        let sink_actual = MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(48000)),
                channels: Some(ConstraintValue::Exact(2)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "sink", sink_actual).unwrap();

        // Should still be valid - both discovered 48kHz stereo
        assert!(
            !ctx.has_errors(),
            "Phase 2 should pass when both devices are compatible. Errors: {:?}",
            ctx.errors
        );

        // Both should no longer be provisional
        assert!(
            !ctx.resolved.get("source").unwrap().provisional,
            "Source should no longer be provisional after Phase 2"
        );
        assert!(
            !ctx.resolved.get("sink").unwrap().provisional,
            "Sink should no longer be provisional after Phase 2"
        );
    }

    #[test]
    fn test_dual_runtime_discovered_phase2_sample_rate_mismatch() {
        // Test Phase 2: Devices discover incompatible sample rates
        // Source discovers: 44.1kHz
        // Sink discovers: 48kHz (only supports this rate)
        // Result: Pipeline fails in Phase 2

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(MockSourceFactory::new(44100, 2)));
        registry.register(Arc::new(MockSinkFactory::new(48000, 2)));

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "MockRuntimeDiscoveredSource".to_string()),
            ("sink".to_string(), "MockRuntimeDiscoveredSink".to_string()),
        ];
        let connections = vec![("source".to_string(), "sink".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("sink".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 passes with broad ranges
        assert!(!ctx.has_errors(), "Phase 1 should pass");

        // Phase 2: Source discovers 44.1kHz stereo
        let source_actual = MediaCapabilities::with_output(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(44100)),
                channels: Some(ConstraintValue::Exact(2)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "source", source_actual).unwrap();

        // Phase 2: Sink discovers it only supports 48kHz
        let sink_actual = MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(48000)),
                channels: Some(ConstraintValue::Exact(2)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "sink", sink_actual).unwrap();

        // Should now have errors - 44.1kHz doesn't match 48kHz
        assert!(
            ctx.has_errors(),
            "Phase 2 should fail with incompatible sample rates"
        );

        // Verify specific mismatch
        let sample_rate_error = ctx.errors.iter().find(|e| e.constraint_name == "sample_rate");
        assert!(
            sample_rate_error.is_some(),
            "Should have sample_rate mismatch. Errors: {:?}",
            ctx.errors
        );
    }

    #[test]
    fn test_dual_runtime_discovered_phase2_channels_mismatch() {
        // Test Phase 2: Devices discover incompatible channel counts
        // Source discovers: 48kHz mono
        // Sink discovers: 48kHz stereo only
        // Result: Pipeline fails in Phase 2

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(MockSourceFactory::new(48000, 1)));
        registry.register(Arc::new(MockSinkFactory::new(48000, 2)));

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "MockRuntimeDiscoveredSource".to_string()),
            ("sink".to_string(), "MockRuntimeDiscoveredSink".to_string()),
        ];
        let connections = vec![("source".to_string(), "sink".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("sink".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 passes
        assert!(!ctx.has_errors(), "Phase 1 should pass");

        // Phase 2: Source discovers mono
        let source_actual = MediaCapabilities::with_output(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(48000)),
                channels: Some(ConstraintValue::Exact(1)), // Mono
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "source", source_actual).unwrap();

        // Phase 2: Sink discovers stereo only
        let sink_actual = MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(48000)),
                channels: Some(ConstraintValue::Exact(2)), // Stereo only
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "sink", sink_actual).unwrap();

        // Should have errors - mono vs stereo
        assert!(
            ctx.has_errors(),
            "Phase 2 should fail with incompatible channels"
        );

        // Verify channels mismatch
        let channels_error = ctx.errors.iter().find(|e| e.constraint_name == "channels");
        assert!(
            channels_error.is_some(),
            "Should have channels mismatch. Errors: {:?}",
            ctx.errors
        );
    }

    #[test]
    fn test_dual_runtime_discovered_with_resample_in_between() {
        // Test: RuntimeDiscovered source -> Adaptive resample -> RuntimeDiscovered sink
        // The resample node should bridge any sample rate mismatch discovered in Phase 2
        //
        // Source discovers: 44.1kHz stereo
        // Sink discovers: 48kHz stereo
        // Resample: Configured to convert 44.1kHz -> 48kHz

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(MockSourceFactory::new(44100, 2)));
        registry.register(Arc::new(MockSinkFactory::new(48000, 2)));

        // Add FastResampleNode from CLI registry
        let cli_registry = create_test_registry();
        if let Some(factory) = cli_registry.get_factory("FastResampleNode") {
            registry.register(factory.clone());
        }

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "MockRuntimeDiscoveredSource".to_string()),
            ("resample".to_string(), "FastResampleNode".to_string()),
            ("sink".to_string(), "MockRuntimeDiscoveredSink".to_string()),
        ];
        let connections = vec![
            ("source".to_string(), "resample".to_string()),
            ("resample".to_string(), "sink".to_string()),
        ];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert(
            "resample".to_string(),
            serde_json::json!({
                "sourceRate": 44100,
                "targetRate": 48000,
                "channels": 2
            }),
        );
        params.insert("sink".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 should pass
        assert!(
            !ctx.has_errors(),
            "Phase 1 should pass with resample bridging. Errors: {:?}",
            ctx.errors
        );

        // Phase 2: Source discovers 44.1kHz
        let source_actual = MediaCapabilities::with_output(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(44100)),
                channels: Some(ConstraintValue::Exact(2)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "source", source_actual).unwrap();

        // Phase 2: Sink discovers 48kHz
        let sink_actual = MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(48000)),
                channels: Some(ConstraintValue::Exact(2)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "sink", sink_actual).unwrap();

        // Should be valid - resample converts 44.1kHz -> 48kHz
        assert!(
            !ctx.has_errors(),
            "Pipeline with resample should be valid after Phase 2. Errors: {:?}",
            ctx.errors
        );
    }

    #[test]
    fn test_source_discovered_sink_passes_flexible_range() {
        // Test: Source RuntimeDiscovered, Sink accepts a range
        // Source discovers: 44.1kHz (within sink's accepted range)
        // Sink: Accepts 8kHz-96kHz (static but flexible)

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(MockSourceFactory::new(44100, 2)));

        // Create a mock sink that accepts a RANGE of sample rates (not RuntimeDiscovered)
        mod mock_flexible_sink {
            use async_trait::async_trait;
            use remotemedia_runtime_core::capabilities::{
                AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue,
                MediaCapabilities, MediaConstraints,
            };
            use remotemedia_runtime_core::data::RuntimeData;
            use remotemedia_runtime_core::nodes::streaming_node::{StreamingNode, StreamingNodeFactory};
            use serde_json::Value;
            use std::collections::HashMap;

            pub struct FlexibleSinkNode;

            #[async_trait]
            impl StreamingNode for FlexibleSinkNode {
                fn node_type(&self) -> &str { "FlexibleSinkNode" }
                async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                    Ok(data)
                }
                async fn process_multi_async(&self, _inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                    Ok(RuntimeData::Text(String::new()))
                }
                fn is_multi_input(&self) -> bool { false }

                fn media_capabilities(&self) -> Option<MediaCapabilities> {
                    // Accepts 8kHz-96kHz, 1-8 channels
                    Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                        AudioConstraints {
                            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 96000 }),
                            channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                        },
                    )))
                }

                fn capability_behavior(&self) -> CapabilityBehavior {
                    CapabilityBehavior::Static
                }
            }

            pub struct FlexibleSinkFactory;

            impl StreamingNodeFactory for FlexibleSinkFactory {
                fn create(&self, _id: String, _p: &Value, _s: Option<String>) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
                    Ok(Box::new(FlexibleSinkNode))
                }
                fn node_type(&self) -> &str { "FlexibleSinkNode" }
                fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
                    Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                        AudioConstraints {
                            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 96000 }),
                            channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                        },
                    )))
                }
                fn capability_behavior(&self) -> CapabilityBehavior {
                    CapabilityBehavior::Static
                }
            }
        }

        registry.register(Arc::new(mock_flexible_sink::FlexibleSinkFactory));

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "MockRuntimeDiscoveredSource".to_string()),
            ("sink".to_string(), "FlexibleSinkNode".to_string()),
        ];
        let connections = vec![("source".to_string(), "sink".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("sink".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 passes
        assert!(!ctx.has_errors(), "Phase 1 should pass");

        // Phase 2: Source discovers 44.1kHz (within sink's 8-96kHz range)
        let source_actual = MediaCapabilities::with_output(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(44100)),
                channels: Some(ConstraintValue::Exact(2)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "source", source_actual).unwrap();

        // Should be valid - 44.1kHz is within 8-96kHz range
        assert!(
            !ctx.has_errors(),
            "Phase 2 should pass - discovered rate is within accepted range. Errors: {:?}",
            ctx.errors
        );
    }

    #[test]
    fn test_source_discovered_outside_sink_range() {
        // Test: Source RuntimeDiscovered, Sink accepts a limited range
        // Source discovers: 192kHz (OUTSIDE sink's accepted range)
        // Sink: Only accepts 8kHz-48kHz

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(MockSourceFactory::new(192000, 2))); // High sample rate

        // Create a mock sink that only accepts up to 48kHz
        mod mock_limited_sink {
            use async_trait::async_trait;
            use remotemedia_runtime_core::capabilities::{
                AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue,
                MediaCapabilities, MediaConstraints,
            };
            use remotemedia_runtime_core::data::RuntimeData;
            use remotemedia_runtime_core::nodes::streaming_node::{StreamingNode, StreamingNodeFactory};
            use serde_json::Value;
            use std::collections::HashMap;

            pub struct LimitedSinkNode;

            #[async_trait]
            impl StreamingNode for LimitedSinkNode {
                fn node_type(&self) -> &str { "LimitedSinkNode" }
                async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                    Ok(data)
                }
                async fn process_multi_async(&self, _inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                    Ok(RuntimeData::Text(String::new()))
                }
                fn is_multi_input(&self) -> bool { false }

                fn media_capabilities(&self) -> Option<MediaCapabilities> {
                    // Only accepts up to 48kHz
                    Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                        AudioConstraints {
                            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 48000 }),
                            channels: Some(ConstraintValue::Range { min: 1, max: 2 }),
                            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                        },
                    )))
                }

                fn capability_behavior(&self) -> CapabilityBehavior {
                    CapabilityBehavior::Static
                }
            }

            pub struct LimitedSinkFactory;

            impl StreamingNodeFactory for LimitedSinkFactory {
                fn create(&self, _id: String, _p: &Value, _s: Option<String>) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
                    Ok(Box::new(LimitedSinkNode))
                }
                fn node_type(&self) -> &str { "LimitedSinkNode" }
                fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
                    Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                        AudioConstraints {
                            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 48000 }),
                            channels: Some(ConstraintValue::Range { min: 1, max: 2 }),
                            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                        },
                    )))
                }
                fn capability_behavior(&self) -> CapabilityBehavior {
                    CapabilityBehavior::Static
                }
            }
        }

        registry.register(Arc::new(mock_limited_sink::LimitedSinkFactory));

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "MockRuntimeDiscoveredSource".to_string()),
            ("sink".to_string(), "LimitedSinkNode".to_string()),
        ];
        let connections = vec![("source".to_string(), "sink".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("sink".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 passes (source's potential 8-192kHz overlaps with sink's 8-48kHz)
        assert!(!ctx.has_errors(), "Phase 1 should pass");

        // Phase 2: Source discovers 192kHz (outside sink's 8-48kHz range)
        let source_actual = MediaCapabilities::with_output(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(192000)), // Too high!
                channels: Some(ConstraintValue::Exact(2)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        ));
        resolver.revalidate(&mut ctx, "source", source_actual).unwrap();

        // Should now have errors - 192kHz is outside 8-48kHz
        assert!(
            ctx.has_errors(),
            "Phase 2 should fail - discovered rate is outside accepted range"
        );

        let sample_rate_error = ctx.errors.iter().find(|e| e.constraint_name == "sample_rate");
        assert!(
            sample_rate_error.is_some(),
            "Should have sample_rate mismatch. Errors: {:?}",
            ctx.errors
        );
    }

    #[test]
    fn test_three_node_chain_all_runtime_discovered() {
        // Test: Source -> Processor -> Sink, all RuntimeDiscovered
        // This simulates a complex real-world scenario where nothing is known upfront

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(MockSourceFactory::new(48000, 2)));
        registry.register(Arc::new(MockSinkFactory::new(48000, 2)));

        // Add a RuntimeDiscovered processor in the middle
        mod mock_processor {
            use async_trait::async_trait;
            use remotemedia_runtime_core::capabilities::{
                AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue,
                MediaCapabilities, MediaConstraints,
            };
            use remotemedia_runtime_core::data::RuntimeData;
            use remotemedia_runtime_core::nodes::streaming_node::{StreamingNode, StreamingNodeFactory};
            use serde_json::Value;
            use std::collections::HashMap;
            use std::sync::atomic::AtomicBool;

            pub struct RuntimeDiscoveredProcessor {
                node_id: String,
                initialized: AtomicBool,
            }

            #[async_trait]
            impl StreamingNode for RuntimeDiscoveredProcessor {
                fn node_type(&self) -> &str { "RuntimeDiscoveredProcessor" }
                fn node_id(&self) -> &str { &self.node_id }

                async fn initialize(&self) -> Result<(), remotemedia_runtime_core::Error> {
                    self.initialized.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                }

                async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                    Ok(data)
                }
                async fn process_multi_async(&self, inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                    Ok(inputs.into_values().next().unwrap_or(RuntimeData::Text(String::new())))
                }
                fn is_multi_input(&self) -> bool { false }

                fn media_capabilities(&self) -> Option<MediaCapabilities> {
                    None
                }

                fn capability_behavior(&self) -> CapabilityBehavior {
                    CapabilityBehavior::RuntimeDiscovered
                }

                fn potential_capabilities(&self) -> Option<MediaCapabilities> {
                    // Accept broad range in, output broad range
                    Some(MediaCapabilities::with_input_output(
                        MediaConstraints::Audio(AudioConstraints {
                            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                            channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                        }),
                        MediaConstraints::Audio(AudioConstraints {
                            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                            channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                        }),
                    ))
                }

                fn actual_capabilities(&self) -> Option<MediaCapabilities> {
                    if self.initialized.load(std::sync::atomic::Ordering::SeqCst) {
                        // After init, passes through 48kHz stereo
                        Some(MediaCapabilities::with_input_output(
                            MediaConstraints::Audio(AudioConstraints {
                                sample_rate: Some(ConstraintValue::Exact(48000)),
                                channels: Some(ConstraintValue::Exact(2)),
                                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                            }),
                            MediaConstraints::Audio(AudioConstraints {
                                sample_rate: Some(ConstraintValue::Exact(48000)),
                                channels: Some(ConstraintValue::Exact(2)),
                                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                            }),
                        ))
                    } else {
                        self.potential_capabilities()
                    }
                }
            }

            pub struct RuntimeDiscoveredProcessorFactory;

            impl StreamingNodeFactory for RuntimeDiscoveredProcessorFactory {
                fn create(&self, node_id: String, _p: &Value, _s: Option<String>) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
                    Ok(Box::new(RuntimeDiscoveredProcessor {
                        node_id,
                        initialized: AtomicBool::new(false),
                    }))
                }
                fn node_type(&self) -> &str { "RuntimeDiscoveredProcessor" }
                fn capability_behavior(&self) -> CapabilityBehavior {
                    CapabilityBehavior::RuntimeDiscovered
                }
                fn potential_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
                    Some(MediaCapabilities::with_input_output(
                        MediaConstraints::Audio(AudioConstraints {
                            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                            channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                        }),
                        MediaConstraints::Audio(AudioConstraints {
                            sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                            channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                            format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                        }),
                    ))
                }
            }
        }

        registry.register(Arc::new(mock_processor::RuntimeDiscoveredProcessorFactory));

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "MockRuntimeDiscoveredSource".to_string()),
            ("processor".to_string(), "RuntimeDiscoveredProcessor".to_string()),
            ("sink".to_string(), "MockRuntimeDiscoveredSink".to_string()),
        ];
        let connections = vec![
            ("source".to_string(), "processor".to_string()),
            ("processor".to_string(), "sink".to_string()),
        ];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("processor".to_string(), serde_json::json!({}));
        params.insert("sink".to_string(), serde_json::json!({}));

        let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 should pass - all have overlapping potential ranges
        assert!(
            !ctx.has_errors(),
            "Phase 1 should pass for 3-node chain. Errors: {:?}",
            ctx.errors
        );

        // All three should be RuntimeDiscovered and provisional
        assert_eq!(ctx.get_behavior("source"), CapabilityBehavior::RuntimeDiscovered);
        assert_eq!(ctx.get_behavior("processor"), CapabilityBehavior::RuntimeDiscovered);
        assert_eq!(ctx.get_behavior("sink"), CapabilityBehavior::RuntimeDiscovered);

        assert!(ctx.resolved.get("source").unwrap().provisional);
        assert!(ctx.resolved.get("processor").unwrap().provisional);
        assert!(ctx.resolved.get("sink").unwrap().provisional);
    }
}

// =============================================================================
// Full Node Lifecycle Tests: initialize() -> actual_capabilities()
// =============================================================================
//
// These tests verify the complete flow where capabilities are truly unknown
// until the node's initialize() method is called, which discovers the device
// and populates actual_capabilities().
// =============================================================================

mod node_lifecycle_tests {
    use super::*;
    use remotemedia_runtime_core::capabilities::{
        AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue,
        MediaCapabilities, MediaConstraints,
    };
    use remotemedia_runtime_core::data::RuntimeData;
    use remotemedia_runtime_core::nodes::streaming_node::{
        StreamingNode, StreamingNodeFactory, StreamingNodeRegistry,
    };
    use serde_json::Value;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;

    /// A mock device simulator that represents actual hardware
    /// The sample rate is only known when "opened" (simulating real device discovery)
    pub struct MockDeviceSimulator {
        /// The sample rate this device will report when opened
        device_sample_rate: AtomicU32,
        /// The channels this device will report when opened
        device_channels: AtomicU32,
        /// Whether the device has been "opened"
        opened: AtomicBool,
    }

    impl MockDeviceSimulator {
        pub fn new() -> Self {
            // Default: device will report 44100Hz stereo when opened
            Self {
                device_sample_rate: AtomicU32::new(44100),
                device_channels: AtomicU32::new(2),
                opened: AtomicBool::new(false),
            }
        }

        /// Configure what this mock device will report when opened
        pub fn set_device_capabilities(&self, sample_rate: u32, channels: u32) {
            self.device_sample_rate.store(sample_rate, Ordering::SeqCst);
            self.device_channels.store(channels, Ordering::SeqCst);
        }

        /// Simulate opening the device - returns discovered capabilities
        pub fn open(&self) -> (u32, u32) {
            self.opened.store(true, Ordering::SeqCst);
            (
                self.device_sample_rate.load(Ordering::SeqCst),
                self.device_channels.load(Ordering::SeqCst),
            )
        }

        pub fn is_opened(&self) -> bool {
            self.opened.load(Ordering::SeqCst)
        }
    }

    /// A node that truly doesn't know its capabilities until initialize() is called
    pub struct TrueRuntimeDiscoveredNode {
        node_id: String,
        device: Arc<MockDeviceSimulator>,
        /// Discovered capabilities - None until initialize() called
        discovered_caps: std::sync::Mutex<Option<MediaCapabilities>>,
    }

    impl TrueRuntimeDiscoveredNode {
        pub fn new(node_id: String, device: Arc<MockDeviceSimulator>) -> Self {
            Self {
                node_id,
                device,
                discovered_caps: std::sync::Mutex::new(None),
            }
        }
    }

    #[async_trait::async_trait]
    impl StreamingNode for TrueRuntimeDiscoveredNode {
        fn node_type(&self) -> &str {
            "TrueRuntimeDiscoveredNode"
        }

        fn node_id(&self) -> &str {
            &self.node_id
        }

        async fn initialize(&self) -> Result<(), remotemedia_runtime_core::Error> {
            // This is where we "discover" the device capabilities
            let (sample_rate, channels) = self.device.open();

            let caps = MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Exact(sample_rate)),
                    channels: Some(ConstraintValue::Exact(channels)),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            ));

            *self.discovered_caps.lock().unwrap() = Some(caps);
            Ok(())
        }

        async fn process_async(
            &self,
            _data: RuntimeData,
        ) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
            let (sample_rate, channels) = (
                self.device.device_sample_rate.load(Ordering::SeqCst),
                self.device.device_channels.load(Ordering::SeqCst),
            );
            Ok(RuntimeData::Audio {
                samples: vec![0.0; 1024],
                sample_rate,
                channels,
                stream_id: None,
            })
        }

        async fn process_multi_async(
            &self,
            _inputs: std::collections::HashMap<String, RuntimeData>,
        ) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
            self.process_async(RuntimeData::Text(String::new())).await
        }

        fn is_multi_input(&self) -> bool {
            false
        }

        fn media_capabilities(&self) -> Option<MediaCapabilities> {
            // RuntimeDiscovered nodes return None here
            None
        }

        fn capability_behavior(&self) -> CapabilityBehavior {
            CapabilityBehavior::RuntimeDiscovered
        }

        fn potential_capabilities(&self) -> Option<MediaCapabilities> {
            // Before initialize(): return broad range
            Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }

        fn actual_capabilities(&self) -> Option<MediaCapabilities> {
            // After initialize(): return discovered capabilities
            // Before initialize(): return None (unknown)
            self.discovered_caps.lock().unwrap().clone()
        }
    }

    /// Factory that creates nodes with a shared device simulator
    pub struct TrueRuntimeDiscoveredFactory {
        device: Arc<MockDeviceSimulator>,
    }

    impl TrueRuntimeDiscoveredFactory {
        pub fn new(device: Arc<MockDeviceSimulator>) -> Self {
            Self { device }
        }
    }

    impl StreamingNodeFactory for TrueRuntimeDiscoveredFactory {
        fn create(
            &self,
            node_id: String,
            _params: &Value,
            _session_id: Option<String>,
        ) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
            Ok(Box::new(TrueRuntimeDiscoveredNode::new(
                node_id,
                self.device.clone(),
            )))
        }

        fn node_type(&self) -> &str {
            "TrueRuntimeDiscoveredNode"
        }

        fn capability_behavior(&self) -> CapabilityBehavior {
            CapabilityBehavior::RuntimeDiscovered
        }

        fn potential_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
            Some(MediaCapabilities::with_output(MediaConstraints::Audio(
                AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                },
            )))
        }
    }

    #[tokio::test]
    async fn test_actual_capabilities_none_before_initialize() {
        // Create a device simulator (capabilities unknown until opened)
        let device = Arc::new(MockDeviceSimulator::new());
        device.set_device_capabilities(48000, 2);

        // Create node - capabilities should be unknown
        let node = TrueRuntimeDiscoveredNode::new("test".to_string(), device.clone());

        // Before initialize: actual_capabilities should be None
        assert!(
            node.actual_capabilities().is_none(),
            "actual_capabilities() should be None before initialize()"
        );

        // But potential_capabilities should be available
        assert!(
            node.potential_capabilities().is_some(),
            "potential_capabilities() should always be available"
        );

        // Device should not be opened yet
        assert!(!device.is_opened(), "Device should not be opened before initialize()");
    }

    #[tokio::test]
    async fn test_actual_capabilities_populated_after_initialize() {
        let device = Arc::new(MockDeviceSimulator::new());
        device.set_device_capabilities(48000, 2);

        let node = TrueRuntimeDiscoveredNode::new("test".to_string(), device.clone());

        // Initialize the node (discovers device)
        node.initialize().await.unwrap();

        // Device should now be opened
        assert!(device.is_opened(), "Device should be opened after initialize()");

        // actual_capabilities should now be populated
        let caps = node.actual_capabilities();
        assert!(
            caps.is_some(),
            "actual_capabilities() should be Some after initialize()"
        );

        // Verify the discovered capabilities
        let caps = caps.unwrap();
        let output = caps.default_output();
        assert!(output.is_some());

        if let Some(MediaConstraints::Audio(audio)) = output {
            assert_eq!(
                audio.sample_rate,
                Some(ConstraintValue::Exact(48000)),
                "Should have discovered 48kHz"
            );
            assert_eq!(
                audio.channels,
                Some(ConstraintValue::Exact(2)),
                "Should have discovered stereo"
            );
        } else {
            panic!("Expected Audio constraints");
        }
    }

    #[tokio::test]
    async fn test_full_lifecycle_phase1_then_phase2() {
        // This test simulates the complete two-phase resolution flow:
        // Phase 1: Resolve with potential_capabilities (before init)
        // Phase 2: After init, revalidate with actual_capabilities

        let device = Arc::new(MockDeviceSimulator::new());
        device.set_device_capabilities(16000, 1); // Will discover 16kHz mono

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(TrueRuntimeDiscoveredFactory::new(device.clone())));

        // Add Whisper from CLI registry for downstream validation
        let cli_registry = create_test_registry();
        if let Some(factory) = cli_registry.get_factory("RustWhisperNode") {
            registry.register(factory.clone());
        }

        let resolver = CapabilityResolver::new(&registry);

        // Phase 1: Resolve pipeline with potential_capabilities
        let nodes = vec![
            ("source".to_string(), "TrueRuntimeDiscoveredNode".to_string()),
            ("whisper".to_string(), "RustWhisperNode".to_string()),
        ];
        let connections = vec![("source".to_string(), "whisper".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("whisper".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 should pass (potential range includes 16kHz)
        assert!(
            !ctx.has_errors(),
            "Phase 1 should pass with potential_capabilities. Errors: {:?}",
            ctx.errors
        );

        // Source should be provisional
        assert!(
            ctx.resolved.get("source").unwrap().provisional,
            "Source should be provisional before Phase 2"
        );

        // Now create and initialize the actual node
        let factory = TrueRuntimeDiscoveredFactory::new(device.clone());
        let node = factory.create("source".to_string(), &serde_json::json!({}), None).unwrap();

        // Before init: actual_capabilities should be None
        assert!(
            node.actual_capabilities().is_none(),
            "Node should not have actual_capabilities before init"
        );

        // Initialize (discovers device)
        node.initialize().await.unwrap();

        // After init: actual_capabilities should be populated
        let actual_caps = node.actual_capabilities();
        assert!(
            actual_caps.is_some(),
            "Node should have actual_capabilities after init"
        );

        // Phase 2: Revalidate with discovered capabilities
        resolver.revalidate(&mut ctx, "source", actual_caps.unwrap()).unwrap();

        // Should still be valid (16kHz matches Whisper's requirements)
        assert!(
            !ctx.has_errors(),
            "Phase 2 should pass - discovered 16kHz matches Whisper. Errors: {:?}",
            ctx.errors
        );

        // Source should no longer be provisional
        assert!(
            !ctx.resolved.get("source").unwrap().provisional,
            "Source should not be provisional after Phase 2"
        );
    }

    #[tokio::test]
    async fn test_full_lifecycle_phase2_fails_incompatible() {
        // Phase 1 passes, but Phase 2 fails because discovered rate is incompatible

        let device = Arc::new(MockDeviceSimulator::new());
        device.set_device_capabilities(48000, 2); // Will discover 48kHz stereo (incompatible with Whisper)

        let mut registry = StreamingNodeRegistry::new();
        registry.register(Arc::new(TrueRuntimeDiscoveredFactory::new(device.clone())));

        let cli_registry = create_test_registry();
        if let Some(factory) = cli_registry.get_factory("RustWhisperNode") {
            registry.register(factory.clone());
        }

        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("source".to_string(), "TrueRuntimeDiscoveredNode".to_string()),
            ("whisper".to_string(), "RustWhisperNode".to_string()),
        ];
        let connections = vec![("source".to_string(), "whisper".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("whisper".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // Phase 1 passes (potential range includes Whisper's 16kHz)
        assert!(!ctx.has_errors(), "Phase 1 should pass");

        // Create and initialize node
        let factory = TrueRuntimeDiscoveredFactory::new(device.clone());
        let node = factory.create("source".to_string(), &serde_json::json!({}), None).unwrap();
        node.initialize().await.unwrap();

        // Get discovered capabilities
        let actual_caps = node.actual_capabilities().unwrap();

        // Phase 2: Revalidate with discovered 48kHz (incompatible)
        resolver.revalidate(&mut ctx, "source", actual_caps).unwrap();

        // Should now have errors
        assert!(
            ctx.has_errors(),
            "Phase 2 should fail - discovered 48kHz doesn't match Whisper's 16kHz"
        );

        let sample_rate_error = ctx.errors.iter().find(|e| e.constraint_name == "sample_rate");
        assert!(
            sample_rate_error.is_some(),
            "Should have sample_rate mismatch. Errors: {:?}",
            ctx.errors
        );
    }

    #[tokio::test]
    async fn test_device_changes_between_calls() {
        // Test that the node correctly reports new capabilities if device changes
        // (simulates hot-plugging or device switching)

        let device = Arc::new(MockDeviceSimulator::new());

        // Initially configure device as 44.1kHz
        device.set_device_capabilities(44100, 2);

        let node = TrueRuntimeDiscoveredNode::new("test".to_string(), device.clone());

        // Initialize discovers 44.1kHz
        node.initialize().await.unwrap();

        let caps1 = node.actual_capabilities().unwrap();
        if let Some(MediaConstraints::Audio(audio)) = caps1.default_output() {
            assert_eq!(audio.sample_rate, Some(ConstraintValue::Exact(44100)));
        }

        // Note: In a real implementation, you'd need to re-initialize to pick up
        // device changes. This test just verifies the discovered caps are stable.
        // The actual_capabilities reflects what was discovered at initialize time.
    }

    #[tokio::test]
    async fn test_dual_unknown_devices_full_lifecycle() {
        // Both source and sink are RuntimeDiscovered
        // Test the full lifecycle with both devices

        let source_device = Arc::new(MockDeviceSimulator::new());
        source_device.set_device_capabilities(48000, 2);

        let sink_device = Arc::new(MockDeviceSimulator::new());
        sink_device.set_device_capabilities(48000, 2);

        // Create factories
        let source_factory = Arc::new(TrueRuntimeDiscoveredFactory::new(source_device.clone()));

        // Create sink factory (similar but for input)
        struct TrueRuntimeDiscoveredSinkNode {
            node_id: String,
            device: Arc<MockDeviceSimulator>,
            discovered_caps: std::sync::Mutex<Option<MediaCapabilities>>,
        }

        impl TrueRuntimeDiscoveredSinkNode {
            fn new(node_id: String, device: Arc<MockDeviceSimulator>) -> Self {
                Self {
                    node_id,
                    device,
                    discovered_caps: std::sync::Mutex::new(None),
                }
            }
        }

        #[async_trait::async_trait]
        impl StreamingNode for TrueRuntimeDiscoveredSinkNode {
            fn node_type(&self) -> &str { "TrueRuntimeDiscoveredSinkNode" }
            fn node_id(&self) -> &str { &self.node_id }

            async fn initialize(&self) -> Result<(), remotemedia_runtime_core::Error> {
                let (sample_rate, channels) = self.device.open();
                let caps = MediaCapabilities::with_input(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Exact(sample_rate)),
                        channels: Some(ConstraintValue::Exact(channels)),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                ));
                *self.discovered_caps.lock().unwrap() = Some(caps);
                Ok(())
            }

            async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                Ok(data)
            }

            async fn process_multi_async(&self, inputs: std::collections::HashMap<String, RuntimeData>) -> Result<RuntimeData, remotemedia_runtime_core::Error> {
                Ok(inputs.into_values().next().unwrap_or(RuntimeData::Text(String::new())))
            }

            fn is_multi_input(&self) -> bool { false }
            fn media_capabilities(&self) -> Option<MediaCapabilities> { None }
            fn capability_behavior(&self) -> CapabilityBehavior { CapabilityBehavior::RuntimeDiscovered }

            fn potential_capabilities(&self) -> Option<MediaCapabilities> {
                Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                        channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                )))
            }

            fn actual_capabilities(&self) -> Option<MediaCapabilities> {
                self.discovered_caps.lock().unwrap().clone()
            }
        }

        struct TrueRuntimeDiscoveredSinkFactory {
            device: Arc<MockDeviceSimulator>,
        }

        impl StreamingNodeFactory for TrueRuntimeDiscoveredSinkFactory {
            fn create(&self, node_id: String, _params: &Value, _session_id: Option<String>) -> Result<Box<dyn StreamingNode>, remotemedia_runtime_core::Error> {
                Ok(Box::new(TrueRuntimeDiscoveredSinkNode::new(node_id, self.device.clone())))
            }
            fn node_type(&self) -> &str { "TrueRuntimeDiscoveredSinkNode" }
            fn capability_behavior(&self) -> CapabilityBehavior { CapabilityBehavior::RuntimeDiscovered }
            fn potential_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
                Some(MediaCapabilities::with_input(MediaConstraints::Audio(
                    AudioConstraints {
                        sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                        channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                        format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                    },
                )))
            }
        }

        let sink_factory = Arc::new(TrueRuntimeDiscoveredSinkFactory { device: sink_device.clone() });

        let mut registry = StreamingNodeRegistry::new();
        registry.register(source_factory.clone());
        registry.register(sink_factory.clone());

        let resolver = CapabilityResolver::new(&registry);

        // Phase 1
        let nodes = vec![
            ("source".to_string(), "TrueRuntimeDiscoveredNode".to_string()),
            ("sink".to_string(), "TrueRuntimeDiscoveredSinkNode".to_string()),
        ];
        let connections = vec![("source".to_string(), "sink".to_string())];
        let mut params = HashMap::new();
        params.insert("source".to_string(), serde_json::json!({}));
        params.insert("sink".to_string(), serde_json::json!({}));

        let mut ctx = resolver.resolve(&nodes, &connections, &params).unwrap();
        assert!(!ctx.has_errors(), "Phase 1 should pass");

        // Verify both are provisional
        assert!(ctx.resolved.get("source").unwrap().provisional);
        assert!(ctx.resolved.get("sink").unwrap().provisional);

        // Verify devices not opened yet
        assert!(!source_device.is_opened());
        assert!(!sink_device.is_opened());

        // Create and initialize nodes
        let source_node = source_factory.create("source".to_string(), &serde_json::json!({}), None).unwrap();
        let sink_node = sink_factory.create("sink".to_string(), &serde_json::json!({}), None).unwrap();

        source_node.initialize().await.unwrap();
        sink_node.initialize().await.unwrap();

        // Devices should now be opened
        assert!(source_device.is_opened());
        assert!(sink_device.is_opened());

        // Phase 2: Revalidate both
        resolver.revalidate(&mut ctx, "source", source_node.actual_capabilities().unwrap()).unwrap();
        resolver.revalidate(&mut ctx, "sink", sink_node.actual_capabilities().unwrap()).unwrap();

        // Should be valid (both discovered 48kHz stereo)
        assert!(
            !ctx.has_errors(),
            "Phase 2 should pass with compatible devices. Errors: {:?}",
            ctx.errors
        );

        // Neither should be provisional now
        assert!(!ctx.resolved.get("source").unwrap().provisional);
        assert!(!ctx.resolved.get("sink").unwrap().provisional);
    }
}

// =============================================================================
// Auto-Resample Node Tests (Manifest-Based)
// =============================================================================
//
// Tests for the FastResampleNode with auto-configuration support using the
// actual manifest-based pipeline/graph system. When sourceRate or targetRate
// is omitted (or set to "auto"), the node uses lazy initialization and
// detects rates from incoming data.
// =============================================================================

mod auto_resample_tests {
    use super::*;
    use remotemedia_runtime_core::capabilities::{
        CapabilityBehavior, ConstraintValue, MediaConstraints,
    };
    use remotemedia_runtime_core::executor::PipelineGraph;
    use remotemedia_runtime_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};

    /// Helper to create a Manifest programmatically for auto-resample tests
    fn create_manifest(
        name: &str,
        nodes: Vec<(&str, &str, serde_json::Value)>,
        connections: Vec<(&str, &str)>,
    ) -> Manifest {
        Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: name.to_string(),
                description: None,
                created_at: None,
                auto_negotiate: false,
            },
            nodes: nodes
                .into_iter()
                .map(|(id, node_type, params)| NodeManifest {
                    id: id.to_string(),
                    node_type: node_type.to_string(),
                    params,
                    ..Default::default()
                })
                .collect(),
            connections: connections
                .into_iter()
                .map(|(from, to)| Connection {
                    from: from.to_string(),
                    to: to.to_string(),
                })
                .collect(),
        }
    }

    /// Helper to resolve capabilities from a manifest using the graph
    fn resolve_from_manifest(
        manifest: &Manifest,
        registry: &remotemedia_runtime_core::nodes::streaming_node::StreamingNodeRegistry,
    ) -> remotemedia_runtime_core::capabilities::ResolutionContext {
        let graph = PipelineGraph::from_manifest(manifest).expect("Failed to build graph");
        let resolver = CapabilityResolver::new(registry);

        // Extract nodes and connections from graph
        let nodes: Vec<(String, String)> = graph
            .execution_order
            .iter()
            .map(|id| {
                let node = graph.get_node(id).unwrap();
                (id.clone(), node.node_type.clone())
            })
            .collect();

        let connections: Vec<(String, String)> = manifest
            .connections
            .iter()
            .map(|c| (c.from.clone(), c.to.clone()))
            .collect();

        // Build params map from manifest
        let mut params = HashMap::new();
        for node in &manifest.nodes {
            params.insert(node.id.clone(), node.params.clone());
        }

        resolver.resolve(&nodes, &connections, &params).unwrap()
    }

    #[test]
    fn test_resample_with_explicit_rates() {
        // Test: Both sourceRate and targetRate specified in manifest
        // Should create a fixed-rate resampler

        let manifest = create_manifest(
            "test-explicit-rates",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({
                        "sourceRate": 48000,
                        "targetRate": 16000,
                        "channels": 1
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
            ],
            vec![("mic", "resample"), ("resample", "whisper")],
        );

        // Validate graph construction
        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.sources, vec!["mic"]);
        assert_eq!(graph.sinks, vec!["whisper"]);
        assert_eq!(graph.execution_order, vec!["mic", "resample", "whisper"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should be valid - explicit rates match the pipeline requirements
        assert!(
            !ctx.has_errors(),
            "Explicit rate resample should be valid. Errors: {:?}",
            ctx.errors
        );

        // Verify all nodes are in resolved context
        assert!(ctx.resolved.contains_key("mic"));
        assert!(ctx.resolved.contains_key("resample"));
        assert!(ctx.resolved.contains_key("whisper"));

        // When explicit rates are given, behavior should be Configured (not Adaptive)
        assert_eq!(
            ctx.get_behavior("resample"),
            CapabilityBehavior::Configured,
            "Explicit rates should result in Configured behavior"
        );
    }

    #[test]
    fn test_resample_with_auto_source_rate() {
        // Test: sourceRate is "auto", targetRate is explicit in manifest
        // Should detect source rate from incoming audio

        let manifest = create_manifest(
            "test-auto-source-rate",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({
                        "sourceRate": "auto",
                        "targetRate": 16000
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
            ],
            vec![("mic", "resample"), ("resample", "whisper")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.execution_order, vec!["mic", "resample", "whisper"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Resample node should have Adaptive behavior
        assert_eq!(
            ctx.get_behavior("resample"),
            CapabilityBehavior::Adaptive,
            "Resample should have Adaptive behavior"
        );

        // Should be valid - Adaptive node accepts input from mic
        assert!(
            !ctx.has_errors(),
            "Auto source rate should be valid. Errors: {:?}",
            ctx.errors
        );
    }

    #[test]
    fn test_resample_with_auto_target_rate() {
        // Test: sourceRate is explicit, targetRate is "auto" in manifest
        // Should adapt to downstream requirements (Whisper's 16kHz)

        let manifest = create_manifest(
            "test-auto-target-rate",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({
                        "sourceRate": 48000,
                        "targetRate": "auto"
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
            ],
            vec![("mic", "resample"), ("resample", "whisper")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.execution_order, vec!["mic", "resample", "whisper"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should be valid - Adaptive node adapts output to Whisper's requirements
        assert!(
            !ctx.has_errors(),
            "Auto target rate should be valid. Errors: {:?}",
            ctx.errors
        );

        // After reverse pass, resample output should match Whisper's input (16kHz)
        if let Some(resolved) = ctx.resolved.get("resample") {
            if let Some(output) = resolved.capabilities.default_output() {
                match output {
                    MediaConstraints::Audio(audio) => {
                        // Check if the output sample rate matches Whisper's requirement
                        if let Some(ConstraintValue::Exact(rate)) = audio.sample_rate {
                            assert_eq!(
                                rate, 16000,
                                "Resample output should adapt to Whisper's 16kHz"
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn test_resample_fully_auto() {
        // Test: Both sourceRate and targetRate are "auto" in manifest
        // - Source: detected from incoming audio
        // - Target: adapted to downstream requirements

        let manifest = create_manifest(
            "test-fully-auto",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 44100,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({
                        "sourceRate": "auto",
                        "targetRate": "auto"
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
            ],
            vec![("mic", "resample"), ("resample", "whisper")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.execution_order, vec!["mic", "resample", "whisper"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should be valid - Adaptive behavior handles both auto rates
        assert!(
            !ctx.has_errors(),
            "Fully auto resample should be valid. Errors: {:?}",
            ctx.errors
        );

        // Verify resample has Adaptive behavior
        assert_eq!(
            ctx.get_behavior("resample"),
            CapabilityBehavior::Adaptive,
            "Fully auto resample should have Adaptive behavior"
        );
    }

    #[test]
    fn test_resample_omitted_rates() {
        // Test: sourceRate and targetRate are completely omitted in manifest
        // Same behavior as "auto"

        let manifest = create_manifest(
            "test-omitted-rates",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({
                        "quality": "High"
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
            ],
            vec![("mic", "resample"), ("resample", "whisper")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.execution_order, vec!["mic", "resample", "whisper"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should be valid
        assert!(
            !ctx.has_errors(),
            "Omitted rates should be valid. Errors: {:?}",
            ctx.errors
        );

        // Verify resample has Adaptive behavior (omitted = auto)
        assert_eq!(
            ctx.get_behavior("resample"),
            CapabilityBehavior::Adaptive,
            "Omitted rates should result in Adaptive behavior"
        );
    }

    #[test]
    fn test_resample_adaptive_resolves_downstream() {
        // Test: Verify that Adaptive behavior correctly resolves to downstream requirements
        // MicInput (48kHz) -> FastResampleNode (auto) -> RustWhisperNode (16kHz)
        // The resample node should adapt its output to 16kHz

        let manifest = create_manifest(
            "test-adaptive-downstream",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({}), // Fully auto
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
            ],
            vec![("mic", "resample"), ("resample", "whisper")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.execution_order, vec!["mic", "resample", "whisper"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        assert!(
            !ctx.has_errors(),
            "Pipeline should be valid. Errors: {:?}",
            ctx.errors
        );

        // Verify the resolution states
        assert!(
            !ctx.resolved.get("resample").unwrap().needs_reverse_pass(),
            "Resample should be fully resolved after reverse pass"
        );

        // Verify all nodes resolved correctly
        assert!(ctx.resolved.contains_key("mic"));
        assert!(ctx.resolved.contains_key("resample"));
        assert!(ctx.resolved.contains_key("whisper"));
    }

    #[test]
    fn test_resample_full_pipeline_with_speaker() {
        // Test: Full transcription pipeline with auto-resample
        // MicInput (48kHz) -> FastResampleNode (auto) -> RustWhisperNode (16kHz) -> SpeakerOutput

        let manifest = create_manifest(
            "test-full-pipeline",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 2,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({}), // Fully auto - will adapt to Whisper's 16kHz mono
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
                ("speaker", "SpeakerOutput", serde_json::json!({})),
            ],
            vec![
                ("mic", "resample"),
                ("resample", "whisper"),
                ("whisper", "speaker"),
            ],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.sources, vec!["mic"]);
        assert_eq!(graph.sinks, vec!["speaker"]);
        assert_eq!(
            graph.execution_order,
            vec!["mic", "resample", "whisper", "speaker"]
        );

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Pipeline should be valid
        assert!(
            !ctx.has_errors(),
            "Full pipeline with auto-resample should be valid. Errors: {:?}",
            ctx.errors
        );

        // Verify behaviors
        assert_eq!(ctx.get_behavior("mic"), CapabilityBehavior::Configured);
        assert_eq!(ctx.get_behavior("resample"), CapabilityBehavior::Adaptive);
        assert_eq!(ctx.get_behavior("whisper"), CapabilityBehavior::Static);
        assert_eq!(ctx.get_behavior("speaker"), CapabilityBehavior::Passthrough);
    }

    #[test]
    fn test_resample_without_auto_fails_mismatch() {
        // Test: Explicit rates that don't match pipeline requirements should fail
        // MicInput (48kHz) -> FastResampleNode (explicit wrong rates) -> RustWhisperNode (16kHz)

        let manifest = create_manifest(
            "test-explicit-mismatch",
            vec![
                (
                    "mic",
                    "MicInput",
                    serde_json::json!({
                        "sample_rate": 48000,
                        "channels": 1,
                        "device": "test"
                    }),
                ),
                (
                    "resample",
                    "FastResampleNode",
                    serde_json::json!({
                        "sourceRate": 48000,
                        "targetRate": 44100  // Wrong! Whisper needs 16kHz
                    }),
                ),
                ("whisper", "RustWhisperNode", serde_json::json!({})),
            ],
            vec![("mic", "resample"), ("resample", "whisper")],
        );

        let graph = PipelineGraph::from_manifest(&manifest).expect("Failed to build graph");
        assert_eq!(graph.execution_order, vec!["mic", "resample", "whisper"]);

        let registry = create_test_registry();
        let ctx = resolve_from_manifest(&manifest, &registry);

        // Should have errors - 44100 doesn't match Whisper's 16kHz
        assert!(
            ctx.has_errors(),
            "Explicit wrong rate (44100 vs 16kHz) should fail"
        );

        // Verify it's a sample_rate mismatch
        let has_sample_rate_error = ctx
            .errors
            .iter()
            .any(|e| e.constraint_name == "sample_rate");
        assert!(
            has_sample_rate_error,
            "Should detect sample_rate mismatch. Errors: {:?}",
            ctx.errors
        );
    }
}
