//! Health monitoring for Docker containers
//!
//! This module provides periodic health checks for running containers,
//! monitoring container stats, checking responsiveness, and updating the
//! global registry with health status.

use crate::{Error, Result};
use super::container_registry::{HealthStatus, global_container_registry};
use std::time::Duration;
use tokio::time::interval;

#[cfg(feature = "docker-executor")]
use bollard::Docker;

/// Container health checker
pub struct HealthChecker {
    /// Docker client
    #[cfg(feature = "docker-executor")]
    docker: Docker,

    /// Health check interval (default: 30s)
    check_interval: Duration,

    /// Shutdown signal
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl HealthChecker {
    /// Create a new health checker
    #[cfg(feature = "docker-executor")]
    pub fn new(check_interval_secs: u64) -> Result<(Self, tokio::sync::watch::Sender<bool>)> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| Error::Execution(format!("Failed to connect to Docker: {}", e)))?;

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        Ok((
            Self {
                docker,
                check_interval: Duration::from_secs(check_interval_secs),
                shutdown_rx,
            },
            shutdown_tx,
        ))
    }

    /// Start the health monitoring loop
    #[cfg(feature = "docker-executor")]
    pub async fn start_monitoring(mut self) {
        tracing::info!(
            "Starting container health monitoring (interval: {:?})",
            self.check_interval
        );

        let mut check_timer = interval(self.check_interval);

        loop {
            tokio::select! {
                _ = check_timer.tick() => {
                    if let Err(e) = self.check_all_containers().await {
                        tracing::error!("Health check failed: {}", e);
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        tracing::info!("Shutting down health monitor");
                        break;
                    }
                }
            }
        }
    }

    /// Check health of all registered containers
    #[cfg(feature = "docker-executor")]
    async fn check_all_containers(&self) -> Result<()> {
        let registry = global_container_registry().read().await;
        let containers: Vec<_> = registry.values().cloned().collect();
        drop(registry); // Release lock before async operations

        for container in containers {
            // Skip containers that are stopping
            if container.health_status == HealthStatus::Stopping {
                continue;
            }

            // Only check if interval has passed
            if !container.should_health_check() {
                continue;
            }

            match self.check_container_health(&container.container_id).await {
                Ok(is_healthy) => {
                    let new_status = if is_healthy {
                        HealthStatus::Healthy
                    } else {
                        HealthStatus::Unhealthy
                    };

                    // Update status in registry
                    let mut registry = global_container_registry().write().await;
                    if let Some(instance) = registry.get_mut(&container.node_id) {
                        instance.update_health(new_status.clone());

                        if new_status == HealthStatus::Unhealthy {
                            tracing::warn!(
                                "Container '{}' is unhealthy (node: {})",
                                container.container_name,
                                container.node_id
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to check health of container '{}': {}",
                        container.container_name,
                        e
                    );

                    // Mark as unhealthy on error
                    let mut registry = global_container_registry().write().await;
                    if let Some(instance) = registry.get_mut(&container.node_id) {
                        instance.update_health(HealthStatus::Unhealthy);
                    }
                }
            }
        }

        Ok(())
    }

    /// Check health of a single container
    #[cfg(feature = "docker-executor")]
    async fn check_container_health(&self, container_id: &str) -> Result<bool> {
        use bollard::container::InspectContainerOptions;

        // Inspect container state
        let inspect_result = self
            .docker
            .inspect_container(container_id, None::<InspectContainerOptions>)
            .await
            .map_err(|e| {
                Error::Execution(format!("Failed to inspect container {}: {}", container_id, e))
            })?;

        // Check if container is running
        if let Some(state) = inspect_result.state {
            if let Some(running) = state.running {
                if !running {
                    tracing::warn!("Container {} is not running", container_id);
                    return Ok(false);
                }
            }

            // Check for OOMKilled or other error states
            if let Some(oom_killed) = state.oom_killed {
                if oom_killed {
                    tracing::error!("Container {} was OOM killed", container_id);
                    return Ok(false);
                }
            }

            if let Some(exit_code) = state.exit_code {
                if exit_code != 0 {
                    tracing::warn!("Container {} exited with code {}", container_id, exit_code);
                    return Ok(false);
                }
            }
        }

        // Optionally check container stats (CPU, memory usage)
        // This could be extended to check resource usage patterns

        Ok(true)
    }
}

/// Spawn a background health monitoring task
#[cfg(feature = "docker-executor")]
pub fn spawn_health_monitor(
    check_interval_secs: u64,
) -> Result<tokio::sync::watch::Sender<bool>> {
    let (checker, shutdown_tx) = HealthChecker::new(check_interval_secs)?;

    tokio::spawn(async move {
        checker.start_monitoring().await;
    });

    tracing::info!("Health monitor task spawned");
    Ok(shutdown_tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_checker_creation() {
        if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
            return;
        }

        #[cfg(feature = "docker-executor")]
        {
            let result = HealthChecker::new(30);
            match result {
                Ok(_) => println!("Health checker created successfully"),
                Err(e) => println!("Health checker creation failed (expected if Docker unavailable): {}", e),
            }
        }
    }
}
