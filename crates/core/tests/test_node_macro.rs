//! Tests for the #[node] unified macro
//!
//! These tests verify that the macro correctly generates:
//! - Config struct with correct derives and serde attributes
//! - Node struct with config field and state fields
//! - AsyncStreamingNode trait implementation
//! - NodeConfigSchema implementation
//! - Inventory registration

use remotemedia_core::data::RuntimeData;
use remotemedia_core::Error;

// =============================================================================
// Test: Basic node with config and state fields (US1)
// =============================================================================

/// Test node demonstrating the unified macro
#[remotemedia_core::node(
    node_type = "TestEcho",
    category = "utility",
    description = "Test node for macro verification",
    accepts = "text",
    produces = "text"
)]
pub struct TestEchoNode {
    /// Prefix to add to echoed text
    #[config]
    pub prefix: String,

    /// Whether to uppercase the text
    #[config]
    pub uppercase: bool,

    /// Call counter (internal state)
    #[state]
    call_count: u64,
}

impl TestEchoNode {
    /// Process implementation - delegates from AsyncStreamingNode::process
    pub async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        match data {
            RuntimeData::Text(text) => {
                let result = if self.config.uppercase {
                    format!("{}{}", self.config.prefix, text.to_uppercase())
                } else {
                    format!("{}{}", self.config.prefix, text)
                };
                Ok(RuntimeData::Text(result))
            }
            _ => Ok(data),
        }
    }
}

// =============================================================================
// Test: Config struct generation (US1)
// =============================================================================

#[test]
fn test_config_struct_exists() {
    // Config struct should exist and be constructible
    let config = TestEchoNodeConfig {
        prefix: "Hello: ".to_string(),
        uppercase: false,
    };
    assert_eq!(config.prefix, "Hello: ");
    assert!(!config.uppercase);
}

#[test]
fn test_config_struct_default() {
    // Config should implement Default
    let config = TestEchoNodeConfig::default();
    // Default values should be from Default::default() for each type
    assert_eq!(config.prefix, "");
    assert!(!config.uppercase);
}

#[test]
fn test_config_serialization() {
    use serde_json;

    let config = TestEchoNodeConfig {
        prefix: "Test: ".to_string(),
        uppercase: true,
    };

    // Should serialize with camelCase keys
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"prefix\""));
    assert!(json.contains("\"uppercase\""));

    // Should deserialize from camelCase
    let json_input = r#"{"prefix": "Deser: ", "uppercase": false}"#;
    let deserialized: TestEchoNodeConfig = serde_json::from_str(json_input).unwrap();
    assert_eq!(deserialized.prefix, "Deser: ");
    assert!(!deserialized.uppercase);
}

// =============================================================================
// Test: Node struct generation (US1)
// =============================================================================

#[test]
fn test_node_struct_new() {
    let config = TestEchoNodeConfig {
        prefix: "New: ".to_string(),
        uppercase: false,
    };
    let node = TestEchoNode::new(config);
    assert_eq!(node.config.prefix, "New: ");
}

#[test]
fn test_node_struct_with_default() {
    let node = TestEchoNode::with_default();
    // State fields should be default-initialized
    assert_eq!(node.call_count, 0);
}

// =============================================================================
// Test: AsyncStreamingNode trait (US2)
// =============================================================================

#[test]
fn test_node_type() {
    use remotemedia_core::nodes::AsyncStreamingNode;

    let node = TestEchoNode::with_default();
    assert_eq!(node.node_type(), "TestEcho");
}

#[tokio::test]
async fn test_process() {
    use remotemedia_core::nodes::AsyncStreamingNode;

    let config = TestEchoNodeConfig {
        prefix: "Echo: ".to_string(),
        uppercase: false,
    };
    let node = TestEchoNode::new(config);

    let input = RuntimeData::Text("hello".to_string());
    let output = node.process(input).await.unwrap();

    match output {
        RuntimeData::Text(text) => assert_eq!(text, "Echo: hello"),
        _ => panic!("Expected Text output"),
    }
}

#[tokio::test]
async fn test_process_uppercase() {
    use remotemedia_core::nodes::AsyncStreamingNode;

    let config = TestEchoNodeConfig {
        prefix: "".to_string(),
        uppercase: true,
    };
    let node = TestEchoNode::new(config);

    let input = RuntimeData::Text("hello".to_string());
    let output = node.process(input).await.unwrap();

    match output {
        RuntimeData::Text(text) => assert_eq!(text, "HELLO"),
        _ => panic!("Expected Text output"),
    }
}

// =============================================================================
// Test: NodeConfigSchema implementation (US3)
// =============================================================================

#[test]
fn test_node_config_schema() {
    use remotemedia_core::nodes::schema::NodeConfigSchema;

    assert_eq!(TestEchoNodeConfig::node_type(), "TestEcho");
    assert_eq!(TestEchoNodeConfig::category(), Some("utility".to_string()));
    assert_eq!(
        TestEchoNodeConfig::description(),
        Some("Test node for macro verification".to_string())
    );
}

#[test]
fn test_node_config_json_schema() {
    use remotemedia_core::nodes::schema::NodeConfigSchema;

    let schema = TestEchoNodeConfig::config_json_schema();
    // Schema should be a valid JSON object with properties
    assert!(schema.is_object());
    let obj = schema.as_object().unwrap();
    // schemars generates a "$schema", "title", "type", and "properties" keys
    assert!(obj.contains_key("properties") || obj.contains_key("$schema"));
}

// =============================================================================
// Test: Node with custom defaults (US1)
// =============================================================================

#[remotemedia_core::node(
    node_type = "CustomDefault",
    category = "test"
)]
pub struct CustomDefaultNode {
    /// Threshold with default
    #[config(default = 0.5)]
    pub threshold: f64,

    /// Counter with default
    #[config(default = 100)]
    pub max_count: u32,

    #[state]
    current_count: u32,
}

impl CustomDefaultNode {
    pub async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

#[test]
fn test_custom_defaults() {
    let config = CustomDefaultNodeConfig::default();
    assert!((config.threshold - 0.5).abs() < f64::EPSILON);
    assert_eq!(config.max_count, 100);
}

// =============================================================================
// Test: Node type inference (US2)
// =============================================================================

/// Node without explicit node_type - should infer from struct name
#[remotemedia_core::node(
    category = "test"
)]
pub struct InferredTypeNode {
    #[config]
    pub value: i32,

    #[state]
    internal: bool,
}

impl InferredTypeNode {
    pub async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

#[test]
fn test_inferred_node_type() {
    use remotemedia_core::nodes::AsyncStreamingNode;

    let node = InferredTypeNode::with_default();
    // Should strip "Node" suffix from struct name
    assert_eq!(node.node_type(), "InferredType");
}

// =============================================================================
// Test: Inventory registration (US3/US4)
// =============================================================================

#[test]
fn test_inventory_registration() {
    use remotemedia_core::nodes::schema::collect_registered_configs;

    let configs = collect_registered_configs();
    // Should find at least one of our test nodes
    let found = configs
        .iter()
        .any(|schema| schema.node_type == "TestEcho" || schema.node_type == "CustomDefault");
    assert!(found, "Test nodes should be registered via inventory");
}
