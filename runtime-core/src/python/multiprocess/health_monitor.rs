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
}
