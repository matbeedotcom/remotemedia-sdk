//! T016: Unit test for missing required parameter validation
//!
//! Tests that missing required parameters produce clear error messages.

use remotemedia_core::validation::{SchemaValidator, ValidationConstraint};
use serde_json::json;

fn create_test_validator() -> SchemaValidator {
    let mut validator = SchemaValidator::empty();

    let schema = json!({
        "type": "object",
        "properties": {
            "model_path": { "type": "string" },
            "sample_rate": { "type": "integer" },
            "channels": { "type": "integer" },
            "optional_param": { "type": "string" }
        },
        "required": ["model_path", "sample_rate", "channels"]
    });

    validator.add_schema("RequiredTestNode", schema).unwrap();
    validator
}

#[test]
fn test_single_missing_required() {
    let validator = create_test_validator();

    let params = json!({
        "sample_rate": 16000,
        "channels": 1
        // Missing "model_path"
    });

    let result = validator.validate_node("node1", "RequiredTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Required
            && e.message.contains("model_path")
    }));
}

#[test]
fn test_multiple_missing_required() {
    let validator = create_test_validator();

    let params = json!({
        "optional_param": "value"
        // Missing all required params
    });

    let result = validator.validate_node("node1", "RequiredTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    let required_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.constraint == ValidationConstraint::Required)
        .collect();

    // Should have errors for model_path, sample_rate, and channels
    assert_eq!(required_errors.len(), 3);
}

#[test]
fn test_empty_object_missing_all_required() {
    let validator = create_test_validator();

    let params = json!({});

    let result = validator.validate_node("node1", "RequiredTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 3); // All 3 required fields missing
}

#[test]
fn test_required_error_message_format() {
    let validator = create_test_validator();

    let params = json!({
        "sample_rate": 16000,
        "channels": 1
    });

    let result = validator.validate_node("asr_node", "RequiredTestNode", &params);
    let errors = result.unwrap_err();

    let error = &errors[0];
    // Error message should be self-explanatory (SC-003)
    assert!(error.message.contains("model_path"));
    assert!(error.message.contains("required") || error.message.contains("Required"));
    assert_eq!(error.node_id, "asr_node");
}

#[test]
fn test_optional_param_can_be_missing() {
    let validator = create_test_validator();

    let params = json!({
        "model_path": "/path/to/model",
        "sample_rate": 16000,
        "channels": 1
        // "optional_param" is not required
    });

    let result = validator.validate_node("node1", "RequiredTestNode", &params);
    assert!(result.is_ok());
}

#[test]
fn test_null_value_for_required_param() {
    let validator = create_test_validator();

    let params = json!({
        "model_path": null,  // Explicit null is not the same as missing
        "sample_rate": 16000,
        "channels": 1
    });

    let result = validator.validate_node("node1", "RequiredTestNode", &params);
    // null for a string type should fail type validation, not required
    assert!(result.is_err());
}

#[test]
fn test_required_in_nested_object() {
    let mut validator = SchemaValidator::empty();

    let schema = json!({
        "type": "object",
        "properties": {
            "config": {
                "type": "object",
                "properties": {
                    "api_key": { "type": "string" },
                    "endpoint": { "type": "string" }
                },
                "required": ["api_key"]
            }
        }
    });

    validator.add_schema("NestedRequiredNode", schema).unwrap();

    let params = json!({
        "config": {
            "endpoint": "https://api.example.com"
            // Missing required "api_key"
        }
    });

    let result = validator.validate_node("node1", "NestedRequiredNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Required
            && e.message.contains("api_key")
    }));
}
