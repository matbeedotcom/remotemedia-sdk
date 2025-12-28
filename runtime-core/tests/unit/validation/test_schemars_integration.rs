//! T043: Unit test for schemars range attributes in validation
//!
//! Tests that schemars validation attributes are:
//! 1. Correctly reflected in generated JSON Schema
//! 2. Properly validated by SchemaValidator

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::nodes::schema::NodeConfigSchema;
use remotemedia_runtime_core::validation::{SchemaValidator, ValidationConstraint};
use remotemedia_runtime_core::Error;
use serde_json::json;

// =============================================================================
// Test Node: With schemars range constraints
// =============================================================================

/// Node with schemars range validation attributes
#[remotemedia_runtime_core::node(
    node_type = "RangeValidatedNode",
    category = "test",
    description = "Node with range-validated parameters"
)]
pub struct RangeValidatedNode {
    /// Threshold must be between 0.0 and 1.0
    #[config(default = 0.5)]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub threshold: f64,

    /// Sample rate must be at least 8000
    #[config(default = 16000)]
    #[schemars(range(min = 8000))]
    pub sample_rate: u32,

    /// Max items must be at most 100
    #[config(default = 10)]
    #[schemars(range(max = 100))]
    pub max_items: u32,

    #[state]
    counter: u32,
}

impl RangeValidatedNode {
    pub async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

#[test]
fn test_range_constraints_in_schema() {
    let schema = RangeValidatedNodeConfig::config_json_schema();
    let properties = schema["properties"].as_object().unwrap();

    // Check threshold has minimum and maximum
    let threshold = &properties["threshold"];
    assert!(
        threshold.get("minimum").is_some() || threshold.get("exclusiveMinimum").is_some(),
        "threshold should have minimum constraint: {:?}",
        threshold
    );
    assert!(
        threshold.get("maximum").is_some() || threshold.get("exclusiveMaximum").is_some(),
        "threshold should have maximum constraint: {:?}",
        threshold
    );
}

#[test]
fn test_minimum_constraint_in_schema() {
    let schema = RangeValidatedNodeConfig::config_json_schema();
    let properties = schema["properties"].as_object().unwrap();

    // Check sample_rate has minimum
    let sample_rate = &properties["sampleRate"];
    let has_min = sample_rate.get("minimum").is_some()
        || sample_rate.get("exclusiveMinimum").is_some();
    assert!(
        has_min,
        "sample_rate should have minimum constraint: {:?}",
        sample_rate
    );
}

#[test]
fn test_maximum_constraint_in_schema() {
    let schema = RangeValidatedNodeConfig::config_json_schema();
    let properties = schema["properties"].as_object().unwrap();

    // Check max_items has maximum
    let max_items = &properties["maxItems"];
    let has_max =
        max_items.get("maximum").is_some() || max_items.get("exclusiveMaximum").is_some();
    assert!(
        has_max,
        "max_items should have maximum constraint: {:?}",
        max_items
    );
}

#[test]
fn test_schema_validates_range_violation() {
    // Create validator with the node's schema
    let mut validator = SchemaValidator::empty();
    let schema = RangeValidatedNodeConfig::config_json_schema();
    validator.add_schema("RangeValidatedNode", schema).unwrap();

    // Test threshold above maximum (1.5 > 1.0)
    let invalid_params = json!({
        "threshold": 1.5,
        "sampleRate": 16000,
        "maxItems": 10
    });

    let result = validator.validate_node("test", "RangeValidatedNode", &invalid_params);
    assert!(result.is_err(), "Should reject threshold above maximum");

    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.constraint == ValidationConstraint::Maximum
                || e.constraint == ValidationConstraint::ExclusiveMaximum),
        "Should have Maximum violation"
    );
}

#[test]
fn test_schema_validates_minimum_violation() {
    let mut validator = SchemaValidator::empty();
    let schema = RangeValidatedNodeConfig::config_json_schema();
    validator.add_schema("RangeValidatedNode", schema).unwrap();

    // Test threshold below minimum (-0.5 < 0.0)
    let invalid_params = json!({
        "threshold": -0.5,
        "sampleRate": 16000,
        "maxItems": 10
    });

    let result = validator.validate_node("test", "RangeValidatedNode", &invalid_params);
    assert!(result.is_err(), "Should reject threshold below minimum");
}

#[test]
fn test_schema_validates_sample_rate_minimum() {
    let mut validator = SchemaValidator::empty();
    let schema = RangeValidatedNodeConfig::config_json_schema();
    validator.add_schema("RangeValidatedNode", schema).unwrap();

    // Test sample_rate below minimum (4000 < 8000)
    let invalid_params = json!({
        "threshold": 0.5,
        "sampleRate": 4000,
        "maxItems": 10
    });

    let result = validator.validate_node("test", "RangeValidatedNode", &invalid_params);
    assert!(result.is_err(), "Should reject sample_rate below minimum");
}

