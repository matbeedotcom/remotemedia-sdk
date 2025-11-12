//! Docker executor implementation
//!
//! Main executor struct for Docker-based node execution with iceoryx2 IPC.

use crate::{Error, Result};
use super::config::DockerizedNodeConfiguration;
use super::container_manager::ContainerManager;

#[cfg(feature = "multiprocess")]
use super::ipc_bridge::ContainerIpcThread;

/// Docker executor for running Python nodes in isolated containers
pub struct DockerExecutor {
    /// Container manager for Docker operations
    container_manager: ContainerManager,

    /// Node configuration
    config: DockerizedNodeConfiguration,

    /// Session ID (set during initialization)
    session_id: Option<String>,

    /// Container ID (once created)
    container_id: Option<String>,

    /// Image cache for reusing built images
    #[cfg(feature = "docker-executor")]
    image_cache: std::sync::Arc<tokio::sync::Mutex<super::image_builder::ImageCache>>,

    /// Channel registry for iceoryx2 IPC
    #[cfg(feature = "multiprocess")]
    channel_registry: std::sync::Arc<crate::python::multiprocess::ipc_channel::ChannelRegistry>,

    /// IPC thread handle (once spawned)
    #[cfg(feature = "multiprocess")]
    ipc_thread: Option<ContainerIpcThread>,
}

impl DockerExecutor {
    /// Create new Docker executor for a node
    pub fn new(
        config: DockerizedNodeConfiguration,
        image_cache_path: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        let container_manager = ContainerManager::new()?;

        // Initialize image cache
        #[cfg(feature = "docker-executor")]
        let cache_path = image_cache_path.unwrap_or_else(|| {
            let mut path = std::env::temp_dir();
            path.push("remotemedia_image_cache.db");
            path
        });

        #[cfg(feature = "docker-executor")]
        let image_cache = super::image_builder::ImageCache::new(&cache_path)?;

        #[cfg(feature = "multiprocess")]
        let channel_registry =
            std::sync::Arc::new(crate::python::multiprocess::ipc_channel::ChannelRegistry::new());

        Ok(Self {
            container_manager,
            config,
            session_id: None,
            container_id: None,
            #[cfg(feature = "docker-executor")]
            image_cache: std::sync::Arc::new(tokio::sync::Mutex::new(image_cache)),
            #[cfg(feature = "multiprocess")]
            channel_registry,
            #[cfg(feature = "multiprocess")]
            ipc_thread: None,
        })
    }

