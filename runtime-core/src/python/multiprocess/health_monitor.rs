//! Health monitoring for multiprocess nodes
//!
//! Provides event-driven monitoring of process health without polling,
//! using OS signals for immediate notification of process termination.

use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

use super::process_manager::{ExitReason, ProcessHandle, ProcessStatus};

// Docker container registry removed - health monitoring now integrated in docker_support.rs

/// Process health event types
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    /// Process started successfully
    ProcessStarted {
        pid: u32,
        node_id: String,
        timestamp: Instant,
    },

    /// Process became ready
    ProcessReady {
        pid: u32,
        node_id: String,
        timestamp: Instant,
    },

    /// Process exited
    ProcessExited {
        pid: u32,
        reason: ExitReason,
        timestamp: Instant,
    },

    /// Process failed to initialize
    InitializationFailed {
        pid: u32,
        node_id: String,
        error: String,
        timestamp: Instant,
    },

    /// Pipeline terminated due to node failure
    PipelineTerminated {
        session_id: String,
        failed_node: String,
        reason: String,
        timestamp: Instant,
    },

    /// Container became unhealthy
    #[cfg(feature = "docker")]
    ContainerUnhealthy {
        container_id: String,
        node_id: String,
        timestamp: Instant,
    },

    /// Container health check completed
    #[cfg(feature = "docker")]
    ContainerHealthChecked {
        container_id: String,
        node_id: String,
        is_healthy: bool,
        memory_usage: Option<u64>,
        cpu_usage: Option<f32>,
        timestamp: Instant,
    },
}

/// Health statistics for a process
#[derive(Debug, Clone, Default)]
pub struct ProcessHealthStats {
    /// Process uptime
    pub uptime: Duration,

    /// CPU usage percentage
    pub cpu_usage: f32,

    /// Memory usage in bytes
    pub memory_usage: u64,

    /// Number of restarts
    pub restart_count: u32,

    /// Last health check time
    pub last_check: Option<Instant>,

    /// Is process responsive
    pub is_responsive: bool,
}

/// Health status for a container
#[cfg(feature = "docker")]
#[derive(Debug, Clone)]
pub struct ContainerHealth {
    pub container_id: String,
    pub node_id: String,
    pub is_running: bool,
    pub memory_usage: Option<u64>,
    pub cpu_usage: Option<f32>,
    pub last_check: Instant,
}

/// Health monitor for process supervision
pub struct HealthMonitor {
    /// Process health statistics
    health_stats: Arc<RwLock<HashMap<u32, ProcessHealthStats>>>,

    /// Event channel sender
    event_sender: mpsc::UnboundedSender<ProcessEvent>,

    /// Event channel receiver
    event_receiver: Arc<RwLock<mpsc::UnboundedReceiver<ProcessEvent>>>,

    /// Event handlers
    event_handlers: Arc<RwLock<Vec<Box<dyn Fn(ProcessEvent) + Send + Sync>>>>,

