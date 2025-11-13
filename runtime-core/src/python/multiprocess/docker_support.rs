//! Docker support for multiprocess executor
//!
//! Provides Docker container management functionality integrated with
//! the multiprocess executor system, including container lifecycle,
//! IPC volume mounting, health monitoring, and resource usage monitoring.
//!
//! # Resource Monitoring
//!
//! The module provides real-time resource usage monitoring via Docker's stats API:
//!
//! ```no_run
//! use remotemedia_runtime_core::python::multiprocess::docker_support::DockerSupport;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let docker_support = DockerSupport::new().await?;
//! let container_id = "my_container_id";
//!
//! // Monitor resource usage
//! let stats = docker_support.monitor_resource_usage(container_id).await?;
//! println!("CPU: {:.2}%", stats.cpu_percent);
//! println!("Memory: {} MB", stats.memory_mb);
//! if let Some(limit) = stats.memory_limit_mb {
//!     println!("Memory Limit: {} MB", limit);
//! }
//! # Ok(())
//! # }
//! ```

use crate::{Error, Result};
use bollard::Docker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

/// Configuration for Docker-based node execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerNodeConfig {
    /// Python version (e.g., "3.9", "3.10", "3.11")
    pub python_version: String,

    /// Optional custom base image
    pub base_image: Option<String>,

    /// System packages to install via apt-get
    #[serde(default)]
    pub system_packages: Vec<String>,

    /// Python packages to install via pip
    #[serde(default)]
    pub python_packages: Vec<String>,

    /// Memory limit in megabytes
    pub memory_mb: u64,

    /// CPU cores allocation (e.g., 2.5)
    pub cpu_cores: f32,

    /// GPU device IDs or ["all"]
    #[serde(default)]
    pub gpu_devices: Vec<String>,

    /// Shared memory size in megabytes (default: 2048)
    #[serde(default = "default_shm_size")]
    pub shm_size_mb: u64,

    /// Environment variables
    #[serde(default)]
    pub env_vars: HashMap<String, String>,

    /// Additional volume mounts
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
}

/// Volume mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Host filesystem path
    pub host_path: std::path::PathBuf,

    /// Container mount point
    pub container_path: std::path::PathBuf,

    /// Whether mount is read-only
    #[serde(default)]
    pub read_only: bool,
}

fn default_shm_size() -> u64 {
    2048 // 2GB default for shared memory
}

/// Resource usage statistics from Docker container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsageStats {
    /// CPU usage as percentage (0-100% per core, can exceed 100% for multi-core)
    pub cpu_percent: f32,

    /// Current memory usage in megabytes
    pub memory_mb: u64,

    /// Memory limit in megabytes (if set)
    pub memory_limit_mb: Option<u64>,
}

impl DockerNodeConfig {
    /// Compute a SHA256 hash of the configuration for caching and image tagging
    ///
    /// Generates a deterministic hash that includes all configuration details:
    /// - python_version: Base Python version (e.g., "3.10")
    /// - base_image: Custom base image if specified
    /// - system_packages: System-level dependencies (sorted for determinism)
    /// - python_packages: Python package dependencies (sorted for determinism)
    /// - memory_mb: Memory limit in megabytes
    /// - cpu_cores: CPU allocation in cores
    /// - shm_size_mb: Shared memory size for IPC
    ///
    /// The hash is deterministic: the same configuration will always produce
    /// the same hash, enabling:
    /// - Image caching and reuse
    /// - Consistent image tagging
    /// - Configuration change detection
    ///
    /// # Returns
    /// A 64-character hexadecimal SHA256 hash string
    pub fn compute_config_hash(&self) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();

        // Hash Python version - fundamental to environment
        hasher.update(self.python_version.as_bytes());

        // Hash base image if specified
        if let Some(base) = &self.base_image {
            hasher.update(base.as_bytes());
        }

        // Sort system packages for deterministic hashing
        let mut sys_pkgs = self.system_packages.clone();
        sys_pkgs.sort();
        for pkg in &sys_pkgs {
            hasher.update(pkg.as_bytes());
            hasher.update(b"\0"); // Null separator between packages
        }

        // Sort Python packages for deterministic hashing
        let mut py_pkgs = self.python_packages.clone();
        py_pkgs.sort();
        for pkg in &py_pkgs {
            hasher.update(pkg.as_bytes());
            hasher.update(b"\0"); // Null separator between packages
        }

        // Hash resource limits as little-endian bytes for consistency
        hasher.update(&self.memory_mb.to_le_bytes());
        hasher.update(&self.cpu_cores.to_le_bytes());
        hasher.update(&self.shm_size_mb.to_le_bytes());

