//! Integration tests for retry logic and circuit breaker
//!
//! Tests T113-T116: Transient error injection, retry behavior, circuit breaker tripping

use remotemedia_runtime::{Error, Result};
use remotemedia_runtime::executor::retry::{RetryPolicy, CircuitBreaker, execute_with_retry};
use remotemedia_runtime::executor::scheduler::{Scheduler, ExecutionContext};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use serde_json::Value;

/// Test T113: Transient error injection with successful retry
#[tokio::test]
async fn test_transient_error_with_retry() {
    let attempt_count = Arc::new(AtomicU32::new(0));
    let attempt_clone = attempt_count.clone();

    let result: Result<String> = execute_with_retry(
        RetryPolicy::fixed(3, Duration::from_millis(10)),
        || {
            let attempt = attempt_clone.clone();
            async move {
                let current = attempt.fetch_add(1, Ordering::SeqCst) + 1;
                if current < 3 {
                    // Simulate transient network error
                    Err(Error::transport("Connection timeout"))
                } else {
                    Ok("Success!".to_string())
                }
            }
        },
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Success!");
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
}

/// Test T114: Successful retry after 2 failures
#[tokio::test]
async fn test_successful_retry_after_failures() {
    let attempt_count = Arc::new(AtomicU32::new(0));
    let attempt_clone = attempt_count.clone();

    let policy = RetryPolicy::Exponential {
        base_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(100),
        max_attempts: 3,
        multiplier: 2.0,
    };

    let result: Result<i32> = execute_with_retry(policy, || {
        let attempt = attempt_clone.clone();
        async move {
            let current = attempt.fetch_add(1, Ordering::SeqCst) + 1;
            if current <= 2 {
                // Fail first 2 attempts
                Err(Error::execution("Temporary resource unavailable"))
            } else {
                Ok(42)
            }
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3); // Initial + 2 retries
}

/// Test T115: Immediate failure on non-retryable errors
#[tokio::test]
async fn test_immediate_failure_non_retryable() {
    let attempt_count = Arc::new(AtomicU32::new(0));
    let attempt_clone = attempt_count.clone();

    let result: Result<()> = execute_with_retry(
        RetryPolicy::fixed(3, Duration::from_millis(10)),
        || {
            let attempt = attempt_clone.clone();
            async move {
                attempt.fetch_add(1, Ordering::SeqCst);
                // Non-retryable error: manifest error
                Err(Error::manifest("Invalid pipeline configuration"))
            }
        },
    )
    .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::Manifest { .. }));
    // Should not retry - only 1 attempt
    assert_eq!(attempt_count.load(Ordering::SeqCst), 1);
}

/// Test T116: Circuit breaker tripping after 5 failures
#[tokio::test]
async fn test_circuit_breaker_trips_after_failures() {
    let mut circuit_breaker = CircuitBreaker::new(5);

    // Record 4 failures - should not trip yet
    for _ in 0..4 {
        circuit_breaker.record_failure();
        assert!(!circuit_breaker.is_open(), "Circuit breaker should not be open yet");
    }

    // 5th failure should trip the circuit
    circuit_breaker.record_failure();
    assert!(circuit_breaker.is_open(), "Circuit breaker should be open after 5 failures");
    assert_eq!(circuit_breaker.consecutive_failures(), 5);

    // Success should reset the circuit
    circuit_breaker.record_success();
    assert!(!circuit_breaker.is_open(), "Circuit breaker should be closed after success");
    assert_eq!(circuit_breaker.consecutive_failures(), 0);
}

/// Test: Circuit breaker with scheduler integration
#[tokio::test]
async fn test_scheduler_with_circuit_breaker() {
    let scheduler = Scheduler::new(2, "test_pipeline")
        .with_circuit_breaker(CircuitBreaker::new(3));

    let failure_count = Arc::new(AtomicU32::new(0));

    // Cause 3 consecutive failures to trip the circuit breaker
    for i in 0..3 {
        let ctx = ExecutionContext::new("test_pipeline", format!("failing_node_{}", i))
            .with_input(Value::Null);

        let failure_clone = failure_count.clone();
        let result = scheduler
            .execute_node_with_retry(ctx, |_| async move {
                failure_clone.fetch_add(1, Ordering::SeqCst);
                Err(Error::execution("Simulated failure"))
            })
            .await;

        assert!(result.is_err());
    }

    // Circuit should now be open - next attempt should fail immediately
    let ctx = ExecutionContext::new("test_pipeline", "node_after_trip")
        .with_input(Value::Null);

    let result = scheduler
        .execute_node_with_retry(ctx, |_| async move { Ok(Value::from(42)) })
        .await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Circuit breaker is open"));
}

/// Test: Retry with exponential backoff timing
#[tokio::test]
async fn test_exponential_backoff_timing() {
    let policy = RetryPolicy::Exponential {
        base_delay: Duration::from_millis(50),
        max_delay: Duration::from_millis(500),
        max_attempts: 4,
        multiplier: 2.0,
    };

    // Verify delay calculation
    assert_eq!(policy.delay_for_attempt(0), Some(Duration::from_millis(50)));
    assert_eq!(policy.delay_for_attempt(1), Some(Duration::from_millis(100)));
    assert_eq!(policy.delay_for_attempt(2), Some(Duration::from_millis(200)));
    assert_eq!(policy.delay_for_attempt(3), Some(Duration::from_millis(400)));
    assert_eq!(policy.delay_for_attempt(4), None); // Exhausted
}

/// Test: Retry exhaustion after max attempts
#[tokio::test]
async fn test_retry_exhaustion() {
    let attempt_count = Arc::new(AtomicU32::new(0));
    let attempt_clone = attempt_count.clone();

    let result: Result<()> = execute_with_retry(
        RetryPolicy::fixed(2, Duration::from_millis(10)),
        || {
            let attempt = attempt_clone.clone();
            async move {
                attempt.fetch_add(1, Ordering::SeqCst);
                Err(Error::execution("Persistent failure"))
            }
        },
    )
    .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Retry exhausted"));
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3); // Initial + 2 retries
}

/// Test: Mixed retryable and non-retryable errors
#[tokio::test]
async fn test_mixed_error_types() {
    // Non-retryable errors
    assert!(!Error::manifest("test").is_retryable());
    assert!(!Error::Serialization(serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::Other,
        "test"
    )))
    .is_retryable());

    // Retryable errors
    assert!(Error::execution("test").is_retryable());
    assert!(Error::transport("test").is_retryable());
    assert!(Error::python_vm("test").is_retryable());
    assert!(Error::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "test"
    ))
    .is_retryable());
}

/// Test: Circuit breaker reset
#[tokio::test]
async fn test_circuit_breaker_reset() {
    let mut breaker = CircuitBreaker::new(3);

    // Trip the circuit
    for _ in 0..3 {
        breaker.record_failure();
    }
    assert!(breaker.is_open());

    // Manual reset
    breaker.reset();
    assert!(!breaker.is_open());
    assert_eq!(breaker.consecutive_failures(), 0);
}

/// Test: Default retry policy
#[tokio::test]
async fn test_default_retry_policy() {
    let policy = RetryPolicy::default();

    // Default should be 3 attempts with exponential backoff
    assert_eq!(policy.max_attempts(), 3);
    assert_eq!(policy.delay_for_attempt(0), Some(Duration::from_millis(100)));
    assert_eq!(policy.delay_for_attempt(1), Some(Duration::from_millis(200)));
    assert_eq!(policy.delay_for_attempt(2), Some(Duration::from_millis(400)));
}

