//! T042: Unit test for #[node] macro config_schema generation
//!
//! Tests that the #[node] macro correctly generates:
//! - Config struct with JsonSchema derive
//! - NodeConfigSchema trait implementation
//! - JSON Schema output with correct properties

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::nodes::schema::NodeConfigSchema;
use remotemedia_runtime_core::Error;

// =============================================================================
// Test Node: Basic schema generation
// =============================================================================

/// Test node with various config field types
#[remotemedia_runtime_core::node(
    node_type = "SchemaTestNode",
    category = "test",
    description = "Node for testing schema generation"
)]
pub struct SchemaTestNode {
    /// A string parameter
    #[config]
    pub name: String,

    /// A numeric parameter
    #[config]
    pub threshold: f64,

    /// A boolean parameter
    #[config]
    pub enabled: bool,

    /// An integer parameter
    #[config]
    pub count: i32,

    #[state]
    internal: u32,
}

impl SchemaTestNode {
    pub async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

#[test]
fn test_macro_generates_config_schema() {
    // Config struct should exist and implement NodeConfigSchema
    let schema = SchemaTestNodeConfig::config_json_schema();

    // Should be a valid JSON object
    assert!(schema.is_object(), "Schema should be a JSON object");
}

#[test]
fn test_schema_has_properties() {
    let schema = SchemaTestNodeConfig::config_json_schema();
    let obj = schema.as_object().unwrap();

    // schemars generates a schema with "properties" key
    assert!(
        obj.contains_key("properties"),
        "Schema should have properties: {:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_schema_includes_all_config_fields() {
    let schema = SchemaTestNodeConfig::config_json_schema();
    let properties = schema["properties"].as_object().unwrap();

    // All #[config] fields should be in properties
    assert!(
        properties.contains_key("name"),
        "Schema should include 'name' property"
    );
    assert!(
        properties.contains_key("threshold"),
        "Schema should include 'threshold' property"
    );
    assert!(
        properties.contains_key("enabled"),
        "Schema should include 'enabled' property"
    );
    assert!(
        properties.contains_key("count"),
        "Schema should include 'count' property"
    );
}

#[test]
fn test_schema_excludes_state_fields() {
    let schema = SchemaTestNodeConfig::config_json_schema();
    let properties = schema["properties"].as_object().unwrap();

    // #[state] fields should NOT be in properties
    assert!(
        !properties.contains_key("internal"),
        "Schema should NOT include state field 'internal'"
    );
}

#[test]
fn test_schema_has_correct_types() {
    let schema = SchemaTestNodeConfig::config_json_schema();
    let properties = schema["properties"].as_object().unwrap();

    // Check type for string field
    let name_type = properties["name"]["type"].as_str().unwrap();
    assert_eq!(name_type, "string", "name should be string type");

    // Check type for numeric field
    let threshold_type = properties["threshold"]["type"].as_str().unwrap();
    assert_eq!(threshold_type, "number", "threshold should be number type");

    // Check type for boolean field
    let enabled_type = properties["enabled"]["type"].as_str().unwrap();
    assert_eq!(enabled_type, "boolean", "enabled should be boolean type");

    // Check type for integer field
    let count_type = properties["count"]["type"].as_str().unwrap();
    assert_eq!(count_type, "integer", "count should be integer type");
}

#[test]
fn test_node_config_schema_metadata() {
    assert_eq!(
        SchemaTestNodeConfig::node_type(),
        "SchemaTestNode",
        "node_type should match macro attribute"
    );
    assert_eq!(
        SchemaTestNodeConfig::category(),
        Some("test".to_string()),
        "category should match macro attribute"
    );
    assert_eq!(
        SchemaTestNodeConfig::description(),
        Some("Node for testing schema generation".to_string()),
        "description should match macro attribute"
    );
}

#[test]
fn test_to_node_schema_includes_config() {
    let node_schema = SchemaTestNodeConfig::to_node_schema();

    assert_eq!(node_schema.node_type, "SchemaTestNode");
    assert!(
        node_schema.config_schema.is_some(),
        "NodeSchema should include config_schema"
    );

    let config_schema = node_schema.config_schema.unwrap();
    assert!(config_schema.is_object());
}

// =============================================================================
// Test Node: With default values
// =============================================================================

/// Node with custom defaults in config
#[remotemedia_runtime_core::node(
    node_type = "DefaultsTestNode",
    category = "test"
)]
pub struct DefaultsTestNode {
    #[config(default = 0.5)]
    pub threshold: f64,

    #[config(default = "default_name".to_string())]
    pub name: String,

    #[config(default = true)]
    pub enabled: bool,

    #[state]
    counter: u32,
}

impl DefaultsTestNode {
    pub async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

#[test]
fn test_schema_with_defaults() {
    // Config should use custom defaults
    let config = DefaultsTestNodeConfig::default();
    assert!((config.threshold - 0.5).abs() < f64::EPSILON);
    assert_eq!(config.name, "default_name");
    assert!(config.enabled);
}

#[test]
fn test_default_config_json() {
    let default_json = DefaultsTestNodeConfig::default_config();

    // Should return Some with default values
    assert!(default_json.is_some());

    let json = default_json.unwrap();
    assert!(json.is_object());

    // Check default values are present
    assert!((json["threshold"].as_f64().unwrap() - 0.5).abs() < f64::EPSILON);
}

// =============================================================================
// Test Node: Optional fields
// =============================================================================

/// Node with Option<T> fields
#[remotemedia_runtime_core::node(
    node_type = "OptionalFieldsNode",
    category = "test"
)]
pub struct OptionalFieldsNode {
    #[config]
    pub required_name: String,

    #[config]
    pub optional_value: Option<i32>,

    #[config]
    pub optional_string: Option<String>,

    #[state]
    counter: u32,
}

impl OptionalFieldsNode {
    pub async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

#[test]
fn test_optional_fields_in_schema() {
    let schema = OptionalFieldsNodeConfig::config_json_schema();
    let properties = schema["properties"].as_object().unwrap();

    // All fields should be present
    assert!(properties.contains_key("requiredName") || properties.contains_key("required_name"));
    assert!(properties.contains_key("optionalValue") || properties.contains_key("optional_value"));
    assert!(
        properties.contains_key("optionalString") || properties.contains_key("optional_string")
    );
}

#[test]
fn test_optional_fields_can_be_null() {
    // Option<T> fields should allow None
    let config = OptionalFieldsNodeConfig {
        required_name: "test".to_string(),
        optional_value: None,
        optional_string: None,
    };

    // Should serialize without the optional fields or with null
    let json = serde_json::to_value(&config).unwrap();
    assert!(json.is_object());
}

// =============================================================================
// Test: Schema collection via inventory
// =============================================================================

#[test]
fn test_schemas_registered_via_inventory() {
    use remotemedia_runtime_core::nodes::schema::collect_registered_configs;

    let schemas = collect_registered_configs();

    // Should find our test nodes
    let found_schema_test = schemas.iter().any(|s| s.node_type == "SchemaTestNode");
    let found_defaults = schemas.iter().any(|s| s.node_type == "DefaultsTestNode");

    assert!(
        found_schema_test,
        "SchemaTestNode should be registered via inventory"
    );
    assert!(
        found_defaults,
        "DefaultsTestNode should be registered via inventory"
    );
}
