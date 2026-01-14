//! T019: Unit test for nested object validation
//!
//! Tests that validation works correctly for deeply nested structures.

use remotemedia_core::validation::{SchemaValidator, ValidationConstraint};
use serde_json::json;

fn create_test_validator() -> SchemaValidator {
    let mut validator = SchemaValidator::empty();

    let schema = json!({
        "type": "object",
        "properties": {
            "model": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "config": {
                        "type": "object",
                        "properties": {
                            "batch_size": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 64
                            },
                            "precision": {
                                "type": "string",
                                "enum": ["fp16", "fp32", "int8"]
                            }
                        },
                        "required": ["batch_size"]
                    }
                },
                "required": ["path"]
            },
            "audio": {
                "type": "object",
                "properties": {
                    "sample_rate": { "type": "integer" },
                    "channels": { "type": "integer" }
                }
            }
        }
    });

    validator.add_schema("NestedTestNode", schema).unwrap();
    validator
}

#[test]
fn test_nested_type_error() {
    let validator = create_test_validator();

    let params = json!({
        "model": {
            "path": "/path/to/model",
            "config": {
                "batch_size": "not a number"  // Should be integer
            }
        }
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Type
            && e.path.contains("batch_size")
    }));
}

#[test]
fn test_nested_path_in_error() {
    let validator = create_test_validator();

    let params = json!({
        "model": {
            "path": "/path/to/model",
            "config": {
                "batch_size": "invalid"
            }
        }
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    let errors = result.unwrap_err();

    let error = &errors[0];
    // Path should show the full nested path
    assert!(error.path.contains("model"));
    assert!(error.path.contains("config"));
    assert!(error.path.contains("batch_size"));
}

#[test]
fn test_nested_required_missing() {
    let validator = create_test_validator();

    let params = json!({
        "model": {
            "config": {
                "batch_size": 16
            }
            // Missing required "path"
        }
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Required
            && e.message.contains("path")
    }));
}

#[test]
fn test_deeply_nested_required_missing() {
    let validator = create_test_validator();

    let params = json!({
        "model": {
            "path": "/path/to/model",
            "config": {
                // Missing required "batch_size"
                "precision": "fp16"
            }
        }
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.constraint == ValidationConstraint::Required
            && e.message.contains("batch_size")
    }));
}

#[test]
fn test_nested_range_violation() {
    let validator = create_test_validator();

    let params = json!({
        "model": {
            "path": "/path/to/model",
            "config": {
                "batch_size": 100  // Above maximum 64
            }
        }
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Maximum));
}

#[test]
fn test_nested_enum_violation() {
    let validator = create_test_validator();

    let params = json!({
        "model": {
            "path": "/path/to/model",
            "config": {
                "batch_size": 16,
                "precision": "fp64"  // Not in enum
            }
        }
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Enum));
}

#[test]
fn test_valid_nested_object() {
    let validator = create_test_validator();

    let params = json!({
        "model": {
            "path": "/path/to/model",
            "config": {
                "batch_size": 16,
                "precision": "fp16"
            }
        },
        "audio": {
            "sample_rate": 16000,
            "channels": 1
        }
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_ok());
}

#[test]
fn test_wrong_type_for_nested_object() {
    let validator = create_test_validator();

    let params = json!({
        "model": "not an object"  // Should be object
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Type));
}

#[test]
fn test_array_instead_of_nested_object() {
    let validator = create_test_validator();

    let params = json!({
        "model": ["not", "an", "object"]
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Type));
}

#[test]
fn test_multiple_nested_errors() {
    let validator = create_test_validator();

    let params = json!({
        "model": {
            // Missing required "path"
            "config": {
                "batch_size": "invalid",  // Type error
                "precision": "invalid"    // Enum error
            }
        }
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    // Should have multiple errors
    assert!(errors.len() >= 2);
}

#[test]
fn test_empty_nested_object() {
    let validator = create_test_validator();

    let params = json!({
        "model": {}  // Empty object, missing required "path"
    });

    let result = validator.validate_node("node1", "NestedTestNode", &params);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.constraint == ValidationConstraint::Required));
}