    /// Initialize Docker node (T028)
    ///
    /// 1. Validates Docker daemon accessible (FR-008)
    /// 2. Builds or pulls Docker image
    /// 3. Creates container with iceoryx2 mounts
    /// 4. Starts container
    /// 5. Sets up IPC channels
    pub async fn initialize(&mut self, session_id: String) -> Result<()> {
        tracing::info!(
            "Initializing Docker executor for node '{}' in session '{}'",
            self.config.node_id,
            session_id
        );

        // 1. Validate configuration (FR-004, FR-013, FR-014)
        self.config.validate()?;

        // 2. Docker daemon accessibility already validated in ContainerManager::new() (FR-008)

        // 3. Build or pull Docker image
        #[cfg(all(feature = "docker-executor", feature = "multiprocess"))]
        let image_tag = {
            // Build image using bollard (will use cache if available)
            let docker = bollard::Docker::connect_with_local_defaults()
                .map_err(|e| Error::Execution(format!("Failed to connect to Docker: {}", e)))?;

            let mut cache = self.image_cache.lock().await;

            match super::image_builder::build_docker_image(
                &docker,
                &mut *cache,
                &self.config.config,
                &self.config.node_id,
            )
            .await
            {
                Ok(tag) => {
                    tracing::info!("Docker image ready: {}", tag);
                    tag
                }
                Err(e) => {
                    tracing::error!("Failed to build Docker image for node '{}': {}", self.config.node_id, e);
                    return Err(e);
                }
            }
        };

        #[cfg(not(all(feature = "docker-executor", feature = "multiprocess")))]
        let image_tag = format!("python:{}-slim", self.config.config.python_version);

        // 4. Check if container already exists in global registry (FR-012)
        #[cfg(feature = "docker-executor")]
        let (container_id, _container_name, _is_shared) = {
            use super::container_registry::{
                get_or_create_container, register_container, add_session_to_container,
                ContainerSessionInstance, HealthStatus,
            };

            if let Some(existing) = get_or_create_container(&self.config.node_id).await {
                tracing::info!(
                    "Reusing existing container '{}' for node '{}' (ref_count: {})",
                    existing.container_name,
                    self.config.node_id,
                    existing.ref_count() + 1
                );

                // Add this session to the existing container (FR-015)
                add_session_to_container(&self.config.node_id, session_id.clone())
                    .await?;

                (existing.container_id, existing.container_name, true)
            } else {
                // Create new container
                let container_name = format!("remotemedia_{}_{}", session_id, self.config.node_id);

                let container_id = self
                    .container_manager
                    .create_container(
                        &container_name,
                        &image_tag,
                        &self.config.config.resource_limits,
                        &self.config.config.env,
                    )
                    .await?;

                tracing::info!("Created new container '{}' for node '{}'", container_name, self.config.node_id);

                // Start container
                self.container_manager.start_container(&container_id).await?;

                // Register in global registry
                let mut instance = ContainerSessionInstance::new(
                    container_id.clone(),
                    container_name.clone(),
                    self.config.node_id.clone(),
                    image_tag.clone(),
                );

                instance.add_session(session_id.clone());
                instance.update_health(HealthStatus::Healthy);

                register_container(self.config.node_id.clone(), instance).await?;

                (container_id, container_name, false)
            }
        };

        #[cfg(not(feature = "docker-executor"))]
        let (container_id, _container_name, _is_shared) = {
            let container_name = format!("remotemedia_{}_{}", session_id, self.config.node_id);
            let container_id = self
                .container_manager
                .create_container(
                    &container_name,
                    &image_tag,
                    &self.config.config.resource_limits,
                    &self.config.config.env,
                )
                .await?;

            self.container_manager.start_container(&container_id).await?;
            (container_id, container_name, false)
        };

        self.container_id = Some(container_id.clone());

        // 5b. Start Python node runner inside container via docker exec
        // The runner will create iceoryx2 subscriber/publisher and listen for data
        #[cfg(feature = "multiprocess")]
        {
            tracing::info!(
                "Starting Python node runner in container for node '{}'",
                self.config.node_id
            );

            // Build command to run Python node runner
            // This matches how multiprocess executor spawns Python processes
            let cmd = vec![
                "python".to_string(),
                "-m".to_string(),
                "remotemedia.core.multiprocessing.runner".to_string(),
                "--node-type".to_string(),
                self.config.node_type.clone(),
                "--node-id".to_string(),
                self.config.node_id.clone(),
                "--session-id".to_string(),
                session_id.clone(),
            ];

            match self.container_manager.exec_in_container(&container_id, cmd).await {
                Ok(exec_id) => {
                    tracing::info!(
                        "✓ Python runner started in container (exec_id: {})",
                        exec_id
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to start Python runner in container: {}",
                        e
                    );
                    return Err(Error::Execution(format!(
                        "Python runner failed to start in container. \
                         Ensure remotemedia package is installed. \
                         Build container with: docker build -f docker/Dockerfile.remotemedia-node -t remotemedia/python-node:py3.10 . \
                         Error: {}",
                        e
                    )));
                }
            }
        }

        // 6. Setup IPC channels
        #[cfg(feature = "multiprocess")]
        {
            let ipc_thread = ContainerIpcThread::spawn(
                self.config.node_id.clone(),
                session_id.clone(),
                self.channel_registry.clone(),
            )?;

            self.ipc_thread = Some(ipc_thread);
        }

        self.session_id = Some(session_id);

        tracing::info!(
            "Docker executor initialized successfully for node '{}' (container: {})",
            self.config.node_id,
            container_id
        );

        Ok(())
    }

    /// Execute streaming node (T029)
    ///
    /// Sends data via IPC bridge, receives outputs, routes to session router
    #[cfg(feature = "multiprocess")]
    pub async fn execute_streaming(
        &self,
        input_data: crate::python::multiprocess::data_transfer::RuntimeData,
    ) -> Result<Vec<crate::python::multiprocess::data_transfer::RuntimeData>> {
        // Get IPC thread handle
        let ipc_thread = self.ipc_thread.as_ref().ok_or_else(|| {
            Error::Execution(format!(
                "Docker executor not initialized for node '{}'",
                self.config.node_id
            ))
        })?;

        // Send data via IPC bridge (fire-and-forget pattern)
        ipc_thread.send_data(input_data).await?;

        // Return empty Vec - outputs are routed asynchronously via output callback
        // This follows the multiprocess "fire-and-forget" pattern where outputs
        // are continuously drained by the IPC thread and forwarded to session router
        Ok(Vec::new())
    }

