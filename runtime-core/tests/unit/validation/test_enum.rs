//! T018: Unit test for enum constraint violation
//!
//! Tests that enum violations produce clear error messages with valid options.

use remotemedia_runtime_core::validation::{SchemaValidator, ValidationConstraint};
use serde_json::json;

fn create_test_validator() -> SchemaValidator {
    let mut validator = SchemaValidator::empty();

    let schema = json!({
        "type": "object",
        "properties": {
            "mode": {
                "type": "string",
                "enum": ["fast", "balanced", "accurate"]
            },
            "format": {
                "type": "string",
                "enum": ["wav", "mp3", "flac", "ogg"]
            },
            "priority": {
                "type": "integer",
                "enum": [1, 2, 3, 4, 5]
            }
        }
    });

    validator.add_schema("EnumTestNode", schema).unwrap();
    validator
}

#[test]
fn test_invalid_enum_value() {
    let validator = create_test_validator();

    let params = json!({
        "mode": "turbo"  // Not in enum
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Enum));
}

#[test]
fn test_valid_enum_value() {
    let validator = create_test_validator();

    let params = json!({
        "mode": "fast"
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_ok());
}

#[test]
fn test_case_sensitive_enum() {
    let validator = create_test_validator();

    let params = json!({
        "mode": "Fast"  // Wrong case
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Enum));
}

#[test]
fn test_enum_error_lists_valid_options() {
    let validator = create_test_validator();

    let params = json!({
        "mode": "invalid"
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    let errors = result.unwrap_err();

    let error = errors.iter().find(|e| e.constraint == ValidationConstraint::Enum).unwrap();
    // Expected should list valid options
    assert!(error.expected.contains("fast"));
    assert!(error.expected.contains("balanced"));
    assert!(error.expected.contains("accurate"));
}

#[test]
fn test_enum_error_shows_received_value() {
    let validator = create_test_validator();

    let params = json!({
        "mode": "invalid_mode"
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    let errors = result.unwrap_err();

    let error = errors.iter().find(|e| e.constraint == ValidationConstraint::Enum).unwrap();
    assert!(error.received.contains("invalid_mode"));
}

#[test]
fn test_integer_enum() {
    let validator = create_test_validator();

    let params = json!({
        "priority": 10  // Not in enum [1, 2, 3, 4, 5]
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Enum));
}

#[test]
fn test_valid_integer_enum() {
    let validator = create_test_validator();

    let params = json!({
        "priority": 3
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_ok());
}

#[test]
fn test_multiple_enum_errors() {
    let validator = create_test_validator();

    let params = json!({
        "mode": "invalid_mode",
        "format": "invalid_format"
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    let enum_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.constraint == ValidationConstraint::Enum)
        .collect();
    assert_eq!(enum_errors.len(), 2);
}

#[test]
fn test_wrong_type_for_enum() {
    let validator = create_test_validator();

    let params = json!({
        "mode": 123  // Number instead of string enum
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_err());
    // Could be type error or enum error depending on schema validation order
}

#[test]
fn test_null_for_enum() {
    let validator = create_test_validator();

    let params = json!({
        "mode": null
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_err());
}

#[test]
fn test_empty_string_not_in_enum() {
    let validator = create_test_validator();

    let params = json!({
        "mode": ""
    });

    let result = validator.validate_node("node1", "EnumTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Enum));
}