        // Finalize and return as hexadecimal string
        format!("{:x}", hasher.finalize())
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate Python version
        const SUPPORTED_VERSIONS: &[&str] = &["3.9", "3.10", "3.11"];
        if !SUPPORTED_VERSIONS.contains(&self.python_version.as_str()) {
            return Err(Error::Execution(format!(
                "Unsupported Python version: {}. Supported: {:?}",
                self.python_version, SUPPORTED_VERSIONS
            )));
        }

        // Validate memory limits
        if self.memory_mb < 512 {
            return Err(Error::Execution(format!(
                "Memory limit too low: {}MB. Minimum: 512MB",
                self.memory_mb
            )));
        }

        // Validate CPU cores
        if self.cpu_cores < 0.1 {
            return Err(Error::Execution(format!(
                "CPU cores too low: {}. Minimum: 0.1",
                self.cpu_cores
            )));
        }

        // Validate shared memory size
        if self.shm_size_mb < 64 {
            return Err(Error::Execution(format!(
                "Shared memory too low: {}MB. Minimum: 64MB",
                self.shm_size_mb
            )));
        }

        Ok(())
    }
}

/// Docker support module for multiprocess executor
pub struct DockerSupport {
    docker: Arc<Docker>,
}

impl DockerSupport {
    /// Create a new Docker support instance with logging
    #[instrument(skip_all)]
    pub async fn new() -> Result<Self> {
        info!("Initializing Docker support");

        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => {
                info!("Successfully connected to Docker daemon");
                d
            }
            Err(e) => {
                error!("Failed to connect to Docker daemon: {}", e);
                return Err(Error::Execution(format!("Failed to connect to Docker: {}", e)));
            }
        };

        // Verify Docker is responsive
        match docker.version().await {
            Ok(version) => {
                info!(
                    "Docker daemon version: {}, API version: {}",
                    version.version.unwrap_or_else(|| "unknown".to_string()),
                    version.api_version.unwrap_or_else(|| "unknown".to_string())
                );
            }
            Err(e) => {
                warn!("Could not retrieve Docker version info: {}", e);
            }
        }

