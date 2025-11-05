//! Multiprocess executor for Python nodes
//!
//! Implements NodeExecutor trait to manage Python nodes running in separate processes
//! with iceoryx2 shared memory IPC for zero-copy data transfer.

use crate::executor::node_executor::{NodeContext, NodeExecutor};
use crate::{Error, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(feature = "multiprocess")]
use super::process_manager::{ProcessHandle, ProcessManager, ProcessStatus, ExitReason};
#[cfg(feature = "multiprocess")]
use super::ipc_channel::{ChannelHandle, ChannelRegistry};
#[cfg(feature = "multiprocess")]
use super::health_monitor::{HealthMonitor, ProcessEvent};

/// Configuration for multiprocess executor
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MultiprocessConfig {
    /// Maximum processes per session (None = unlimited)
    #[serde(default = "default_max_processes")]
    pub max_processes_per_session: Option<usize>,

    /// Channel buffer capacity (number of messages)
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,

    /// Process initialization timeout in seconds
    #[serde(default = "default_init_timeout")]
    pub init_timeout_secs: u64,

    /// Python executable path
    #[serde(default = "default_python_executable")]
    pub python_executable: std::path::PathBuf,

    /// Enable backpressure on channels
    #[serde(default = "default_backpressure")]
    pub enable_backpressure: bool,
}

// Default value functions for serde
fn default_max_processes() -> Option<usize> {
    Some(10)
}

fn default_channel_capacity() -> usize {
    100
}

fn default_init_timeout() -> u64 {
    30
}

fn default_python_executable() -> std::path::PathBuf {
    std::path::PathBuf::from("python")
}

fn default_backpressure() -> bool {
    true
}

impl Default for MultiprocessConfig {
    fn default() -> Self {
        Self {
            max_processes_per_session: Some(10),
            channel_capacity: 100,
            init_timeout_secs: 30,
            python_executable: std::path::PathBuf::from("python"),
            enable_backpressure: true,
        }
    }
}

impl MultiprocessConfig {
    /// Load configuration from a TOML file
    ///
    /// # Arguments
    /// * `path` - Path to the TOML configuration file
    ///
    /// # Returns
    /// * `Ok(MultiprocessConfig)` - Loaded configuration with defaults for missing fields
    /// * `Err(Error)` - If file cannot be read or parsed
    ///
    /// # Example
    /// ```no_run
    /// use remotemedia_runtime::python::multiprocess::MultiprocessConfig;
    /// use std::path::Path;
    ///
    /// let config = MultiprocessConfig::from_file(Path::new("runtime.toml"))?;
    /// # Ok::<(), remotemedia_runtime::Error>(())
    /// ```
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| Error::ConfigError(format!("Failed to read config file {:?}: {}", path, e)))?;

        Self::from_toml_str(&contents)
    }

    /// Load configuration from a TOML string
    ///
    /// # Arguments
    /// * `toml_str` - TOML configuration string
    ///
    /// # Returns
    /// * `Ok(MultiprocessConfig)` - Parsed configuration with defaults for missing fields
    /// * `Err(Error)` - If TOML cannot be parsed
    ///
    /// # Example
    /// ```
    /// use remotemedia_runtime::python::multiprocess::MultiprocessConfig;
    ///
    /// let config_str = r#"
    ///     max_processes_per_session = 20
    ///     channel_capacity = 200
    ///     init_timeout_secs = 60
    ///     python_executable = "/usr/bin/python3"
    ///     enable_backpressure = true
    /// "#;
    ///
    /// let config = MultiprocessConfig::from_toml_str(config_str)?;
    /// assert_eq!(config.channel_capacity, 200);
    /// # Ok::<(), remotemedia_runtime::Error>(())
    /// ```
    pub fn from_toml_str(toml_str: &str) -> Result<Self> {
        // Parse the TOML string
        let config: MultiprocessConfig = toml::from_str(toml_str)
            .map_err(|e| Error::ConfigError(format!("Failed to parse TOML config: {}", e)))?;

        Ok(config)
    }

    /// Load configuration from runtime.toml in the current directory
    ///
    /// Falls back to default configuration if the file doesn't exist.
    ///
    /// # Returns
    /// * `Ok(MultiprocessConfig)` - Loaded or default configuration
    /// * `Err(Error)` - If file exists but cannot be parsed
    pub fn from_default_file() -> Result<Self> {
        let path = std::path::Path::new("runtime.toml");

        if path.exists() {
            Self::from_file(path)
        } else {
            Ok(Self::default())
        }
    }
}