#[test]
fn test_schema_validates_max_items_maximum() {
    let mut validator = SchemaValidator::empty();
    let schema = RangeValidatedNodeConfig::config_json_schema();
    validator.add_schema("RangeValidatedNode", schema).unwrap();

    // Test max_items above maximum (150 > 100)
    let invalid_params = json!({
        "threshold": 0.5,
        "sampleRate": 16000,
        "maxItems": 150
    });

    let result = validator.validate_node("test", "RangeValidatedNode", &invalid_params);
    assert!(result.is_err(), "Should reject max_items above maximum");
}

#[test]
fn test_valid_params_within_range() {
    let mut validator = SchemaValidator::empty();
    let schema = RangeValidatedNodeConfig::config_json_schema();
    validator.add_schema("RangeValidatedNode", schema).unwrap();

    // All values within valid range
    let valid_params = json!({
        "threshold": 0.5,
        "sampleRate": 16000,
        "maxItems": 50
    });

    let result = validator.validate_node("test", "RangeValidatedNode", &valid_params);
    assert!(result.is_ok(), "Valid params should pass validation");
}

#[test]
fn test_boundary_values_valid() {
    let mut validator = SchemaValidator::empty();
    let schema = RangeValidatedNodeConfig::config_json_schema();
    validator.add_schema("RangeValidatedNode", schema).unwrap();

    // Values at exact boundaries (inclusive)
    let boundary_params = json!({
        "threshold": 0.0,  // At minimum
        "sampleRate": 8000,  // At minimum
        "maxItems": 100  // At maximum
    });

    let result = validator.validate_node("test", "RangeValidatedNode", &boundary_params);
    assert!(result.is_ok(), "Boundary values should be valid");
}

// =============================================================================
// Test Node: With string length constraints
// =============================================================================

/// Node with string length validation
#[remotemedia_runtime_core::node(
    node_type = "StringValidatedNode",
    category = "test"
)]
pub struct StringValidatedNode {
    /// Name must be 1-100 characters
    #[config]
    #[schemars(length(min = 1, max = 100))]
    pub name: String,

    /// Code must be exactly 6 characters (pattern would be better but using length)
    #[config]
    #[schemars(length(min = 6, max = 6))]
    pub code: String,

    #[state]
    counter: u32,
}

impl StringValidatedNode {
    pub async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

#[test]
fn test_string_length_in_schema() {
    let schema = StringValidatedNodeConfig::config_json_schema();
    let properties = schema["properties"].as_object().unwrap();

    let name = &properties["name"];
    // Should have minLength/maxLength
    assert!(
        name.get("minLength").is_some() || name.get("maxLength").is_some(),
        "name should have length constraints: {:?}",
        name
    );
}

#[test]
fn test_string_length_validation() {
    let mut validator = SchemaValidator::empty();
    let schema = StringValidatedNodeConfig::config_json_schema();
    validator.add_schema("StringValidatedNode", schema).unwrap();

    // Empty name (too short)
    let invalid_params = json!({
        "name": "",
        "code": "ABC123"
    });

    let result = validator.validate_node("test", "StringValidatedNode", &invalid_params);
    assert!(result.is_err(), "Should reject empty name");

    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.constraint == ValidationConstraint::MinLength),
        "Should have MinLength violation"
    );
}

// =============================================================================
// Test: Schema integration with PipelineRunner validation
// =============================================================================

#[test]
fn test_node_schema_can_be_added_to_validator() {
    let mut validator = SchemaValidator::empty();

    // Get schema from the node's config
    let node_schema = RangeValidatedNodeConfig::to_node_schema();

    // Should be able to add to validator
    if let Some(config_schema) = node_schema.config_schema {
        let result = validator.add_schema(&node_schema.node_type, config_schema);
        assert!(result.is_ok(), "Should be able to add node schema to validator");
    }
}

#[test]
fn test_collected_schemas_are_valid() {
    use remotemedia_runtime_core::nodes::schema::collect_registered_configs;

    let registry = collect_registered_configs();

    for schema in registry.iter() {
        if let Some(ref config_schema) = schema.config_schema {
            // Each schema should be valid JSON
            assert!(
                config_schema.is_object(),
                "Schema for {} should be a JSON object",
                schema.node_type
            );

            // Should have type: "object" or properties
            let obj = config_schema.as_object().unwrap();
            let has_structure =
                obj.contains_key("properties") || obj.contains_key("type") || obj.is_empty();
            assert!(
                has_structure,
                "Schema for {} should have valid structure: {:?}",
                schema.node_type,
                obj.keys().collect::<Vec<_>>()
            );
        }
    }
}
