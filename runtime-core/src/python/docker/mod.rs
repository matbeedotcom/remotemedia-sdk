//! Docker executor for running Python nodes in isolated containers with iceoryx2 IPC
//!
//! This module provides Docker-based node execution with zero-copy data transfer
//! via iceoryx2 shared memory IPC. It extends the multiprocess executor architecture
//! to support containerized nodes with environment isolation and resource limits.
//!
//! # Architecture
//!
//! The Docker executor follows the same patterns as the multiprocess executor:
//! - Dedicated IPC threads for iceoryx2 Publisher/Subscriber (!Send types)
//! - Session-scoped channel naming to prevent conflicts
//! - Reference counting for shared containers across sessions
//! - Health monitoring and graceful cleanup
//!
//! # Modules
//!
//! - `config`: Docker node configuration and validation
//! - `container_manager`: Docker container lifecycle management
//! - `image_builder`: Docker image building and caching
//! - `docker_executor`: Main executor implementing StreamingNodeExecutor trait
//! - `ipc_bridge`: Adapts multiprocess IPC patterns for containers
//! - `container_registry`: Global registry for sharing containers across sessions (FR-012, FR-015)
//!
//! # Feature Flag
//!
//! This module requires the `docker-executor` feature flag to be enabled.

#![cfg(feature = "docker-executor")]

pub mod config;
pub mod container_manager;
pub mod container_registry;
pub mod docker_executor;
pub mod health_check;
pub mod image_builder;
pub mod ipc_bridge;

// Re-export main types
pub use config::{DockerExecutorConfig, DockerizedNodeConfiguration, ResourceLimits};
pub use container_registry::{ContainerSessionInstance, HealthStatus};
pub use docker_executor::DockerExecutor;
pub use health_check::spawn_health_monitor;
