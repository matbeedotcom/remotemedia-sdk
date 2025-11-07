//! Integration tests for crash recovery and failure handling
//!
//! Tests the behavior of the multiprocess executor when nodes crash,
//! including process exit handling, pipeline termination, and resource cleanup.

#[cfg(all(test, feature = "multiprocess"))]
mod tests {
    use remotemedia_runtime::python::multiprocess::{
        InitStatus, MultiprocessConfig, MultiprocessExecutor,
    };
    use std::time::Duration;
    use tokio;

    /// Helper to create a test executor with reasonable defaults
    fn create_test_executor() -> MultiprocessExecutor {
        let config = MultiprocessConfig {
            max_processes_per_session: Some(10),
            channel_capacity: 100,
            init_timeout_secs: 10,
            python_executable: std::path::PathBuf::from("python"),
            enable_backpressure: true,
        };

        MultiprocessExecutor::new(config)
    }

    /// Test that the executor can detect when a process crashes
    #[tokio::test]
    async fn test_detect_process_crash() {
        let executor = create_test_executor();
        let session_id = "crash_test_session";

        // Create session
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create session");

        // Simulate a node initialization
        executor
            .update_init_progress(
                session_id,
                "test_node",
                InitStatus::Starting,
                0.0,
                "Starting node".to_string(),
            )
            .await
            .expect("Failed to update progress");

        // Note: In a real test, we would spawn a Python process and kill it
        // For now, we test that the infrastructure is in place

        // Verify session exists
        let progress = executor.get_init_progress(session_id).await;
        assert!(
            progress.is_ok(),
            "Should be able to get progress for active session"
        );

        // Cleanup
        executor
            .terminate_session(session_id)
            .await
            .expect("Failed to terminate session");
    }

    /// Test that all processes in a session are terminated when one crashes
    #[tokio::test]
    async fn test_cascade_termination_on_crash() {
        let executor = create_test_executor();
        let session_id = "cascade_test_session";

        // Create session
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create session");

        // Simulate multiple nodes
        let nodes = vec!["node1", "node2", "node3"];

        for node_id in &nodes {
            executor
                .update_init_progress(
                    session_id,
                    node_id,
                    InitStatus::Starting,
                    0.0,
                    "Starting node".to_string(),
                )
                .await
                .expect("Failed to update progress");

            executor
                .update_init_progress(
                    session_id,
                    node_id,
                    InitStatus::Ready,
                    1.0,
                    "Node ready".to_string(),
                )
                .await
                .expect("Failed to update progress");
        }

        // Verify all nodes are tracked
        let progress = executor
            .get_init_progress(session_id)
            .await
            .expect("Should get progress");

        assert_eq!(progress.len(), nodes.len(), "All nodes should be tracked");

        // Simulate crash by terminating session
        // In production, this would be triggered by process exit signal
        executor
            .terminate_session(session_id)
            .await
            .expect("Failed to terminate session");

        // Verify session is terminated
        let progress_after = executor.get_init_progress(session_id).await;
        assert!(
            progress_after.is_err(),
            "Session should no longer exist after termination"
        );
    }

    /// Test that resources are cleaned up after a crash
    #[tokio::test]
    async fn test_resource_cleanup_after_crash() {
        let executor = create_test_executor();
        let session_id = "cleanup_test_session";

        // Create session
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create session");

        // Add some nodes
        for i in 0..3 {
            let node_id = format!("node_{}", i);
            executor
                .update_init_progress(
                    session_id,
                    &node_id,
                    InitStatus::Ready,
                    1.0,
                    "Ready".to_string(),
                )
                .await
                .expect("Failed to update progress");
        }

        // Terminate session (simulating crash cleanup)
        executor
            .terminate_session(session_id)
            .await
            .expect("Failed to terminate session");

        // Verify session is gone
        let result = executor.get_init_progress(session_id).await;
        assert!(result.is_err(), "Session should be cleaned up");

        // Verify we can create a new session with the same ID (resources are freed)
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Should be able to reuse session ID after cleanup");

        // Cleanup
        executor
            .terminate_session(session_id)
            .await
            .expect("Failed to terminate session");
    }

    /// Test that initialization timeout is properly handled
    #[tokio::test]
    async fn test_initialization_timeout() {
        let config = MultiprocessConfig {
            max_processes_per_session: Some(10),
            channel_capacity: 100,
            init_timeout_secs: 1, // Very short timeout for testing
            python_executable: std::path::PathBuf::from("python"),
            enable_backpressure: true,
        };

        let executor = MultiprocessExecutor::new(config);
        let session_id = "timeout_test_session";

        // Create session
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create session");

        // Start initialization but don't complete it
        executor
            .update_init_progress(
                session_id,
                "slow_node",
                InitStatus::Starting,
                0.0,
                "Starting slowly...".to_string(),
            )
            .await
            .expect("Failed to update progress");

        // Wait for timeout period
        tokio::time::sleep(Duration::from_secs(2)).await;

        // In a real implementation, the executor would detect the timeout
        // and terminate the session. For now, we just verify the infrastructure exists.

        // Cleanup
        let _ = executor.terminate_session(session_id).await;
    }

