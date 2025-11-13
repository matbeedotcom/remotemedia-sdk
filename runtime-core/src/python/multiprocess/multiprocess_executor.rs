//! Multiprocess executor for Python nodes
//!
//! Implements NodeExecutor trait to manage Python nodes running in separate processes
//! with iceoryx2 shared memory IPC for zero-copy data transfer.

use crate::executor::node_executor::{
    NodeContext as ExecutorNodeContext, NodeExecutor as ExecutorNodeExecutor,
};
use crate::nodes::{NodeContext as NodesNodeContext, NodeExecutor as NodesNodeExecutor};
use crate::{Error, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

#[cfg(feature = "multiprocess")]
use super::data_transfer::RuntimeData as IPCRuntimeData;
#[cfg(feature = "multiprocess")]
use super::health_monitor::{HealthMonitor, ProcessEvent};
#[cfg(feature = "multiprocess")]
use super::ipc_channel::{ChannelHandle, ChannelRegistry};
#[cfg(feature = "multiprocess")]
use super::process_manager::{ExitReason, ProcessHandle, ProcessManager};

#[cfg(feature = "docker")]
use super::docker_support::DockerSupport;

// NOTE: Publisher caching attempts and why they failed:
//
// ATTEMPT 1: Global static cache with Mutex
//   - Problem: Publisher contains Rc<> which is !Send + !Sync
//   - Error: "cannot be sent/shared between threads safely"
//   - Code: static PUBLISHER_CACHE: OnceLock<Mutex<HashMap<String, Publisher>>> = ...
//
// ATTEMPT 2: Thread-local storage (TLS)
//   - Problem: Publisher has lifetime tied to ChannelRegistry (&'a ChannelRegistry)
//   - Error: "does not live long enough" - registry lives in spawn_blocking closure
//   - Publishers can't outlive the registry that created them
//   - Code: thread_local! { static PUBLISHER_CACHE: RefCell<HashMap<...>> = ... }
//
// ATTEMPT 3: Store publishers in SessionState
//   - Problem: Publisher is !Send, can't be stored in async-accessible SessionState
//   - Would require wrapping in unsafe or redesigning entire session storage
//
// ROOT CAUSE:
// iceoryx2::Publisher is !Send + !Sync and has lifetime bounds to its registry.
// This makes it impossible to cache in Rust's standard patterns (static, TLS, Arc).
//
// CURRENT SOLUTION:
// Create a new publisher for each send with a 50ms delay for iceoryx2 routing.
// This is correct but adds latency. Future optimization would require:
// - Restructuring to create publishers during initialization
// - Storing them in a non-Send-friendly way (e.g., blocking thread with channels)
// - Or contributing to iceoryx2 to make Publisher Send+Sync

/// Global session storage
/// Maps session_id -> HashMap<node_id, IPC thread command sender>
/// This is shared across all MultiprocessExecutor instances to ensure
/// session_router can find IPC threads regardless of which executor instance it uses
#[cfg(feature = "multiprocess")]
static GLOBAL_SESSIONS: OnceLock<
    Arc<RwLock<HashMap<String, HashMap<String, tokio::sync::mpsc::Sender<IpcCommand>>>>>,
> = OnceLock::new();

#[cfg(feature = "multiprocess")]
fn global_sessions(
) -> Arc<RwLock<HashMap<String, HashMap<String, tokio::sync::mpsc::Sender<IpcCommand>>>>> {
    GLOBAL_SESSIONS
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

/// Commands sent to the dedicated IPC thread
#[cfg(feature = "multiprocess")]
enum IpcCommand {
    /// Send data to the node's input channel
    SendData { data: IPCRuntimeData },
    /// Register a callback for continuous output forwarding
    RegisterOutputCallback {
        callback_tx: tokio::sync::mpsc::UnboundedSender<IPCRuntimeData>,
    },
    /// Request graceful shutdown of the IPC thread
    Shutdown,
}

/// Responses from the dedicated IPC thread
#[cfg(feature = "multiprocess")]
enum IpcResponse {
    /// Data received from the node's output channel
    OutputData(IPCRuntimeData),
    /// Acknowledgment that data was sent successfully
    SendComplete,
    /// Error occurred during IPC operation
    Error(String),
}

/// Handle to a node's dedicated IPC thread
#[cfg(feature = "multiprocess")]
pub struct NodeIpcThread {
    /// Channel to send commands to the IPC thread
    command_tx: tokio::sync::mpsc::Sender<IpcCommand>,
    /// Channel to receive responses from the IPC thread
    response_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<IpcResponse>>>,
    /// Handle to the OS thread (for cleanup)
    thread_handle: Option<std::thread::JoinHandle<()>>,
    /// Node ID this thread serves
    node_id: String,
}

#[cfg(feature = "multiprocess")]
impl std::fmt::Debug for NodeIpcThread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeIpcThread")
            .field("node_id", &self.node_id)
            .field("thread_handle", &self.thread_handle.is_some())
            .finish()
    }
}

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
    300  // 5 minutes for model loading (e.g., Kokoro TTS)
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
            init_timeout_secs: 300,  // 5 minutes for model loading
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
        let contents = std::fs::read_to_string(path).map_err(|e| {
            Error::ConfigError(format!("Failed to read config file {:?}: {}", path, e))
        })?;

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

    /// Dedicated IPC threads for each node (one thread per node)
    /// These threads own the persistent publishers/subscribers
    #[cfg(feature = "multiprocess")]
    pub ipc_threads: HashMap<String, NodeIpcThread>,

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
    current_context: Option<ExecutorNodeContext>,

    /// Docker support
    #[cfg(feature = "docker")]
    docker_support: Option<Arc<DockerSupport>>,
}