    /// Configuration
    init_timeout: Duration,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(init_timeout_secs: u64) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        Self {
            health_stats: Arc::new(RwLock::new(HashMap::new())),
            event_sender: sender,
            event_receiver: Arc::new(RwLock::new(receiver)),
            event_handlers: Arc::new(RwLock::new(Vec::new())),
            init_timeout: Duration::from_secs(init_timeout_secs),
        }
    }

    /// Start monitoring a process
    pub async fn monitor_process(&self, process: ProcessHandle) {
        tracing::info!("Starting health monitoring for process {}", process.id);

        // Initialize health stats
        self.health_stats.write().await.insert(
            process.id,
            ProcessHealthStats {
                last_check: Some(Instant::now()),
                is_responsive: true,
                ..Default::default()
            },
        );

        // Send process started event
        let _ = self.event_sender.send(ProcessEvent::ProcessStarted {
            pid: process.id,
            node_id: process.node_id.clone(),
            timestamp: Instant::now(),
        });

        // Start initialization timeout monitor
        self.monitor_initialization(process.clone()).await;

        // Start exit monitoring (event-driven via process manager)
        self.monitor_exit(process).await;
    }

    /// Monitor process initialization with timeout
    async fn monitor_initialization(&self, process: ProcessHandle) {
        let timeout = self.init_timeout;
        let event_sender = self.event_sender.clone();
        let health_stats = self.health_stats.clone();

        tokio::spawn(async move {
            let start = Instant::now();

            // Wait for process to become ready or timeout
            while start.elapsed() < timeout {
                // Check process status
                let status = process.status.read().await.clone();

                match status {
                    ProcessStatus::Ready => {
                        // Process initialized successfully
                        let _ = event_sender.send(ProcessEvent::ProcessReady {
                            pid: process.id,
                            node_id: process.node_id.clone(),
                            timestamp: Instant::now(),
                        });

                        // Update health stats
                        if let Some(stats) = health_stats.write().await.get_mut(&process.id) {
                            stats.uptime = start.elapsed();
                            stats.is_responsive = true;
                        }

                        return;
                    }
                    ProcessStatus::Error(ref error) => {
                        // Initialization failed
                        let _ = event_sender.send(ProcessEvent::InitializationFailed {
                            pid: process.id,
                            node_id: process.node_id.clone(),
                            error: error.clone(),
                            timestamp: Instant::now(),
                        });
                        return;
                    }
                    ProcessStatus::Stopped => {
                        // Process terminated during initialization
                        return;
                    }
                    _ => {
                        // Still initializing, continue waiting
                    }
                }

                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            // Initialization timeout
            tracing::error!(
                "Process {} initialization timeout after {:?}",
                process.id,
                timeout
            );

            let _ = event_sender.send(ProcessEvent::InitializationFailed {
                pid: process.id,
                node_id: process.node_id,
                error: format!("Initialization timeout after {:?}", timeout),
                timestamp: Instant::now(),
            });
        });
    }

    /// Monitor process exit (event-driven)
    async fn monitor_exit(&self, process: ProcessHandle) {
        let event_sender = self.event_sender.clone();
        let health_stats = self.health_stats.clone();

        tokio::spawn(async move {
            // This task will be notified by the process manager when exit occurs
            // For now, we just register that we're monitoring
            tracing::debug!("Monitoring exit for process {}", process.id);

            // The actual exit detection happens in process_manager.rs
            // through the exit handlers mechanism
        });
    }

    /// Handle process exit event
    pub async fn handle_process_exit(
        &self,
        pid: u32,
        reason: ExitReason,
        session_id: Option<String>,
        node_id: Option<String>,
    ) -> Result<()> {
        tracing::info!("Handling process exit: {} with reason: {:?}", pid, reason);

        // Remove from health stats
        self.health_stats.write().await.remove(&pid);

        // Send exit event
        let _ = self.event_sender.send(ProcessEvent::ProcessExited {
            pid,
            reason: reason.clone(),
            timestamp: Instant::now(),
        });

        // Check if this should trigger pipeline termination
        match reason {
            ExitReason::Error(_) | ExitReason::Killed => {
                if let (Some(session_id), Some(node_id)) = (session_id, node_id) {
                    // Pipeline termination due to node failure
                    let _ = self.event_sender.send(ProcessEvent::PipelineTerminated {
                        session_id,
                        failed_node: node_id,
                        reason: format!("Node process {} crashed", pid),
                        timestamp: Instant::now(),
                    });
                }
            }
            _ => {
                // Normal termination, no pipeline termination needed
            }
        }

        // Process event handlers
        self.process_events().await;

        Ok(())
    }

    /// Register an event handler
    pub async fn on_event<F>(&self, handler: F)
    where
        F: Fn(ProcessEvent) + Send + Sync + 'static,
    {
        self.event_handlers.write().await.push(Box::new(handler));
    }

    /// Process pending events
    pub async fn process_events(&self) {
        let mut receiver = self.event_receiver.write().await;
        let handlers = self.event_handlers.read().await;

        // Process all pending events
        while let Ok(event) = receiver.try_recv() {
            tracing::debug!("Processing event: {:?}", event);

            // Call all registered handlers
            for handler in handlers.iter() {
                handler(event.clone());
            }
        }
    }

    /// Get health statistics for a process
    pub async fn get_health_stats(&self, pid: u32) -> Option<ProcessHealthStats> {
        self.health_stats.read().await.get(&pid).cloned()
    }

    /// Get all health statistics
    pub async fn get_all_health_stats(&self) -> HashMap<u32, ProcessHealthStats> {
        self.health_stats.read().await.clone()
    }

    /// Update process health stats (called periodically if needed)
    pub async fn update_health_stats(&self, pid: u32, stats: ProcessHealthStats) {
        self.health_stats.write().await.insert(pid, stats);
    }

    /// Check if a process is healthy
    pub async fn is_healthy(&self, pid: u32) -> bool {
        if let Some(stats) = self.health_stats.read().await.get(&pid) {
            stats.is_responsive
        } else {
            false
        }
    }

    /// Check health of a Docker container
    #[cfg(feature = "docker")]
    pub async fn check_container_health(
        &self,
        container_id: &str,
        node_id: &str,
    ) -> crate::Result<ContainerHealth> {
        use bollard::Docker;

        tracing::debug!(
            "Checking health of container {} for node {}",
            container_id,
            node_id
        );

        // Connect to Docker
        let docker = Docker::connect_with_local_defaults().map_err(|e| {
            crate::Error::Execution(format!("Failed to connect to Docker: {}", e))
        })?;

        // Check if container is running
        let inspect_result = docker
            .inspect_container(
                container_id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
            .map_err(|e| {
                crate::Error::Execution(format!(
                    "Failed to inspect container {}: {}",
                    container_id, e
                ))
            })?;

        let is_running = inspect_result
            .state
            .as_ref()
            .and_then(|s| s.running)
            .unwrap_or(false);

        // Get container stats if running
        // Note: Stats collection is optional and may not be available in all environments
        let (memory_usage, cpu_usage) = if is_running {
            match self
                .get_container_stats(container_id, &docker)
                .await
            {
                Ok((mem, cpu)) => (Some(mem), Some(cpu)),
                Err(e) => {
                    tracing::debug!(
                        "Stats not available for container {}: {}",
                        container_id,
                        e
                    );
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        let health = ContainerHealth {
            container_id: container_id.to_string(),
            node_id: node_id.to_string(),
            is_running,
            memory_usage,
            cpu_usage,
            last_check: Instant::now(),
        };

        // Send health check event
        let _ = self
            .event_sender
            .send(ProcessEvent::ContainerHealthChecked {
                container_id: container_id.to_string(),
                node_id: node_id.to_string(),
                is_healthy: is_running,
                memory_usage,
                cpu_usage,
                timestamp: Instant::now(),
            });

        // Log and handle unhealthy containers
        if !is_running {
            tracing::warn!(
                "Container {} for node {} is not running",
                container_id,
                node_id
            );
            self.handle_unhealthy_container(container_id, node_id)
                .await?;
        }

        Ok(health)
    }

    /// Get container resource statistics
    #[cfg(feature = "docker")]
    async fn get_container_stats(
        &self,
        container_id: &str,
        docker: &bollard::Docker,
    ) -> crate::Result<(u64, f32)> {
        use futures::StreamExt;

        // Use new-style query parameters
        let options = Some(
            bollard::query_parameters::StatsOptionsBuilder::new()
                .stream(false)
                .one_shot(true)
                .build(),
        );

        let mut stats_stream = docker.stats(container_id, options);

        if let Some(Ok(stats)) = stats_stream.next().await {
            // Extract memory usage from optional MemoryStats
            let memory_usage = stats
                .memory_stats
                .as_ref()
                .and_then(|ms| ms.usage)
                .unwrap_or(0);

            // Extract CPU stats from optional CpuStats
            let cpu_usage = if let (Some(cpu_stats), Some(precpu_stats)) =
                (stats.cpu_stats.as_ref(), stats.precpu_stats.as_ref())
            {
                // Get total usage
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

                if system_delta > 0 && cpu_delta > 0 {
                    let cpu_count = cpu_stats.online_cpus.unwrap_or(1) as f64;
                    ((cpu_delta as f64 / system_delta as f64) * cpu_count * 100.0) as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };

            Ok((memory_usage, cpu_usage))
        } else {
            Err(crate::Error::Execution(
                "Failed to get container stats".to_string(),
            ))
        }
    }

    /// Handle an unhealthy container
    #[cfg(feature = "docker")]
    async fn handle_unhealthy_container(
        &self,
        container_id: &str,
        node_id: &str,
    ) -> crate::Result<()> {
        tracing::error!(
            "Container {} for node {} is unhealthy",
            container_id,
            node_id
        );

        // Emit unhealthy event
        let _ = self
            .event_sender
            .send(ProcessEvent::ContainerUnhealthy {
                container_id: container_id.to_string(),
                node_id: node_id.to_string(),
                timestamp: Instant::now(),
            });

        // Container health status is now managed in docker_support.rs
        // TODO: Update Docker health status if needed

        Ok(())
    }

    /// Monitor all containers in a session
    #[cfg(feature = "docker")]
    pub async fn monitor_session_containers(
        &self,
        session_id: &str,
        containers: &[(String, String)], // (container_id, node_id) pairs
    ) -> Vec<ContainerHealth> {
        let mut health_results = Vec::new();

        for (container_id, node_id) in containers {
            match self
                .check_container_health(container_id, node_id)
                .await
            {
                Ok(health) => health_results.push(health),
                Err(e) => {
                    tracing::error!(
                        "Failed to check health of container {} for node {}: {}",
                        container_id,
                        node_id,
                        e
                    );
                }
            }
        }

        // Log overall session health
        let healthy_count = health_results.iter().filter(|h| h.is_running).count();
        let total_count = health_results.len();

        if healthy_count < total_count {
            tracing::warn!(
                "Session {} health: {}/{} containers running",
                session_id,
                healthy_count,
                total_count
            );
        } else {
            tracing::debug!(
                "Session {} health: all {} containers running",
                session_id,
                total_count
            );
        }

        health_results
    }

    /// Check health of a single container by node_id (looks up in registry)
    #[cfg(feature = "docker")]
    pub async fn check_container_by_node_id(
        &self,
        node_id: &str,
    ) -> crate::Result<ContainerHealth> {
        // Container registry removed - health checking now managed in docker_support.rs
        // This function would need to be reimplemented using the new Docker support
        Err(crate::Error::Execution(format!(
            "Container health check not yet reimplemented for node_id: {}",
            node_id
        )))
    }
}

/// Global event bus for process events
pub struct EventBus<T> {
    sender: tokio::sync::broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> EventBus<T> {
    /// Create a new event bus
    pub fn new() -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(100);
        Self { sender }
    }

    /// Emit an event
    pub fn emit(&self, event: T) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to events and spawn a handler task
    pub fn subscribe<F>(&self, handler: F)
    where
        F: Fn(T) + Send + 'static,
    {
        let mut receiver = self.sender.subscribe();

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv().await {
                handler(event);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let monitor = HealthMonitor::new(30);

        let stats = monitor.get_all_health_stats().await;
        assert!(stats.is_empty());
    }

    #[tokio::test]
    async fn test_event_handling() {
        let monitor = HealthMonitor::new(30);

        let event_received = Arc::new(RwLock::new(false));
        let event_received_clone = event_received.clone();

        // Register event handler
        monitor
            .on_event(move |event| {
                if matches!(event, ProcessEvent::ProcessStarted { .. }) {
                    let event_received = event_received_clone.clone();
                    tokio::spawn(async move {
                        *event_received.write().await = true;
                    });
                }
            })
            .await;

        // Send a process started event
        let _ = monitor.event_sender.send(ProcessEvent::ProcessStarted {
            pid: 1234,
            node_id: "test_node".to_string(),
            timestamp: Instant::now(),
        });

        // Process events
        monitor.process_events().await;

        // Allow async handler to complete
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(*event_received.read().await);
    }

    #[tokio::test]
    async fn test_health_stats() {
        let monitor = HealthMonitor::new(30);

        // Add health stats for a process
        let stats = ProcessHealthStats {
            uptime: Duration::from_secs(60),
            cpu_usage: 25.5,
            memory_usage: 1024 * 1024 * 100, // 100MB
            restart_count: 0,
            last_check: Some(Instant::now()),
            is_responsive: true,
        };

        monitor.update_health_stats(1234, stats.clone()).await;

        // Retrieve stats
        let retrieved = monitor.get_health_stats(1234).await;
        assert!(retrieved.is_some());

        let retrieved_stats = retrieved.unwrap();
        assert_eq!(retrieved_stats.cpu_usage, 25.5);
        assert_eq!(retrieved_stats.memory_usage, 1024 * 1024 * 100);
        assert!(retrieved_stats.is_responsive);

        // Check health
        assert!(monitor.is_healthy(1234).await);
    }

    #[tokio::test]
    #[cfg(feature = "docker")]
    async fn test_container_health_structure() {
        // Test that ContainerHealth can be created and accessed
        let health = ContainerHealth {
            container_id: "test123".to_string(),
            node_id: "test_node".to_string(),
            is_running: true,
            memory_usage: Some(1024 * 1024 * 50), // 50MB
            cpu_usage: Some(15.5),
            last_check: Instant::now(),
        };

        assert_eq!(health.container_id, "test123");
        assert_eq!(health.node_id, "test_node");
        assert!(health.is_running);
        assert_eq!(health.memory_usage, Some(1024 * 1024 * 50));
        assert_eq!(health.cpu_usage, Some(15.5));
    }

    #[tokio::test]
    #[cfg(feature = "docker")]
    async fn test_container_events() {
        let monitor = HealthMonitor::new(30);

        // Register event handler
        let event_received = Arc::new(RwLock::new(false));
        let event_received_clone = event_received.clone();

        monitor
            .on_event(move |event| {
                if matches!(event, ProcessEvent::ContainerUnhealthy { .. }) {
                    let event_received = event_received_clone.clone();
                    tokio::spawn(async move {
                        *event_received.write().await = true;
                    });
                }
            })
            .await;

        // Send a container unhealthy event
        let _ = monitor
            .event_sender
            .send(ProcessEvent::ContainerUnhealthy {
                container_id: "test_container".to_string(),
                node_id: "test_node".to_string(),
                timestamp: Instant::now(),
            });

        // Process events
        monitor.process_events().await;

        // Allow async handler to complete
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(*event_received.read().await);
    }
}
