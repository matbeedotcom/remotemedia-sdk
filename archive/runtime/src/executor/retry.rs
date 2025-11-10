//! Retry policy and execution
//!
//! Implements exponential backoff and retry logic for failed operations.

use crate::executor::error::ExecutionErrorExt;
use crate::{Error, Result};
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// Retry policy for failed operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RetryPolicy {
    /// No retries
    None,

    /// Fixed number of retry attempts with constant delay
    Fixed {
        /// Number of retry attempts
        attempts: usize,
        /// Delay between retries
        delay: Duration,
    },

    /// Exponential backoff retries
    Exponential {
        /// Base delay for first retry
        base_delay: Duration,
        /// Maximum delay between retries
        max_delay: Duration,
        /// Maximum number of attempts
        max_attempts: usize,
        /// Backoff multiplier (typically 2.0)
        multiplier: f64,
    },
}

impl RetryPolicy {
    /// Create a fixed retry policy
    pub fn fixed(attempts: usize, delay: Duration) -> Self {
        RetryPolicy::Fixed { attempts, delay }
    }

    /// Create an exponential backoff policy
    pub fn exponential(max_attempts: usize) -> Self {
        RetryPolicy::Exponential {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            max_attempts,
            multiplier: 2.0,
        }
    }

    /// Get delay for a specific attempt number (0-indexed)
    pub fn delay_for_attempt(&self, attempt: usize) -> Option<Duration> {
        match self {
            RetryPolicy::None => None,
            RetryPolicy::Fixed { attempts, delay } => {
                if attempt < *attempts {
                    Some(*delay)
                } else {
                    None
                }
            }
            RetryPolicy::Exponential {
                base_delay,
                max_delay,
                max_attempts,
                multiplier,
            } => {
                if attempt >= *max_attempts {
                    return None;
                }

                let delay_ms = (base_delay.as_millis() as f64) * multiplier.powi(attempt as i32);
                let delay = Duration::from_millis(delay_ms as u64);

                Some(delay.min(*max_delay))
            }
        }
    }

    /// Get maximum number of attempts
    pub fn max_attempts(&self) -> usize {
        match self {
            RetryPolicy::None => 0,
            RetryPolicy::Fixed { attempts, .. } => *attempts,
            RetryPolicy::Exponential { max_attempts, .. } => *max_attempts,
        }
    }
}

impl Default for RetryPolicy {
    /// Default retry policy: 3 attempts with exponential backoff (100/200/400ms)
    fn default() -> Self {
        RetryPolicy::Exponential {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(400),
            max_attempts: 3,
            multiplier: 2.0,
        }
    }
}

/// Circuit breaker to prevent cascading failures
///
/// Tracks consecutive failures and "trips" (opens) after a threshold,
/// preventing further execution attempts until reset.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Maximum consecutive failures before tripping
    failure_threshold: usize,
    /// Current count of consecutive failures
    consecutive_failures: usize,
    /// Whether the circuit is currently open (tripped)
    is_open: bool,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given failure threshold
    pub fn new(failure_threshold: usize) -> Self {
        Self {
            failure_threshold,
            consecutive_failures: 0,
            is_open: false,
        }
    }

    /// Record a successful execution
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.is_open = false;
    }

    /// Record a failed execution
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.failure_threshold {
            self.is_open = true;
            tracing::warn!(
                "Circuit breaker tripped after {} consecutive failures",
                self.consecutive_failures
            );
        }
    }

    /// Check if the circuit is open (preventing execution)
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Reset the circuit breaker to closed state
    pub fn reset(&mut self) {
        self.consecutive_failures = 0;
        self.is_open = false;
        tracing::info!("Circuit breaker reset");
    }

    /// Get the current number of consecutive failures
    pub fn consecutive_failures(&self) -> usize {
        self.consecutive_failures
    }
}

impl Default for CircuitBreaker {
    /// Default circuit breaker: trips after 5 consecutive failures
    fn default() -> Self {
        Self::new(5)
    }
}

