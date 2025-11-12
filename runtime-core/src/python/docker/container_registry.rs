//! Global container registry for sharing containers across sessions
//!
//! This module implements FR-012 (container sharing) and FR-015 (reference counting).
//! Containers are shared across pipeline sessions when they have identical configurations,
//! enabling resource efficiency and faster startup times.

use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

/// Container health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// Container is starting up
    Starting,
    /// Container is healthy and ready
    Healthy,
    /// Container is unhealthy (failed health checks)
    Unhealthy,
    /// Container is stopping
    Stopping,
}

/// Container session instance with reference counting (FR-015)
#[derive(Debug, Clone)]
pub struct ContainerSessionInstance {
    /// Docker container ID
    pub container_id: String,

    /// Docker container name
    pub container_name: String,

    /// Node ID this container runs
    pub node_id: String,

    /// Docker image ID used
    pub image_id: String,

    /// Session IDs currently using this container
    pub session_ids: Vec<String>,

    /// Reference count (number of active sessions)
    pub reference_count: Arc<AtomicUsize>,

    /// Health status
    pub health_status: HealthStatus,

    /// When container started
    pub started_at: std::time::Instant,

    /// Last health check time
    pub last_health_check: std::time::Instant,
}

impl ContainerSessionInstance {
    /// Create a new container instance
    pub fn new(
        container_id: String,
        container_name: String,
        node_id: String,
        image_id: String,
    ) -> Self {
        let now = std::time::Instant::now();
        Self {
            container_id,
            container_name,
            node_id,
            image_id,
            session_ids: Vec::new(),
            reference_count: Arc::new(AtomicUsize::new(0)),
            health_status: HealthStatus::Starting,
            started_at: now,
            last_health_check: now,
        }
    }

    /// Add a session to this container (FR-015)
    pub fn add_session(&mut self, session_id: String) {
        if !self.session_ids.contains(&session_id) {
            self.session_ids.push(session_id);
            self.reference_count.fetch_add(1, Ordering::SeqCst);

            tracing::debug!(
                "Added session to container '{}': {} sessions now active",
                self.container_name,
                self.reference_count.load(Ordering::SeqCst)
            );
        }
    }

    /// Remove a session from this container (FR-015)
    /// Returns true if reference count reaches zero (container should be stopped)
    pub fn remove_session(&mut self, session_id: &str) -> bool {
        if let Some(pos) = self.session_ids.iter().position(|s| s == session_id) {
            self.session_ids.remove(pos);
            let prev_count = self.reference_count.fetch_sub(1, Ordering::SeqCst);

            tracing::debug!(
                "Removed session from container '{}': {} sessions remaining",
                self.container_name,
                prev_count - 1
            );

            // Return true if this was the last session
            prev_count == 1
        } else {
            false
        }
    }

    /// Get current reference count
    pub fn ref_count(&self) -> usize {
        self.reference_count.load(Ordering::SeqCst)
    }

    /// Check if health check is due
    pub fn should_health_check(&self) -> bool {
        const HEALTH_CHECK_INTERVAL_SECS: u64 = 30;
        self.last_health_check.elapsed().as_secs() >= HEALTH_CHECK_INTERVAL_SECS
    }

    /// Update health status
    pub fn update_health(&mut self, status: HealthStatus) {
        self.health_status = status;
        self.last_health_check = std::time::Instant::now();
    }

    /// Transition to stopping state
    pub fn mark_stopping(&mut self) {
        self.health_status = HealthStatus::Stopping;
    }
}

/// Global container registry (FR-012)
pub type ContainerRegistry = Arc<RwLock<HashMap<String, ContainerSessionInstance>>>;

/// Global singleton registry
static GLOBAL_REGISTRY: OnceLock<ContainerRegistry> = OnceLock::new();

/// Get or initialize the global container registry
pub fn global_container_registry() -> &'static ContainerRegistry {
    GLOBAL_REGISTRY.get_or_init(|| {
        tracing::info!("Initializing global container registry");
        Arc::new(RwLock::new(HashMap::new()))
    })
}

/// Get or create a container for a node (FR-012)
pub async fn get_or_create_container(
    node_id: &str,
) -> Option<ContainerSessionInstance> {
    let registry = global_container_registry().read().await;
    registry.get(node_id).cloned()
}

/// Register a new container in the global registry
pub async fn register_container(
    node_id: String,
    instance: ContainerSessionInstance,
) -> Result<()> {
    let mut registry = global_container_registry().write().await;

    if registry.contains_key(&node_id) {
        tracing::warn!("Container for node '{}' already exists in registry", node_id);
    }

    tracing::info!(
        "Registering container '{}' for node '{}' in global registry",
        instance.container_name,
        node_id
    );

    registry.insert(node_id, instance);
    Ok(())
}

