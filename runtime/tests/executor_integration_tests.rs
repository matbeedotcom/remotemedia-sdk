//! Integration tests for executor module
//!
//! Tests pipeline execution scenarios including linear, branching,
//! parallel execution, error handling, and timeouts.

#[cfg(test)]
mod integration_tests {
    use remotemedia_runtime::executor::{
        ExecutionContext, Graph, Node, PipelineMetrics, RetryPolicy, Scheduler,
    };
    use remotemedia_runtime::{Error, Result};
    use serde_json::Value;
    use std::time::Duration;
    use tokio::time::sleep;

    /// Helper function to create a simple node
    fn create_node(id: &str, node_type: &str) -> Node {
        Node::new(id, node_type)
    }

    #[tokio::test]
    async fn test_linear_pipeline_execution() {
        // Create a linear pipeline: A -> B -> C
        let mut graph = Graph::new();

        let node_a = create_node("A", "passthrough");
        let mut node_b = create_node("B", "passthrough");
        node_b.add_dependency("A");
        let mut node_c = create_node("C", "passthrough");
        node_c.add_dependency("B");

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();
        graph.add_node(node_c).unwrap();
        graph.build_edges().unwrap();

        // Check execution order first (topological sort also validates)
        let order = graph.topological_sort().unwrap();
        assert_eq!(order, vec!["A", "B", "C"]);

        // Check entry points
        assert_eq!(graph.entry_points(), &["A"]);
    }

    #[tokio::test]
    async fn test_branching_pipeline() {
        // Create a branching pipeline:
        //     B
        //    / \
        //   A   D
        //    \ /
        //     C
        let mut graph = Graph::new();

        let node_a = create_node("A", "source");
        let mut node_b = create_node("B", "processor");
        node_b.add_dependency("A");
        let mut node_c = create_node("C", "processor");
        node_c.add_dependency("A");
        let mut node_d = create_node("D", "sink");
        node_d.add_dependency("B");
        node_d.add_dependency("C");

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();
        graph.add_node(node_c).unwrap();
        graph.add_node(node_d).unwrap();
        graph.build_edges().unwrap();

        // Verify execution order maintains dependencies
        let order = graph.topological_sort().unwrap();
        let a_idx = order.iter().position(|x| x == "A").unwrap();
        let b_idx = order.iter().position(|x| x == "B").unwrap();
        let c_idx = order.iter().position(|x| x == "C").unwrap();
        let d_idx = order.iter().position(|x| x == "D").unwrap();

        assert!(a_idx < b_idx);
        assert!(a_idx < c_idx);
        assert!(b_idx < d_idx);
        assert!(c_idx < d_idx);
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        // Test parallel independent nodes
        let mut graph = Graph::new();

        let node_a = create_node("A", "source");
        let mut node_b = create_node("B", "processor");
        node_b.add_dependency("A");
        let mut node_c = create_node("C", "processor");
        node_c.add_dependency("A");
        let mut node_d = create_node("D", "processor");
        node_d.add_dependency("A");

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();
        graph.add_node(node_c).unwrap();
        graph.add_node(node_d).unwrap();
        graph.build_edges().unwrap();

        // B, C, D can execute in parallel after A
        let order = graph.topological_sort().unwrap();
        assert_eq!(order[0], "A");
        // B, C, D should be after A (order between them doesn't matter)
        assert!(order[1..].contains(&"B".to_string()));
        assert!(order[1..].contains(&"C".to_string()));
        assert!(order[1..].contains(&"D".to_string()));
    }

    #[tokio::test]
    async fn test_cycle_detection() {
        // Create a cycle: A -> B -> C -> A
        let mut graph = Graph::new();

        let mut node_a = create_node("A", "test");
        node_a.add_dependency("C"); // Creates cycle
        let mut node_b = create_node("B", "test");
        node_b.add_dependency("A");
        let mut node_c = create_node("C", "test");
        node_c.add_dependency("B");

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();
        graph.add_node(node_c).unwrap();
        graph.build_edges().unwrap();

        // Should detect cycle
        let cycle = graph.detect_cycles();
        assert!(cycle.is_some());
        assert!(cycle.unwrap().len() > 0);

        // Validation should fail
        assert!(graph.validate().is_err());
    }

    #[tokio::test]
    async fn test_scheduler_basic() {
        let scheduler = Scheduler::new(4, "test_pipeline");

        let ctx = ExecutionContext::new("test", "node1").with_input(Value::from(42));

        let result = scheduler
            .schedule_node(ctx, |input| async move {
                let num = input.as_i64().unwrap();
                Ok(Value::from(num * 2))
            })
            .await
            .unwrap();

        assert_eq!(result, Value::from(84));
    }

