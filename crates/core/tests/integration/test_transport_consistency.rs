//! T034: Cross-transport validation consistency test
//!
//! Verifies that validation errors have consistent structure and content
//! regardless of which transport layer returns them.
//!
//! This test validates the core ValidationError structure that all transports use.

use remotemedia_core::validation::{SchemaValidator, ValidationConstraint, ValidationError};
use serde_json::json;

/// Create a validator with test schemas that exercise all constraint types
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
            "buffer_size": {
                "type": "integer",
                "minimum": 1
            }
        },
        "required": ["threshold", "mode"]
    });

    validator.add_schema("ConsistencyTestNode", schema).unwrap();
    validator
}

#[test]
fn test_validation_error_structure_is_complete() {
    // GIVEN: A validator and invalid parameters
    let validator = create_test_validator();
    let params = json!({
        "threshold": "not a number",  // Type error
        "mode": "invalid"             // Enum error
    });

    // WHEN: Validation fails
    let result = validator.validate_node("test_node", "ConsistencyTestNode", &params);

    // THEN: Error structure contains all required fields
    let errors = result.unwrap_err();
    assert!(!errors.is_empty(), "Should have validation errors");

    for error in &errors {
        // All required fields must be present and non-empty
        assert!(!error.node_id.is_empty(), "node_id must be set");
        assert!(!error.node_type.is_empty(), "node_type must be set");
        assert!(!error.path.is_empty(), "path must be set");
        assert!(!error.expected.is_empty(), "expected must be set");
        assert!(!error.received.is_empty(), "received must be set");
        assert!(!error.message.is_empty(), "message must be set");

        // Constraint must be a known variant
        match &error.constraint {
            ValidationConstraint::Type
            | ValidationConstraint::Required
            | ValidationConstraint::Minimum
            | ValidationConstraint::Maximum
            | ValidationConstraint::ExclusiveMinimum
            | ValidationConstraint::ExclusiveMaximum
            | ValidationConstraint::Enum
            | ValidationConstraint::Pattern
            | ValidationConstraint::MinLength
            | ValidationConstraint::MaxLength
            | ValidationConstraint::MinItems
            | ValidationConstraint::MaxItems
            | ValidationConstraint::AdditionalProperties
            | ValidationConstraint::Format
            | ValidationConstraint::Other(_) => {}
        }
    }
}

#[test]
fn test_validation_errors_are_json_serializable() {
    // GIVEN: Validation errors
    let validator = create_test_validator();
    let params = json!({
        "threshold": "invalid",
        "mode": "wrong"
    });

    let result = validator.validate_node("test_node", "ConsistencyTestNode", &params);
    let errors = result.unwrap_err();

    // WHEN: Serialized to JSON
    let json_result = serde_json::to_string(&errors);

    // THEN: Serialization succeeds and produces valid JSON
    assert!(json_result.is_ok(), "Errors must be JSON serializable");

    let json_str = json_result.unwrap();
    assert!(!json_str.is_empty());

    // AND: Can be deserialized back
    let parsed: Vec<ValidationError> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed.len(), errors.len());
}

#[test]
fn test_error_message_contains_context() {
    // GIVEN: A validator and invalid parameters
    let validator = create_test_validator();
    let params = json!({
        "threshold": 1.5,  // Above maximum
        "mode": "fast"
    });

    // WHEN: Validation fails
    let result = validator.validate_node("my_node", "ConsistencyTestNode", &params);
    let errors = result.unwrap_err();

    // THEN: Error message contains node context
    let error = &errors[0];
    assert!(
        error.message.contains("my_node"),
        "Message should contain node_id"
    );
    assert!(
        error.message.contains("ConsistencyTestNode"),
        "Message should contain node_type"
    );
    assert!(
        error.message.contains("threshold"),
        "Message should contain parameter name"
    );
}

#[test]
fn test_constraint_type_matches_error() {
    let validator = create_test_validator();

    // Type mismatch
    let params = json!({ "threshold": "string", "mode": "fast" });
    let errors = validator
        .validate_node("n1", "ConsistencyTestNode", &params)
        .unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Type));

    // Missing required
    let params = json!({ "threshold": 0.5 }); // mode missing
    let errors = validator
        .validate_node("n2", "ConsistencyTestNode", &params)
        .unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Required));

    // Maximum exceeded
    let params = json!({ "threshold": 1.5, "mode": "fast" });
    let errors = validator
        .validate_node("n3", "ConsistencyTestNode", &params)
        .unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Maximum));

    // Minimum exceeded
    let params = json!({ "threshold": -0.5, "mode": "fast" });
    let errors = validator
        .validate_node("n4", "ConsistencyTestNode", &params)
        .unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Minimum));

    // Enum violation
    let params = json!({ "threshold": 0.5, "mode": "invalid_mode" });
    let errors = validator
        .validate_node("n5", "ConsistencyTestNode", &params)
        .unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Enum));
}

#[test]
fn test_path_uses_json_pointer_format() {
    let validator = create_test_validator();
    let params = json!({ "threshold": "invalid", "mode": "fast" });

    let errors = validator
        .validate_node("n1", "ConsistencyTestNode", &params)
        .unwrap_err();

    // Path should be JSON pointer format (starts with /)
    let error = &errors[0];
    assert!(
        error.path.starts_with('/'),
        "Path should be JSON pointer format: {}",
        error.path
    );
}

#[test]
fn test_node_id_preserved_in_all_errors() {
    let validator = create_test_validator();
    let params = json!({
        "threshold": "invalid",  // Type error
        "mode": "wrong"          // Enum error
    });

    let errors = validator
        .validate_node("unique_node_123", "ConsistencyTestNode", &params)
        .unwrap_err();

    // All errors should have the same node_id
    for error in &errors {
        assert_eq!(
            error.node_id, "unique_node_123",
            "All errors should preserve node_id"
        );
    }
}

#[test]
fn test_node_type_preserved_in_all_errors() {
    let validator = create_test_validator();
    let params = json!({
        "threshold": "invalid",
        "mode": "wrong"
    });

    let errors = validator
        .validate_node("n1", "ConsistencyTestNode", &params)
        .unwrap_err();

    // All errors should have the same node_type
    for error in &errors {
        assert_eq!(
            error.node_type, "ConsistencyTestNode",
            "All errors should preserve node_type"
        );
    }
}

#[test]
fn test_multiple_errors_from_same_node() {
    let validator = create_test_validator();

    // Multiple validation failures on same node
    let params = json!({
        "threshold": "not_a_number",
        "mode": "invalid_mode",
        "buffer_size": -5
    });

    let errors = validator
        .validate_node("multi_error_node", "ConsistencyTestNode", &params)
        .unwrap_err();

    // Should have at least 2 errors (threshold type + mode enum)
    assert!(
        errors.len() >= 2,
        "Should report multiple errors from same node"
    );

    // All should have same node_id
    assert!(errors.iter().all(|e| e.node_id == "multi_error_node"));
}
