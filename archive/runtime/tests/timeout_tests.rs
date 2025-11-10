//! Timeout handling tests for executor module
//!
//! Tests various timeout scenarios including node timeouts,
//! pipeline timeouts, and graceful cancellation.

#[cfg(test)]
mod timeout_tests {
    use remotemedia_runtime::executor::{ExecutionContext, Scheduler};
    use remotemedia_runtime::{Error, Result};
    use serde_json::Value;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_node_timeout() {
        let scheduler = Scheduler::new(2, "test_pipeline");

        let ctx = ExecutionContext::new("test", "slow_node")
            .with_input(Value::Null)
            .with_timeout(Duration::from_millis(100));

        let result = scheduler
            .schedule_node(ctx, |_| async move {
                sleep(Duration::from_secs(1)).await;
                Ok(Value::Null)
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Timeout") || err.to_string().contains("timeout"));
    }

    #[tokio::test]
    async fn test_node_completes_before_timeout() {
        let scheduler = Scheduler::new(2, "test_pipeline");

        let ctx = ExecutionContext::new("test", "fast_node")
            .with_input(Value::from(42))
            .with_timeout(Duration::from_secs(1));

        let result = scheduler
            .schedule_node(ctx, |input| async move {
                sleep(Duration::from_millis(50)).await;
                Ok(input)
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::from(42));
    }

    #[tokio::test]
    async fn test_default_timeout() {
        let scheduler =
            Scheduler::new(2, "test_pipeline").with_default_timeout(Duration::from_millis(100));

        // Node without explicit timeout should use default
        let ctx = ExecutionContext::new("test", "slow_node").with_input(Value::Null);

        let result = scheduler
            .schedule_node(ctx, |_| async move {
                sleep(Duration::from_secs(1)).await;
                Ok(Value::Null)
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Timeout"));
    }

    #[tokio::test]
    async fn test_per_node_timeout_override() {
        let scheduler =
            Scheduler::new(2, "test_pipeline").with_default_timeout(Duration::from_millis(50));

        // Node with explicit timeout should override default
        let ctx = ExecutionContext::new("test", "node")
            .with_input(Value::Null)
            .with_timeout(Duration::from_millis(200));

        let result = scheduler
            .schedule_node(ctx, |_| async move {
                sleep(Duration::from_millis(100)).await;
                Ok(Value::from(42))
            })
            .await;

        // Should succeed because explicit timeout (200ms) > sleep (100ms)
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::from(42));
    }

    #[tokio::test]
    async fn test_parallel_timeout_handling() {
        let scheduler =
            Scheduler::new(4, "test_pipeline").with_default_timeout(Duration::from_millis(100));

        let contexts = vec![
            ExecutionContext::new("test", "fast1").with_input(Value::from(1)),
            ExecutionContext::new("test", "slow1").with_input(Value::from(2)),
            ExecutionContext::new("test", "fast2").with_input(Value::from(3)),
        ];

        let result = scheduler
            .execute_parallel(contexts, |ctx| async move {
                let num = ctx.input_data.as_i64().unwrap();
                if num == 2 {
                    // This node will timeout
                    sleep(Duration::from_secs(1)).await;
                } else {
                    sleep(Duration::from_millis(20)).await;
                }
                Ok(Value::from(num * 2))
            })
            .await;

        // Should fail because one node timed out
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Timeout"));
    }

    #[tokio::test]
    async fn test_timeout_with_metrics() {
        let scheduler = Scheduler::new(2, "test_pipeline");

        // Execute a node that times out
        let ctx = ExecutionContext::new("test", "timeout_node")
            .with_input(Value::Null)
            .with_timeout(Duration::from_millis(50));

        let _ = scheduler
            .schedule_node(ctx, |_| async move {
                sleep(Duration::from_secs(1)).await;
                Ok(Value::Null)
            })
            .await;

        // Metrics should record the timeout as an error
        let metrics = scheduler.get_metrics().await;
        let node_metrics = metrics.get_node_metrics("timeout_node").unwrap();

        assert_eq!(node_metrics.error_count, 1);
        assert_eq!(node_metrics.success_count, 0);
    }

    #[tokio::test]
    async fn test_no_timeout_when_disabled() {
        let scheduler = Scheduler::new(2, "test_pipeline");
        // No default timeout

        let ctx = ExecutionContext::new("test", "long_node").with_input(Value::from(42));
        // No per-node timeout either

        let result = scheduler
            .schedule_node(ctx, |input| async move {
                sleep(Duration::from_millis(100)).await;
                Ok(input)
            })
            .await;

        // Should succeed even though it takes time
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::from(42));
    }

    #[tokio::test]
    async fn test_timeout_error_message_contains_node_id() {
        let scheduler = Scheduler::new(2, "test_pipeline");

        let ctx = ExecutionContext::new("test", "specific_node_id")
            .with_input(Value::Null)
            .with_timeout(Duration::from_millis(50));

        let result = scheduler
            .schedule_node(ctx, |_| async move {
                sleep(Duration::from_secs(1)).await;
                Ok(Value::Null)
            })
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("specific_node_id"));
    }

    #[tokio::test]
    async fn test_rapid_sequential_timeouts() {
        let scheduler =
            Scheduler::new(2, "test_pipeline").with_default_timeout(Duration::from_millis(50));

        // Execute multiple nodes that timeout in sequence
        for i in 0..5 {
            let ctx = ExecutionContext::new("test", format!("timeout_node_{}", i))
                .with_input(Value::Null);

            let result = scheduler
                .schedule_node(ctx, |_| async move {
                    sleep(Duration::from_secs(1)).await;
                    Ok(Value::Null)
                })
                .await;

            assert!(result.is_err());
        }

        let metrics = scheduler.get_metrics().await;
        assert_eq!(metrics.node_metrics().len(), 5);

        // All should be errors
        for i in 0..5 {
            let node_metrics = metrics
                .get_node_metrics(&format!("timeout_node_{}", i))
                .unwrap();
            assert_eq!(node_metrics.error_count, 1);
        }
    }

    #[tokio::test]
    async fn test_timeout_with_retry_policy() {
        use remotemedia_runtime::executor::error::ExecutionErrorExt;
        use remotemedia_runtime::executor::retry::{execute_with_retry, RetryPolicy};
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result: Result<()> =
            execute_with_retry(RetryPolicy::fixed(2, Duration::from_millis(10)), || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    // Timeout errors are retryable
                    Err(Error::timeout("simulated timeout"))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempt.load(Ordering::SeqCst), 3); // Initial + 2 retries
        assert!(result.unwrap_err().to_string().contains("Retry exhausted"));
    }

    #[tokio::test]
    async fn test_zero_timeout() {
        let scheduler = Scheduler::new(2, "test_pipeline");

        let ctx = ExecutionContext::new("test", "instant_node")
            .with_input(Value::from(42))
            .with_timeout(Duration::from_millis(0));

        let result = scheduler
            .schedule_node(ctx, |input| async move {
                // Even instant operations might timeout with 0ms
                Ok(input)
            })
            .await;

        // Zero timeout should immediately timeout (or succeed if fast enough)
        // The exact behavior depends on tokio's implementation
        if result.is_err() {
            assert!(result.unwrap_err().to_string().contains("Timeout"));
        }
    }

    #[tokio::test]
    async fn test_very_long_timeout() {
        let scheduler = Scheduler::new(2, "test_pipeline");

        let ctx = ExecutionContext::new("test", "node")
            .with_input(Value::from(42))
            .with_timeout(Duration::from_secs(3600)); // 1 hour

        let result = scheduler
            .schedule_node(ctx, |input| async move {
                sleep(Duration::from_millis(50)).await;
                Ok(input)
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::from(42));
    }
}