/// Session state for tracking pipeline execution
#[derive(Debug)]
pub struct SessionState {
    /// Session identifier
    pub session_id: String,

    /// Active node processes
    pub node_processes: HashMap<String, ProcessHandle>,

    /// IPC channels for this session
    pub channels: HashMap<String, ChannelHandle>,

    /// Session status
    pub status: SessionStatus,

    /// Creation timestamp
    pub created_at: std::time::Instant,

    /// Initialization progress for each node (node_id -> progress)
    pub init_progress: HashMap<String, InitProgress>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Initializing,
    Ready,
    Running,
    Terminating,
    Terminated,
    Error(String),
}

/// Initialization progress information for a node
#[derive(Debug, Clone)]
pub struct InitProgress {
    /// Node identifier
    pub node_id: String,

    /// Node type
    pub node_type: String,

    /// Current initialization status
    pub status: InitStatus,

    /// Progress percentage (0.0 to 1.0)
    pub progress: f32,

    /// Human-readable progress message
    pub message: String,

    /// Timestamp of this progress update
    pub timestamp: std::time::Instant,
}

/// Node initialization status
#[derive(Debug, Clone, PartialEq)]
pub enum InitStatus {
    /// Node process is starting
    Starting,

    /// Loading model files or dependencies
    LoadingModel,

    /// Connecting to IPC channels
    Connecting,

    /// Node is ready for execution
    Ready,

    /// Initialization failed
    Failed(String),
}

/// Multiprocess executor for Python nodes
#[derive(Clone)]
pub struct MultiprocessExecutor {
    /// Process manager
    process_manager: Arc<ProcessManager>,

    /// Channel registry
    channel_registry: Arc<ChannelRegistry>,

    /// Health monitor
    #[cfg(feature = "multiprocess")]
    health_monitor: Arc<HealthMonitor>,

    /// Active sessions
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,

    /// Configuration
    config: MultiprocessConfig,

    /// Current node context
    current_context: Option<NodeContext>,
}

