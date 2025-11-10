//! Integration tests for multiprocess Python node execution

#[cfg(all(test, feature = "multiprocess"))]
mod tests {
    use remotemedia_runtime::python::multiprocess::{MultiprocessExecutor, MultiprocessConfig};
    use remotemedia_runtime::executor::node_executor::{NodeContext, NodeExecutor};
    use serde_json::json;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use tokio;

    /// Test basic process spawning and lifecycle
    #[tokio::test]
    async fn test_concurrent_node_execution() {
        // Create multiprocess executor
        let config = MultiprocessConfig {
            max_processes_per_session: Some(5),
            channel_capacity: 100,
            init_timeout_secs: 30,
            python_executable: std::path::PathBuf::from("python"),
            enable_backpressure: true,
        };

        let mut executor = MultiprocessExecutor::new(config);

        // Create contexts for multiple nodes
        let node1_ctx = NodeContext {
            node_id: "node1".to_string(),
            node_type: "test_processor".to_string(),
            params: json!({
                "processing_time": 100,
                "output_size": 1024
            }),
            session_id: Some("test_session".to_string()),
            metadata: HashMap::new(),
        };

        let node2_ctx = NodeContext {
            node_id: "node2".to_string(),
            node_type: "test_processor".to_string(),
            params: json!({
                "processing_time": 150,
                "output_size": 2048
            }),
            session_id: Some("test_session".to_string()),
            metadata: HashMap::new(),
        };

        // Initialize both nodes (should spawn processes)
        executor.initialize(&node1_ctx).await.expect("Failed to initialize node1");
        executor.initialize(&node2_ctx).await.expect("Failed to initialize node2");

        // Give processes time to start
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Both nodes should be able to process concurrently
        let start = Instant::now();

        let input = json!({
            "data": "test_data",
            "size": 100
        });

        // Process data in both nodes concurrently
        let (result1, result2) = tokio::join!(
            executor.process(input.clone()),
            executor.process(input.clone())
        );

        let elapsed = start.elapsed();

        // Both should succeed
        assert!(result1.is_ok(), "Node1 processing failed");
        assert!(result2.is_ok(), "Node2 processing failed");

        // Processing should be concurrent (less than sum of individual times)
        // If sequential, would take 250ms+, concurrent should be ~150ms + overhead
        assert!(
            elapsed < Duration::from_millis(200),
            "Processing not concurrent: took {:?}",
            elapsed
        );

        // Cleanup
        executor.cleanup().await.expect("Failed to cleanup");
    }

    /// Test that node failure terminates the pipeline
    #[tokio::test]
    async fn test_pipeline_termination_on_failure() {
        let config = MultiprocessConfig::default();
        let mut executor = MultiprocessExecutor::new(config);

        // Create a node that will fail
        let failing_ctx = NodeContext {
            node_id: "failing_node".to_string(),
            node_type: "failing_processor".to_string(),
            params: json!({
                "fail_after": 500  // Fail after 500ms
            }),
            session_id: Some("fail_test_session".to_string()),
            metadata: HashMap::new(),
        };

        // Create a normal node in the same session
        let normal_ctx = NodeContext {
            node_id: "normal_node".to_string(),
            node_type: "test_processor".to_string(),
            params: json!({}),
            session_id: Some("fail_test_session".to_string()),
            metadata: HashMap::new(),
        };

        // Initialize both nodes
        executor.initialize(&failing_ctx).await.expect("Failed to initialize failing node");
        executor.initialize(&normal_ctx).await.expect("Failed to initialize normal node");

        // Wait for the failing node to crash
        tokio::time::sleep(Duration::from_millis(600)).await;

        // Try to process data - should fail because pipeline is terminated
        let input = json!({"data": "test"});
        let result = executor.process(input).await;

        // Should fail due to pipeline termination
        assert!(
            result.is_err(),
            "Processing should fail after node crash"
        );

        // Cleanup
        let _ = executor.cleanup().await;
    }

