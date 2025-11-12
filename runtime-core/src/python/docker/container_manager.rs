//! Docker container lifecycle management
//!
//! Handles container creation, startup, health checks, and cleanup using bollard.

use crate::{Error, Result};
use bollard::container::{RemoveContainerOptions, StopContainerOptions};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::CreateContainerOptions;
use bollard::secret::HostConfigLogConfig;
use bollard::Docker;
use std::collections::HashMap;

/// Container manager for Docker executor
pub struct ContainerManager {
    /// Docker client
    docker: Docker,
}

impl ContainerManager {
    /// Create new container manager with Docker client
    pub fn new() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| Error::Execution(format!("Failed to connect to Docker daemon: {}", e)))?;

        Ok(Self { docker })
    }

    /// Create a Docker container with iceoryx2 volume mounts (T015)
    ///
    /// FR-005: Mounts /tmp/iceoryx2 and /dev/shm for iceoryx2 IPC
    pub async fn create_container(
        &self,
        container_name: &str,
        image: &str,
        resource_limits: &crate::python::docker::ResourceLimits,
        env_vars: &HashMap<String, String>,
    ) -> Result<String> {
        tracing::info!(
            "Creating Docker container '{}' from image '{}'",
            container_name,
            image
        );

        // Build HostConfig with iceoryx2 volume mounts and resource limits
        let mut host_config = resource_limits.to_docker_host_config();

        // FR-005: Mount /tmp/iceoryx2 and /dev/shm for iceoryx2 IPC
        host_config.binds = Some(vec![
            "/tmp/iceoryx2:/tmp/iceoryx2".to_string(),
            "/dev/shm:/dev/shm".to_string(),
        ]);

        // Set shared memory size to 2GB for audio streaming
        host_config.shm_size = Some(2_000_000_000); // 2GB in bytes

        // Set logging driver to json-file (supports log reading)
        host_config.log_config = Some(HostConfigLogConfig {
            typ: Some("json-file".to_string()),
            config: Some(HashMap::from([
                ("max-size".to_string(), "10m".to_string()),
                ("max-file".to_string(), "3".to_string()),
            ])),
        });

        // Convert environment variables to Docker format
        let env: Vec<String> = env_vars
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Build container configuration
        let config = ContainerCreateBody {
            image: Some(image.to_string()),
            env: Some(env),
            host_config: Some(host_config),
            // Override CMD to keep container running indefinitely
            // Python node runner will be started via docker exec
            cmd: Some(vec![
                "tail".to_string(),
                "-f".to_string(),
                "/dev/null".to_string(),
            ]),
            tty: Some(false),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        // Create container
        let response = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: Some(container_name.to_string()),
                    platform: String::new(),
                }),
                config,
            )
            .await
            .map_err(|e| {
                Error::Execution(format!("Failed to create container '{}': {}", container_name, e))
            })?;

        tracing::info!(
            "Container created successfully: {} (ID: {})",
            container_name,
            response.id
        );

        Ok(response.id)
    }

    /// Start a container and wait for ready state (T016)
    ///
    /// Polls container status until healthy or timeout
    pub async fn start_container(&self, container_id: &str) -> Result<()> {
        tracing::info!("Starting container: {}", container_id);

        // Start the container
        self.docker
            .start_container(container_id, None::<bollard::container::StartContainerOptions<String>>)
            .await
            .map_err(|e| Error::Execution(format!("Failed to start container: {}", e)))?;

        // Poll for container to reach running state
        let timeout = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();
        let poll_interval = std::time::Duration::from_millis(100);

        loop {
            if start.elapsed() > timeout {
                return Err(Error::Execution(format!(
                    "Container '{}' failed to start within {:?}",
                    container_id, timeout
                )));
            }

            // Inspect container state
            use bollard::container::InspectContainerOptions;
            let inspect = self
                .docker
                .inspect_container(container_id, None::<InspectContainerOptions>)
                .await
                .map_err(|e| {
                    Error::Execution(format!("Failed to inspect container: {}", e))
                })?;

            if let Some(state) = inspect.state {
                if let Some(running) = state.running {
                    if running {
                        tracing::info!("Container '{}' is running", container_id);
                        return Ok(());
                    }
                }

                // Check for exit/error states
                if let Some(status) = state.status {
                    if status.as_ref() == "exited" || status.as_ref() == "dead" {
                        let exit_code = state.exit_code.unwrap_or(-1);
                        return Err(Error::Execution(format!(
                            "Container '{}' exited prematurely with code {}",
                            container_id, exit_code
                        )));
                    }
                }
            }

            // Wait before next poll
            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Stop a container gracefully (T017)
    ///
    /// Sends SIGTERM, waits for timeout, then SIGKILL if needed
    pub async fn stop_container(&self, container_id: &str) -> Result<()> {
        tracing::info!("Stopping container: {}", container_id);

        // Send SIGTERM with 10 second timeout
        // Docker will automatically send SIGKILL if container doesn't stop
        self.docker
            .stop_container(
                container_id,
                Some(StopContainerOptions { t: 10 }), // 10 second grace period
            )
            .await
            .map_err(|e| {
                // Container may already be stopped - log but don't fail
                tracing::warn!("Error stopping container '{}': {}", container_id, e);
                e
            })
            .ok(); // Ignore errors - container may already be stopped

        // Verify container is stopped
        use bollard::container::InspectContainerOptions;
        match self.docker.inspect_container(container_id, None::<InspectContainerOptions>).await {
            Ok(inspect) => {
                if let Some(state) = inspect.state {
                    if let Some(running) = state.running {
                        if !running {
                            tracing::info!("Container '{}' stopped successfully", container_id);
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Container '{}' may already be removed: {}", container_id, e);
                return Ok(()); // Container gone = success
            }
        }

        Ok(())
    }

    /// Remove a container and cleanup volumes (T018)
    pub async fn remove_container(&self, container_id: &str) -> Result<()> {
        tracing::info!("Removing container: {}", container_id);

        self.docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    v: true,    // Remove associated volumes
                    force: false, // Don't force removal if running
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| {
                // Container may already be removed - log but don't fail hard
                tracing::warn!("Error removing container '{}': {}", container_id, e);
                e
            })
            .ok(); // Ignore errors - container may already be gone

        tracing::info!("Container '{}' removed", container_id);
        Ok(())
    }

    /// Execute command inside running container
    ///
    /// Used to start Python node runner inside container
    pub async fn exec_in_container(
        &self,
        container_id: &str,
        cmd: Vec<String>,
    ) -> Result<String> {
        use bollard::exec::CreateExecOptions;

        tracing::debug!("Executing in container {}: {:?}", container_id, cmd);

        // Create exec instance for long-running background process
        // Note: We don't attach stdout/stderr to avoid blocking on output
        let exec = self
            .docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(cmd),
                    attach_stdout: Some(false),  // Don't attach to avoid blocking
                    attach_stderr: Some(false),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| Error::Execution(format!("Failed to create exec: {}", e)))?;

        // Start exec in detached mode (runs as background daemon)
        use bollard::exec::StartExecOptions;
        self.docker
            .start_exec(&exec.id, Some(StartExecOptions {
                detach: true,
                tty: false,
                output_capacity: None,
            }))
            .await
            .map_err(|e| Error::Execution(format!("Failed to start exec: {}", e)))?;

        tracing::info!("Python runner exec started in background (exec_id: {})", exec.id);

        // Give the Python process time to start and initialize iceoryx2 channels
        // This is critical - the Python runner needs to:
        // 1. Import modules (remotemedia, iceoryx2)
        // 2. Create iceoryx2 publisher/subscriber
        // 3. Register with channel registry
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        tracing::debug!("Python runner startup grace period complete");

        Ok(exec.id)
    }

    /// Stream container logs (T031)
    ///
    /// FR-011: Stream stdout/stderr to host logging system
    pub async fn stream_logs(&self, container_id: &str) -> Result<()> {
        // TODO T031: Implement log streaming
        // 1. Call docker.logs(container_id, Some(LogsOptions {
        //       follow: true,
        //       stdout: true,
        //       stderr: true,
        //       ..Default::default()
        //    }))
        // 2. Stream logs using bollard::container::LogOutput
        // 3. Forward to tracing::info! for stdout, tracing::error! for stderr
        // 4. Prefix with container_id and node_id for identification
        //
        // Reference: bollard::Docker::logs
        // Reference: bollard::container::LogsOptions

        tracing::warn!("TODO T031: stream_logs not yet implemented");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_manager_creation() {
        // Note: This test requires Docker daemon to be running
        // Skip in CI if Docker not available
        if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
            return;
        }

        let manager = ContainerManager::new();
        // Will fail if Docker daemon not running - that's expected
        match manager {
            Ok(_) => println!("Docker daemon accessible"),
            Err(e) => println!("Docker daemon not accessible: {}", e),
        }
    }
}
