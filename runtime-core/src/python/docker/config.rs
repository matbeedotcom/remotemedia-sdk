//! Docker node configuration and validation
//!
//! Defines configuration structures for Docker-based node execution,
//! including Python environment, dependencies, and resource limits.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Supported Python versions for Docker executor
pub const SUPPORTED_PYTHON_VERSIONS: &[&str] = &["3.9", "3.10", "3.11"];

/// Dockerized node configuration (combines manifest config with node ID)
#[derive(Debug, Clone)]
pub struct DockerizedNodeConfiguration {
    pub node_id: String,
    pub config: DockerExecutorConfig,
    pub config_hash: String,
}

impl DockerizedNodeConfiguration {
    /// Create from node ID and manifest config
    pub fn new(node_id: String, config: DockerExecutorConfig) -> Self {
        let config_hash = config.compute_config_hash();
        Self {
            node_id,
            config,
            config_hash,
        }
    }

    /// Validate entire configuration
    pub fn validate(&self) -> Result<()> {
        if self.node_id.trim().is_empty() {
            return Err(Error::InvalidManifest(
                "Node ID cannot be empty".to_string(),
            ));
        }
        self.config.validate()
    }
}

/// Docker executor configuration from pipeline manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerExecutorConfig {
    /// Python version (e.g., "3.10")
    pub python_version: String,

    /// System dependencies to install via apt-get
    #[serde(default)]
    pub system_dependencies: Vec<String>,

    /// Python packages to install via pip
    #[serde(default)]
    pub python_packages: Vec<String>,

    /// Resource limits (CPU, memory)
    pub resource_limits: ResourceLimits,

    /// Optional custom base image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_image: Option<String>,

    /// Optional environment variables
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

impl DockerExecutorConfig {
    /// Validate configuration according to FR-004, FR-013, FR-014
    pub fn validate(&self) -> Result<()> {
        // FR-013: Python version must be supported
        if !SUPPORTED_PYTHON_VERSIONS.contains(&self.python_version.as_str()) {
            return Err(Error::InvalidManifest(format!(
                "Unsupported Python version '{}'. Supported versions: {:?}",
                self.python_version, SUPPORTED_PYTHON_VERSIONS
            )));
        }

        // FR-014: Resource limits must be valid
        self.resource_limits.validate()?;

        // System dependencies must be non-empty strings
        for dep in &self.system_dependencies {
            if dep.trim().is_empty() {
                return Err(Error::InvalidManifest(
                    "System dependency cannot be empty".to_string(),
                ));
            }
        }

        // Python packages must be non-empty strings
        for pkg in &self.python_packages {
            if pkg.trim().is_empty() {
                return Err(Error::InvalidManifest(
                    "Python package cannot be empty".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Compute SHA256 hash of configuration for image cache lookup
    pub fn compute_config_hash(&self) -> String {
        let mut hasher = Sha256::new();

        // Hash all configuration fields deterministically
        hasher.update(self.python_version.as_bytes());
        hasher.update(
            self.base_image
                .as_ref()
                .map(|s| s.as_bytes())
                .unwrap_or(b""),
        );

        // Sort dependencies for deterministic hashing
        let mut sorted_deps = self.system_dependencies.clone();
        sorted_deps.sort();
        for dep in &sorted_deps {
            hasher.update(dep.as_bytes());
        }

        let mut sorted_pkgs = self.python_packages.clone();
        sorted_pkgs.sort();
        for pkg in &sorted_pkgs {
            hasher.update(pkg.as_bytes());
        }

        hasher.update(&self.resource_limits.memory_mb.to_le_bytes());
        hasher.update(&self.resource_limits.cpu_cores.to_le_bytes());

        hex::encode(hasher.finalize())
    }
}

/// Resource limits for Docker containers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Memory limit in megabytes
    pub memory_mb: u64,

    /// CPU cores (fractional values allowed)
    pub cpu_cores: f32,
}

impl ResourceLimits {
    /// Validate resource limits according to FR-014
    pub fn validate(&self) -> Result<()> {
        // FR-014: Memory must be at least 128MB (minimum for Python runtime)
        const MIN_MEMORY_MB: u64 = 128;
        if self.memory_mb < MIN_MEMORY_MB {
            return Err(Error::InvalidManifest(format!(
                "Memory limit too low: {}MB. Minimum required: {}MB",
                self.memory_mb, MIN_MEMORY_MB
            )));
        }

        // FR-014: CPU must be at least 0.1 cores
        const MIN_CPU_CORES: f32 = 0.1;
        if self.cpu_cores < MIN_CPU_CORES {
            return Err(Error::InvalidManifest(format!(
                "CPU limit too low: {} cores. Minimum required: {} cores",
                self.cpu_cores, MIN_CPU_CORES
            )));
        }

        // Validate against host limits
        let host_cpu_count = num_cpus::get() as f32;
        if self.cpu_cores > host_cpu_count {
            return Err(Error::InvalidManifest(format!(
                "CPU limit exceeds host CPU count: {} requested, {} available",
                self.cpu_cores, host_cpu_count
            )));
        }

        Ok(())
    }

    /// Convert to Docker HostConfig format
    #[cfg(feature = "docker-executor")]
    pub fn to_docker_host_config(&self) -> bollard::models::HostConfig {
        bollard::models::HostConfig {
            memory: Some(self.memory_mb as i64 * 1_048_576), // Convert MB to bytes
            nano_cpus: Some((self.cpu_cores * 1_000_000_000.0) as i64), // Convert cores to nano CPUs
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_validation() {
        // Valid limits
        let valid = ResourceLimits {
            memory_mb: 2048,
            cpu_cores: 2.0,
        };
        assert!(valid.validate().is_ok());

        // Memory too low
        let low_mem = ResourceLimits {
            memory_mb: 64,
            cpu_cores: 1.0,
        };
        assert!(low_mem.validate().is_err());

        // CPU too low
        let low_cpu = ResourceLimits {
            memory_mb: 512,
            cpu_cores: 0.05,
        };
        assert!(low_cpu.validate().is_err());
    }

    #[test]
    fn test_config_hash_deterministic() {
        let config = DockerExecutorConfig {
            python_version: "3.10".to_string(),
            system_dependencies: vec!["ffmpeg".to_string(), "libsndfile1".to_string()],
            python_packages: vec!["numpy==1.24.0".to_string()],
            resource_limits: ResourceLimits {
                memory_mb: 2048,
                cpu_cores: 2.0,
            },
            base_image: None,
            env: Default::default(),
        };

        let hash1 = config.compute_config_hash();
        let hash2 = config.compute_config_hash();
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 produces 64 hex chars
    }
}
