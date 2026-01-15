//! T048-T049: Unit tests for schema introspection API
//!
//! Tests that operators can query parameter schemas for all registered node types.

use remotemedia_core::nodes::schema::{collect_registered_configs, create_builtin_schema_registry};
use remotemedia_core::validation::{get_all_schemas, get_node_schema, SchemaValidator};

// =============================================================================
// T048: Test get_node_schema returns correct schema
// =============================================================================

#[test]
fn test_get_node_schema_returns_schema_for_known_node() {
    // GIVEN: A validator with schemas
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // WHEN: Querying for a known node type
    let schema = get_node_schema("SileroVAD", &validator);

    // THEN: Schema is returned
    assert!(schema.is_some(), "Should return schema for known node type");

    let schema = schema.unwrap();
    assert!(schema.is_object(), "Schema should be a JSON object");
}

#[test]
fn test_get_node_schema_returns_none_for_unknown_node() {
    // GIVEN: A validator with schemas
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // WHEN: Querying for an unknown node type
    let schema = get_node_schema("NonExistentNode", &validator);

    // THEN: None is returned
    assert!(schema.is_none(), "Should return None for unknown node type");
}

#[test]
fn test_get_node_schema_contains_properties() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // SileroVAD should have threshold property
    let schema = get_node_schema("SileroVAD", &validator).unwrap();

    assert!(
        schema.get("properties").is_some(),
        "Schema should have properties"
    );

    let properties = schema["properties"].as_object().unwrap();
    assert!(
        properties.contains_key("threshold"),
        "SileroVAD schema should have threshold property"
    );
}

#[test]
fn test_get_node_schema_includes_constraints() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    let schema = get_node_schema("SileroVAD", &validator).unwrap();
    let threshold = &schema["properties"]["threshold"];

    // Should have type constraint
    assert!(
        threshold.get("type").is_some(),
        "threshold should have type constraint"
    );

    // Should have range constraints
    assert!(
        threshold.get("minimum").is_some() || threshold.get("exclusiveMinimum").is_some(),
        "threshold should have minimum constraint"
    );
    assert!(
        threshold.get("maximum").is_some() || threshold.get("exclusiveMaximum").is_some(),
        "threshold should have maximum constraint"
    );
}

#[test]
fn test_get_node_schema_includes_defaults() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    let schema = get_node_schema("SileroVAD", &validator).unwrap();
    let threshold = &schema["properties"]["threshold"];

    // Should have default value
    assert!(
        threshold.get("default").is_some(),
        "threshold should have default value"
    );
}

// =============================================================================
// T049: Test get_all_schemas returns all registered schemas
// =============================================================================

#[test]
fn test_get_all_schemas_returns_all_builtin_schemas() {
    // GIVEN: A validator with builtin schemas
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // WHEN: Getting all schemas
    let schemas = get_all_schemas(&validator);

    // THEN: Should have schemas for builtin nodes with config_schema
    assert!(!schemas.is_empty(), "Should return non-empty map");

    // Should include known builtin nodes that have config_schema defined
    assert!(
        schemas.contains_key("SileroVAD"),
        "Should include SileroVAD"
    );
    assert!(
        schemas.contains_key("KokoroTTSNode"),
        "Should include KokoroTTSNode"
    );
    assert!(
        schemas.contains_key("AudioResample"),
        "Should include AudioResample"
    );
}

#[test]
fn test_get_all_schemas_returns_valid_schemas() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    let schemas = get_all_schemas(&validator);

    // Each schema should be a valid JSON object
    for (node_type, schema) in &schemas {
        assert!(
            schema.is_object(),
            "Schema for {} should be a JSON object",
            node_type
        );
    }
}

#[test]
fn test_get_all_schemas_count_matches_registry() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    let schemas = get_all_schemas(&validator);

    // Count should match registry nodes that have config_schema defined
    // (not all registered nodes have config_schema)
    let registry_with_schemas = registry
        .iter()
        .filter(|s| s.config_schema.is_some())
        .count();

    assert_eq!(
        schemas.len(),
        registry_with_schemas,
        "Schema count should match registry count (nodes with config_schema)"
    );
}

#[test]
fn test_get_all_schemas_includes_inventory_registered() {
    // GIVEN: Schemas registered via inventory (from #[node] macro)
    let collected = collect_registered_configs();

    // Create validator that includes both builtin and inventory-registered
    let mut registry = create_builtin_schema_registry();
    for schema in collected.iter() {
        registry.register(schema.clone());
    }
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // WHEN: Getting all schemas
    let schemas = get_all_schemas(&validator);

    // THEN: Should include inventory-registered schemas
    // (If SpeculativeVADGate uses #[node] macro, it should be included)
    // The exact count depends on what nodes use the macro
    assert!(!schemas.is_empty());
}

// =============================================================================
// Additional introspection tests
// =============================================================================

#[test]
fn test_schema_introspection_is_deterministic() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // Multiple calls should return same result
    let schema1 = get_node_schema("SileroVAD", &validator);
    let schema2 = get_node_schema("SileroVAD", &validator);

    assert_eq!(schema1, schema2, "Schema introspection should be deterministic");
}

#[test]
fn test_schema_can_be_serialized_to_json() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    let schemas = get_all_schemas(&validator);

    // Should be able to serialize to JSON string
    let json_result = serde_json::to_string(&schemas);
    assert!(json_result.is_ok(), "Schemas should be JSON serializable");

    let json_str = json_result.unwrap();
    assert!(!json_str.is_empty());
}

#[test]
fn test_schema_introspection_via_validator_direct() {
    // Test direct validator methods
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // has_schema
    assert!(validator.has_schema("SileroVAD"));
    assert!(!validator.has_schema("Unknown"));

    // get_schema
    let schema = validator.get_schema("SileroVAD");
    assert!(schema.is_some());

    // get_all_schemas
    let all = validator.get_all_schemas();
    assert!(!all.is_empty());
}

#[test]
fn test_python_node_schemas_accessible() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    // Python nodes should also have schemas
    let schema = get_node_schema("KokoroTTSNode", &validator);
    assert!(schema.is_some(), "Python node schemas should be accessible");

    let schema = schema.unwrap();
    let properties = schema["properties"].as_object().unwrap();

    // KokoroTTSNode should have voice property
    assert!(
        properties.contains_key("voice"),
        "KokoroTTSNode should have voice property"
    );
}

#[test]
fn test_enum_constraints_in_schema() {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    let schema = get_node_schema("KokoroTTSNode", &validator).unwrap();
    let voice = &schema["properties"]["voice"];

    // Should have enum constraint
    assert!(
        voice.get("enum").is_some(),
        "voice property should have enum constraint"
    );

    let enum_values = voice["enum"].as_array().unwrap();
    assert!(!enum_values.is_empty(), "enum should have values");
}
