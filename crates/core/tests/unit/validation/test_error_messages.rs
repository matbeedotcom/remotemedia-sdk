//! T024: Verify error messages meet SC-003 criteria (self-explanatory)
//!
//! Tests that error messages are clear, actionable, and self-explanatory.

use remotemedia_core::validation::{SchemaValidator, ValidationConstraint, ValidationError};
use serde_json::json;

fn create_test_validator() -> SchemaValidator {
    let mut validator = SchemaValidator::empty();

    let schema = json!({
        "type": "object",
        "properties": {
            "threshold": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0
            },
            "mode": {
                "type": "string",
                "enum": ["fast", "balanced", "accurate"]
            },
            "model_path": {
                "type": "string"
            },
            "config": {
                "type": "object",
                "properties": {
                    "batch_size": { "type": "integer" }
                }
            }
        },
        "required": ["threshold", "model_path"]
    });

    validator.add_schema("ErrorMessageTestNode", schema).unwrap();
    validator
}

/// SC-003: Error messages must be self-explanatory
/// They should contain enough information for a developer to fix the issue
/// without needing to look up documentation.
#[test]
fn test_error_message_contains_node_id() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": "invalid"
    });

    let result = validator.validate_node("my_asr_node", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    // Node ID should be accessible
    assert_eq!(errors[0].node_id, "my_asr_node");
    // And in the formatted message
    let message = format!("{}", errors[0]);
    assert!(message.contains("my_asr_node"));
}

#[test]
fn test_error_message_contains_node_type() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": "invalid"
    });

    let result = validator.validate_node("node1", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    assert_eq!(errors[0].node_type, "ErrorMessageTestNode");
    let message = format!("{}", errors[0]);
    assert!(message.contains("ErrorMessageTestNode"));
}

#[test]
fn test_error_message_contains_parameter_path() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 0.5,
        "config": {
            "batch_size": "invalid"
        }
    });

    let result = validator.validate_node("node1", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    // Path should include the full path to the parameter
    let error = &errors[0];
    assert!(error.path.contains("config"));
    assert!(error.path.contains("batch_size"));
}

#[test]
fn test_type_error_message_shows_expected_and_received() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": "high"  // Expected number, got string
    });

    let result = validator.validate_node("node1", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    let error = &errors[0];
    // Should show what type was expected
    assert!(!error.expected.is_empty());
    // Should show what type was received
    assert!(error.received.contains("string"));

    // Full message should be clear
    let message = format!("{}", error);
    assert!(message.contains("threshold"));
}

#[test]
fn test_required_error_message_names_missing_field() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 0.5
        // Missing required "model_path"
    });

    let result = validator.validate_node("node1", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    let error = errors.iter().find(|e| e.constraint == ValidationConstraint::Required).unwrap();
    // Message should name the missing field
    assert!(error.message.contains("model_path"));
}

#[test]
fn test_range_error_message_shows_limits() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 1.5,  // Above maximum 1.0
        "model_path": "/path"
    });

    let result = validator.validate_node("node1", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    let error = &errors[0];
    // Should show the limit
    assert!(error.expected.contains("1"));
    // Should show the actual value
    assert!(error.received.contains("1.5"));
}

#[test]
fn test_enum_error_message_lists_valid_options() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 0.5,
        "model_path": "/path",
        "mode": "invalid"
    });

    let result = validator.validate_node("node1", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    let error = errors.iter().find(|e| e.constraint == ValidationConstraint::Enum).unwrap();
    // Should list valid options
    assert!(error.expected.contains("fast"));
    assert!(error.expected.contains("balanced"));
    assert!(error.expected.contains("accurate"));
    // Should show what was provided
    assert!(error.received.contains("invalid"));
}

#[test]
fn test_display_trait_produces_readable_message() {
    let error = ValidationError::new(
        "audio_processor",
        "AudioNode",
        "/config/sample_rate",
        ValidationConstraint::Type,
        "integer",
        "string",
    );

    let message = format!("{}", error);

    // Message should be a complete, readable sentence
    assert!(message.contains("audio_processor"));
    assert!(message.contains("AudioNode"));
    assert!(message.contains("sample_rate"));
    assert!(message.contains("integer"));
    assert!(message.contains("string"));
}

#[test]
fn test_constraint_display_is_human_readable() {
    // All constraints should have human-readable display
    let constraints = vec![
        ValidationConstraint::Type,
        ValidationConstraint::Required,
        ValidationConstraint::Minimum,
        ValidationConstraint::Maximum,
        ValidationConstraint::Enum,
        ValidationConstraint::Pattern,
        ValidationConstraint::MinLength,
        ValidationConstraint::MaxLength,
    ];

    for constraint in constraints {
        let display = format!("{}", constraint);
        // Should not be empty
        assert!(!display.is_empty());
        // Should be lowercase and readable (allow punctuation like apostrophes)
        assert!(
            display.chars().all(|c| c.is_lowercase() || c.is_whitespace() || c.is_ascii_punctuation()),
            "Display '{}' should be human-readable lowercase text",
            display
        );
    }
}

#[test]
fn test_error_message_is_actionable() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 2.5,  // Above max
        "model_path": "/path"
    });

    let result = validator.validate_node("my_node", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    let error = &errors[0];
    let message = format!("{}", error);

    // A developer should be able to understand from the message:
    // 1. Which node has the problem
    assert!(message.contains("my_node"));
    // 2. Which parameter is wrong
    assert!(message.contains("threshold"));
    // 3. What the constraint is (either "maximum" or "<=" for range constraint)
    assert!(
        message.contains("maximum") || message.contains("Maximum") || message.contains("<="),
        "Message should indicate maximum constraint: {}",
        message
    );
    // 4. What value was provided
    assert!(message.contains("2.5"));
}

#[test]
fn test_multiple_errors_are_all_descriptive() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": "not a number",
        "mode": "invalid_mode"
        // Missing model_path
    });

    let result = validator.validate_node("node1", "ErrorMessageTestNode", &params);
    let errors = result.unwrap_err();

    // Each error should be independently understandable
    for error in &errors {
        let message = format!("{}", error);
        // Each message should identify the problem parameter
        assert!(!message.is_empty());
        // Each should have node context
        assert!(message.contains("node1") || message.contains("ErrorMessageTestNode"));
    }
}
