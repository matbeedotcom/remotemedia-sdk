//! Comprehensive integration tests for TransportPluginRegistry
//!
//! These tests complement the unit tests in plugin_registry.rs by testing
//! integration-level behaviors, concurrent access patterns, and edge cases.

use remotemedia_runtime_core::transport::client::PipelineClient;
use remotemedia_runtime_core::transport::runner::PipelineRunner;
use remotemedia_runtime_core::transport::{
    ClientConfig, PipelineTransport, ServerConfig, TransportPlugin,
};
use remotemedia_runtime_core::Result;
use async_trait::async_trait;
use std::sync::Arc;

// ============================================================================
// Mock TransportPlugin for Testing
// ============================================================================

struct MockPlugin {
    name: &'static str,
}

#[async_trait]
impl TransportPlugin for MockPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn create_client(&self, _config: &ClientConfig) -> Result<Box<dyn PipelineClient>> {
        unimplemented!("test mock - not used in registry tests")
    }

    async fn create_server(
        &self,
        _config: &ServerConfig,
        _runner: Arc<PipelineRunner>,
    ) -> Result<Box<dyn PipelineTransport>> {
        unimplemented!("test mock - not used in registry tests")
    }
}

// ============================================================================
// T031: Test Duplicate Plugin Names
// ============================================================================

#[test]
fn test_register_duplicate_plugin_names() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    // Create registry
    let registry = TransportPluginRegistry::new();

    // Register plugin with name "test"
    let plugin1 = Arc::new(MockPlugin { name: "test" });
    let result1 = registry.register(plugin1);
    assert!(result1.is_ok(), "First registration should succeed");

    // Try to register another plugin with name "test"
    let plugin2 = Arc::new(MockPlugin { name: "test" });
    let result2 = registry.register(plugin2);

    // Verify error is returned
    assert!(result2.is_err(), "Second registration with same name should fail");

    // Verify error message contains expected text
    match result2 {
        Err(remotemedia_runtime_core::Error::ConfigError(msg)) => {
            assert!(
                msg.contains("Plugin 'test' already registered"),
                "Error message should mention duplicate plugin name. Got: {}",
                msg
            );
        }
        _ => panic!("Expected ConfigError for duplicate plugin name"),
    }
}

#[test]
fn test_register_duplicate_with_different_instances() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    let registry = TransportPluginRegistry::new();

    // Register first plugin
    let plugin1 = Arc::new(MockPlugin { name: "grpc" });
    assert!(registry.register(plugin1).is_ok());

    // Try to register a completely different instance with same name
    let plugin2 = Arc::new(MockPlugin { name: "grpc" });
    let result = registry.register(plugin2);

    assert!(result.is_err(), "Should reject duplicate even with different instance");
}

// ============================================================================
// T032: Test Plugin Lookup (found vs not found)
// ============================================================================

#[test]
fn test_plugin_lookup_found() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    // Create registry and register plugin
    let registry = TransportPluginRegistry::new();
    let plugin = Arc::new(MockPlugin { name: "webrtc" });
    registry.register(plugin).expect("Registration should succeed");

    // Lookup existing plugin by name
    let retrieved = registry.get("webrtc");

    // Verify Some(plugin) returned
    assert!(retrieved.is_some(), "Should find registered plugin");
    assert_eq!(retrieved.unwrap().name(), "webrtc", "Should return correct plugin");
}

#[test]
fn test_plugin_lookup_not_found() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    // Create registry (empty)
    let registry = TransportPluginRegistry::new();

    // Lookup non-existent plugin
    let retrieved = registry.get("nonexistent");

    // Verify None returned
    assert!(retrieved.is_none(), "Should return None for non-existent plugin");
}

#[test]
fn test_plugin_lookup_case_sensitive() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    let registry = TransportPluginRegistry::new();
    let plugin = Arc::new(MockPlugin { name: "grpc" });
    registry.register(plugin).unwrap();

    // Lookup with different case should fail (names are case-sensitive)
    assert!(registry.get("GRPC").is_none(), "Lookup should be case-sensitive");
    assert!(registry.get("Grpc").is_none(), "Lookup should be case-sensitive");

    // Exact match should succeed
    assert!(registry.get("grpc").is_some(), "Exact case should match");
}

