//! Integration tests for gRPC multiprocess pipeline execution
//!
//! Tests the integration of multiprocess Python nodes with the gRPC service,
//! verifying executor routing, session management, and resource cleanup.

#[cfg(all(test, feature = "grpc-transport", feature = "multiprocess"))]
mod tests {
    use remotemedia_runtime::grpc_service::executor_registry::{
        ExecutorRegistry, ExecutorType, PatternRule,
    };
    use remotemedia_runtime::python::multiprocess::{MultiprocessConfig, MultiprocessExecutor};

    /// Test executor registry initialization and pattern matching
    #[tokio::test]
    async fn test_executor_registry_pattern_matching() {
        let mut registry = ExecutorRegistry::new();

        // Register native nodes
        registry.register_explicit("AudioChunkerNode", ExecutorType::Native);
        registry.register_explicit("FastResampleNode", ExecutorType::Native);

        // Register Python multiprocess pattern
        let python_pattern = PatternRule::new(
            r"^(Whisper|LFM2|VibeVoice)Node$",
            ExecutorType::Multiprocess,
            100,
            "Python AI nodes",
        )
        .expect("Failed to create pattern");
        registry.register_pattern(python_pattern);

        // Set default
        registry.set_default(ExecutorType::Native);

        // Test explicit mappings
        assert_eq!(
            registry.get_executor_for_node("AudioChunkerNode"),
            ExecutorType::Native
        );
        assert_eq!(
            registry.get_executor_for_node("FastResampleNode"),
            ExecutorType::Native
        );

        // Test pattern matching for Python nodes
        assert_eq!(
            registry.get_executor_for_node("WhisperNode"),
            ExecutorType::Multiprocess
        );
        assert_eq!(
            registry.get_executor_for_node("LFM2Node"),
            ExecutorType::Multiprocess
        );
        assert_eq!(
            registry.get_executor_for_node("VibeVoiceNode"),
            ExecutorType::Multiprocess
        );

        // Test default fallback
        assert_eq!(
            registry.get_executor_for_node("UnknownNode"),
            ExecutorType::Native
        );
    }

    /// Test that multiple executors can coexist in registry
    #[tokio::test]
    async fn test_multiple_executor_types() {
        let mut registry = ExecutorRegistry::new();

        // Register different node types for different executors
        registry.register_explicit("AudioChunkerNode", ExecutorType::Native);

        #[cfg(feature = "multiprocess")]
        {
            let python_pattern = PatternRule::new(
                r".*Node$",
                ExecutorType::Multiprocess,
                50,
                "Default Python nodes",
            )
            .expect("Failed to create pattern");
            registry.register_pattern(python_pattern);
        }

        let summary = registry.summary();
        assert_eq!(summary.explicit_mappings_count, 1);

        #[cfg(feature = "multiprocess")]
        assert_eq!(summary.pattern_rules_count, 1);

        #[cfg(not(feature = "multiprocess"))]
        assert_eq!(summary.pattern_rules_count, 0);
    }

    /// Test multiprocess executor initialization
    #[tokio::test]
    async fn test_multiprocess_executor_initialization() {
        let config = MultiprocessConfig {
            max_processes_per_session: Some(5),
            channel_capacity: 100,
            init_timeout_secs: 10,
            python_executable: std::path::PathBuf::from("python"),
            enable_backpressure: true,
        };

        let executor = MultiprocessExecutor::new(config);

        // Create test session
        let session_id = "test_grpc_session";
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create session");

        // Verify session exists
        let progress = executor.get_init_progress(session_id).await;
        assert!(progress.is_ok(), "Session should exist");

        // Cleanup
        executor
            .terminate_session(session_id)
            .await
            .expect("Failed to terminate session");
    }

    /// Test session context creation and node assignment
    #[tokio::test]
    async fn test_session_context_node_assignment() {
        use remotemedia_runtime::grpc_service::execution::SessionExecutionContext;

        let session_id = "test_session_123";
        let session_ctx = SessionExecutionContext::new(session_id.to_string());

        // Assign nodes to executors
        session_ctx
            .assign_node("node1".to_string(), ExecutorType::Native)
            .await;
        session_ctx
            .assign_node("node2".to_string(), ExecutorType::Multiprocess)
            .await;
        session_ctx
            .assign_node("node3".to_string(), ExecutorType::Native)
            .await;

        // Verify assignments
        assert_eq!(
            session_ctx.get_node_executor("node1").await,
            Some(ExecutorType::Native)
        );
        assert_eq!(
            session_ctx.get_node_executor("node2").await,
            Some(ExecutorType::Multiprocess)
        );
        assert_eq!(
            session_ctx.get_node_executor("node3").await,
            Some(ExecutorType::Native)
        );
        assert_eq!(session_ctx.get_node_executor("node4").await, None);
    }

    /// Test executor bridge creation and initialization
    #[tokio::test]
    async fn test_executor_bridges_initialization() {
        use remotemedia_runtime::executor::{executor_bridge::*, Executor};
        use std::sync::Arc;

        // Create executors
        let native_executor = Arc::new(Executor::new());
        let mp_config = MultiprocessConfig::default();
        let mp_executor = Arc::new(MultiprocessExecutor::new(mp_config));

        // Create bridges
        let native_bridge = NativeExecutorBridge::new(Arc::clone(&native_executor));
        assert_eq!(native_bridge.executor_type_name(), "native");

        let session_id = "test_bridge_session";
        mp_executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create MP session");

        let mp_bridge =
            MultiprocessExecutorBridge::new(Arc::clone(&mp_executor), session_id.to_string());
        assert_eq!(mp_bridge.executor_type_name(), "multiprocess");

        // Cleanup
        mp_executor
            .terminate_session(session_id)
            .await
            .expect("Failed to terminate session");
    }

    /// Test manifest loading from fixtures
    #[tokio::test]
    async fn test_load_multiprocess_manifest() {
        use std::path::PathBuf;

        let manifest_path = PathBuf::from("tests/fixtures/multiprocess_manifest.json");

        if !manifest_path.exists() {
            // Skip if running in environment where fixtures aren't available
            eprintln!("Skipping test - fixture not found: {:?}", manifest_path);
            return;
        }

        let manifest_json =
            std::fs::read_to_string(&manifest_path).expect("Failed to read manifest fixture");

        let manifest: serde_json::Value =
            serde_json::from_str(&manifest_json).expect("Failed to parse manifest JSON");

        // Verify structure
        assert_eq!(manifest["version"], "v1");
        assert_eq!(manifest["metadata"]["name"], "test-multiprocess-pipeline");

        // Verify multiprocess config exists
        assert!(manifest["metadata"]["multiprocess"].is_object());
        assert_eq!(
            manifest["metadata"]["multiprocess"]["max_processes_per_session"],
            5
        );

        // Verify nodes
        let nodes = manifest["nodes"].as_array().expect("nodes should be array");
        assert_eq!(nodes.len(), 3);

        // Verify connections
        let connections = manifest["connections"]
            .as_array()
            .expect("connections should be array");
        assert_eq!(connections.len(), 2);
    }
}

// Export SessionExecutionContext for testing
#[cfg(all(test, feature = "grpc-transport"))]
pub use remotemedia_runtime::grpc_service::execution::SessionExecutionContext;
