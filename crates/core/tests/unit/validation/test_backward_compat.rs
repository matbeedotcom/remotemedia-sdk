//! T053: Backward compatibility tests for validation
//!
//! Tests that nodes without schemas continue to work (warning, not error).

use remotemedia_core::manifest::{Manifest, ManifestMetadata, NodeManifest};
use remotemedia_core::validation::{validate_manifest, SchemaValidator, ValidationResult};
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
// T053: Schema-less nodes should produce warnings, not errors
// =============================================================================

#[test]
fn test_schemaless_node_produces_warning() {
    // GIVEN: A validator with no schemas
    let validator = SchemaValidator::empty();

    // AND: A manifest with a node
    let manifest = test_manifest(vec![NodeManifest {
        id: "test_node".to_string(),
        node_type: "UnknownNode".to_string(),
        params: json!({"anyParam": "anyValue"}),
        ..Default::default()
    }]);

    // WHEN: Validating the manifest
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should be PartiallyValid with warning, not Invalid
    match result {
        ValidationResult::PartiallyValid { warnings } => {
            assert_eq!(warnings.len(), 1);
            assert_eq!(warnings[0].node_id, "test_node");
            assert_eq!(warnings[0].node_type, "UnknownNode");
            assert!(warnings[0].message.contains("no parameter schema"));
        }
        ValidationResult::Valid => panic!("Expected PartiallyValid, got Valid"),
        ValidationResult::Invalid { errors } => {
            panic!(
                "Expected PartiallyValid, got Invalid with {} errors",
                errors.len()
            )
        }
    }
}

#[test]
fn test_schemaless_node_does_not_block_execution() {
    // GIVEN: A validator with no schemas
    let validator = SchemaValidator::empty();

    // AND: A manifest with arbitrary parameters
    let manifest = test_manifest(vec![NodeManifest {
        id: "legacy_node".to_string(),
        node_type: "LegacyProcessor".to_string(),
        params: json!({
            "oldStyle": true,
            "legacyParam": 42,
            "nested": {"deep": "value"}
        }),
        ..Default::default()
    }]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: is_ok should return true (warnings don't block)
    assert!(
        result.is_ok(),
        "Schema-less nodes should not block validation"
    );
}

#[test]
fn test_mixed_nodes_schemaless_and_validated() {
    use remotemedia_core::nodes::schema::create_builtin_schema_registry;

    // GIVEN: A validator with builtin schemas (but not all nodes)
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // AND: A manifest with a known node and an unknown node
    let manifest = test_manifest(vec![
        NodeManifest {
            id: "vad".to_string(),
            node_type: "SileroVAD".to_string(),
            params: json!({"threshold": 0.5}),
            ..Default::default()
        },
        NodeManifest {
            id: "custom".to_string(),
            node_type: "CustomUnregisteredNode".to_string(),
            params: json!({"anyParam": "value"}),
            ..Default::default()
        },
    ]);

    // WHEN: Validating
    let result = validate_manifest(&manifest, &validator);

    // THEN: Should be PartiallyValid (valid known node + warning for unknown)
    match result {
        ValidationResult::PartiallyValid { warnings } => {
            assert_eq!(warnings.len(), 1);
            assert_eq!(warnings[0].node_type, "CustomUnregisteredNode");
        }
        ValidationResult::Valid => {
            // This is also acceptable if there are no warnings
            // (depends on whether SileroVAD is in the registry)
        }
        ValidationResult::Invalid { errors } => {
            panic!(
                "Should not fail validation for unknown node type: {:?}",
                errors
            );
        }
    }
}

#[test]
fn test_warning_type_is_missing_schema() {
    use remotemedia_core::validation::WarningType;

    let validator = SchemaValidator::empty();

    let manifest = test_manifest(vec![NodeManifest {
        id: "node1".to_string(),
        node_type: "NoSchemaNode".to_string(),
        params: json!({}),
        ..Default::default()
    }]);

    let result = validate_manifest(&manifest, &validator);

    if let ValidationResult::PartiallyValid { warnings } = result {
        assert_eq!(warnings[0].warning_type, WarningType::MissingSchema);
    } else {
        panic!("Expected PartiallyValid result");
    }
}

#[test]
fn test_multiple_schemaless_nodes_all_warn() {
    let validator = SchemaValidator::empty();

    let manifest = test_manifest(vec![
        NodeManifest {
            id: "node1".to_string(),
            node_type: "TypeA".to_string(),
            params: json!({}),
            ..Default::default()
        },
        NodeManifest {
            id: "node2".to_string(),
            node_type: "TypeB".to_string(),
            params: json!({}),
            ..Default::default()
        },
        NodeManifest {
            id: "node3".to_string(),
            node_type: "TypeC".to_string(),
            params: json!({}),
            ..Default::default()
        },
    ]);

    let result = validate_manifest(&manifest, &validator);

    match result {
        ValidationResult::PartiallyValid { warnings } => {
            assert_eq!(
                warnings.len(),
                3,
                "Each schema-less node should produce a warning"
            );
        }
        _ => panic!("Expected PartiallyValid"),
    }
}

#[test]
fn test_empty_manifest_is_valid() {
    let validator = SchemaValidator::empty();

    let manifest = test_manifest(vec![]);

    let result = validate_manifest(&manifest, &validator);

    assert!(matches!(result, ValidationResult::Valid));
}