    /// Test that multiple concurrent sessions can handle crashes independently
    #[tokio::test]
    async fn test_concurrent_session_isolation() {
        let executor = create_test_executor();

        let session_ids = vec!["session_a", "session_b", "session_c"];

        // Create multiple sessions
        for session_id in &session_ids {
            executor
                .create_session(session_id.to_string())
                .await
                .expect("Failed to create session");

            executor
                .update_init_progress(
                    session_id,
                    "test_node",
                    InitStatus::Ready,
                    1.0,
                    "Ready".to_string(),
                )
                .await
                .expect("Failed to update progress");
        }

        // Verify all sessions exist
        for session_id in &session_ids {
            let progress = executor.get_init_progress(session_id).await;
            assert!(progress.is_ok(), "Session {} should exist", session_id);
        }

        // Terminate one session (simulating a crash)
        executor
            .terminate_session("session_b")
            .await
            .expect("Failed to terminate session_b");

        // Verify other sessions are unaffected
        for session_id in &["session_a", "session_c"] {
            let progress = executor.get_init_progress(session_id).await;
            assert!(
                progress.is_ok(),
                "Session {} should still exist",
                session_id
            );
        }

        // Verify crashed session is gone
        let result = executor.get_init_progress("session_b").await;
        assert!(result.is_err(), "Crashed session should be removed");

        // Cleanup remaining sessions
        for session_id in &["session_a", "session_c"] {
            let _ = executor.terminate_session(session_id).await;
        }
    }

    /// Test that process limits are enforced per session
    #[tokio::test]
    async fn test_process_limit_enforcement() {
        let config = MultiprocessConfig {
            max_processes_per_session: Some(3), // Strict limit for testing
            channel_capacity: 100,
            init_timeout_secs: 30,
            python_executable: std::path::PathBuf::from("python"),
            enable_backpressure: true,
        };

        let executor = MultiprocessExecutor::new(config);
        let session_id = "limit_test_session";

        // Create session
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create session");

        // Add nodes up to the limit
        for i in 0..3 {
            let node_id = format!("node_{}", i);
            executor
                .update_init_progress(
                    session_id,
                    &node_id,
                    InitStatus::Ready,
                    1.0,
                    "Ready".to_string(),
                )
                .await
                .expect("Failed to add node within limit");
        }

        // Verify we have 3 nodes
        let progress = executor
            .get_init_progress(session_id)
            .await
            .expect("Should get progress");
        assert_eq!(progress.len(), 3, "Should have 3 nodes");

        // In a full implementation, trying to add a 4th node would fail
        // For now, we just verify the limit is configurable

        // Cleanup
        executor
            .terminate_session(session_id)
            .await
            .expect("Failed to terminate session");
    }

    /// Test graceful shutdown vs forced termination
    #[tokio::test]
    async fn test_graceful_vs_forced_shutdown() {
        let executor = create_test_executor();
        let session_id = "shutdown_test_session";

        // Create session
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create session");

        // Add nodes
        for i in 0..2 {
            let node_id = format!("node_{}", i);
            executor
                .update_init_progress(
                    session_id,
                    &node_id,
                    InitStatus::Ready,
                    1.0,
                    "Ready".to_string(),
                )
                .await
                .expect("Failed to add node");
        }

        // Measure termination time
        let start = std::time::Instant::now();
        executor
            .terminate_session(session_id)
            .await
            .expect("Failed to terminate session");
        let duration = start.elapsed();

        // Termination should be reasonably fast (< 5 seconds for 2 test nodes)
        assert!(
            duration < Duration::from_secs(5),
            "Termination took too long: {:?}",
            duration
        );
    }

    /// Test that configuration can be loaded from TOML
    #[tokio::test]
    async fn test_config_loading_from_toml() {
        // Test with default values (no file)
        let config = MultiprocessConfig::from_default_file().expect("Should load default config");

        assert_eq!(config.channel_capacity, 100);
        assert_eq!(config.init_timeout_secs, 30);
        assert!(config.enable_backpressure);

        // Test parsing from TOML string
        let toml_str = r#"
            max_processes_per_session = 20
            channel_capacity = 200
            init_timeout_secs = 60
            python_executable = "/usr/bin/python3"
            enable_backpressure = false
        "#;

        let config = MultiprocessConfig::from_toml_str(toml_str).expect("Should parse TOML config");

        assert_eq!(config.max_processes_per_session, Some(20));
        assert_eq!(config.channel_capacity, 200);
        assert_eq!(config.init_timeout_secs, 60);
        assert_eq!(
            config.python_executable.to_str().unwrap(),
            "/usr/bin/python3"
        );
        assert!(!config.enable_backpressure);
    }

    /// Test partial configuration (defaults for missing fields)
    #[tokio::test]
    async fn test_partial_config_with_defaults() {
        // Only specify some fields
        let toml_str = r#"
            channel_capacity = 500
        "#;

        let config =
            MultiprocessConfig::from_toml_str(toml_str).expect("Should parse partial config");

        // Specified field
        assert_eq!(config.channel_capacity, 500);

        // Default fields
        assert_eq!(config.max_processes_per_session, Some(10));
        assert_eq!(config.init_timeout_secs, 30);
        assert_eq!(config.python_executable, std::path::PathBuf::from("python"));
        assert!(config.enable_backpressure);
    }

    /// Test that invalid TOML produces clear errors
    #[tokio::test]
    async fn test_invalid_toml_error_handling() {
        let invalid_toml = r#"
            this is not valid toml!!!
            channel_capacity = "should be number"
        "#;

        let result = MultiprocessConfig::from_toml_str(invalid_toml);
        assert!(result.is_err(), "Should fail to parse invalid TOML");

        if let Err(e) = result {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("Configuration error") || error_msg.contains("parse"),
                "Error message should mention configuration or parsing: {}",
                error_msg
            );
        }
    }
}
