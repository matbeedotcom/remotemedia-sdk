//! T017: Unit test for range constraint violation (min/max)
//!
//! Tests that numeric range violations produce clear error messages.

use remotemedia_core::validation::{SchemaValidator, ValidationConstraint};
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
            "sample_rate": {
                "type": "integer",
                "minimum": 8000,
                "maximum": 48000
            },
            "temperature": {
                "type": "number",
                "exclusiveMinimum": 0.0,
                "exclusiveMaximum": 2.0
            },
            "name": {
                "type": "string",
                "minLength": 1,
                "maxLength": 100
            },
            "tags": {
                "type": "array",
                "minItems": 1,
                "maxItems": 10
            }
        }
    });

    validator.add_schema("RangeTestNode", schema).unwrap();
    validator
}

#[test]
fn test_below_minimum() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": -0.5  // Below minimum 0.0
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Minimum));
}

#[test]
fn test_above_maximum() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 1.5  // Above maximum 1.0
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Maximum));
}

#[test]
fn test_at_minimum_boundary_valid() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 0.0  // Exactly at minimum (inclusive)
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_ok());
}

#[test]
fn test_at_maximum_boundary_valid() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": 1.0  // Exactly at maximum (inclusive)
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_ok());
}

#[test]
fn test_exclusive_minimum_at_boundary_invalid() {
    let validator = create_test_validator();

    let params = json!({
        "temperature": 0.0  // At exclusiveMinimum boundary (invalid)
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::ExclusiveMinimum));
}

#[test]
fn test_exclusive_maximum_at_boundary_invalid() {
    let validator = create_test_validator();

    let params = json!({
        "temperature": 2.0  // At exclusiveMaximum boundary (invalid)
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::ExclusiveMaximum));
}

#[test]
fn test_exclusive_minimum_just_above_valid() {
    let validator = create_test_validator();

    let params = json!({
        "temperature": 0.001  // Just above exclusiveMinimum
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_ok());
}

#[test]
fn test_integer_range() {
    let validator = create_test_validator();

    let params = json!({
        "sample_rate": 4000  // Below minimum 8000
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Minimum));
}

#[test]
fn test_string_min_length() {
    let validator = create_test_validator();

    let params = json!({
        "name": ""  // Empty string, minLength is 1
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::MinLength));
}

#[test]
fn test_string_max_length() {
    let validator = create_test_validator();

    let params = json!({
        "name": "x".repeat(101)  // 101 chars, maxLength is 100
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::MaxLength));
}

#[test]
fn test_array_min_items() {
    let validator = create_test_validator();

    let params = json!({
        "tags": []  // Empty array, minItems is 1
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::MinItems));
}

#[test]
fn test_array_max_items() {
    let validator = create_test_validator();

    let params = json!({
        "tags": ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"]  // 11 items, maxItems is 10
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::MaxItems));
}

#[test]
fn test_range_error_message_includes_limits() {
    let validator = create_test_validator();

    let params = json!({
        "threshold": -0.5
    });

    let result = validator.validate_node("node1", "RangeTestNode", &params);
    let errors = result.unwrap_err();

    let error = errors.iter().find(|e| e.constraint == ValidationConstraint::Minimum).unwrap();
    // Message should include the limit
    assert!(error.expected.contains("0"));
    // And the actual value
    assert!(error.received.contains("-0.5"));
}