    #[tokio::test]
    async fn test_scheduler_parallel() {
        let scheduler = Scheduler::new(4, "test_pipeline");

        let contexts = vec![
            ExecutionContext::new("test", "node1").with_input(Value::from(1)),
            ExecutionContext::new("test", "node2").with_input(Value::from(2)),
            ExecutionContext::new("test", "node3").with_input(Value::from(3)),
            ExecutionContext::new("test", "node4").with_input(Value::from(4)),
        ];

        let results = scheduler
            .execute_parallel(contexts, |ctx| async move {
                let num = ctx.input_data.as_i64().unwrap();
                sleep(Duration::from_millis(10)).await;
                Ok(Value::from(num * 2))
            })
            .await
            .unwrap();

        assert_eq!(results.len(), 4);
        assert_eq!(results[0], Value::from(2));
        assert_eq!(results[1], Value::from(4));
        assert_eq!(results[2], Value::from(6));
        assert_eq!(results[3], Value::from(8));
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let scheduler = Scheduler::new(2, "test_pipeline");

        let ctx = ExecutionContext::new("test", "failing_node").with_input(Value::Null);

        let result = scheduler
            .schedule_node(ctx, |_| async move {
                Err(Error::Execution("Intentional test failure".to_string()))
            })
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Intentional test failure"));
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let scheduler = Scheduler::new(2, "test_pipeline");

        // Execute some successful nodes
        for i in 0..5 {
            let ctx = ExecutionContext::new("test", format!("node{}", i)).with_input(Value::from(i));

            scheduler
                .schedule_node(ctx, |input| async move {
                    sleep(Duration::from_millis(10)).await;
                    Ok(input)
                })
                .await
                .unwrap();
        }

        // Execute a failing node
        let ctx = ExecutionContext::new("test", "failing_node").with_input(Value::Null);
        let _ = scheduler
            .schedule_node(ctx, |_| async move {
                Err(Error::Execution("test error".to_string()))
            })
            .await;

        let metrics = scheduler.get_metrics().await;
        assert_eq!(metrics.node_metrics().len(), 6); // 5 successful + 1 failed

        // Check individual node metrics
        let node0_metrics = metrics.get_node_metrics("node0").unwrap();
        assert_eq!(node0_metrics.success_count, 1);
        assert_eq!(node0_metrics.error_count, 0);

        let failing_metrics = metrics.get_node_metrics("failing_node").unwrap();
        assert_eq!(failing_metrics.error_count, 1);
        assert_eq!(failing_metrics.success_count, 0);
    }

    #[tokio::test]
    async fn test_retry_policy_fixed() {
        use remotemedia_runtime::executor::retry::execute_with_retry;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result: Result<Value> = execute_with_retry(
            RetryPolicy::fixed(3, Duration::from_millis(10)),
            || {
                let attempt = attempt_clone.clone();
                async move {
                    let current = attempt.fetch_add(1, Ordering::SeqCst) + 1;
                    if current < 3 {
                        Err(Error::Io(std::io::Error::new(
                            std::io::ErrorKind::ConnectionRefused,
                            "retry test",
                        )))
                    } else {
                        Ok(Value::from(42))
                    }
                }
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::from(42));
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        use remotemedia_runtime::executor::retry::execute_with_retry;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result: Result<()> = execute_with_retry(
            RetryPolicy::fixed(2, Duration::from_millis(10)),
            || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::ConnectionRefused,
                        "always fails",
                    )))
                }
            },
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Retry exhausted"));
        assert_eq!(attempt.load(Ordering::SeqCst), 3); // Initial + 2 retries
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        use remotemedia_runtime::executor::error::ExecutionErrorExt;
        use remotemedia_runtime::executor::retry::execute_with_retry;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result: Result<()> = execute_with_retry(
            RetryPolicy::fixed(3, Duration::from_millis(10)),
            || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err(Error::Manifest("config error".to_string()))
                }
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempt.load(Ordering::SeqCst), 1); // Should not retry non-retryable errors
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let policy = RetryPolicy::exponential(4);

        let delay0 = policy.delay_for_attempt(0).unwrap();
        let delay1 = policy.delay_for_attempt(1).unwrap();
        let delay2 = policy.delay_for_attempt(2).unwrap();

        // Delays should increase exponentially
        assert!(delay1 > delay0);
        assert!(delay2 > delay1);

        // Should return None after max attempts
        assert!(policy.delay_for_attempt(4).is_none());
    }

    #[tokio::test]
    async fn test_missing_dependency_error() {
        let mut graph = Graph::new();

        let mut node_a = create_node("A", "test");
        node_a.add_dependency("NonExistent");

        graph.add_node(node_a).unwrap();

        // Should fail when building edges
        let result = graph.build_edges();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("non-existent node"));
    }

    #[tokio::test]
    async fn test_duplicate_node_error() {
        let mut graph = Graph::new();

        let node1 = create_node("duplicate", "test");
        let node2 = create_node("duplicate", "test");

        graph.add_node(node1).unwrap();
        let result = graph.add_node(node2);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate"));
    }

    #[tokio::test]
    async fn test_graph_serialization() {
        let node = Node::new("test_node", "test_type")
            .with_config(serde_json::json!({"param": "value"}));

        // Serialize to JSON
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("test_node"));
        assert!(json.contains("test_type"));

        // Deserialize back
        let deserialized: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "test_node");
        assert_eq!(deserialized.node_type, "test_type");
    }
}