impl MultiprocessExecutor {
    /// Create a new multiprocess executor
    pub fn new(config: MultiprocessConfig) -> Self {
        #[cfg(feature = "multiprocess")]
        let health_monitor = Arc::new(HealthMonitor::new(config.init_timeout_secs));

        let executor = Self {
            process_manager: Arc::new(ProcessManager::new(config.clone())),
            channel_registry: Arc::new(ChannelRegistry::new()),
            #[cfg(feature = "multiprocess")]
            health_monitor: health_monitor.clone(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
            current_context: None,
        };

        // Setup pipeline termination on node failure
        #[cfg(feature = "multiprocess")]
        executor.setup_failure_handling();

        executor
    }

    /// Setup failure handling for pipeline termination
    #[cfg(feature = "multiprocess")]
    fn setup_failure_handling(&self) {
        let sessions = self.sessions.clone();
        let health_monitor = self.health_monitor.clone();

        // Register process exit handler
        let process_manager = self.process_manager.clone();
        let process_manager_for_handler = process_manager.clone();
        let sessions_for_handler = sessions.clone();
        let health_monitor_for_handler = health_monitor.clone();

        tokio::spawn(async move {
            process_manager.on_process_exit(move |pid, reason| {
                let sessions = sessions_for_handler.clone();
                let health_monitor = health_monitor_for_handler.clone();
                let process_manager_clone = process_manager_for_handler.clone();

                tokio::spawn(async move {
                    // Find which session this process belongs to
                    let session_info = {
                        let sessions_guard = sessions.read().await;
                        sessions_guard.values()
                            .find(|s| s.node_processes.values().any(|p| p.id == pid))
                            .map(|s| (s.session_id.clone(),
                                     s.node_processes.iter()
                                         .find(|(_, p)| p.id == pid)
                                         .map(|(node_id, _)| node_id.clone())))
                    };

                    if let Some((session_id, Some(node_id))) = session_info {
                        // Handle process exit
                        let _ = health_monitor.handle_process_exit(
                            pid,
                            reason.clone(),
                            Some(session_id.clone()),
                            Some(node_id.clone()),
                        ).await;

                        // Terminate pipeline on error
                        match reason {
                            ExitReason::Error(_) | ExitReason::Killed | ExitReason::Timeout => {
                                tracing::error!(
                                    "Node {} (PID {}) failed, terminating pipeline {}",
                                    node_id, pid, session_id
                                );

                                // Terminate the entire session
                                if let Err(e) = Self::terminate_session_static(
                                    sessions.clone(),
                                    process_manager_clone,
                                    &session_id,
                                ).await {
                                    tracing::error!(
                                        "Failed to terminate session {}: {}",
                                        session_id, e
                                    );
                                }
                            }
                            ExitReason::Normal => {
                                tracing::info!(
                                    "Node {} (PID {}) exited normally",
                                    node_id, pid
                                );
                            }
                        }
                    }
                });
            }).await;
        });

        // Register health event handler for pipeline termination
        let sessions_for_events = sessions.clone();

        tokio::spawn(async move {
            health_monitor.on_event(move |event| {
                if let ProcessEvent::PipelineTerminated { session_id, failed_node, reason, .. } = event {
                    tracing::error!(
                        "Pipeline {} terminated: node {} failed - {}",
                        session_id, failed_node, reason
                    );

                    let sessions = sessions_for_events.clone();
                    tokio::spawn(async move {
                        // Update session status
                        if let Some(session) = sessions.write().await.get_mut(&session_id) {
                            session.status = SessionStatus::Error(reason);
                        }
                    });
                }
            }).await;
        });
    }

    /// Static version of terminate_session for use in closures
    #[cfg(feature = "multiprocess")]
    async fn terminate_session_static(
        sessions: Arc<RwLock<HashMap<String, SessionState>>>,
        process_manager: Arc<ProcessManager>,
        session_id: &str,
    ) -> Result<()> {
        let mut sessions_guard = sessions.write().await;

        if let Some(mut session) = sessions_guard.remove(session_id) {
            session.status = SessionStatus::Terminating;

            // Terminate all processes in the session
            for (_, process) in session.node_processes.drain() {
                process_manager.terminate_process(
                    process,
                    std::time::Duration::from_secs(5),
                ).await?;
            }

            // Channels will be cleaned up when processes exit
        }

        Ok(())
    }

    /// Create a new session for pipeline execution
    pub async fn create_session(
        &self,
        session_id: String,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        // Check if session already exists
        if sessions.contains_key(&session_id) {
            return Err(Error::Execution(format!(
                "Session {} already exists", session_id
            )));
        }

        // Create new session state
        let session = SessionState {
            session_id: session_id.clone(),
            node_processes: HashMap::new(),
            channels: HashMap::new(),
            status: SessionStatus::Initializing,
            created_at: std::time::Instant::now(),
            init_progress: HashMap::new(),
        };

        sessions.insert(session_id, session);
        Ok(())
    }

    /// Wait for all nodes in a session to complete initialization
    pub async fn wait_for_initialization(
        &self,
        session_id: &str,
        timeout: std::time::Duration,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        loop {
            // Check if timeout expired
            if start.elapsed() > timeout {
                return Err(Error::Execution(format!(
                    "Session {} initialization timeout after {}s",
                    session_id,
                    timeout.as_secs()
                )));
            }

            // Check session status
            let sessions = self.sessions.read().await;
            let session = sessions.get(session_id)
                .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

            // Check if any nodes failed initialization
            for (node_id, progress) in &session.init_progress {
                if let InitStatus::Failed(reason) = &progress.status {
                    return Err(Error::Execution(format!(
                        "Node {} failed to initialize: {}",
                        node_id, reason
                    )));
                }
            }

            // Check if all nodes are ready
            if !session.node_processes.is_empty() {
                let all_ready = session.node_processes.keys().all(|node_id| {
                    session.init_progress.get(node_id)
                        .map(|p| p.status == InitStatus::Ready)
                        .unwrap_or(false)
                });

                if all_ready {
                    // Update session status to Ready
                    drop(sessions);
                    let mut sessions = self.sessions.write().await;
                    if let Some(session) = sessions.get_mut(session_id) {
                        session.status = SessionStatus::Ready;
                    }
                    return Ok(());
                }
            }

            drop(sessions);

            // Wait before checking again
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Update initialization progress for a node
    pub async fn update_init_progress(
        &self,
        session_id: &str,
        node_id: &str,
        status: InitStatus,
        progress: f32,
        message: String,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        // Get node type from process handle
        let node_type = session.node_processes.get(node_id)
            .map(|p| p.node_type.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Create or update progress entry
        let init_progress = InitProgress {
            node_id: node_id.to_string(),
            node_type,
            status,
            progress: progress.clamp(0.0, 1.0),
            message,
            timestamp: std::time::Instant::now(),
        };

        session.init_progress.insert(node_id.to_string(), init_progress.clone());

        // Log progress
        tracing::info!(
            "Session {}, Node {}: {} ({}%)",
            session_id,
            node_id,
            init_progress.message,
            (init_progress.progress * 100.0) as u8
        );

        Ok(())
    }

    /// Get initialization progress for all nodes in a session
    pub async fn get_init_progress(&self, session_id: &str) -> Result<Vec<InitProgress>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        Ok(session.init_progress.values().cloned().collect())
    }

    /// Terminate a session and cleanup resources
    pub async fn terminate_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(mut session) = sessions.remove(session_id) {
            session.status = SessionStatus::Terminating;

            // Terminate all processes in the session
            #[cfg(feature = "multiprocess")]
            {
                for (_, process) in session.node_processes.drain() {
                    self.process_manager.terminate_process(
                        process,
                        std::time::Duration::from_secs(5),
                    ).await?;
                }

                // Cleanup channels
                for (_, channel) in session.channels.drain() {
                    self.channel_registry.destroy_channel(channel).await?;
                }
            }

            Ok(())
        } else {
            Err(Error::Execution(format!(
                "Session {} not found", session_id
            )))
        }
    }

    /// Connect two nodes with a channel for data transfer
    #[cfg(feature = "multiprocess")]
    pub async fn connect_nodes(
        &self,
        session_id: &str,
        from_node: &str,
        to_node: &str,
        channel_name: Option<&str>,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        let session = sessions.get_mut(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        // Generate channel name if not provided
        let channel_name = channel_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}_to_{}", from_node, to_node));

        // Verify both nodes exist in the session
        if !session.node_processes.contains_key(from_node) {
            return Err(Error::Execution(format!("Source node {} not found in session", from_node)));
        }
        if !session.node_processes.contains_key(to_node) {
            return Err(Error::Execution(format!("Destination node {} not found in session", to_node)));
        }

        // Create the channel
        let channel = self.channel_registry.create_channel(
            &channel_name,
            self.config.channel_capacity,
            self.config.enable_backpressure,
        ).await?;

        // Store channel in session
        session.channels.insert(channel_name.clone(), channel);

        tracing::info!(
            "Connected nodes {} -> {} via channel {}",
            from_node,
            to_node,
            channel_name
        );

        Ok(())
    }

    /// Get channel statistics for a session
    #[cfg(feature = "multiprocess")]
    pub async fn get_channel_stats(&self, session_id: &str, channel_name: &str) -> Result<super::ipc_channel::ChannelStats> {
        let sessions = self.sessions.read().await;

        let session = sessions.get(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        let channel = session.channels.get(channel_name)
            .ok_or_else(|| Error::Execution(format!("Channel {} not found", channel_name)))?;

        let stats = channel.stats.read().await;
        Ok(stats.clone())
    }
}

#[async_trait]
impl NodeExecutor for MultiprocessExecutor {
    async fn initialize(&mut self, ctx: &NodeContext) -> Result<()> {
        tracing::info!(
            "Initializing multiprocess node: {} ({})",
            ctx.node_id,
            ctx.node_type
        );

        // Store context for later use
        self.current_context = Some(ctx.clone());

        // Get or create session
        let session_id = ctx.session_id.clone()
            .unwrap_or_else(|| format!("default_{}", ctx.node_id));

        // Ensure session exists
        if !self.sessions.read().await.contains_key(&session_id) {
            self.create_session(session_id.clone()).await?;
        }

        // Spawn process for this node
        #[cfg(feature = "multiprocess")]
        {
            let process = self.process_manager.spawn_node(
                &ctx.node_type,
                &ctx.node_id,
                &ctx.params,
            ).await?;

            // Add process to session
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(&session_id) {
                session.node_processes.insert(ctx.node_id.clone(), process);
            }
        }

        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        #[cfg(feature = "multiprocess")]
        {
            // Get current context
            let ctx = self.current_context.as_ref()
                .ok_or_else(|| Error::Execution("Node not initialized".to_string()))?;

            let session_id = ctx.session_id.clone()
                .unwrap_or_else(|| format!("default_{}", ctx.node_id));

            // Send input to node process via IPC
            // This is a placeholder - actual IPC implementation will be in ipc_channel.rs
            tracing::debug!(
                "Processing input in multiprocess node: {}",
                ctx.node_id
            );

            // For now, just pass through
            Ok(vec![input])
        }

        #[cfg(not(feature = "multiprocess"))]
        {
            Err(Error::Execution("Multiprocess support not enabled".to_string()))
        }
    }

    async fn cleanup(&mut self) -> Result<()> {
        if let Some(ctx) = &self.current_context {
            let session_id = ctx.session_id.clone()
                .unwrap_or_else(|| format!("default_{}", ctx.node_id));

            // Terminate the session
            self.terminate_session(&session_id).await?;
        }

        self.current_context = None;
        Ok(())
    }

    fn is_streaming(&self) -> bool {
        // Multiprocess nodes can support streaming
        true
    }

    async fn finish_streaming(&mut self) -> Result<Vec<Value>> {
        // Flush any remaining data from channels
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_multiprocess_executor_creation() {
        let config = MultiprocessConfig::default();
        let executor = MultiprocessExecutor::new(config);

        // Create a test session
        executor.create_session("test_session".to_string()).await.unwrap();

        // Verify session exists
        let sessions = executor.sessions.read().await;
        assert!(sessions.contains_key("test_session"));

        // Cleanup
        drop(sessions);
        executor.terminate_session("test_session").await.unwrap();
    }

    #[tokio::test]
    async fn test_node_initialization() {
        let config = MultiprocessConfig::default();
        let mut executor = MultiprocessExecutor::new(config);

        let ctx = NodeContext {
            node_id: "test_node".to_string(),
            node_type: "test_processor".to_string(),
            params: serde_json::json!({"param": "value"}),
            session_id: Some("test_session".to_string()),
            metadata: HashMap::new(),
        };

        // Initialize should create session and prepare for process spawn
        executor.initialize(&ctx).await.unwrap();

        // Cleanup
        executor.cleanup().await.unwrap();
    }
}