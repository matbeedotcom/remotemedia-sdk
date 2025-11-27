//! Automatic fallback logic

use crate::modes::ExecutionMode;
use std::time::Duration;

/// Fallback configuration
pub struct FallbackConfig {
    /// Maximum retries before falling back
    pub max_retries: u32,
    /// Timeout for remote operations
    pub timeout: Duration,
    /// Interval for health checks
    pub health_check_interval: Duration,
    /// Minimum time to stay in fallback mode
    pub min_fallback_duration: Duration,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            timeout: Duration::from_secs(5),
            health_check_interval: Duration::from_secs(30),
            min_fallback_duration: Duration::from_secs(60),
        }
    }
}

/// Check if remote server is available
pub async fn check_remote_health(server_url: &str, timeout: Duration) -> bool {
    // TODO: Implement actual health check
    // For now, return false to simulate offline state
    tracing::debug!("Checking remote health: {} (timeout: {:?})", server_url, timeout);
    false
}

/// Determine if fallback should be triggered
pub fn should_fallback(
    consecutive_failures: u32,
    config: &FallbackConfig,
) -> bool {
    consecutive_failures >= config.max_retries
}

/// Determine if reconnection should be attempted
pub fn should_attempt_reconnect(
    time_in_fallback: Duration,
    config: &FallbackConfig,
) -> bool {
    time_in_fallback >= config.min_fallback_duration
}

/// Get local fallback mode from current mode
pub fn get_fallback_mode(current: &ExecutionMode) -> ExecutionMode {
    match current {
        ExecutionMode::Hybrid { .. } | ExecutionMode::Remote { .. } => ExecutionMode::Local,
        ExecutionMode::Local => ExecutionMode::Local,
    }
}