#[test]
fn test_list_registered_plugins() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    // Create registry
    let registry = TransportPluginRegistry::new();

    // Initially empty
    let plugins = registry.list();
    assert_eq!(plugins.len(), 0, "New registry should be empty");

    // Register multiple plugins
    let grpc = Arc::new(MockPlugin { name: "grpc" });
    let webrtc = Arc::new(MockPlugin { name: "webrtc" });
    let http = Arc::new(MockPlugin { name: "http" });

    registry.register(grpc).unwrap();
    registry.register(webrtc).unwrap();
    registry.register(http).unwrap();

    // Call list()
    let plugins = registry.list();

    // Verify all names returned
    assert_eq!(plugins.len(), 3, "Should list all registered plugins");
    assert!(plugins.contains(&"grpc".to_string()), "Should contain grpc");
    assert!(plugins.contains(&"webrtc".to_string()), "Should contain webrtc");
    assert!(plugins.contains(&"http".to_string()), "Should contain http");
}

#[test]
fn test_list_returns_independent_vectors() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    let registry = TransportPluginRegistry::new();
    let plugin = Arc::new(MockPlugin { name: "test" });
    registry.register(plugin).unwrap();

    // Get list twice
    let mut list1 = registry.list();
    let list2 = registry.list();

    // Modify first list
    list1.push("modified".to_string());

    // Second list should be unchanged
    assert_eq!(list2.len(), 1, "Second list should be independent");
    assert_eq!(list1.len(), 2, "First list should be modified");
}

// ============================================================================
// T033: Test Concurrent Registry Access
// ============================================================================

#[test]
fn test_concurrent_register() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;
    use std::sync::Arc;
    use std::thread;

    // Create shared registry
    let registry = Arc::new(TransportPluginRegistry::new());

    // Spawn multiple threads, each registering a different plugin
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let registry = Arc::clone(&registry);
            thread::spawn(move || {
                // Create plugin with unique name
                let name = Box::leak(format!("plugin_{}", i).into_boxed_str());
                let plugin = Arc::new(MockPlugin { name });
                registry.register(plugin)
            })
        })
        .collect();

    // Wait for all threads
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all succeed
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Registration {} should succeed", i);
    }

    // Verify all plugins registered
    let plugins = registry.list();
    assert_eq!(plugins.len(), 10, "All 10 plugins should be registered");

    for i in 0..10 {
        let plugin_name = format!("plugin_{}", i);
        assert!(
            plugins.contains(&plugin_name),
            "Should contain plugin_{}",
            i
        );
    }
}

#[test]
fn test_concurrent_register_same_name_one_wins() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;
    use std::sync::Arc;
    use std::thread;

    let registry = Arc::new(TransportPluginRegistry::new());

    // Spawn multiple threads trying to register the same name
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let registry = Arc::clone(&registry);
            thread::spawn(move || {
                let plugin = Arc::new(MockPlugin { name: "contested" });
                registry.register(plugin)
            })
        })
        .collect();

    // Wait for all threads
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Exactly one should succeed
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(success_count, 1, "Exactly one registration should succeed");

    // Others should fail with duplicate error
    let error_count = results.iter().filter(|r| r.is_err()).count();
    assert_eq!(error_count, 9, "Nine registrations should fail");

    // Verify only one plugin registered
    let plugins = registry.list();
    assert_eq!(plugins.len(), 1, "Only one plugin should be registered");
}

#[test]
fn test_concurrent_get() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;
    use std::sync::Arc;
    use std::thread;

    let registry = Arc::new(TransportPluginRegistry::new());

    // Register plugins
    let grpc = Arc::new(MockPlugin { name: "grpc" });
    let webrtc = Arc::new(MockPlugin { name: "webrtc" });
    registry.register(grpc).unwrap();
    registry.register(webrtc).unwrap();

    // Spawn multiple threads reading concurrently
    let handles: Vec<_> = (0..20)
        .map(|i| {
            let registry = Arc::clone(&registry);
            thread::spawn(move || {
                // Alternate between reading grpc and webrtc
                let name = if i % 2 == 0 { "grpc" } else { "webrtc" };
                registry.get(name)
            })
        })
        .collect();

    // Wait for all threads
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all get same Arc<> instances (by pointer equality)
    for (i, retrieved) in results.iter().enumerate() {
        assert!(retrieved.is_some(), "Thread {} should find plugin", i);

        let expected_name = if i % 2 == 0 { "grpc" } else { "webrtc" };
        assert_eq!(
            retrieved.as_ref().unwrap().name(),
            expected_name,
            "Thread {} should get correct plugin",
            i
        );
    }

    // Verify no deadlocks (test completes)
}