/// Execute a function with retry logic
pub async fn execute_with_retry<F, Fut, T>(policy: RetryPolicy, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let max_attempts = policy.max_attempts();

    if max_attempts == 0 {
        // No retry policy
        return operation().await;
    }

    let mut last_error: Option<Error> = None;
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                // Check if error is retryable
                if !err.is_retryable() {
                    return Err(err);
                }

                last_error = Some(err);
                attempt += 1;

                // Check if we should retry
                if let Some(delay) = policy.delay_for_attempt(attempt - 1) {
                    tracing::warn!(
                        "Operation failed (attempt {}/{}), retrying in {:?}",
                        attempt,
                        max_attempts,
                        delay
                    );
                    sleep(delay).await;
                } else {
                    // No more retries
                    break;
                }
            }
        }
    }

    // All retries exhausted
    Err(Error::execution(format!(
        "Retry exhausted after {} attempts: {}",
        attempt,
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "Unknown error".to_string())
    )))
}

/// Execute a synchronous function with retry logic
pub fn execute_with_retry_sync<F, T>(policy: RetryPolicy, mut operation: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let max_attempts = policy.max_attempts();

    if max_attempts == 0 {
        return operation();
    }

    let mut last_error: Option<Error> = None;
    let mut attempt = 0;

    loop {
        match operation() {
            Ok(result) => return Ok(result),
            Err(err) => {
                if !err.is_retryable() {
                    return Err(err);
                }

                last_error = Some(err);
                attempt += 1;

                if let Some(delay) = policy.delay_for_attempt(attempt - 1) {
                    tracing::warn!(
                        "Operation failed (attempt {}/{}), retrying in {:?}",
                        attempt,
                        max_attempts,
                        delay
                    );
                    std::thread::sleep(delay);
                } else {
                    break;
                }
            }
        }
    }

    Err(Error::execution(format!(
        "Retry exhausted after {} attempts: {}",
        attempt,
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "Unknown error".to_string())
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_policy() {
        let policy = RetryPolicy::fixed(3, Duration::from_millis(100));

        assert_eq!(
            policy.delay_for_attempt(0),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            policy.delay_for_attempt(1),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            policy.delay_for_attempt(2),
            Some(Duration::from_millis(100))
        );
        assert_eq!(policy.delay_for_attempt(3), None);
    }

    #[test]
    fn test_exponential_policy() {
        let policy = RetryPolicy::exponential(4);

        let delay0 = policy.delay_for_attempt(0).unwrap();
        let delay1 = policy.delay_for_attempt(1).unwrap();
        let delay2 = policy.delay_for_attempt(2).unwrap();

        assert!(delay1 > delay0);
        assert!(delay2 > delay1);
        assert_eq!(policy.delay_for_attempt(4), None);
    }

    #[tokio::test]
    async fn test_execute_with_retry_success() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result: Result<i32> =
            execute_with_retry(RetryPolicy::fixed(3, Duration::from_millis(10)), || {
                let attempt = attempt_clone.clone();
                async move {
                    let current = attempt.fetch_add(1, Ordering::SeqCst) + 1;
                    if current < 3 {
                        Err(Error::timeout("test"))
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_execute_with_retry_exhausted() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result: Result<()> =
            execute_with_retry(RetryPolicy::fixed(2, Duration::from_millis(10)), || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err(Error::timeout("test"))
                }
            })
            .await;

        assert!(result.unwrap_err().to_string().contains("Retry exhausted"));
        assert_eq!(attempt.load(Ordering::SeqCst), 3); // Initial + 2 retries
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result: Result<()> =
            execute_with_retry(RetryPolicy::fixed(3, Duration::from_millis(10)), || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err(Error::Manifest("test".into()))
                }
            })
            .await;

        assert!(matches!(result, Err(Error::Manifest(_))));
        assert_eq!(attempt.load(Ordering::SeqCst), 1); // Should not retry
    }
}