    /// Test concurrent sessions isolation
    #[tokio::test]
    async fn test_session_isolation() {
        let config = MultiprocessConfig::default();

        // Create two separate executors for different sessions
        let mut executor1 = MultiprocessExecutor::new(config.clone());
        let mut executor2 = MultiprocessExecutor::new(config);

        // Session 1 nodes
        let session1_node = NodeContext {
            node_id: "s1_node1".to_string(),
            node_type: "test_processor".to_string(),
            params: json!({}),
            session_id: Some("session1".to_string()),
            metadata: HashMap::new(),
        };

        // Session 2 nodes
        let session2_node = NodeContext {
            node_id: "s2_node1".to_string(),
            node_type: "test_processor".to_string(),
            params: json!({}),
            session_id: Some("session2".to_string()),
            metadata: HashMap::new(),
        };

        // Initialize nodes in different sessions
        executor1.initialize(&session1_node).await.expect("Failed to init session1");
        executor2.initialize(&session2_node).await.expect("Failed to init session2");

        // Process data in both sessions concurrently
        let input = json!({"data": "test"});

        let (result1, result2) = tokio::join!(
            executor1.process(input.clone()),
            executor2.process(input.clone())
        );

        // Both should succeed independently
        assert!(result1.is_ok(), "Session1 processing failed");
        assert!(result2.is_ok(), "Session2 processing failed");

        // Cleanup one session shouldn't affect the other
        executor1.cleanup().await.expect("Failed to cleanup session1");

        // Session 2 should still work
        let result2_after = executor2.process(input).await;
        assert!(result2_after.is_ok(), "Session2 should still work after session1 cleanup");

        // Cleanup session 2
        executor2.cleanup().await.expect("Failed to cleanup session2");
    }

    /// Test resource limits enforcement
    #[tokio::test]
    async fn test_process_limit() {
        let config = MultiprocessConfig {
            max_processes_per_session: Some(2),  // Limit to 2 processes
            ..Default::default()
        };

        let mut executor = MultiprocessExecutor::new(config);

        // Try to create 3 nodes (should fail on the 3rd)
        let mut contexts = Vec::new();
        for i in 1..=3 {
            contexts.push(NodeContext {
                node_id: format!("node{}", i),
                node_type: "test_processor".to_string(),
                params: json!({}),
                session_id: Some("limited_session".to_string()),
                metadata: HashMap::new(),
            });
        }

        // First two should succeed
        executor.initialize(&contexts[0]).await.expect("Node1 should initialize");
        executor.initialize(&contexts[1]).await.expect("Node2 should initialize");

        // Third should fail due to process limit
        let result = executor.initialize(&contexts[2]).await;
        assert!(
            result.is_err(),
            "Node3 should fail due to process limit"
        );

        // Cleanup
        executor.cleanup().await.expect("Failed to cleanup");
    }

    /// Benchmark latency between concurrent nodes
    #[tokio::test]
    async fn test_concurrent_latency() {
        let config = MultiprocessConfig::default();
        let mut executor = MultiprocessExecutor::new(config);

        // Create pipeline with multiple nodes
        let num_nodes = 5;
        let mut node_contexts = Vec::new();

        for i in 0..num_nodes {
            node_contexts.push(NodeContext {
                node_id: format!("latency_node_{}", i),
                node_type: "echo_processor".to_string(),
                params: json!({
                    "delay_ms": 10  // Small processing delay
                }),
                session_id: Some("latency_session".to_string()),
                metadata: HashMap::new(),
            });
        }

        // Initialize all nodes
        for ctx in &node_contexts {
            executor.initialize(ctx).await.expect("Failed to initialize node");
        }

        // Measure end-to-end latency for concurrent processing
        let input = json!({"data": vec![0u8; 1024]});  // 1KB payload
        let iterations = 10;
        let mut total_time = Duration::ZERO;

        for _ in 0..iterations {
            let start = Instant::now();

            // Process in all nodes concurrently
            let mut handles = Vec::new();
            for _ in 0..num_nodes {
                let input_clone = input.clone();
                handles.push(executor.process(input_clone));
            }

            // Wait for all to complete
            for handle in handles {
                handle.await.expect("Processing failed");
            }

            total_time += start.elapsed();
        }

        let avg_latency = total_time / iterations;

        println!(
            "Average concurrent processing latency for {} nodes: {:?}",
            num_nodes, avg_latency
        );

        // Should be well under 500ms for concurrent execution
        assert!(
            avg_latency < Duration::from_millis(500),
            "Latency too high: {:?}",
            avg_latency
        );

        // Cleanup
        executor.cleanup().await.expect("Failed to cleanup");
    }