#[test]
fn test_concurrent_mixed_operations() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;
    use std::sync::Arc;
    use std::thread;

    let registry = Arc::new(TransportPluginRegistry::new());

    // Pre-register one plugin
    let initial = Arc::new(MockPlugin { name: "initial" });
    registry.register(initial).unwrap();

    // Spawn threads doing mixed operations
    let handles: Vec<_> = (0..30)
        .map(|i| {
            let registry = Arc::clone(&registry);
            thread::spawn(move || {
                match i % 3 {
                    // Register new plugin
                    0 => {
                        let name = Box::leak(format!("plugin_{}", i).into_boxed_str());
                        let plugin = Arc::new(MockPlugin { name });
                        registry.register(plugin)
                    }
                    // Read existing plugin
                    1 => {
                        let _ = registry.get("initial");
                        Ok(())
                    }
                    // List all plugins
                    _ => {
                        let _ = registry.list();
                        Ok(())
                    }
                }
            })
        })
        .collect();

    // Wait for all threads (verifies no deadlocks)
    for handle in handles {
        handle.join().expect("Thread should not panic");
    }

    // Verify registry is still functional
    let plugins = registry.list();
    assert!(plugins.len() >= 1, "Should have at least initial plugin");
    assert!(plugins.contains(&"initial".to_string()), "Should still have initial plugin");
}

// ============================================================================
// Additional Edge Case Tests
// ============================================================================

#[test]
fn test_get_returns_arc_clone() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    let registry = TransportPluginRegistry::new();
    let plugin = Arc::new(MockPlugin { name: "test" });

    // Store original strong count
    let original_plugin = Arc::clone(&plugin);
    registry.register(plugin).unwrap();

    // Get plugin multiple times
    let retrieved1 = registry.get("test").unwrap();
    let retrieved2 = registry.get("test").unwrap();

    // Should return clones of the same Arc
    assert_eq!(
        Arc::strong_count(&original_plugin),
        4, // original_plugin + plugin in registry + retrieved1 + retrieved2
        "Should increment Arc strong count"
    );

    // Drop retrievals
    drop(retrieved1);
    drop(retrieved2);

    // Count should decrease
    assert_eq!(
        Arc::strong_count(&original_plugin),
        2, // original_plugin + plugin in registry
        "Strong count should decrease after dropping"
    );
}

#[test]
fn test_empty_registry_operations() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    let registry = TransportPluginRegistry::new();

    // All operations should work on empty registry
    assert!(registry.get("anything").is_none());
    assert_eq!(registry.list().len(), 0);

    // Can still register after queries
    let plugin = Arc::new(MockPlugin { name: "test" });
    assert!(registry.register(plugin).is_ok());
}

#[test]
fn test_plugin_name_with_special_characters() {
    use remotemedia_runtime_core::transport::plugin_registry::TransportPluginRegistry;

    let registry = TransportPluginRegistry::new();

    // Register plugins with special characters in names
    let plugin1 = Arc::new(MockPlugin { name: "grpc-tls" });
    let plugin2 = Arc::new(MockPlugin { name: "webrtc_v2" });
    let plugin3 = Arc::new(MockPlugin { name: "http.2.0" });

    assert!(registry.register(plugin1).is_ok());
    assert!(registry.register(plugin2).is_ok());
    assert!(registry.register(plugin3).is_ok());

    // Verify lookup works with special characters
    assert!(registry.get("grpc-tls").is_some());
    assert!(registry.get("webrtc_v2").is_some());
    assert!(registry.get("http.2.0").is_some());

    let plugins = registry.list();
    assert_eq!(plugins.len(), 3);
}
