//! Process management for multiprocess Python nodes
//!
//! Handles spawning, monitoring, and lifecycle management of Python node processes.

use crate::{Error, Result};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use serde_json::Value;
use std::time::{Duration, Instant};

use super::multiprocess_executor::MultiprocessConfig;

/// Process lifecycle states
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
    Idle,         // Process created but not initialized
    Initializing, // Loading models/resources
    Ready,        // Initialized and waiting for data
    Processing,   // Actively processing data
    Stopping,     // Graceful shutdown initiated
    Stopped,      // Process terminated
    Error(String), // Failed with error message
}

/// Exit reasons for process termination
#[derive(Debug, Clone)]
pub enum ExitReason {
    Normal,        // Clean exit
    Error(i32),    // Exit with error code
    Killed,        // Terminated by signal
    Timeout,       // Initialization timeout
}

/// Handle to a running process
#[derive(Debug, Clone)]
pub struct ProcessHandle {
    /// Process ID
    pub id: u32,

    /// Node ID in the pipeline
    pub node_id: String,

    /// Node type identifier
    pub node_type: String,

    /// Current process status
    pub status: Arc<RwLock<ProcessStatus>>,

    /// Process start time
    pub started_at: Instant,

    /// Internal process handle
    pub inner: Arc<Mutex<Option<Child>>>,
}

impl ProcessHandle {
    /// Check if the process is still alive
    pub async fn is_alive(&self) -> bool {
        if let Some(ref mut child) = *self.inner.lock().await {
            child.try_wait().ok().flatten().is_none()
        } else {
            false
        }
    }

    /// Get the exit status if process has terminated
    pub async fn exit_status(&self) -> Option<std::process::ExitStatus> {
        if let Some(ref mut child) = *self.inner.lock().await {
            child.try_wait().ok().flatten()
        } else {
            None
        }
    }

    /// Kill the process forcefully
    pub async fn kill(&self) -> Result<()> {
        if let Some(ref mut child) = *self.inner.lock().await {
            child.kill()
                .map_err(|e| Error::Execution(format!("Failed to kill process: {}", e)))?;
        }
        Ok(())
    }
}

/// Process spawn configuration
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    /// Python executable path
    pub python_executable: std::path::PathBuf,

    /// Additional Python path entries
    pub python_path: Vec<std::path::PathBuf>,

    /// Environment variables
    pub env_vars: HashMap<String, String>,

    /// Working directory
    pub working_dir: Option<std::path::PathBuf>,

    /// Capture stdout/stderr
    pub capture_output: bool,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            python_executable: std::path::PathBuf::from("python"),
            python_path: Vec::new(),
            env_vars: HashMap::new(),
            working_dir: None,
            capture_output: true,
        }
    }
}

/// Process manager for spawning and monitoring Python nodes
pub struct ProcessManager {
    /// Active processes
    processes: Arc<RwLock<HashMap<u32, ProcessHandle>>>,

    /// Spawn configuration
    spawn_config: SpawnConfig,

    /// Configuration
    config: MultiprocessConfig,

    /// Process event handlers
    exit_handlers: Arc<RwLock<Vec<Box<dyn Fn(u32, ExitReason) + Send + Sync>>>>,
}

impl ProcessManager {
    /// Create a new process manager
    pub fn new(config: MultiprocessConfig) -> Self {
        let spawn_config = SpawnConfig {
            python_executable: config.python_executable.clone(),
            ..Default::default()
        };

        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            spawn_config,
            config,
            exit_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Spawn a new Python node process
    pub async fn spawn_node(
        &self,
        node_type: &str,
        node_id: &str,
        params: &Value,
        session_id: &str,
    ) -> Result<ProcessHandle> {
        tracing::info!("Spawning process for node: {} ({}) in session: {}", node_id, node_type, session_id);

        // Build command for Python subprocess
        let mut command = Command::new(&self.spawn_config.python_executable);

        // Add multiprocess runner module
        command.args([
            "-m", "remotemedia.core.multiprocessing.runner",
            "--node-type", node_type,
            "--node-id", node_id,
            "--session-id", session_id,  // Pass session_id for channel naming
            "--params-stdin",  // Signal that params come from stdin
        ]);

        // Set environment variables
        for (key, value) in &self.spawn_config.env_vars {
            command.env(key, value);
        }

        // Set Python path
        if !self.spawn_config.python_path.is_empty() {
            let python_path = self.spawn_config.python_path
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join(if cfg!(windows) { ";" } else { ":" });

            command.env("PYTHONPATH", python_path);
        }

        // Set working directory
        if let Some(ref dir) = self.spawn_config.working_dir {
            command.current_dir(dir);
        }

        // Configure process group for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        // Configure I/O
        command.stdin(Stdio::piped());  // Always need stdin for params
        if self.spawn_config.capture_output {
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());
        }

        // Spawn the process
        let mut child = command
            .spawn()
            .map_err(|e| Error::Execution(format!("Failed to spawn process: {}", e)))?;

        let pid = child.id();

        // Capture stderr for logging if process crashes
        let stderr_handle = if self.spawn_config.capture_output {
            child.stderr.take()
        } else {
            None
        };

        // Write params to stdin (avoids command-line length limits)
        if !params.is_null() {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                let params_json = params.to_string();
                stdin.write_all(params_json.as_bytes())
                    .map_err(|e| Error::Execution(format!("Failed to write params to stdin: {}", e)))?;
                // Drop stdin to close the pipe
                drop(stdin);
            }
        }

