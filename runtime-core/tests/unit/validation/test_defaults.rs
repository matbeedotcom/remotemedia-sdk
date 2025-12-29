//! T055: Tests for optional parameters with defaults
//!
//! Tests that optional parameters with defaults work correctly:
//! - Missing optional params don't cause validation errors
//! - Default values are documented in schema
//! - Schema introspection reveals defaults

use remotemedia_runtime_core::manifest::{Manifest, ManifestMetadata, NodeManifest};
use remotemedia_runtime_core::validation::{validate_manifest, SchemaValidator, ValidationResult};
use serde_json::json;

/// Helper to create a minimal test manifest
fn test_manifest(nodes: Vec<NodeManifest>) -> Manifest {
    Manifest {
        version: "1.0".to_string(),
        metadata: ManifestMetadata {
            name: "test".to_string(),
                ..Default::default()
            },
        nodes,
        connections: vec![],
    }
}

// =============================================================================
// T055: Optional parameters with defaults
// =============================================================================

#[test]
fn test_optional_param_can_be_missing() {
    // GIVEN: A schema with optional param (not in required array)
    let mut validator = SchemaValidator::empty();
    validator
        .add_schema(
            "OptionalNode",
            json!({
                "type": "object",
                "properties": {
                    "required_param": {"type": "string"},
                    "optional_param": {
                        "type": "number",
                        "default": 0.5
                    }
                },
                "required": ["required_param"]
            }),
        )
        .unwrap();

    // AND: Params without the optional value
    let manifest = test_manifest(vec![NodeManifest {
        id: "test".to_string(),
        node_type: "OptionalNode".to_string(),
        params: json!({
            "required_param": "value"
            // optional_param omitted
        }),
        ..Default::default()
    }]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should be valid
    assert!(
        matches!(result, ValidationResult::Valid),
        "Missing optional param should not cause error"
    );
}

#[test]
fn test_all_optional_params_can_be_omitted() {
    // GIVEN: A schema with only optional params
    let mut validator = SchemaValidator::empty();
    validator
        .add_schema(
            "AllOptionalNode",
            json!({
                "type": "object",
                "properties": {
                    "threshold": {
                        "type": "number",
                        "default": 0.5
                    },
                    "mode": {
                        "type": "string",
                        "default": "auto"
                    },
                    "enabled": {
                        "type": "boolean",
                        "default": true
                    }
                }
                // No "required" array = all optional
            }),
        )
        .unwrap();

    // AND: Empty params object
    let manifest = test_manifest(vec![NodeManifest {
        id: "test".to_string(),
        node_type: "AllOptionalNode".to_string(),
        params: json!({}),
        ..Default::default()
    }]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should be valid
    assert!(
        matches!(result, ValidationResult::Valid),
        "All-optional node with empty params should be valid"
    );
}

#[test]
fn test_provided_optional_params_still_validated() {
    // GIVEN: A schema with optional param that has constraints
    let mut validator = SchemaValidator::empty();
    validator
        .add_schema(
            "ConstrainedOptionalNode",
            json!({
                "type": "object",
                "properties": {
                    "threshold": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "default": 0.5
                    }
                }
            }),
        )
        .unwrap();

    // AND: Params with invalid optional value
    let manifest = test_manifest(vec![NodeManifest {
        id: "test".to_string(),
        node_type: "ConstrainedOptionalNode".to_string(),
        params: json!({
            "threshold": 2.0  // Invalid - above maximum
        }),
        ..Default::default()
    }]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should fail validation
    assert!(
        matches!(result, ValidationResult::Invalid { .. }),
        "Provided optional params should still be validated"
    );
}

#[test]
fn test_schema_contains_default_values() {
    // GIVEN: A schema with defaults
    let mut validator = SchemaValidator::empty();
    validator
        .add_schema(
            "DefaultsNode",
            json!({
                "type": "object",
                "properties": {
                    "threshold": {
                        "type": "number",
                        "default": 0.75
                    },
                    "name": {
                        "type": "string",
                        "default": "default_name"
                    }
                }
            }),
        )
        .unwrap();

    // WHEN: Getting the schema
    let schema = validator.get_schema("DefaultsNode").unwrap();

    // THEN: Defaults should be in the schema
    let threshold_default = schema["properties"]["threshold"]["default"].as_f64();
    assert_eq!(threshold_default, Some(0.75));

    let name_default = schema["properties"]["name"]["default"].as_str();
    assert_eq!(name_default, Some("default_name"));
}

#[test]
fn test_null_value_for_optional_without_nullable() {
    // GIVEN: A schema with optional param (not nullable)
    let mut validator = SchemaValidator::empty();
    validator
        .add_schema(
            "NonNullableOptional",
            json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "number",
                        "default": 10
                    }
                }
            }),
        )
        .unwrap();

    // AND: Params with explicit null (not the same as missing!)
    let manifest = test_manifest(vec![NodeManifest {
        id: "test".to_string(),
        node_type: "NonNullableOptional".to_string(),
        params: json!({
            "value": null
        }),
        ..Default::default()
    }]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should fail - null is not a number
    // Note: Missing is OK, but explicit null violates type
    assert!(
        matches!(result, ValidationResult::Invalid { .. }),
        "Explicit null should fail type validation"
    );
}

#[test]
fn test_optional_with_nullable_type() {
    // GIVEN: A schema with nullable optional param
    let mut validator = SchemaValidator::empty();
    validator
        .add_schema(
            "NullableOptional",
            json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": ["number", "null"],
                        "default": null
                    }
                }
            }),
        )
        .unwrap();

    // AND: Params with explicit null
    let manifest = test_manifest(vec![NodeManifest {
        id: "test".to_string(),
        node_type: "NullableOptional".to_string(),
        params: json!({
            "value": null
        }),
        ..Default::default()
    }]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should be valid - null is allowed
    assert!(
        matches!(result, ValidationResult::Valid),
        "Nullable type should accept null"
    );
}

#[test]
fn test_nested_optional_defaults() {
    // GIVEN: A schema with nested optional params
    let mut validator = SchemaValidator::empty();
    validator
        .add_schema(
            "NestedDefaultsNode",
            json!({
                "type": "object",
                "properties": {
                    "config": {
                        "type": "object",
                        "properties": {
                            "threshold": {
                                "type": "number",
                                "default": 0.5
                            },
                            "mode": {
                                "type": "string",
                                "default": "auto"
                            }
                        },
                        "default": {}
                    }
                }
            }),
        )
        .unwrap();

    // AND: Params with partial nested config
    let manifest = test_manifest(vec![NodeManifest {
        id: "test".to_string(),
        node_type: "NestedDefaultsNode".to_string(),
        params: json!({
            "config": {
                "threshold": 0.8
                // mode uses default
            }
        }),
        ..Default::default()
    }]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should be valid
    assert!(
        matches!(result, ValidationResult::Valid),
        "Partial nested config with defaults should be valid"
    );
}

#[test]
fn test_array_optional_default() {
    // GIVEN: A schema with optional array param
    let mut validator = SchemaValidator::empty();
    validator
        .add_schema(
            "ArrayDefaultNode",
            json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {"type": "string"},
                        "default": []
                    }
                }
            }),
        )
        .unwrap();

    // AND: Params without the array
    let manifest = test_manifest(vec![NodeManifest {
        id: "test".to_string(),
        node_type: "ArrayDefaultNode".to_string(),
        params: json!({}),
        ..Default::default()
    }]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should be valid
    assert!(
        matches!(result, ValidationResult::Valid),
        "Missing optional array should be valid"
    );
}
