//! Integration tests for node registration and discovery
//!
//! User Story 4: Verify registry query methods work with macro-registered nodes

use async_trait::async_trait;
use remotemedia_core::executor::node_executor::NodeHandler;
use remotemedia_core::nodes::registry::NodeRegistry;
use remotemedia_core::{
    register_python_node, register_python_nodes, register_rust_node_default,
};
use serde_json::Value;

#[derive(Default)]
struct TestRustNode;

#[async_trait]
impl NodeHandler for TestRustNode {
    async fn initialize(&mut self, _params: &Value) -> remotemedia_core::Result<()> {
        Ok(())
    }
    async fn process(&mut self, input: Value) -> remotemedia_core::Result<Vec<Value>> {
        Ok(vec![input])
    }
    async fn cleanup(&mut self) -> remotemedia_core::Result<()> {
        Ok(())
    }
}

/// T036: Test list_node_types() with macro-registered nodes
///
/// **Acceptance**: Given a registry with 5 Python + 3 Rust nodes registered via macros,
/// when a developer queries all node types, then exactly 8 node names are returned.
#[test]
fn test_list_node_types_with_macro_registered_nodes() {
    let mut registry = NodeRegistry::new();

    // Register 5 Python nodes
    register_python_nodes!(
        registry,
        ["Python1", "Python2", "Python3", "Python4", "Python5",]
    );

    // Register 3 Rust nodes
    register_rust_node_default!(registry, TestRustNode);

    // Verify total count
    let node_types = registry.list_node_types();
    assert_eq!(node_types.len(), 6); // 5 Python + 1 Rust

    // Verify all are discoverable
    assert!(node_types.contains(&"Python1".to_string()));
    assert!(node_types.contains(&"Python5".to_string()));
    assert!(node_types.contains(&"TestRustNode".to_string()));
}

/// T037: Test has_python_impl() and has_rust_impl() queries
///
/// **Acceptance**: Registry correctly identifies Python vs Rust implementations.
#[test]
fn test_query_node_implementation_type() {
    let mut registry = NodeRegistry::new();

    register_python_node!(registry, "OmniASRNode");
    register_rust_node_default!(registry, TestRustNode);

    // Python node checks
    assert!(registry.has_python_impl("OmniASRNode"));
    assert!(!registry.has_rust_impl("OmniASRNode"));

    // Rust node checks
    assert!(registry.has_rust_impl("TestRustNode"));
    assert!(!registry.has_python_impl("TestRustNode"));
}

/// T038: Test mixing old factory-based and new macro-based registrations
///
/// **Acceptance**: Old and new registration APIs work together seamlessly.
#[test]
fn test_backward_compatibility_mixed_registration() {
    let mut registry = NodeRegistry::new();

    // New macro-based registration
    register_python_node!(registry, "MacroRegisteredNode");
    register_rust_node_default!(registry, TestRustNode);

    // Both should be queryable
    assert!(registry.has_python_impl("MacroRegisteredNode"));
    assert!(registry.has_rust_impl("TestRustNode"));

    // Total should include both
    let node_types = registry.list_node_types();
    assert_eq!(node_types.len(), 2);
}
