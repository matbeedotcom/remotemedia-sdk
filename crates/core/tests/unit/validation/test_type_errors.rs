//! T015: Unit test for type mismatch validation
//!
//! Tests that type mismatches produce clear, actionable error messages.

use remotemedia_core::validation::{SchemaValidator, ValidationConstraint};
use serde_json::json;

fn create_test_validator() -> SchemaValidator {
    let mut validator = SchemaValidator::empty();

    let schema = json!({
        "type": "object",
        "properties": {
            "threshold": { "type": "number" },
            "enabled": { "type": "boolean" },
            "name": { "type": "string" },
            "count": { "type": "integer" },
            "items": { "type": "array" },
            "config": { "type": "object" }
        },
        "required": ["threshold"]
    });

    validator.add_schema("TypeTestNode", schema).unwrap();
    validator
}

#[test]
fn test_string_instead_of_number() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": "high"  // Should be number
    });

    let result = validator.validate_node("node1", "TypeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].constraint, ValidationConstraint::Type);
    assert!(errors[0].path.contains("threshold"));
    assert!(errors[0].message.contains("threshold"));
    assert!(errors[0].received.contains("string"));
}

#[test]
fn test_number_instead_of_string() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 0.5,
        "name": 123  // Should be string
    });

    let result = validator.validate_node("node1", "TypeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Type && e.path.contains("name")
    }));
}

#[test]
fn test_string_instead_of_boolean() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 0.5,
        "enabled": "true"  // Should be boolean, not string "true"
    });

    let result = validator.validate_node("node1", "TypeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Type && e.path.contains("enabled")
    }));
}

#[test]
fn test_string_instead_of_array() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 0.5,
        "items": "not an array"
    });

    let result = validator.validate_node("node1", "TypeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Type && e.path.contains("items")
    }));
}

#[test]
fn test_array_instead_of_object() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 0.5,
        "config": ["not", "an", "object"]
    });

    let result = validator.validate_node("node1", "TypeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Type && e.path.contains("config")
    }));
}

#[test]
fn test_null_instead_of_number() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": null
    });

    let result = validator.validate_node("node1", "TypeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Type));
}

#[test]
fn test_multiple_type_errors() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": "not a number",
        "enabled": "not a boolean",
        "count": "not an integer"
    });

    let result = validator.validate_node("node1", "TypeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    // Should have at least 3 type errors
    assert!(errors.len() >= 3);
    assert!(errors.iter().all(|e| e.constraint == ValidationConstraint::Type));
}

#[test]
fn test_error_includes_node_id() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": "invalid"
    });

    let result = validator.validate_node("my_custom_node_id", "TypeTestNode", &params);
    let errors = result.unwrap_err();

    assert_eq!(errors[0].node_id, "my_custom_node_id");
}

#[test]
fn test_error_includes_node_type() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": "invalid"
    });

    let result = validator.validate_node("node1", "TypeTestNode", &params);
    let errors = result.unwrap_err();

    assert_eq!(errors[0].node_type, "TypeTestNode");
}
