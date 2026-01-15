//! Unit tests for registration macros
//!
//! These tests verify the behavior of the ergonomic node registration macros.
//! Following TDD approach: tests written first, verified to fail, then implementation.

use async_trait::async_trait;
use remotemedia_core::executor::node_executor::NodeHandler;
use remotemedia_core::nodes::registry::NodeRegistry;
use remotemedia_core::Result;
use remotemedia_core::{
    register_python_node, register_python_nodes, register_rust_node, register_rust_node_default,
};
use serde_json::Value;

#[cfg(test)]
mod register_python_node_tests {
    use super::*;

    /// T009: Test register_python_node! with single node
    ///
    /// **Acceptance**: Given an empty registry, when a developer registers
    /// a Python node named "OmniASRNode", then the registry contains exactly
    /// one node and has_python_impl("OmniASRNode") returns true.
    #[test]
    fn test_register_single_python_node() {
        let mut registry = NodeRegistry::new();

        // Register single Python node using macro
        register_python_node!(registry, "OmniASRNode");

        // Verify registration
        assert!(registry.has_python_impl("OmniASRNode"));
        assert_eq!(registry.list_node_types().len(), 1);
        assert!(registry
            .list_node_types()
            .contains(&"OmniASRNode".to_string()));
    }

    /// T010: Test register_python_node! with multiple sequential registrations
    ///
    /// **Acceptance**: Given a registry with 3 existing nodes, when a developer
    /// registers a new Python node "KokoroTTSNode", then the registry contains
    /// 4 nodes and the new node can be instantiated by its name.
    #[test]
    fn test_register_multiple_python_nodes_sequentially() {
        let mut registry = NodeRegistry::new();

        // Register multiple nodes sequentially
        register_python_node!(registry, "Node1");
        register_python_node!(registry, "Node2");
        register_python_node!(registry, "Node3");
        register_python_node!(registry, "KokoroTTSNode");

        // Verify all registered
        assert_eq!(registry.list_node_types().len(), 4);
        assert!(registry.has_python_impl("Node1"));
        assert!(registry.has_python_impl("Node2"));
        assert!(registry.has_python_impl("Node3"));
        assert!(registry.has_python_impl("KokoroTTSNode"));
    }

    /// T011: Test verifying node is queryable after registration
    ///
    /// **Acceptance**: After registration, the node should be discoverable
    /// via registry query methods and identified as a Python implementation.
    #[test]
    fn test_python_node_is_queryable_after_registration() {
        let mut registry = NodeRegistry::new();

        register_python_node!(registry, "TestPythonNode");

        // Verify node is queryable
        assert!(registry.has_python_impl("TestPythonNode"));
        assert!(!registry.has_rust_impl("TestPythonNode")); // Should be Python, not Rust

        // Verify it appears in list
        let node_types = registry.list_node_types();
        assert!(node_types.contains(&"TestPythonNode".to_string()));
    }
}

#[cfg(test)]
mod register_python_nodes_tests {
    use super::*;

    /// T017: Test register_python_nodes! with array of 5 nodes
    ///
    /// **Acceptance**: Given an empty registry, when a developer registers
    /// 5 Python nodes in one batch operation, then all 5 are registered.
    #[test]
    fn test_batch_register_five_nodes() {
        let mut registry = NodeRegistry::new();

        register_python_nodes!(registry, ["ASR", "TTS", "Resample", "VAD", "Chunker"]);

        assert_eq!(registry.list_node_types().len(), 5);
        assert!(registry.has_python_impl("ASR"));
        assert!(registry.has_python_impl("TTS"));
        assert!(registry.has_python_impl("Resample"));
        assert!(registry.has_python_impl("VAD"));
        assert!(registry.has_python_impl("Chunker"));
    }

    /// T018: Test register_python_nodes! verifying all nodes are queryable
    ///
    /// **Acceptance**: All batch-registered nodes should be queryable
    /// via registry methods.
    #[test]
    fn test_batch_registered_nodes_are_queryable() {
        let mut registry = NodeRegistry::new();

        register_python_nodes!(registry, ["Node1", "Node2", "Node3"]);

        // Verify all nodes queryable
        for node_name in ["Node1", "Node2", "Node3"] {
            assert!(registry.has_python_impl(node_name));
            assert!(!registry.has_rust_impl(node_name));
        }

        let node_types = registry.list_node_types();
        assert_eq!(node_types.len(), 3);
    }

    /// T019: Test register_python_nodes! with trailing comma syntax
    ///
    /// **Acceptance**: Trailing comma should be accepted and not cause errors.
    #[test]
    fn test_batch_register_with_trailing_comma() {
        let mut registry = NodeRegistry::new();

        // Trailing comma is valid Rust syntax
        register_python_nodes!(registry, ["Node1", "Node2",]);

        assert_eq!(registry.list_node_types().len(), 2);
    }