impl MultiprocessExecutor {
    /// Process RuntimeData with streaming callback via IPC channels
    #[cfg(feature = "multiprocess")]
    pub async fn process_runtime_data_streaming<F>(
        &self, // Changed from &mut self - all state is behind Arc/RwLock
        input: crate::data::RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize>
    where
        F: FnMut(crate::data::RuntimeData) -> Result<()> + Send,
    {
        let ctx = self
            .current_context
            .as_ref()
            .ok_or_else(|| Error::Execution("Node not initialized".to_string()))?;

        let session_id = session_id
            .or_else(|| ctx.session_id.clone())
            .unwrap_or_else(|| format!("default_{}", ctx.node_id));

        tracing::info!(
            "MultiprocessExecutor::process_runtime_data_streaming for node {} (session: {})",
            ctx.node_id,
            session_id
        );

        // Convert input to IPC format
        let ipc_data = Self::to_ipc_runtime_data(&input, &session_id);

        tracing::info!(
            "[Multiprocess] Converted input data for node '{}': {:?} with {} bytes payload",
            ctx.node_id,
            ipc_data.data_type,
            ipc_data.payload.len()
        );

        // Get the IPC thread for this node
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        let ipc_thread_cmd_tx = session
            .ipc_threads
            .get(&ctx.node_id)
            .ok_or_else(|| {
                Error::Execution(format!("IPC thread not found for node {}", ctx.node_id))
            })?
            .command_tx
            .clone();

        let ipc_thread_resp_rx = session
            .ipc_threads
            .get(&ctx.node_id)
            .ok_or_else(|| {
                Error::Execution(format!("IPC thread not found for node {}", ctx.node_id))
            })?
            .response_rx
            .clone();

        drop(sessions); // Release read lock

        // Send data to IPC thread (fast, no blocking!)
        tracing::info!(
            "[Multiprocess] Sending data to IPC thread for node '{}'",
            ctx.node_id
        );
        ipc_thread_cmd_tx
            .send(IpcCommand::SendData { data: ipc_data })
            .await
            .map_err(|e| Error::Execution(format!("Failed to send to IPC thread: {}", e)))?;

        // Wait for send acknowledgment
        let mut resp_rx = ipc_thread_resp_rx.lock().await;
        match tokio::time::timeout(std::time::Duration::from_secs(5), resp_rx.recv()).await {
            Ok(Some(IpcResponse::SendComplete)) => {
                tracing::debug!("Data sent successfully to node: {}", ctx.node_id);
            }
            Ok(Some(IpcResponse::Error(e))) => {
                return Err(Error::Execution(format!("IPC send error: {}", e)));
            }
            Ok(None) | Err(_) => {
                return Err(Error::Execution(format!(
                    "Timeout waiting for send confirmation"
                )));
            }
            _ => {}
        }

        // Collect outputs from IPC thread continuously
        // The callback will control when to stop (e.g., based on end markers)
        let mut output_count = 0;

        tracing::info!(
            "[Multiprocess] Continuously collecting output from node '{}'",
            ctx.node_id
        );

        loop {
            tracing::debug!("[Multiprocess] Waiting for response from IPC thread for node '{}'", ctx.node_id);

            match resp_rx.recv().await {
                Some(IpcResponse::OutputData(ipc_output)) => {
                    tracing::debug!(
                        "[Multiprocess] Received OutputData from node '{}': type={:?}, {} bytes",
                        ctx.node_id,
                        ipc_output.data_type,
                        ipc_output.payload.len()
                    );

                    let output_data = Self::from_ipc_runtime_data(ipc_output)?;

                    tracing::debug!("[Multiprocess] Converted to RuntimeData, invoking callback for node '{}'", ctx.node_id);

                    // Forward all outputs - the callback decides when to stop
                    match callback(output_data) {
                        Ok(_) => {
                            output_count += 1;
                            tracing::debug!("[Multiprocess] Callback succeeded for node '{}', total outputs: {}", ctx.node_id, output_count);
                        }
                        Err(e) => {
                            // Callback returned error - stop collecting
                            tracing::info!(
                                "Callback stopped collection for node {} after {} outputs: {}",
                                ctx.node_id,
                                output_count,
                                e
                            );
                            return Ok(output_count);
                        }
                    }
                }
                Some(IpcResponse::SendComplete) => {
                    tracing::debug!("[Multiprocess] Received SendComplete from node '{}', continuing to poll for outputs", ctx.node_id);
                    // Acknowledgment - ignore and continue polling for outputs
                    continue;
                }
                Some(IpcResponse::Error(e)) => {
                    tracing::error!("[Multiprocess] IPC error from node '{}': {}", ctx.node_id, e);
                    return Err(Error::Execution(format!("IPC error: {}", e)));
                }
                None => {
                    // IPC thread disconnected
                    tracing::info!(
                        "IPC thread disconnected for node {} after {} outputs",
                        ctx.node_id,
                        output_count
                    );
                    return Ok(output_count);
                }
            }
        }
    }

    /// Register a callback for continuous output forwarding from a node's IPC thread
    /// The callback will receive ALL outputs from the node, independent of any input processing
    #[cfg(feature = "multiprocess")]
    pub async fn register_output_callback(
        &self,
        node_id: &str,
        session_id: &str,
        callback_tx: tokio::sync::mpsc::UnboundedSender<IPCRuntimeData>,
    ) -> Result<()> {
        // Get the IPC thread from global sessions storage
        let global_sessions = global_sessions();
        let sessions = global_sessions.read().await;

        let session = sessions
            .get(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        let ipc_thread_cmd_tx = session
            .get(node_id)
            .ok_or_else(|| Error::Execution(format!("IPC thread not found for node {}", node_id)))?
            .clone();

        drop(sessions);

        // Send register command to IPC thread
        ipc_thread_cmd_tx
            .send(IpcCommand::RegisterOutputCallback { callback_tx })
            .await
            .map_err(|e| {
                Error::Execution(format!(
                    "Failed to register callback for node {}: {}",
                    node_id, e
                ))
            })?;

        tracing::info!(
            "Registered output callback for node '{}' in session '{}'",
            node_id,
            session_id
        );
        Ok(())
    }

    /// Send data to a specific node without collecting outputs
    /// Use this when a background output draining task is already registered
    #[cfg(feature = "multiprocess")]
    pub async fn send_data_to_node(
        &self,
        node_id: &str,
        session_id: &str,
        input: crate::data::RuntimeData,
    ) -> Result<()> {
        tracing::debug!(
            "send_data_to_node: Sending to node '{}' in session '{}'",
            node_id,
            session_id
        );

        // Convert input to IPC format
        let ipc_data = Self::to_ipc_runtime_data(&input, session_id);

        // Get the IPC thread from global sessions storage
        let global_sessions = global_sessions();
        let sessions = global_sessions.read().await;

        let session = sessions
            .get(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        let ipc_thread_cmd_tx = session
            .get(node_id)
            .ok_or_else(|| Error::Execution(format!("IPC thread not found for node {}", node_id)))?
            .clone();

        drop(sessions);

        // Send data to IPC thread (no waiting for outputs)
        tracing::info!(
            "[Multiprocess] Sending data to node '{}' via IPC thread (fire-and-forget)",
            node_id
        );
        ipc_thread_cmd_tx
            .send(IpcCommand::SendData { data: ipc_data })
            .await
            .map_err(|e| {
                Error::Execution(format!(
                    "Failed to send to IPC thread for node {}: {}",
                    node_id, e
                ))
            })?;

        Ok(())
    }

    /// Send data to a node's IPC thread without waiting for response (fire-and-forget)
    /// This is used for routing data between nodes in the pipeline
    #[cfg(feature = "multiprocess")]
    pub async fn send_to_node_async(
        &self,
        node_id: &str,
        session_id: &str,
        data: crate::data::RuntimeData,
    ) -> Result<()> {
        // Convert to IPC format
        let ipc_data = Self::to_ipc_runtime_data(&data, session_id);

        // Get the IPC thread from global sessions storage
        let global_sessions = global_sessions();
        let sessions = global_sessions.read().await;

        let session = sessions.get(session_id).ok_or_else(|| {
            let available: Vec<_> = sessions.keys().collect();
            Error::Execution(format!(
                "Session {} not found in global sessions. Available: {:?}",
                session_id, available
            ))
        })?;

        let ipc_thread_cmd_tx = session
            .get(node_id)
            .ok_or_else(|| {
                let available: Vec<_> = session.keys().collect();
                Error::Execution(format!(
                    "IPC thread not found for node {} in session {}. Available nodes: {:?}",
                    node_id, session_id, available
                ))
            })?
            .clone();

        drop(sessions); // Release read lock

        // Send data to IPC thread (no waiting for response)
        tracing::debug!(
            "send_to_node_async: Sending data to node '{}' in session '{}'",
            node_id,
            session_id
        );
        ipc_thread_cmd_tx
            .send(IpcCommand::SendData { data: ipc_data })
            .await
            .map_err(|e| {
                Error::Execution(format!(
                    "Failed to send to IPC thread for node {}: {}",
                    node_id, e
                ))
            })?;

        Ok(())
    }

    /// Convert main RuntimeData to IPC RuntimeData
    #[cfg(feature = "multiprocess")]
    pub fn to_ipc_runtime_data(
        data: &crate::data::RuntimeData,
        session_id: &str,
    ) -> IPCRuntimeData {
        
        use crate::data::RuntimeData as MainRD;

        match data {
            MainRD::Text(text) => IPCRuntimeData::text(text, session_id),
            MainRD::Audio {
                samples,
                sample_rate,
                channels,
            } => {
                // RuntimeData::Audio has inline f32 samples, convert directly
                IPCRuntimeData::audio(samples, *sample_rate, *channels as u16, session_id)
            }
            MainRD::Binary(bytes) => {
                // Binary data
                IPCRuntimeData::text(&format!("Binary data: {} bytes", bytes.len()), session_id)
            }
            MainRD::ControlMessage {
                message_type,
                segment_id,
                timestamp_ms,
                metadata,
            } => {
                // Spec 007: Control message for flow control
                IPCRuntimeData::control_message(
                    message_type,
                    segment_id.as_deref(),
                    *timestamp_ms,
                    metadata,
                    session_id,
                )
            }
            _ => {
                // For now, convert unsupported types to text representation
                IPCRuntimeData::text(&format!("{:?}", data), session_id)
            }
        }
    }

    /// Convert IPC RuntimeData back to main RuntimeData
    #[cfg(feature = "multiprocess")]
    pub fn from_ipc_runtime_data(ipc_data: IPCRuntimeData) -> Result<crate::data::RuntimeData> {
        use super::data_transfer::DataType;
        use crate::data::RuntimeData as MainRD;

        match ipc_data.data_type {
            DataType::Text => {
                let text = String::from_utf8_lossy(&ipc_data.payload).to_string();
                Ok(MainRD::Text(text))
            }
            DataType::Audio => {
                // IPC payload is f32 samples as little-endian bytes
                let samples: Vec<f32> = ipc_data
                    .payload
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                // Create audio with inline fields
                Ok(MainRD::Audio {
                    samples,
                    sample_rate: 24000, // TODO: Extract from IPC metadata
                    channels: 1,        // TODO: Extract from IPC metadata
                })
            }
            _ => Err(Error::Execution(format!(
                "Unsupported IPC data type: {:?}",
                ipc_data.data_type
            ))),
        }
    }

    /// Create a new multiprocess executor
    pub fn new(config: MultiprocessConfig) -> Self {
        #[cfg(feature = "multiprocess")]
        let health_monitor = Arc::new(HealthMonitor::new(config.init_timeout_secs));

        // Use the global shared channel registry to ensure all executors
        // share the same iceoryx2 Node instance
        let channel_registry = ChannelRegistry::global();

        // Initialize Docker support if feature is enabled
        // Note: Docker initialization is deferred to avoid blocking in constructors
        // Use docker_support() method which will lazily initialize if needed
        #[cfg(feature = "docker")]
        let docker_support = None;

        let executor = Self {
            process_manager: Arc::new(ProcessManager::new(config.clone())),
            channel_registry,
            #[cfg(feature = "multiprocess")]
            health_monitor: health_monitor.clone(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
            current_context: None,
            #[cfg(feature = "docker")]
            docker_support,
        };

        // Setup pipeline termination on node failure
        #[cfg(feature = "multiprocess")]
        executor.setup_failure_handling();

        executor
    }

    /// Get the channel registry for IPC communication
    pub fn channel_registry(&self) -> &Arc<ChannelRegistry> {
        &self.channel_registry
    }

    /// Get the Docker support instance if available
    #[cfg(feature = "docker")]
    pub fn docker_support(&self) -> Option<&Arc<DockerSupport>> {
        self.docker_support.as_ref()
    }

    /// Initialize Docker support asynchronously
    /// This should be called once during executor initialization to enable Docker functionality
    #[cfg(feature = "docker")]
    pub async fn initialize_docker_support(&mut self) -> Result<()> {
        if self.docker_support.is_some() {
            // Already initialized
            return Ok(());
        }

        match DockerSupport::new().await {
            Ok(ds) => {
                tracing::info!("Docker support initialized successfully");
                self.docker_support = Some(Arc::new(ds));
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Docker support unavailable: {}. Falling back to regular multiprocess.", e);
                // Don't fail - just continue without Docker support
                Ok(())
            }
        }
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
            process_manager
                .on_process_exit(move |pid, reason| {
                    let sessions = sessions_for_handler.clone();
                    let health_monitor = health_monitor_for_handler.clone();
                    let process_manager_clone = process_manager_for_handler.clone();

                    tokio::spawn(async move {
                        // Find which session this process belongs to
                        let session_info = {
                            let sessions_guard = sessions.read().await;
                            sessions_guard
                                .values()
                                .find(|s| s.node_processes.values().any(|p| p.id == pid))
                                .map(|s| {
                                    (
                                        s.session_id.clone(),
                                        s.node_processes
                                            .iter()
                                            .find(|(_, p)| p.id == pid)
                                            .map(|(node_id, _)| node_id.clone()),
                                    )
                                })
                        };

                        if let Some((session_id, Some(node_id))) = session_info {
                            // Handle process exit
                            let _ = health_monitor
                                .handle_process_exit(
                                    pid,
                                    reason.clone(),
                                    Some(session_id.clone()),
                                    Some(node_id.clone()),
                                )
                                .await;

                            // Terminate pipeline on error
                            match reason {
                                ExitReason::Error(_) | ExitReason::Killed | ExitReason::Timeout => {
                                    tracing::error!(
                                        "Node {} (PID {}) failed, terminating pipeline {}",
                                        node_id,
                                        pid,
                                        session_id
                                    );

                                    // Terminate the entire session
                                    if let Err(e) = Self::terminate_session_static(
                                        sessions.clone(),
                                        process_manager_clone,
                                        &session_id,
                                    )
                                    .await
                                    {
                                        tracing::error!(
                                            "Failed to terminate session {}: {}",
                                            session_id,
                                            e
                                        );
                                    }
                                }
                                ExitReason::Normal => {
                                    tracing::info!(
                                        "Node {} (PID {}) exited normally",
                                        node_id,
                                        pid
                                    );
                                }
                            }
                        }
                    });
                })
                .await;
        });

        // Register health event handler for pipeline termination
        let sessions_for_events = sessions.clone();

        tokio::spawn(async move {
            health_monitor
                .on_event(move |event| {
                    if let ProcessEvent::PipelineTerminated {
                        session_id,
                        failed_node,
                        reason,
                        ..
                    } = event
                    {
                        tracing::error!(
                            "Pipeline {} terminated: node {} failed - {}",
                            session_id,
                            failed_node,
                            reason
                        );

                        let sessions = sessions_for_events.clone();
                        tokio::spawn(async move {
                            // Update session status
                            if let Some(session) = sessions.write().await.get_mut(&session_id) {
                                session.status = SessionStatus::Error(reason);
                            }
                        });
                    }
                })
                .await;
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
                process_manager
                    .terminate_process(process, std::time::Duration::from_secs(5))
                    .await?;
            }

            // Channels will be cleaned up when processes exit
        }

        Ok(())
    }

    /// Create a new session for pipeline execution
    pub async fn create_session(&self, session_id: String) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        // Check if session already exists
        if sessions.contains_key(&session_id) {
            return Err(Error::Execution(format!(
                "Session {} already exists",
                session_id
            )));
        }

        // Create new session state
        let session = SessionState {
            session_id: session_id.clone(),
            node_processes: HashMap::new(),
            channels: HashMap::new(),
            #[cfg(feature = "multiprocess")]
            ipc_threads: HashMap::new(),
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
            let session = sessions
                .get(session_id)
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
                    session
                        .init_progress
                        .get(node_id)
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

            // Yield to allow other tasks to run before checking again
            tokio::task::yield_now().await;
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
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        // Get node type from process handle
        let node_type = session
            .node_processes
            .get(node_id)
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

        session
            .init_progress
            .insert(node_id.to_string(), init_progress.clone());

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
        let session = sessions
            .get(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        Ok(session.init_progress.values().cloned().collect())
    }

    /// Wait for a Python process to signal it's ready via iceoryx2 control channel
    async fn wait_for_ready_signal_ipc(
        &self,
        session_id: &str,
        node_id: &str,
        timeout: std::time::Duration,
    ) -> Result<bool> {
        let start = std::time::Instant::now();
        let control_channel_name = format!("control/{}_{}", session_id, node_id);

        tracing::info!("Subscribing to control channel: {}", control_channel_name);

        // Create subscriber for the control channel in a blocking task
        let registry = self.channel_registry.clone();
        let channel_name = control_channel_name.clone();
        let node_id_clone = node_id.to_string();

        // First, create the control channel (Python will open_or_create)
        self.channel_registry
            .create_channel(
                &control_channel_name,
                10,    // Small capacity for control messages
                false, // No backpressure for control
            )
            .await?;

        // Now poll for the READY signal - need direct iceoryx2 subscriber (not RuntimeData wrapper)
        let ready = tokio::task::spawn_blocking(move || -> Result<bool> {
            let handle = tokio::runtime::Handle::current();

            // Create raw subscriber for control channel (bypasses RuntimeData deserialization)
            let subscriber = handle.block_on(registry.create_raw_subscriber(&channel_name))?;

            tracing::info!("Control channel subscriber created, polling for READY signal...");
            let mut poll_count = 0;

            loop {
                // Check timeout
                if start.elapsed() > timeout {
                    tracing::warn!(
                        "Timeout waiting for READY signal from node {} after {} polls",
                        node_id_clone,
                        poll_count
                    );
                    return Ok(false);
                }

                poll_count += 1;
                if poll_count % 100 == 0 {
                    tracing::debug!(
                        "Control channel poll #{} - still waiting for READY",
                        poll_count
                    );
                }

                // Try to receive READY signal (raw bytes, not RuntimeData)
                match subscriber
                    .receive()
                    .map_err(|e| Error::Execution(format!("Receive error: {:?}", e)))?
                {
                    Some(sample) => {
                        let bytes = sample.payload();

                        // Debug: log what we received
                        tracing::info!(
                            "Received control message #{} - {} bytes: {:?}, as_str: {:?}",
                            poll_count,
                            bytes.len(),
                            bytes,
                            std::str::from_utf8(bytes).unwrap_or("<invalid utf8>")
                        );

                        // Check if it's the READY signal
                        if bytes == b"READY" {
                            tracing::info!("Node {} signaled READY via iceoryx2", node_id_clone);
                            return Ok(true);
                        } else {
                            tracing::warn!(
                                "Received non-READY control message - expected 'READY', got: {:?}",
                                std::str::from_utf8(bytes).unwrap_or("<invalid utf8>")
                            );
                        }
                    }
                    None => {
                        // No data yet, yield to scheduler
                        std::thread::yield_now();
                    }
                }
            }
        })
        .await
        .map_err(|e| Error::Execution(format!("Join error: {}", e)))??;

        Ok(ready)
    }

    /// Spawn a dedicated IPC thread for a node
    /// This thread owns persistent publishers/subscribers and never recreates them
    #[cfg(feature = "multiprocess")]
    async fn spawn_ipc_thread(
        &self,
        node_id: &str,
        session_id: &str,
        input_channel_name: &str,
        output_channel_name: &str,
    ) -> Result<NodeIpcThread> {
        // Create channels for async <-> thread communication
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<IpcCommand>(100);
        let (resp_tx, resp_rx) = tokio::sync::mpsc::channel::<IpcResponse>(100);

        let registry = self.channel_registry.clone();
        let input_ch = input_channel_name.to_string();
        let output_ch = output_channel_name.to_string();
        let node_id_clone = node_id.to_string();

        // Spawn dedicated OS thread
        let handle = std::thread::spawn(move || {
            tracing::info!("IPC thread starting for node: {}", node_id_clone);

            // Create tokio runtime for this thread (needed for async create_publisher/subscriber)
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime for IPC thread");

            // Create persistent publishers/subscribers ONCE
            let (publisher, subscriber) = rt.block_on(async {
                let pub_result = registry.create_publisher(&input_ch).await;
                let sub_result = registry.create_subscriber(&output_ch).await;
                (pub_result, sub_result)
            });

            let publisher = match publisher {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Failed to create publisher: {}", e);
                    return;
                }
            };

            let subscriber = match subscriber {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create subscriber: {}", e);
                    return;
                }
            };

            // CRITICAL: One-time delay for iceoryx2 routing to stabilize
            // std::thread::sleep(std::time::Duration::from_millis(50));
            tracing::info!(
                "IPC thread ready for node: {} (publishers created)",
                node_id_clone
            );

            // Optional callback for continuous output forwarding
            let mut output_callback: Option<tokio::sync::mpsc::UnboundedSender<IPCRuntimeData>> =
                None;

            // Main loop: process commands using persistent publishers/subscribers
            loop {
                // Check for commands (non-blocking poll)
                match cmd_rx.try_recv() {
                    Ok(IpcCommand::SendData { data }) => {
                        // Send using persistent publisher (no delay needed!)
                        tracing::debug!(
                            "IPC thread sending {} bytes for node: {}",
                            data.payload.len(),
                            node_id_clone
                        );

                        if let Err(e) = publisher.publish(data) {
                            let _ = resp_tx.blocking_send(IpcResponse::Error(format!(
                                "Publish failed: {}",
                                e
                            )));
                            continue;
                        }

                        // Acknowledge send
                        let _ = resp_tx.blocking_send(IpcResponse::SendComplete);

                        // Continue polling in the main loop - don't break out!
                        // The subscriber will be polled continuously even between commands
                        // to ensure we capture all output from streaming nodes
                    }
                    Ok(IpcCommand::RegisterOutputCallback { callback_tx }) => {
                        tracing::info!(
                            "IPC thread registered output callback for node: {}",
                            node_id_clone
                        );
                        output_callback = Some(callback_tx);
                    }
                    Ok(IpcCommand::Shutdown) => {
                        tracing::info!("IPC thread shutting down for node: {}", node_id_clone);
                        break;
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                        // No command, poll subscriber for incoming data
                        match subscriber.receive() {
                            Ok(Some(output_data)) => {
                                // If we have a registered callback, use it (for continuous forwarding)
                                if let Some(ref cb) = output_callback {
                                    if let Err(e) = cb.send(output_data.clone()) {
                                        tracing::error!(
                                            "Failed to send output via callback for node {}: {}",
                                            node_id_clone,
                                            e
                                        );
                                        // Callback channel closed, clear it
                                        output_callback = None;
                                    }
                                }
                                // Also send via response channel (for backwards compat)
                                let _ = resp_tx.blocking_send(IpcResponse::OutputData(output_data));
                            }
                            Ok(None) => {
                                // No data, yield to scheduler (avoids 100% CPU but minimal latency)
                                std::thread::yield_now();
                            }
                            Err(e) => {
                                tracing::error!("Receive error for node {}: {}", node_id_clone, e);
                            }
                        }
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        tracing::info!("Command channel closed for node: {}", node_id_clone);
                        break;
                    }
                }
            }

            tracing::info!("IPC thread exited for node: {}", node_id_clone);
        });

        Ok(NodeIpcThread {
            command_tx: cmd_tx,
            response_rx: Arc::new(tokio::sync::Mutex::new(resp_rx)),
            thread_handle: Some(handle),
            node_id: node_id.to_string(),
        })
    }

    /// Terminate a session and cleanup all associated resources
    ///
    /// This method performs comprehensive cleanup in the following order:
    /// 1. Shuts down IPC threads and removes from global sessions storage
    /// 2. Cleans up Docker containers (if Docker support is enabled)
    /// 3. Terminates all Python node processes
    /// 4. Cleans up iceoryx2 IPC channels
    ///
    /// # Arguments
    /// * `session_id` - The unique session identifier to terminate
    ///
    /// # Returns
    /// * `Ok(())` - Session successfully terminated and resources cleaned up
    /// * `Err(Error)` - If session not found or process termination fails
    ///
    /// # Docker Cleanup (T019)
    /// When Docker support is enabled, this method automatically:
    /// - Lists all containers labeled with `remotemedia.session_id=<session_id>`
    /// - Gracefully stops running containers with 5-second timeout
    /// - Forcefully removes containers and associated volumes
    /// - Logs warnings for any containers that fail to clean up (doesn't fail the overall cleanup)
    ///
    /// # IPC Cleanup
    /// The method ensures:
    /// - All IPC threads receive shutdown commands
    /// - Channels are properly destroyed
    /// - Global session storage is updated
    ///
    /// # Error Handling
    /// - Docker cleanup errors are logged as warnings but do not fail the entire session termination
    /// - Process termination errors are propagated as failures
    /// - This ensures robust cleanup even if some resources are unavailable
    pub async fn terminate_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(mut session) = sessions.remove(session_id) {
            session.status = SessionStatus::Terminating;

            // Shutdown IPC threads first
            #[cfg(feature = "multiprocess")]
            {
                for (node_id, mut ipc_thread) in session.ipc_threads.drain() {
                    tracing::info!("Shutting down IPC thread for node: {}", node_id);

                    // Send shutdown command
                    let _ = ipc_thread.command_tx.send(IpcCommand::Shutdown).await;

                    // Wait for thread to exit (with timeout)
                    if let Some(handle) = ipc_thread.thread_handle.take() {
                        std::thread::spawn(move || {
                            let _ = handle.join();
                        });
                    }
                }

                // Remove session from global sessions storage
                let global_sessions = global_sessions();
                let mut global_sessions_guard = global_sessions.write().await;
                global_sessions_guard.remove(session_id);
                tracing::info!(
                    "Removed session {} from global sessions storage",
                    session_id
                );
                drop(global_sessions_guard);
            }

            // Cleanup Docker containers before terminating processes
            #[cfg(feature = "docker")]
            {
                if let Some(docker_support) = &self.docker_support {
                    tracing::info!(
                        "Cleaning up Docker containers for session: {}",
                        session_id
                    );

                    // Use the built-in cleanup_session_containers which is more efficient
                    // It uses Docker labels to find and clean up all containers for this session
                    match docker_support.cleanup_session_containers(session_id).await {
                        Ok(removed_containers) => {
                            if !removed_containers.is_empty() {
                                tracing::info!(
                                    "Cleaned up {} Docker containers for session {}",
                                    removed_containers.len(),
                                    session_id
                                );
                                for container_id in removed_containers {
                                    tracing::debug!(
                                        "Removed Docker container: {} from session {}",
                                        container_id,
                                        session_id
                                    );
                                }
                            } else {
                                tracing::debug!(
                                    "No Docker containers found for session {} to clean up",
                                    session_id
                                );
                            }
                        }
                        Err(e) => {
                            // Log warning but don't fail the entire cleanup process
                            // Docker cleanup is important but should not block session termination
                            tracing::warn!(
                                "Failed to clean up Docker containers for session {}: {}. \
                                 This may leave orphaned containers that require manual cleanup.",
                                session_id,
                                e
                            );
                        }
                    }
                }
            }

            // Then terminate processes
            #[cfg(feature = "multiprocess")]
            {
                for (_, process) in session.node_processes.drain() {
                    self.process_manager
                        .terminate_process(process, std::time::Duration::from_secs(5))
                        .await?;
                }

                // Cleanup channels
                for (_, channel) in session.channels.drain() {
                    self.channel_registry.destroy_channel(channel).await?;
                }
            }

            Ok(())
        } else {
            Err(Error::Execution(format!(
                "Session {} not found",
                session_id
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

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        // Generate channel name if not provided
        let channel_name = channel_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}_to_{}", from_node, to_node));

        // Verify both nodes exist in the session
        if !session.node_processes.contains_key(from_node) {
            return Err(Error::Execution(format!(
                "Source node {} not found in session",
                from_node
            )));
        }
        if !session.node_processes.contains_key(to_node) {
            return Err(Error::Execution(format!(
                "Destination node {} not found in session",
                to_node
            )));
        }

        // Create the channel
        let channel = self
            .channel_registry
            .create_channel(
                &channel_name,
                self.config.channel_capacity,
                self.config.enable_backpressure,
            )
            .await?;

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
    pub async fn get_channel_stats(
        &self,
        session_id: &str,
        channel_name: &str,
    ) -> Result<super::ipc_channel::ChannelStats> {
        let sessions = self.sessions.read().await;

        let session = sessions
            .get(session_id)
            .ok_or_else(|| Error::Execution(format!("Session {} not found", session_id)))?;

        let channel = session
            .channels
            .get(channel_name)
            .ok_or_else(|| Error::Execution(format!("Channel {} not found", channel_name)))?;

        let stats = channel.stats.read().await;
        Ok(stats.clone())
    }
}

#[async_trait]
impl ExecutorNodeExecutor for MultiprocessExecutor {
    async fn initialize(&mut self, ctx: &ExecutorNodeContext) -> Result<()> {
        tracing::info!(
            "Initializing multiprocess node: {} ({})",
            ctx.node_id,
            ctx.node_type
        );

        // Store context for later use
        self.current_context = Some(ctx.clone());

        // Get or create session
        let session_id = ctx
            .session_id
            .clone()
            .unwrap_or_else(|| format!("default_{}", ctx.node_id));

        // Ensure session exists
        if !self.sessions.read().await.contains_key(&session_id) {
            self.create_session(session_id.clone()).await?;
        }

        // Determine execution mode from context metadata
        #[cfg(feature = "docker")]
        let use_docker = ctx
            .metadata
            .get("use_docker")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        #[cfg(feature = "docker")]
        let docker_config = if use_docker {
            // Parse Docker configuration from metadata
            ctx.metadata
                .get("docker_config")
                .and_then(|v| serde_json::from_value::<super::docker_support::DockerNodeConfig>(v.clone()).ok())
        } else {
            None
        };

        // Initialize Docker support if needed and not already initialized
        #[cfg(feature = "docker")]
        if use_docker && self.docker_support.is_none() {
            tracing::info!("Docker mode requested, initializing Docker support...");
            match DockerSupport::new().await {
                Ok(ds) => {
                    tracing::info!("Docker support initialized successfully");
                    self.docker_support = Some(Arc::new(ds));
                }
                Err(e) => {
                    tracing::warn!("Docker support unavailable: {}. Will attempt fallback to regular multiprocess.", e);
                    // Don't fail immediately - we'll handle this below when checking docker_support
                }
            }
        }

        // Create IPC channels BEFORE spawning process (must exist when Python connects)
        // Prefix with session_id to avoid conflicts and make cleanup easier
        #[cfg(feature = "multiprocess")]
        let (input_channel_name, output_channel_name, input_channel, output_channel) = {
            let input_channel_name = format!("{}_{}_input", session_id, ctx.node_id);
            let output_channel_name = format!("{}_{}_output", session_id, ctx.node_id);

            let input_channel = self
                .channel_registry
                .create_channel(
                    &input_channel_name,
                    self.config.channel_capacity,
                    self.config.enable_backpressure,
                )
                .await?;

            let output_channel = self
                .channel_registry
                .create_channel(
                    &output_channel_name,
                    self.config.channel_capacity,
                    self.config.enable_backpressure,
                )
                .await?;

            tracing::info!(
                "Pre-created IPC channels for node {}: {}, {}",
                ctx.node_id,
                input_channel_name,
                output_channel_name
            );

            (
                input_channel_name,
                output_channel_name,
                input_channel,
                output_channel,
            )
        };

        // NOW spawn process or container (channels already exist)
        #[cfg(feature = "multiprocess")]
        {
            let process = {
                #[cfg(feature = "docker")]
                {
                    if use_docker {
                        if let Some(docker_support) = &self.docker_support {
                            if let Some(docker_config) = docker_config {
                                // Validate Docker configuration
                                docker_config.validate()?;

                                tracing::info!(
                                    "Spawning Docker container for node '{}' with config: {:?}",
                                    ctx.node_id,
                                    docker_config
                                );

                                // Spawn Docker container
                                self.process_manager
                                    .spawn_docker_container(
                                        &ctx.node_type,
                                        &ctx.node_id,
                                        &ctx.params,
                                        &session_id,
                                        docker_support,
                                        &docker_config,
                                    )
                                    .await?
                            } else {
                                return Err(Error::Execution(
                                    "Docker mode enabled but no docker_config provided".to_string(),
                                ));
                            }
                        } else {
                            tracing::warn!(
                                "Docker mode requested but Docker support unavailable, falling back to regular process"
                            );
                            // Fall back to regular multiprocess execution
                            self.process_manager
                                .spawn_node(&ctx.node_type, &ctx.node_id, &ctx.params, &session_id)
                                .await?
                        }
                    } else {
                        // Regular multiprocess execution
                        self.process_manager
                            .spawn_node(&ctx.node_type, &ctx.node_id, &ctx.params, &session_id)
                            .await?
                    }
                }
                #[cfg(not(feature = "docker"))]
                {
                    // Regular multiprocess execution (Docker feature not enabled)
                    self.process_manager
                        .spawn_node(&ctx.node_type, &ctx.node_id, &ctx.params, &session_id)
                        .await?
                }
            };

            // Spawn dedicated IPC thread for this node
            let ipc_thread = self
                .spawn_ipc_thread(
                    &ctx.node_id,
                    &session_id,
                    &input_channel_name,
                    &output_channel_name,
                )
                .await?;

            // Register IPC thread in global sessions storage
            let global_sessions = global_sessions();
            let mut global_sessions_guard = global_sessions.write().await;
            global_sessions_guard
                .entry(session_id.clone())
                .or_insert_with(HashMap::new)
                .insert(ctx.node_id.clone(), ipc_thread.command_tx.clone());
            drop(global_sessions_guard);

            // Add process, channels, and IPC thread to local session
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(&session_id) {
                session.node_processes.insert(ctx.node_id.clone(), process);
                session
                    .channels
                    .insert(input_channel_name.clone(), input_channel);
                session
                    .channels
                    .insert(output_channel_name.clone(), output_channel);
                session.ipc_threads.insert(ctx.node_id.clone(), ipc_thread);

                tracing::info!(
                    "Created IPC thread and channels for node {}: {}, {}",
                    ctx.node_id,
                    input_channel_name,
                    output_channel_name
                );
            }
        }

        // Wait for Python process to signal READY
        #[cfg(feature = "multiprocess")]
        {
            tracing::info!("Waiting for Python process to signal READY via iceoryx2...");

            // Wait for Python to signal it's ready via iceoryx2 control channel
            // Python creates its input subscriber BEFORE sending READY, so when we receive
            // READY signal, Python is fully prepared to receive data
            // Wait for READY signal with configured timeout (allows time for heavy model loading)
            let ready = self
                .wait_for_ready_signal_ipc(
                    &session_id,
                    &ctx.node_id,
                    std::time::Duration::from_secs(self.config.init_timeout_secs),
                )
                .await?;
            if !ready {
                return Err(Error::Execution(format!(
                    "Node {} failed to signal ready within timeout",
                    ctx.node_id
                )));
            }

            tracing::info!("✅ Received READY signal - Python subscriber is ready to receive data");

            // Small delay to ensure Python subscriber is fully registered with iceoryx2's routing tables
            // The subscriber creation and READY signal are very close in time, but iceoryx2 needs a moment
            // to complete the internal pub/sub connection registration
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            tracing::info!(
                "✅ Node initialization complete, IPC thread ready with persistent publishers"
            );
        }

        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        #[cfg(feature = "multiprocess")]
        {
            // Get current context
            let ctx = self
                .current_context
                .as_ref()
                .ok_or_else(|| Error::Execution("Node not initialized".to_string()))?;

            let session_id = ctx
                .session_id
                .clone()
                .unwrap_or_else(|| format!("default_{}", ctx.node_id));

            // Send input to node process via IPC
            // This is a placeholder - actual IPC implementation will be in ipc_channel.rs
            tracing::debug!("Processing input in multiprocess node: {}", ctx.node_id);

            // For now, just pass through
            Ok(vec![input])
        }

        #[cfg(not(feature = "multiprocess"))]
        {
            Err(Error::Execution(
                "Multiprocess support not enabled".to_string(),
            ))
        }
    }

    async fn cleanup(&mut self) -> Result<()> {
        if let Some(ctx) = &self.current_context {
            let session_id = ctx
                .session_id
                .clone()
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

// Implement the old nodes::NodeExecutor trait for compatibility with PythonStreamingNode
#[async_trait]
impl NodesNodeExecutor for MultiprocessExecutor {
    async fn initialize(&mut self, ctx: &NodesNodeContext) -> Result<()> {
        // Convert to new NodeContext format
        let executor_ctx = ExecutorNodeContext {
            node_id: ctx.node_id.clone(),
            node_type: ctx.node_type.clone(),
            params: ctx.params.clone(),
            session_id: ctx.session_id.clone(),
            metadata: ctx.metadata.clone(),
        };

        // Delegate to new trait implementation
        ExecutorNodeExecutor::initialize(self, &executor_ctx).await
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        // Delegate to new trait implementation
        ExecutorNodeExecutor::process(self, input).await
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Delegate to new trait implementation
        ExecutorNodeExecutor::cleanup(self).await
    }

    fn is_streaming(&self) -> bool {
        // Multiprocess nodes can support streaming
        ExecutorNodeExecutor::is_streaming(self)
    }

    async fn finish_streaming(&mut self) -> Result<Vec<Value>> {
        // Delegate to new trait implementation
        ExecutorNodeExecutor::finish_streaming(self).await
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
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
        executor
            .create_session("test_session".to_string())
            .await
            .unwrap();

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

        let ctx = ExecutorNodeContext {
            node_id: "test_node".to_string(),
            node_type: "test_processor".to_string(),
            params: serde_json::json!({"param": "value"}),
            session_id: Some("test_session".to_string()),
            metadata: HashMap::new(),
        };

        // Initialize should create session and prepare for process spawn
        ExecutorNodeExecutor::initialize(&mut executor, &ctx)
            .await
            .unwrap();

        // Cleanup
        ExecutorNodeExecutor::cleanup(&mut executor).await.unwrap();
    }

    #[tokio::test]
    async fn test_session_termination_with_cleanup() {
        let config = MultiprocessConfig::default();
        let executor = MultiprocessExecutor::new(config);

        // Create a test session
        let session_id = "test_cleanup_session";
        executor
            .create_session(session_id.to_string())
            .await
            .expect("Failed to create session");

        // Verify session exists
        {
            let sessions = executor.sessions.read().await;
            assert!(sessions.contains_key(session_id), "Session should exist after creation");
        }

        // Terminate the session - this should trigger cleanup including Docker containers
        // if Docker support is enabled (graceful handling if not available)
        let result = executor.terminate_session(session_id).await;
        assert!(result.is_ok(), "Session termination should succeed: {:?}", result);

        // Verify session is removed
        {
            let sessions = executor.sessions.read().await;
            assert!(!sessions.contains_key(session_id), "Session should be removed after termination");
        }
    }

    #[tokio::test]
    async fn test_terminate_nonexistent_session() {
        let config = MultiprocessConfig::default();
        let executor = MultiprocessExecutor::new(config);

        // Try to terminate a non-existent session
        let result = executor.terminate_session("nonexistent_session").await;

        // Should return an error indicating session not found
        assert!(result.is_err(), "Terminating non-existent session should return an error");
        assert!(
            result.unwrap_err().to_string().contains("not found"),
            "Error should indicate session not found"
        );
    }
}