        Ok(Self {
            docker: Arc::new(docker),
        })
    }

    /// Log Docker container events
    #[instrument(skip(self))]
    pub async fn log_container_event(&self, container_id: &str, event: &str, details: &str) {
        info!(
            container_id = %container_id,
            event = %event,
            details = %details,
            "Docker container event"
        );
    }

    /// Log Docker image operations
    #[instrument(skip(self))]
    pub fn log_image_operation(&self, operation: &str, image: &str, status: &str) {
        debug!(
            operation = %operation,
            image = %image,
            status = %status,
            "Docker image operation"
        );
    }

    /// Check Docker daemon availability
    #[instrument(skip(self))]
    pub async fn check_availability(&self) -> bool {
        match self.docker.ping().await {
            Ok(_) => {
                debug!("Docker daemon is available");
                true
            }
            Err(e) => {
                warn!("Docker daemon not responding: {}", e);
                false
            }
        }
    }

    /// Check if Docker daemon is running and ready for operations
    ///
    /// This is a quick pre-flight check before attempting container operations.
    /// It provides diagnostic information if the daemon is not available.
    ///
    /// Returns `Ok(())` if Docker is responsive, or a detailed error with suggestions.
    #[instrument(skip(self))]
    pub async fn check_daemon_ready(&self) -> Result<()> {
        debug!("Performing pre-flight Docker daemon check");

        // Try a simple ping first
        match self.docker.ping().await {
            Ok(_) => {
                debug!("Docker daemon is responsive and ready");
                Ok(())
            }
            Err(e) => {
                let error_msg = e.to_string();
                error!(
                    error = %error_msg,
                    "Docker daemon is not responding to ping"
                );

                // Provide specific guidance based on error type
                let suggestion = if error_msg.contains("permission denied") {
                    "Docker daemon permission denied. Ensure your user is in the docker group: \
                     'sudo usermod -aG docker $USER' (Linux), or try with 'sudo' (macOS)."
                        .to_string()
                } else if error_msg.contains("Cannot connect") || error_msg.contains("connection refused") {
                    "Docker daemon is not running. Start Docker:\n  \
                     - macOS/Windows: Open Docker Desktop\n  \
                     - Linux: Run 'sudo systemctl start docker'"
                        .to_string()
                } else if error_msg.contains("not found") {
                    "Docker is not installed on this system. Please install Docker first.".to_string()
                } else {
                    format!(
                        "Docker daemon is unreachable. Verify Docker is installed and running. \
                         Original error: {}",
                        error_msg
                    )
                };

                warn!("Docker daemon check failed: {}", suggestion);
                Err(Error::Execution(format!(
                    "Docker daemon is not ready: {}", suggestion
                )))
            }
        }
    }

    /// Validate Docker availability and compatibility
    ///
    /// Performs comprehensive checks to ensure:
    /// - Docker daemon is reachable and responsive
    /// - API version is compatible with this executor
    /// - Docker is properly configured for container operations
    ///
    /// Returns `Ok(())` if Docker is available and compatible, or a detailed error message.
    /// Includes fallback logic to continue with warnings for non-critical failures.
    #[instrument(skip(self))]
    pub async fn validate_docker_availability(&self) -> Result<()> {
        info!("Validating Docker availability and compatibility");

        // Step 1: Ping the Docker daemon - this is critical
        match self.docker.ping().await {
            Ok(_) => {
                debug!("Docker daemon ping successful");
                info!("Docker daemon is responding");
            }
            Err(e) => {
                let error_msg = e.to_string();
                error!("Docker daemon not responding to ping: {}", error_msg);

                // Provide specific error messaging based on the type of failure
                let error_detail = if error_msg.contains("permission denied") {
                    "Permission denied when accessing Docker daemon. \
                     Fix: Add your user to the docker group with 'sudo usermod -aG docker $USER' \
                     or run with 'sudo', then restart Docker."
                } else if error_msg.contains("Cannot connect") || error_msg.contains("connection refused") {
                    "Cannot connect to Docker daemon - it may not be running. \
                     Fix: Start Docker (macOS/Windows: Open Docker Desktop, Linux: sudo systemctl start docker)"
                } else if error_msg.contains("not found") {
                    "Docker is not installed. \
                     Fix: Install Docker from https://docs.docker.com/get-docker/"
                } else {
                    "Docker daemon is unreachable for an unknown reason"
                };

                return Err(Error::Execution(format!(
                    "Docker daemon is not reachable. {}. Original error: {}",
                    error_detail, error_msg
                )));
            }
        }

        // Step 2: Get version information and check API compatibility
        let mut version_valid = true;
        match self.docker.version().await {
            Ok(version) => {
                let docker_version = version.version
                    .unwrap_or_else(|| "unknown".to_string());
                let api_version = version.api_version
                    .unwrap_or_else(|| "unknown".to_string());

                info!(
                    docker_version = %docker_version,
                    api_version = %api_version,
                    "Docker daemon version information retrieved"
                );

                // Log OS/Arch information if available
                if let Some(os) = version.os {
                    if let Some(arch) = version.arch {
                        info!(
                            os = %os,
                            arch = %arch,
                            "Docker system information"
                        );
                    }
                }

                // Validate minimum API version (Docker API 1.40+ required for modern features)
                // This corresponds to Docker 19.03+
                match parse_api_version(&api_version) {
                    Ok(api_ver) => {
                        if api_ver < (1, 40) {
                            warn!(
                                api_version = %api_version,
                                minimum_required = "1.40",
                                "Docker API version is older than recommended. \
                                 Some features may not work correctly. \
                                 Upgrade Docker to 19.03 or later if possible."
                            );
                            // Continue with fallback - older versions may still work
                        } else {
                            debug!(
                                "Docker API version {} is compatible",
                                api_version
                            );
                        }
                    }
                    Err(parse_err) => {
                        warn!(
                            api_version = %api_version,
                            error = %parse_err,
                            "Could not parse Docker API version, proceeding with caution"
                        );
                        version_valid = false;
                        // Continue with fallback
                    }
                }

                debug!(
                    "Docker version validation completed: {} (API {})",
                    docker_version, api_version
                );
            }
            Err(e) => {
                error!("Failed to retrieve Docker version information: {}", e);
                warn!(
                    "Proceeding without version validation. \
                     Docker may be misconfigured. Error: {}",
                    e
                );
                version_valid = false;
                // Fallback: Continue to next validation step
            }
        }

        // Step 3: Get Docker info to verify daemon is fully operational
        let mut info_valid = true;
        match self.docker.info().await {
            Ok(info) => {
                let containers = info.containers.unwrap_or(0);
                let images = info.images.unwrap_or(0);
                let driver = info.driver.unwrap_or_else(|| "unknown".to_string());

                info!(
                    containers = containers,
                    images = images,
                    driver = %driver,
                    "Docker daemon operational status"
                );

                // Check if Docker has proper storage driver
                if driver == "unknown" {
                    warn!("Docker storage driver could not be determined. \
                           Docker may not be fully initialized. \
                           Check: docker info to verify storage configuration");
                    info_valid = false;
                    // Fallback: Continue with warning
                } else {
                    debug!(
                        "Docker is using {} storage driver",
                        driver
                    );
                }

                debug!(
                    "Docker info validation completed: {} containers, {} images",
                    containers, images
                );
            }
            Err(e) => {
                error!("Failed to retrieve Docker daemon info: {}", e);
                warn!(
                    "Could not retrieve Docker system information, proceeding with caution. \
                     Docker daemon may not be fully operational. Error: {}",
                    e
                );
                info_valid = false;
                // Fallback: Continue anyway since daemon responded to ping
            }
        }

        // Summary of validation results
        if version_valid && info_valid {
            info!("Docker availability validation successful - all checks passed");
            Ok(())
        } else {
            warn!(
                "Docker availability validation completed with warnings. \
                 version_valid={}, info_valid={}. \
                 Docker is available but may have reduced functionality.",
                version_valid, info_valid
            );
            Ok(())
        }
    }

    /// Get Docker daemon info with logging
    #[instrument(skip(self))]
    pub async fn get_docker_info(&self) -> Result<String> {
        match self.docker.info().await {
            Ok(info) => {
                let summary = format!(
                    "Docker: {} containers, {} images, driver: {}",
                    info.containers.unwrap_or(0),
                    info.images.unwrap_or(0),
                    info.driver.unwrap_or_else(|| "unknown".to_string())
                );
                info!("{}", summary);
                Ok(summary)
            }
            Err(e) => {
                error!("Failed to get Docker info: {}", e);
                Err(Error::Execution(format!("Failed to get Docker info: {}", e)))
            }
        }
    }

    /// Verify Docker is ready for container operations
    ///
    /// Performs a quick health check before attempting to create/start containers.
    /// This is useful as a pre-flight check before container operations.
    ///
    /// Returns `Ok(())` if Docker is ready, with detailed error messages if not.
    #[instrument(skip(self))]
    pub async fn verify_container_operations_ready(&self) -> Result<()> {
        debug!("Verifying Docker is ready for container operations");

        // First, ensure daemon is responding
        if !self.check_availability().await {
            return Err(Error::Execution(
                "Docker daemon is not responding. \
                 Cannot perform container operations. \
                 Ensure Docker daemon is running."
                    .to_string()
            ));
        }

        // Try to list containers as a functional test
        let options = bollard::query_parameters::ListContainersOptions::default();
        match self.docker.list_containers(Some(options)).await {
            Ok(_) => {
                debug!("Successfully listed containers - Docker is ready for operations");
                info!("Docker is ready for container operations");
                Ok(())
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to list containers - Docker may not be fully operational"
                );

                let error_msg = e.to_string();
                let detail = if error_msg.contains("permission denied") {
                    "Permission denied. Check Docker daemon socket permissions."
                } else if error_msg.contains("refused") {
                    "Connection refused. Docker daemon may not be fully initialized."
                } else {
                    "Unknown error preventing container operations"
                };

                Err(Error::Execution(format!(
                    "Docker is not ready for container operations: {}. \
                     Error: {}",
                    detail, error_msg
                )))
            }
        }
    }

    /// Log Docker readiness status
    ///
    /// Logs the current state of Docker availability and provides operational status.
    /// Useful for diagnostics and monitoring.
    #[instrument(skip(self))]
    pub async fn log_docker_status(&self) {
        info!("Logging Docker operational status");

        // Check daemon availability
        match self.docker.ping().await {
            Ok(_) => {
                info!("Docker daemon status: RESPONSIVE");
            }
            Err(e) => {
                warn!("Docker daemon status: NOT RESPONDING - {}", e);
                return; // No point checking further
            }
        }

        // Get version info
        match self.docker.version().await {
            Ok(version) => {
                let docker_ver = version.version
                    .unwrap_or_else(|| "unknown".to_string());
                let api_ver = version.api_version
                    .unwrap_or_else(|| "unknown".to_string());
                info!(
                    docker_version = %docker_ver,
                    api_version = %api_ver,
                    "Docker version information"
                );
            }
            Err(e) => {
                warn!("Could not retrieve Docker version: {}", e);
            }
        }

        // Get system info
        match self.docker.info().await {
            Ok(info) => {
                let containers = info.containers.unwrap_or(0);
                let images = info.images.unwrap_or(0);
                let driver = info.driver.unwrap_or_else(|| "unknown".to_string());
                let mem_total = info.mem_total.unwrap_or(0);

                info!(
                    containers = containers,
                    images = images,
                    storage_driver = %driver,
                    memory_total_bytes = mem_total,
                    "Docker system information"
                );
            }
            Err(e) => {
                warn!("Could not retrieve Docker system info: {}", e);
            }
        }

        info!("Docker status logging completed");
    }

    /// Get the Docker client reference
    pub fn docker_client(&self) -> &Arc<Docker> {
        &self.docker
    }

    /// Create a Docker container for a Python node
    #[instrument(skip(self, config))]
    pub async fn create_container(
        &self,
        node_id: &str,
        session_id: &str,
        config: &DockerNodeConfig,
    ) -> Result<String> {
        use bollard::container::{Config, CreateContainerOptions};
        use bollard::models::{HostConfig, DeviceRequest};

        info!(
            node_id = %node_id,
            session_id = %session_id,
            "Creating Docker container for node"
        );

        // Container name must be unique
        let container_name = format!("{}_{}", session_id, node_id);

        // Prepare host configuration with resource limits
        let mut host_config = HostConfig {
            memory: Some(config.memory_mb as i64 * 1_048_576), // Convert MB to bytes
            nano_cpus: Some((config.cpu_cores * 1_000_000_000.0) as i64), // Convert cores to nano CPUs
            shm_size: Some(config.shm_size_mb as i64 * 1_048_576), // Shared memory size
            ..Default::default()
        };

        // T014: IPC volume mounting for iceoryx2
        // According to iceoryx2 documentation, we need to mount /tmp and /dev for IPC
        let mut binds = Vec::new();

        // Mount /tmp for Unix domain sockets, file locks, and iceoryx2 service files
        binds.push("/tmp:/tmp".to_string());
        debug!("Added /tmp volume mount for iceoryx2 IPC");

        // Mount /dev for shared memory and semaphores
        binds.push("/dev:/dev".to_string());
        debug!("Added /dev volume mount for iceoryx2 shared memory");

        // Add custom volume mounts from config
        for volume in &config.volumes {
            let mount_str = if volume.read_only {
                format!("{}:{}:ro", volume.host_path.display(), volume.container_path.display())
            } else {
                format!("{}:{}", volume.host_path.display(), volume.container_path.display())
            };
            binds.push(mount_str.clone());
            debug!(
                host_path = %volume.host_path.display(),
                container_path = %volume.container_path.display(),
                read_only = volume.read_only,
                "Added custom volume mount"
            );
        }

        host_config.binds = Some(binds);
        info!("Configured IPC volume mounts for iceoryx2: /tmp and /dev");

        // T037: GPU device passthrough for NVIDIA
        if !config.gpu_devices.is_empty() {
            debug!(
                gpu_devices = ?config.gpu_devices,
                "Configuring GPU device passthrough for NVIDIA"
            );

            // Check if "all" is specified or specific device IDs
            let is_all_devices = config.gpu_devices.contains(&"all".to_string());
            let device_ids = if is_all_devices {
                // Request all available GPUs
                None
            } else {
                // Request specific GPU device IDs
                Some(config.gpu_devices.clone())
            };

            let device_request = DeviceRequest {
                driver: Some("nvidia".to_string()),
                count: if is_all_devices { Some(-1) } else { None }, // -1 means all devices
                device_ids,
                capabilities: Some(vec![vec!["gpu".to_string()]]),
                options: None,
            };

            host_config.device_requests = Some(vec![device_request]);

            info!(
                gpu_count = if is_all_devices { "all".to_string() } else { config.gpu_devices.len().to_string() },
                "Configured NVIDIA GPU device passthrough"
            );
        } else {
            debug!("No GPU devices requested, skipping GPU configuration");
        }

        // Prepare container configuration
        let default_image = format!("python:{}", config.python_version);
        let image = config.base_image.as_ref()
            .unwrap_or(&default_image);

        let mut env = Vec::new();
        env.push(format!("NODE_ID={}", node_id));
        env.push(format!("SESSION_ID={}", session_id));
        env.push("PYTHONUNBUFFERED=1".to_string());

        // Add custom environment variables
        for (key, value) in &config.env_vars {
            env.push(format!("{}={}", key, value));
        }

        debug!(
            image = %image,
            env_vars = env.len(),
            memory_mb = config.memory_mb,
            cpu_cores = config.cpu_cores,
            "Container configuration prepared"
        );

        // Add labels for container tracking
        let mut labels = HashMap::new();
        labels.insert("remotemedia.node_id".to_string(), node_id.to_string());
        labels.insert("remotemedia.session_id".to_string(), session_id.to_string());

        let container_config = Config {
            image: Some(image.clone()),
            env: Some(env),
            host_config: Some(host_config),
            labels: Some(labels),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: container_name.clone(),
            platform: None,
        };

        // Create the container
        match self.docker.create_container(Some(options), container_config).await {
            Ok(response) => {
                info!(
                    container_id = %response.id,
                    container_name = %container_name,
                    "Container created successfully"
                );
                self.log_container_event(&response.id, "created", &format!("Node: {}", node_id)).await;
                Ok(response.id)
            }
            Err(e) => {
                error!(
                    container_name = %container_name,
                    error = %e,
                    "Failed to create container"
                );
                Err(Error::Execution(format!("Failed to create container: {}", e)))
            }
        }
    }

    /// Start a Docker container
    #[instrument(skip(self))]
    pub async fn start_container(&self, container_id: &str) -> Result<()> {
        info!(container_id = %container_id, "Starting container");

        self.docker
            .start_container(container_id, None::<bollard::query_parameters::StartContainerOptions>)
            .await
            .map_err(|e| {
                error!(
                    container_id = %container_id,
                    error = %e,
                    "Failed to start container"
                );
                Error::Execution(format!("Failed to start container: {}", e))
            })?;

        info!(container_id = %container_id, "Container started successfully");
        self.log_container_event(container_id, "started", "Container is now running").await;
        Ok(())
    }

    /// Stop a Docker container
    #[instrument(skip(self))]
    pub async fn stop_container(&self, container_id: &str, timeout: std::time::Duration) -> Result<()> {
        info!(
            container_id = %container_id,
            timeout = ?timeout,
            "Stopping container"
        );

        let options = bollard::query_parameters::StopContainerOptions {
            t: Some(timeout.as_secs() as i32),
            signal: None,
        };

        match self.docker.stop_container(container_id, Some(options)).await {
            Ok(_) => {
                info!(container_id = %container_id, "Container stopped successfully");
                self.log_container_event(container_id, "stopped", "Container stopped successfully").await;
                Ok(())
            }
            Err(e) => {
                error!(
                    container_id = %container_id,
                    error = %e,
                    "Failed to stop container"
                );
                Err(Error::Execution(format!("Failed to stop container: {}", e)))
            }
        }
    }

    /// Remove a Docker container
    #[instrument(skip(self))]
    pub async fn remove_container(&self, container_id: &str, force: bool) -> Result<()> {
        info!(
            container_id = %container_id,
            force = force,
            "Removing container"
        );

        let options = bollard::query_parameters::RemoveContainerOptions {
            force,
            v: true, // Remove volumes associated with the container
            link: false,
        };

        match self.docker.remove_container(container_id, Some(options)).await {
            Ok(_) => {
                info!(container_id = %container_id, "Container removed successfully");
                self.log_container_event(container_id, "removed", "Container removed from system").await;
                Ok(())
            }
            Err(e) => {
                error!(
                    container_id = %container_id,
                    error = %e,
                    "Failed to remove container"
                );
                Err(Error::Execution(format!("Failed to remove container: {}", e)))
            }
        }
    }

    /// Restart a Docker container
    #[instrument(skip(self))]
    pub async fn restart_container(&self, container_id: &str, timeout: std::time::Duration) -> Result<()> {
        info!(
            container_id = %container_id,
            timeout = ?timeout,
            "Restarting container"
        );

        let options = bollard::query_parameters::RestartContainerOptions {
            t: Some(timeout.as_secs() as i32),
            signal: None,
        };

        match self.docker.restart_container(container_id, Some(options)).await {
            Ok(_) => {
                info!(container_id = %container_id, "Container restarted successfully");
                self.log_container_event(container_id, "restarted", "Container restarted successfully").await;
                Ok(())
            }
            Err(e) => {
                error!(
                    container_id = %container_id,
                    error = %e,
                    "Failed to restart container"
                );
                Err(Error::Execution(format!("Failed to restart container: {}", e)))
            }
        }
    }

    /// Check if a container is running
    #[instrument(skip(self))]
    pub async fn is_container_running(&self, container_id: &str) -> Result<bool> {
        debug!(container_id = %container_id, "Checking if container is running");

        match self.docker.inspect_container(container_id, None::<bollard::query_parameters::InspectContainerOptions>).await {
            Ok(info) => {
                let running = info.state
                    .and_then(|s| s.running)
                    .unwrap_or(false);

                debug!(
                    container_id = %container_id,
                    running = running,
                    "Container running status retrieved"
                );
                Ok(running)
            }
            Err(e) => {
                warn!(
                    container_id = %container_id,
                    error = %e,
                    "Failed to inspect container, assuming not running"
                );
                Ok(false) // Assume not running if we can't inspect
            }
        }
    }

    /// Get container logs
    #[instrument(skip(self))]
    pub async fn get_container_logs(&self, container_id: &str, tail: Option<usize>) -> Result<String> {
        use bollard::container::LogsOptions;
        use futures::StreamExt;

        debug!(
            container_id = %container_id,
            tail = ?tail,
            "Retrieving container logs"
        );

        let options = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: tail.map(|n| n.to_string()).unwrap_or_else(|| "all".to_string()),
            follow: false,
            ..Default::default()
        };

        let mut logs_stream = self.docker.logs(container_id, Some(options));
        let mut logs = String::new();

        while let Some(log_result) = logs_stream.next().await {
            match log_result {
                Ok(log_output) => {
                    logs.push_str(&log_output.to_string());
                }
                Err(e) => {
                    warn!(
                        container_id = %container_id,
                        error = %e,
                        "Error reading logs from container"
                    );
                }
            }
        }

        debug!(
            container_id = %container_id,
            log_size = logs.len(),
            "Container logs retrieved"
        );
        Ok(logs)
    }

    /// Monitor resource usage of a Docker container
    ///
    /// Queries the Docker stats API to retrieve real-time resource usage metrics
    /// for a running container. This includes CPU percentage, memory usage, and
    /// memory limits.
    ///
    /// # Arguments
    ///
    /// * `container_id` - The ID or name of the container to monitor
    ///
    /// # Returns
    ///
    /// Returns `ResourceUsageStats` containing:
    /// - `cpu_percent`: CPU usage as percentage (can exceed 100% for multi-core)
    /// - `memory_mb`: Current memory usage in megabytes
    /// - `memory_limit_mb`: Memory limit in megabytes (if configured)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Container does not exist
    /// - Container is stopped or not running
    /// - Failed to retrieve stats from Docker daemon
    #[instrument(skip(self))]
    pub async fn monitor_resource_usage(&self, container_id: &str) -> Result<ResourceUsageStats> {
        use futures::StreamExt;

        debug!(
            container_id = %container_id,
            "Monitoring container resource usage"
        );

        // First verify the container is running
        let is_running = self.is_container_running(container_id).await?;
        if !is_running {
            warn!(
                container_id = %container_id,
                "Cannot monitor resource usage: container is not running"
            );
            return Err(Error::Execution(format!(
                "Container {} is not running, cannot retrieve stats",
                container_id
            )));
        }

        // Use stats API with one-shot mode (don't stream continuously)
        let options = Some(
            bollard::query_parameters::StatsOptionsBuilder::new()
                .stream(false)
                .one_shot(true)
                .build(),
        );

        let mut stats_stream = self.docker.stats(container_id, options);

        // Get the first (and only) stats result
        if let Some(stats_result) = stats_stream.next().await {
            let stats = stats_result.map_err(|e| {
                error!(
                    container_id = %container_id,
                    error = %e,
                    "Failed to retrieve container stats"
                );
                Error::Execution(format!("Failed to get container stats: {}", e))
            })?;

            // Extract memory usage and limit from MemoryStats
            let (memory_usage, memory_limit) = if let Some(memory_stats) = stats.memory_stats.as_ref() {
                let usage = memory_stats.usage.unwrap_or(0);
                let limit = memory_stats.limit;
                (usage, limit)
            } else {
                warn!(
                    container_id = %container_id,
                    "Memory stats not available"
                );
                (0, None)
            };

            // Calculate CPU usage percentage
            let cpu_percent = if let (Some(cpu_stats), Some(precpu_stats)) =
                (stats.cpu_stats.as_ref(), stats.precpu_stats.as_ref())
            {
                // Get total CPU usage
                let total_usage = cpu_stats
                    .cpu_usage
                    .as_ref()
                    .and_then(|u| u.total_usage)
                    .unwrap_or(0);
                let prev_total_usage = precpu_stats
                    .cpu_usage
                    .as_ref()
                    .and_then(|u| u.total_usage)
                    .unwrap_or(0);

                let cpu_delta = total_usage.saturating_sub(prev_total_usage);

                // Get system CPU usage
                let system_usage = cpu_stats.system_cpu_usage.unwrap_or(0);
                let prev_system_usage = precpu_stats.system_cpu_usage.unwrap_or(0);
                let system_delta = system_usage.saturating_sub(prev_system_usage);

                // Calculate percentage based on number of CPU cores
                if system_delta > 0 && cpu_delta > 0 {
                    let cpu_count = cpu_stats.online_cpus.unwrap_or(1) as f64;
                    ((cpu_delta as f64 / system_delta as f64) * cpu_count * 100.0) as f32
                } else {
                    0.0
                }
            } else {
                warn!(
                    container_id = %container_id,
                    "CPU stats not available"
                );
                0.0
            };

            // Convert bytes to megabytes
            let memory_mb = memory_usage / 1_048_576;
            let memory_limit_mb = memory_limit.map(|limit| limit / 1_048_576);

            let resource_stats = ResourceUsageStats {
                cpu_percent,
                memory_mb,
                memory_limit_mb,
            };

            debug!(
                container_id = %container_id,
                cpu_percent = cpu_percent,
                memory_mb = memory_mb,
                memory_limit_mb = ?memory_limit_mb,
                "Successfully retrieved container resource usage"
            );

            // Log the resource usage using the existing logger
            DockerLogger::log_resource_usage(container_id, cpu_percent, memory_mb);

            Ok(resource_stats)
        } else {
            error!(
                container_id = %container_id,
                "Stats stream returned no data"
            );
            Err(Error::Execution(format!(
                "Failed to get container stats: no data returned for container {}",
                container_id
            )))
        }
    }

    /// Clean up stopped containers for a session
    #[instrument(skip(self))]
    pub async fn cleanup_session_containers(&self, session_id: &str) -> Result<Vec<String>> {
        info!(session_id = %session_id, "Cleaning up containers for session");

        let filters = HashMap::from([
            ("label".to_string(), vec![format!("remotemedia.session_id={}", session_id)]),
        ]);

        let options = bollard::container::ListContainersOptions::<String> {
            all: true,
            filters,
            ..Default::default()
        };

        let containers = self.docker.list_containers(Some(options)).await
            .map_err(|e| {
                error!(
                    session_id = %session_id,
                    error = %e,
                    "Failed to list containers"
                );
                Error::Execution(format!("Failed to list containers: {}", e))
            })?;

        let mut removed = Vec::new();

        for container in containers {
            if let Some(id) = container.id {
                // Stop if running
                if container.state == Some(bollard::models::ContainerSummaryStateEnum::RUNNING) {
                    debug!(
                        container_id = %id,
                        "Stopping running container before removal"
                    );
                    self.stop_container(&id, std::time::Duration::from_secs(5)).await?;
                }

                // Remove container
                self.remove_container(&id, true).await?;
                removed.push(id);
            }
        }

        info!(
            session_id = %session_id,
            containers_removed = removed.len(),
            "Session cleanup completed"
        );
        Ok(removed)
    }
}