    /// Test initialization progress tracking
    #[tokio::test]
    async fn test_initialization_progress_tracking() {
        use remotemedia_runtime::python::multiprocess::{InitStatus, InitProgress};

        // Create multiprocess executor
        let config = MultiprocessConfig {
            max_processes_per_session: Some(3),
            channel_capacity: 100,
            init_timeout_secs: 30,
            python_executable: std::path::PathBuf::from("python"),
            enable_backpressure: true,
        };

        let executor = MultiprocessExecutor::new(config);

        // Create a session
        let session_id = "progress_test_session";
        executor.create_session(session_id.to_string()).await
            .expect("Failed to create session");

        // Simulate node initialization progress updates
        let node_id = "test_node_1";

        // Starting
        executor.update_init_progress(
            session_id,
            node_id,
            InitStatus::Starting,
            0.0,
            "Starting node process".to_string()
        ).await.expect("Failed to update progress");

        // Loading model
        executor.update_init_progress(
            session_id,
            node_id,
            InitStatus::LoadingModel,
            0.3,
            "Loading AI model weights".to_string()
        ).await.expect("Failed to update progress");

        // Connecting
        executor.update_init_progress(
            session_id,
            node_id,
            InitStatus::Connecting,
            0.7,
            "Connecting to IPC channels".to_string()
        ).await.expect("Failed to update progress");

        // Ready
        executor.update_init_progress(
            session_id,
            node_id,
            InitStatus::Ready,
            1.0,
            "Node ready for execution".to_string()
        ).await.expect("Failed to update progress");

        // Get progress
        let progress_list = executor.get_init_progress(session_id).await
            .expect("Failed to get progress");

        assert_eq!(progress_list.len(), 1, "Should have progress for one node");

        let progress = &progress_list[0];
        assert_eq!(progress.node_id, node_id);
        assert_eq!(progress.status, InitStatus::Ready);
        assert_eq!(progress.progress, 1.0);
        assert_eq!(progress.message, "Node ready for execution");

        // Cleanup
        executor.terminate_session(session_id).await
            .expect("Failed to terminate session");
    }

    /// Test wait_for_initialization with timeout
    #[tokio::test]
    async fn test_wait_for_initialization() {
        use remotemedia_runtime::python::multiprocess::{InitStatus};

        // Create multiprocess executor
        let config = MultiprocessConfig::default();
        let executor = MultiprocessExecutor::new(config);

        // Create a session
        let session_id = "wait_test_session";
        executor.create_session(session_id.to_string()).await
            .expect("Failed to create session");

        // Simulate node initialization in background task
        let executor_clone = executor.clone();
        let session_id_clone = session_id.to_string();
        tokio::spawn(async move {
            // Simulate gradual initialization
            tokio::time::sleep(Duration::from_millis(100)).await;

            executor_clone.update_init_progress(
                &session_id_clone,
                "node1",
                InitStatus::Starting,
                0.0,
                "Starting".to_string()
            ).await.unwrap();

            tokio::time::sleep(Duration::from_millis(200)).await;

            executor_clone.update_init_progress(
                &session_id_clone,
                "node1",
                InitStatus::LoadingModel,
                0.5,
                "Loading model".to_string()
            ).await.unwrap();

            tokio::time::sleep(Duration::from_millis(200)).await;

            executor_clone.update_init_progress(
                &session_id_clone,
                "node1",
                InitStatus::Ready,
                1.0,
                "Ready".to_string()
            ).await.unwrap();
        });

        // Wait for initialization (should complete successfully)
        let start = Instant::now();
        let result = executor.wait_for_initialization(
            session_id,
            Duration::from_secs(5)
        ).await;

        let elapsed = start.elapsed();

        assert!(result.is_ok(), "Initialization should succeed");
        assert!(
            elapsed < Duration::from_secs(2),
            "Should complete in reasonable time"
        );

        println!("Initialization completed in {:?}", elapsed);

        // Cleanup
        executor.terminate_session(session_id).await
            .expect("Failed to terminate session");
    }

    /// Test initialization timeout
    #[tokio::test]
    async fn test_initialization_timeout() {
        // Create multiprocess executor
        let config = MultiprocessConfig::default();
        let executor = MultiprocessExecutor::new(config);

        // Create a session
        let session_id = "timeout_test_session";
        executor.create_session(session_id.to_string()).await
            .expect("Failed to create session");

        // Don't complete initialization - should timeout
        let result = executor.wait_for_initialization(
            session_id,
            Duration::from_millis(500)  // Short timeout
        ).await;

        assert!(result.is_err(), "Should timeout");

        if let Err(e) = result {
            assert!(
                e.to_string().contains("timeout"),
                "Error should mention timeout"
            );
        }

        // Cleanup
        executor.terminate_session(session_id).await
            .expect("Failed to terminate session");
    }
}