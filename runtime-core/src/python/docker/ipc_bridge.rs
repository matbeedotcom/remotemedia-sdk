//! IPC bridge for Docker containers
//!
//! Adapts multiprocess IPC patterns for containerized nodes,
//! managing iceoryx2 channels and data transfer.
//!
//! CRITICAL: iceoryx2 Publisher/Subscriber are !Send types and must live on
//! dedicated OS threads. See CLAUDE.md and multiprocess_executor.rs:24-50 for details.

use crate::{Error, Result};

#[cfg(feature = "multiprocess")]
use crate::python::multiprocess::data_transfer::RuntimeData as IPCRuntimeData;

/// Commands sent to the dedicated IPC thread for a Docker container
#[cfg(feature = "multiprocess")]
enum IpcCommand {
    /// Send data to the container's input channel
    SendData { data: IPCRuntimeData },
    /// Register a callback for continuous output forwarding
    RegisterOutputCallback {
        callback_tx: tokio::sync::mpsc::UnboundedSender<IPCRuntimeData>,
    },
    /// Request graceful shutdown of the IPC thread
    Shutdown,
}

/// Handle to a container node's dedicated IPC thread
#[cfg(feature = "multiprocess")]
pub struct ContainerIpcThread {
    /// Channel to send commands to the IPC thread
    command_tx: tokio::sync::mpsc::Sender<IpcCommand>,
    /// Handle to the OS thread (for cleanup)
    thread_handle: Option<std::thread::JoinHandle<()>>,
    /// Node ID this thread serves
    node_id: String,
    /// Session ID this thread belongs to
    session_id: String,
}

#[cfg(feature = "multiprocess")]
impl ContainerIpcThread {
    /// Spawn a dedicated IPC thread for a container node (T023-T025)
    ///
    /// Creates an OS thread with persistent iceoryx2 Publisher/Subscriber (!Send types)
    /// Pattern from multiprocess_executor.rs spawn_ipc_thread
    pub fn spawn(
        node_id: String,
        session_id: String,
        channel_registry: std::sync::Arc<crate::python::multiprocess::ipc_channel::ChannelRegistry>,
    ) -> Result<Self> {
        tracing::info!("Spawning IPC thread for container node: {} (session: {})", node_id, session_id);

        // Create session-scoped channel names (T026)
        let input_channel = create_channel_name(&session_id, &node_id, "input");
        let output_channel = create_channel_name(&session_id, &node_id, "output");

        // Create channels for async <-> thread communication
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<IpcCommand>(100);

        let node_id_clone = node_id.clone();

        // Spawn dedicated OS thread for !Send Publisher/Subscriber
        let handle = std::thread::spawn(move || {
            tracing::info!("IPC thread starting for container node: {}", node_id_clone);

            // Create tokio runtime for this thread
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime for IPC thread");

            // Create persistent publishers/subscribers ONCE (FR-010)
            let (publisher, subscriber) = rt.block_on(async {
                let pub_result = channel_registry.create_publisher(&input_channel).await;
                let sub_result = channel_registry.create_subscriber(&output_channel).await;
                (pub_result, sub_result)
            });

            let publisher = match publisher {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Failed to create publisher for {}: {}", node_id_clone, e);
                    return;
                }
            };

            let subscriber = match subscriber {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create subscriber for {}: {}", node_id_clone, e);
                    return;
                }
            };

            tracing::info!("IPC channels ready for container node: {}", node_id_clone);

            // Optional callback for continuous output forwarding
            let mut output_callback: Option<tokio::sync::mpsc::UnboundedSender<IPCRuntimeData>> = None;

            // Main polling loop (T024, T025)
            loop {
                // Check for commands (non-blocking)
                match cmd_rx.try_recv() {
                    Ok(IpcCommand::SendData { data }) => {
                        // T024: Send data via persistent publisher
                        tracing::debug!("Sending data to container node: {}", node_id_clone);

                        if let Err(e) = publisher.publish(data) {
                            tracing::error!("Failed to publish data for {}: {}", node_id_clone, e);
                        }
                    }
                    Ok(IpcCommand::RegisterOutputCallback { callback_tx }) => {
                        // T025: Register callback for continuous output forwarding
                        tracing::info!("Registered output callback for container node: {}", node_id_clone);
                        output_callback = Some(callback_tx);
                    }
                    Ok(IpcCommand::Shutdown) => {
                        tracing::info!("Shutting down IPC thread for container node: {}", node_id_clone);
                        break;
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                        // No command, poll subscriber for outputs (continuous polling)
                        match subscriber.receive() {
                            Ok(Some(output_data)) => {
                                // Forward via callback if registered (T025)
                                if let Some(ref cb) = output_callback {
                                    if let Err(e) = cb.send(output_data) {
                                        tracing::error!(
                                            "Failed to send output via callback for {}: {}",
                                            node_id_clone, e
                                        );
                                        output_callback = None; // Clear broken callback
                                    }
                                }
                            }
                            Ok(None) => {
                                // No data available, yield to scheduler
                                // CRITICAL: Use yield_now, NOT sleep! (low latency requirement)
                                std::thread::yield_now();
                            }
                            Err(e) => {
                                tracing::error!("Receive error for {}: {}", node_id_clone, e);
                            }
                        }
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        tracing::info!("Command channel closed for container node: {}", node_id_clone);
                        break;
                    }
                }
            }

            tracing::info!("IPC thread exited for container node: {}", node_id_clone);
        });

        Ok(Self {
            command_tx: cmd_tx,
            thread_handle: Some(handle),
            node_id,
            session_id,
        })
    }

    /// Send data to container via IPC (T024)
    pub async fn send_data(&self, data: IPCRuntimeData) -> Result<()> {
        self.command_tx
            .send(IpcCommand::SendData { data })
            .await
            .map_err(|e| Error::Execution(format!("Failed to send data command: {}", e)))?;

        Ok(())
    }

    /// Register output callback for continuous draining (T025)
    pub async fn register_output_callback(
        &self,
        callback_tx: tokio::sync::mpsc::UnboundedSender<IPCRuntimeData>,
    ) -> Result<()> {
        self.command_tx
            .send(IpcCommand::RegisterOutputCallback { callback_tx })
            .await
            .map_err(|e| Error::Execution(format!("Failed to register output callback: {}", e)))?;

        Ok(())
    }

    /// Shutdown IPC thread gracefully
    pub async fn shutdown(mut self) -> Result<()> {
        tracing::info!("Shutting down IPC thread for container node: {}", self.node_id);

        // Send shutdown command
        self.command_tx
            .send(IpcCommand::Shutdown)
            .await
            .ok(); // Ignore errors - thread may already be shutting down

        // Wait for thread to exit
        if let Some(handle) = self.thread_handle.take() {
            if let Err(e) = handle.join() {
                tracing::error!("IPC thread panicked for node {}: {:?}", self.node_id, e);
            }
        }

        Ok(())
    }
}

/// Helper function to create session-scoped channel names (T026)
///
/// FR-010: Channels use format "{session_id}_{node_id}_{direction}"
#[cfg(feature = "multiprocess")]
pub fn create_channel_name(session_id: &str, node_id: &str, direction: &str) -> String {
    format!("{}_{}_{}",session_id, node_id, direction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "multiprocess")]
    fn test_channel_name_format() {
        let input_channel = create_channel_name("sess123", "node1", "input");
        assert_eq!(input_channel, "sess123_node1_input");

        let output_channel = create_channel_name("sess123", "node1", "output");
        assert_eq!(output_channel, "sess123_node1_output");
    }
}