/// Parse Docker API version string into tuple format
///
/// Converts version strings like "1.40" or "1.41" into (major, minor) tuples
/// for easy comparison.
fn parse_api_version(version_str: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = version_str.split('.').collect();

    if parts.len() < 2 {
        return Err(Error::Execution(
            format!("Invalid Docker API version format: {}", version_str)
        ));
    }

    let major = parts[0]
        .parse::<u32>()
        .map_err(|_| Error::Execution(
            format!("Could not parse major version from: {}", version_str)
        ))?;

    let minor = parts[1]
        .parse::<u32>()
        .map_err(|_| Error::Execution(
            format!("Could not parse minor version from: {}", version_str)
        ))?;

    Ok((major, minor))
}

/// Log configuration for Docker operations
pub struct DockerLogger;

impl DockerLogger {
    /// Initialize Docker-specific logging
    pub fn init() {
        // This would be called once during executor initialization
        info!("Docker logging initialized");
    }

    /// Log a Docker build step
    pub fn log_build_step(step: usize, total: usize, message: &str) {
        info!(
            step = step,
            total = total,
            progress = format!("{}/{}", step, total),
            "{}", message
        );
    }

    /// Log resource usage
    pub fn log_resource_usage(container_id: &str, cpu: f32, memory_mb: u64) {
        debug!(
            container_id = %container_id,
            cpu_percent = cpu,
            memory_mb = memory_mb,
            "Container resource usage"
        );
    }
}
