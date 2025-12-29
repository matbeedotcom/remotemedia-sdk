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

    // Check that FastResampleNode has Adaptive behavior
    let resample_behavior = ctx.get_behavior("resample");
    assert_eq!(
        resample_behavior,
        CapabilityBehavior::Adaptive,
        "FastResampleNode should have Adaptive behavior"
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
        CapabilityBehavior::Adaptive,
        "Resample should have Adaptive behavior in context"
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

    // Verify resample node is Adaptive
    let resample_behavior = ctx.get_behavior("resample");
    assert_eq!(
        resample_behavior,
        CapabilityBehavior::Adaptive,
        "FastResampleNode should have Adaptive behavior"
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