        // Create process handle
        let handle = ProcessHandle {
            id: pid,
            node_id: node_id.to_string(),
            node_type: node_type.to_string(),
            status: Arc::new(RwLock::new(ProcessStatus::Initializing)),
            started_at: Instant::now(),
            inner: Arc::new(Mutex::new(Some(child))),
        };

        // Register process
        self.processes.write().await.insert(pid, handle.clone());

        // Start monitoring
        self.start_monitoring(handle.clone(), stderr_handle);

        tracing::info!("Process {} spawned for node {}", pid, node_id);
        Ok(handle)
    }

    /// Terminate a process gracefully
    pub async fn terminate_process(
        &self,
        process: ProcessHandle,
        grace_period: Duration,
    ) -> Result<()> {
        tracing::info!("Terminating process {} for node {}", process.id, process.node_id);

        // Update status
        *process.status.write().await = ProcessStatus::Stopping;

        // Try graceful termination first (SIGTERM on Unix, terminate on Windows)
        if let Some(ref mut child) = *process.inner.lock().await {
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;

                let pid = Pid::from_raw(child.id() as i32);
                let _ = kill(pid, Signal::SIGTERM);
            }

            #[cfg(windows)]
            {
                // Windows doesn't have SIGTERM, just try to kill
                let _ = child.kill();
            }

            // Wait for graceful shutdown
            let start = Instant::now();
            while start.elapsed() < grace_period {
                if child.try_wait().ok().flatten().is_some() {
                    // Process terminated gracefully
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            // Force kill if still running
            if child.try_wait().ok().flatten().is_none() {
                tracing::warn!("Process {} did not terminate gracefully, forcing kill", process.id);
                child.kill()
                    .map_err(|e| Error::Execution(format!("Failed to kill process: {}", e)))?;
            }
        }

        // Update status and remove from registry
        *process.status.write().await = ProcessStatus::Stopped;
        self.processes.write().await.remove(&process.id);

        tracing::info!("Process {} terminated", process.id);
        Ok(())
    }

    /// Start monitoring a process
    fn start_monitoring(&self, process: ProcessHandle, stderr: Option<std::process::ChildStderr>) {
        let processes = self.processes.clone();
        let handlers = self.exit_handlers.clone();

        // Spawn task to capture stderr output
        if let Some(stderr) = stderr {
            let node_id = process.node_id.clone();
            let pid = process.id;
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let stderr = tokio::process::ChildStderr::from_std(stderr).unwrap();
                let mut lines = BufReader::new(stderr).lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!("[Process {}] {}: {}", pid, node_id, line);
                }
            });
        }

        // Spawn monitoring task
        tokio::spawn(async move {
            loop {
                // Check process status
                if let Some(exit_status) = process.exit_status().await {
                    // Process exited
                    let reason = if exit_status.success() {
                        ExitReason::Normal
                    } else if let Some(code) = exit_status.code() {
                        ExitReason::Error(code)
                    } else {
                        ExitReason::Killed
                    };

                    tracing::info!(
                        "Process {} exited with reason: {:?}",
                        process.id,
                        reason
                    );

                    // Update status
                    *process.status.write().await = match &reason {
                        ExitReason::Normal => ProcessStatus::Stopped,
                        ExitReason::Error(code) => ProcessStatus::Error(
                            format!("Process exited with code {}", code)
                        ),
                        ExitReason::Killed => ProcessStatus::Error(
                            "Process killed by signal".to_string()
                        ),
                        ExitReason::Timeout => ProcessStatus::Error(
                            "Process initialization timeout".to_string()
                        ),
                    };

                    // Remove from registry
                    processes.write().await.remove(&process.id);

                    // Call exit handlers
                    for handler in handlers.read().await.iter() {
                        handler(process.id, reason.clone());
                    }

                    break;
                }

                // Check periodically
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
    }

    /// Register an exit handler
    pub async fn on_process_exit<F>(&self, handler: F)
    where
        F: Fn(u32, ExitReason) + Send + Sync + 'static,
    {
        self.exit_handlers.write().await.push(Box::new(handler));
    }

    /// Get all active processes
    pub async fn get_processes(&self) -> Vec<ProcessHandle> {
        self.processes.read().await.values().cloned().collect()
    }

    /// Get a specific process by ID
    pub async fn get_process(&self, pid: u32) -> Option<ProcessHandle> {
        self.processes.read().await.get(&pid).cloned()
    }

    /// Terminate all processes
    pub async fn terminate_all(&self, grace_period: Duration) -> Result<()> {
        let processes = self.get_processes().await;

        for process in processes {
            if let Err(e) = self.terminate_process(process, grace_period).await {
                tracing::error!("Failed to terminate process: {}", e);
            }
        }

        Ok(())
    }
}

// Platform-specific imports
#[cfg(unix)]
use nix;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_handle_creation() {
        let handle = ProcessHandle {
            id: 1234,
            node_id: "test_node".to_string(),
            node_type: "test_type".to_string(),
            status: Arc::new(RwLock::new(ProcessStatus::Idle)),
            started_at: Instant::now(),
            inner: Arc::new(Mutex::new(None)),
        };

        assert_eq!(handle.id, 1234);
        assert_eq!(handle.node_id, "test_node");
        assert_eq!(*handle.status.read().await, ProcessStatus::Idle);
    }

    #[tokio::test]
    async fn test_process_manager_creation() {
        let config = MultiprocessConfig::default();
        let manager = ProcessManager::new(config);

        let processes = manager.get_processes().await;
        assert!(processes.is_empty());
    }
}