    /// T020: Test register_python_nodes! with duplicate names
    ///
    /// **Acceptance**: When duplicate names exist in batch, the last
    /// registration should win (registry handles overwrite).
    #[test]
    fn test_batch_register_with_duplicates() {
        let mut registry = NodeRegistry::new();

        // Register with duplicates
        register_python_nodes!(
            registry,
            [
                "Node1", "Node2", "Node1", // Duplicate
                "Node3",
            ]
        );

        // Should have 3 unique nodes (not 4)
        assert_eq!(registry.list_node_types().len(), 3);
        assert!(registry.has_python_impl("Node1"));
        assert!(registry.has_python_impl("Node2"));
        assert!(registry.has_python_impl("Node3"));
    }
}

#[cfg(test)]
mod register_rust_node_tests {
    use super::*;

    // Test node implementations

    #[derive(Default)]
    struct TestDefaultNode;

    #[async_trait]
    impl NodeHandler for TestDefaultNode {
        async fn initialize(&mut self, _params: &Value) -> Result<()> {
            Ok(())
        }
        async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
            Ok(vec![input])
        }
        async fn cleanup(&mut self) -> Result<()> {
            Ok(())
        }
    }

    struct TestCustomNode {
        sample_rate: u32,
    }

    impl TestCustomNode {
        fn new(sample_rate: u32) -> Result<Self> {
            Ok(Self { sample_rate })
        }
    }

    #[async_trait]
    impl NodeHandler for TestCustomNode {
        async fn initialize(&mut self, _params: &Value) -> Result<()> {
            Ok(())
        }
        async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
            Ok(vec![input])
        }
        async fn cleanup(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// T026: Test register_rust_node_default! with Default trait
    ///
    /// **Acceptance**: Given a Rust node type "AudioChunkerNode", when a developer
    /// registers it with default initialization, then the node can be instantiated.
    #[test]
    fn test_register_rust_node_with_default() {
        let mut registry = NodeRegistry::new();

        register_rust_node_default!(registry, TestDefaultNode);

        assert!(registry.has_rust_impl("TestDefaultNode"));
        assert_eq!(registry.list_node_types().len(), 1);
    }

    /// T027: Test register_rust_node! with custom closure
    ///
    /// **Acceptance**: Rust node can be registered with custom initialization closure.
    #[test]
    fn test_register_rust_node_with_custom_closure() {
        let mut registry = NodeRegistry::new();

        register_rust_node!(registry, TestCustomNode, |_params: Value| {
            TestCustomNode::new(44100)
        });

        assert!(registry.has_rust_impl("TestCustomNode"));
    }

    /// T028: Test register_rust_node! with parameter extraction
    ///
    /// **Acceptance**: Given a Rust node type "ResampleNode" that requires sample rate,
    /// when registered with a closure that reads `sample_rate` from params, then the
    /// node is instantiated with the correct value.
    #[test]
    fn test_register_rust_node_with_param_extraction() {
        let mut registry = NodeRegistry::new();

        register_rust_node!(registry, TestCustomNode, |params: Value| {
            let sample_rate = params
                .get("sample_rate")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| {
                    remotemedia_core::Error::Execution("Missing sample_rate".to_string())
                })? as u32;

            TestCustomNode::new(sample_rate)
        });

        assert!(registry.has_rust_impl("TestCustomNode"));

        // Verify we can create with params
        let node_result = registry.create_node(
            "TestCustomNode",
            remotemedia_core::nodes::registry::RuntimeHint::Rust,
            serde_json::json!({"sample_rate": 48000}),
        );

        assert!(node_result.is_ok());
    }

    /// T029: Test register_rust_node! error propagation
    ///
    /// **Acceptance**: When factory closure returns Err, the error should propagate
    /// to the caller instead of crashing the runtime.
    #[test]
    fn test_register_rust_node_error_propagation() {
        let mut registry = NodeRegistry::new();

        register_rust_node!(registry, TestCustomNode, |params: Value| {
            let sample_rate = params
                .get("sample_rate")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| {
                    remotemedia_core::Error::Execution(
                        "Missing required parameter: sample_rate".to_string(),
                    )
                })? as u32;

            TestCustomNode::new(sample_rate)
        });

        // Try to create without required parameter - should fail gracefully
        let node_result = registry.create_node(
            "TestCustomNode",
            remotemedia_core::nodes::registry::RuntimeHint::Rust,
            serde_json::json!({}), // Missing sample_rate
        );

        assert!(node_result.is_err());
    }
}