/// Add a session to an existing container
pub async fn add_session_to_container(
    node_id: &str,
    session_id: String,
) -> Result<()> {
    let mut registry = global_container_registry().write().await;

    if let Some(instance) = registry.get_mut(node_id) {
        instance.add_session(session_id);
        Ok(())
    } else {
        Err(Error::Execution(format!(
            "Container for node '{}' not found in registry",
            node_id
        )))
    }
}

/// Remove a session from a container (FR-015)
/// Returns true if the container should be stopped (ref count reached zero)
pub async fn remove_session_from_container(
    node_id: &str,
    session_id: &str,
) -> Result<bool> {
    let mut registry = global_container_registry().write().await;

    if let Some(instance) = registry.get_mut(node_id) {
        let should_stop = instance.remove_session(session_id);

        if should_stop {
            // Remove from registry when ref count reaches zero
            tracing::info!(
                "Removing container '{}' from registry (no more sessions)",
                instance.container_name
            );
            registry.remove(node_id);
        }

        Ok(should_stop)
    } else {
        // Container not found - already removed
        Ok(false)
    }
}

/// Get all containers in the registry (for monitoring)
pub async fn list_all_containers() -> Vec<ContainerSessionInstance> {
    let registry = global_container_registry().read().await;
    registry.values().cloned().collect()
}

/// Get container count
pub async fn container_count() -> usize {
    let registry = global_container_registry().read().await;
    registry.len()
}

/// Clear the entire registry (for testing/cleanup)
///
/// Available in both unit tests and integration tests for proper test isolation.
pub async fn clear_registry_for_testing() {
    let mut registry = global_container_registry().write().await;
    registry.clear();
    tracing::debug!("Cleared global container registry");
}

// Alias for backwards compatibility with unit tests
#[cfg(test)]
pub async fn clear_registry() {
    clear_registry_for_testing().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_container_instance_reference_counting() {
        let mut instance = ContainerSessionInstance::new(
            "container123".to_string(),
            "test_container".to_string(),
            "node1".to_string(),
            "image123".to_string(),
        );

        // Initially zero references
        assert_eq!(instance.ref_count(), 0);

        // Add first session
        instance.add_session("session1".to_string());
        assert_eq!(instance.ref_count(), 1);
        assert_eq!(instance.session_ids.len(), 1);

        // Add second session
        instance.add_session("session2".to_string());
        assert_eq!(instance.ref_count(), 2);
        assert_eq!(instance.session_ids.len(), 2);

        // Adding same session should not increase count
        instance.add_session("session1".to_string());
        assert_eq!(instance.ref_count(), 2);
        assert_eq!(instance.session_ids.len(), 2);

        // Remove first session
        let should_stop = instance.remove_session("session1");
        assert!(!should_stop);
        assert_eq!(instance.ref_count(), 1);

        // Remove second session - should signal stop
        let should_stop = instance.remove_session("session2");
        assert!(should_stop);
        assert_eq!(instance.ref_count(), 0);
    }

    #[tokio::test]
    async fn test_global_registry_operations() {
        clear_registry().await;

        // Register a container
        let instance = ContainerSessionInstance::new(
            "container456".to_string(),
            "test_container_2".to_string(),
            "node2".to_string(),
            "image456".to_string(),
        );

        register_container("node2".to_string(), instance.clone())
            .await
            .unwrap();

        // Verify it's in the registry
        assert_eq!(container_count().await, 1);

        // Get the container
        let retrieved = get_or_create_container("node2").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().container_id, "container456");

        // Add session
        add_session_to_container("node2", "session1".to_string())
            .await
            .unwrap();

        let retrieved = get_or_create_container("node2").await.unwrap();
        assert_eq!(retrieved.ref_count(), 1);

        // Remove session
        let should_stop = remove_session_from_container("node2", "session1")
            .await
            .unwrap();
        assert!(should_stop);

        // Container should be removed from registry
        assert_eq!(container_count().await, 0);
    }

    #[tokio::test]
    async fn test_health_check_timing() {
        let instance = ContainerSessionInstance::new(
            "container789".to_string(),
            "health_test".to_string(),
            "node3".to_string(),
            "image789".to_string(),
        );

        // Should not need health check immediately
        assert!(!instance.should_health_check());

        // After 30 seconds it should need check (but we can't wait in test)
        // This is validated by the logic, not time-based in tests
    }
}