    /// Register output callback for continuous data forwarding
    #[cfg(feature = "multiprocess")]
    pub async fn register_output_callback(
        &self,
        callback_tx: tokio::sync::mpsc::UnboundedSender<crate::python::multiprocess::data_transfer::RuntimeData>,
    ) -> Result<()> {
        let ipc_thread = self.ipc_thread.as_ref().ok_or_else(|| {
            Error::Execution(format!(
                "Docker executor not initialized for node '{}'",
                self.config.node_id
            ))
        })?;

        ipc_thread.register_output_callback(callback_tx).await
    }

    /// Cleanup Docker node resources (T030, T039)
    ///
    /// Uses reference counting (FR-015) to determine if container should be stopped.
    /// Only stops/removes container when reference count reaches zero.
    pub async fn cleanup(&mut self) -> Result<()> {
        tracing::info!("Cleaning up Docker executor for node '{}'", self.config.node_id);

        // 1. Shutdown IPC thread
        #[cfg(feature = "multiprocess")]
        if let Some(ipc_thread) = self.ipc_thread.take() {
            tracing::debug!("Shutting down IPC thread for node '{}'", self.config.node_id);
            ipc_thread.shutdown().await?;
        }

        // 2. Remove session from container registry and check if should stop (FR-015)
        #[cfg(feature = "docker-executor")]
        let should_stop_container = if let Some(ref session_id) = self.session_id {
            use super::container_registry::remove_session_from_container;

            match remove_session_from_container(&self.config.node_id, session_id).await {
                Ok(should_stop) => {
                    if should_stop {
                        tracing::info!(
                            "Reference count reached zero for node '{}', will stop container",
                            self.config.node_id
                        );
                    } else {
                        tracing::debug!(
                            "Container for node '{}' still has active sessions, keeping alive",
                            self.config.node_id
                        );
                    }
                    should_stop
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to remove session from registry (container may have been already removed): {}",
                        e
                    );
                    true // Proceed with cleanup anyway
                }
            }
        } else {
            true // No session ID, stop container
        };

        #[cfg(not(feature = "docker-executor"))]
        let should_stop_container = true;

        // 3. Stop and remove container only if ref count reached zero
        if should_stop_container {
            if let Some(container_id) = &self.container_id {
                tracing::debug!("Stopping container: {}", container_id);
                self.container_manager.stop_container(container_id).await?;

                // 4. Remove container
                tracing::debug!("Removing container: {}", container_id);
                self.container_manager.remove_container(container_id).await?;
            }
        } else {
            tracing::debug!("Container kept alive for other sessions");
        }

        // 5. Cleanup iceoryx2 service files for this session
        if let (Some(session_id), Some(_)) = (&self.session_id, &self.container_id) {
            let service_pattern = format!(
                "/tmp/iceoryx2/services/{}_{}_*",
                session_id, self.config.node_id
            );

            tracing::debug!("Cleaning up iceoryx2 service files: {}", service_pattern);

            // Use glob to find and remove service files
            if let Ok(entries) = glob::glob(&service_pattern) {
                for entry in entries.flatten() {
                    if let Err(e) = std::fs::remove_file(&entry) {
                        tracing::warn!("Failed to remove iceoryx2 service file {:?}: {}", entry, e);
                    }
                }
            }
        }

        // 6. Clear handles
        self.container_id = None;
        self.session_id = None;

        tracing::info!("Docker executor cleanup complete for node '{}'", self.config.node_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::python::docker::config::*;

    #[test]
    fn test_docker_executor_creation() {
        let config = DockerizedNodeConfiguration::new_without_type(
            "test_node".to_string(),
            DockerExecutorConfig {
                python_version: "3.10".to_string(),
                system_dependencies: vec![],
                python_packages: vec!["iceoryx2".to_string()],
                resource_limits: ResourceLimits {
                    memory_mb: 1024,
                    cpu_cores: 1.0,
                    gpu_devices: vec![],
                },
                base_image: None,
                env: Default::default(),
            },
        );

        if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
            return;
        }

        let executor = DockerExecutor::new(config, None);
        match executor {
            Ok(_) => println!("Docker executor created (Docker daemon accessible)"),
            Err(e) => println!("Docker executor creation failed: {}", e),
        }
    }
}
