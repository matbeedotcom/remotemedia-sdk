//! WebRtcServer napi binding
//!
//! Implements the Node.js WebRtcServer class with Event Emitter pattern
//! using ThreadsafeFunction for async event callbacks.

use super::config::{PeerInfo, SessionInfo, WebRtcServerConfig};
use super::events::{
    DataReceivedData, ErrorData, PeerConnectedData, PeerDisconnectedData, PipelineOutputData,
    SessionEventData,
};
use crate::webrtc::core::WebRtcServerCore;
use crate::webrtc::events::WebRtcEvent;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Type aliases for callback storage
type PeerConnectedCallback = ThreadsafeFunction<PeerConnectedData, ErrorStrategy::Fatal>;
type PeerDisconnectedCallback = ThreadsafeFunction<PeerDisconnectedData, ErrorStrategy::Fatal>;
type PipelineOutputCallback = ThreadsafeFunction<PipelineOutputData, ErrorStrategy::Fatal>;
type DataCallback = ThreadsafeFunction<DataReceivedData, ErrorStrategy::Fatal>;
type ErrorCallback = ThreadsafeFunction<ErrorData, ErrorStrategy::Fatal>;

/// WebRTC server for Node.js
///
/// Provides a WebRTC media server with embedded or external signaling.
/// Uses Event Emitter pattern for callbacks.
#[napi]
pub struct WebRtcServer {
    /// Core server implementation
    core: Arc<WebRtcServerCore>,

    /// Peer connected callbacks
    on_peer_connected: Arc<Mutex<Vec<PeerConnectedCallback>>>,

    /// Peer disconnected callbacks
    on_peer_disconnected: Arc<Mutex<Vec<PeerDisconnectedCallback>>>,

    /// Pipeline output callbacks
    on_pipeline_output: Arc<Mutex<Vec<PipelineOutputCallback>>>,

    /// Raw data callbacks
    on_data: Arc<Mutex<Vec<DataCallback>>>,

    /// Error callbacks
    on_error: Arc<Mutex<Vec<ErrorCallback>>>,

    /// Event loop running flag
    event_loop_running: Arc<AtomicBool>,

    /// Event loop task handle
    event_loop_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

#[napi]
impl WebRtcServer {
    /// Create a new WebRTC server with embedded signaling
    ///
    /// @param config - Server configuration with port set
    /// @returns Promise resolving to WebRtcServer instance
    #[napi(factory)]
    pub async fn create(config: WebRtcServerConfig) -> Result<Self> {
        let core_config = config.to_core_config()?;

        if core_config.port.is_none() {
            return Err(Error::from_reason(
                "Port is required for embedded signaling. Use connect() for external signaling.",
            ));
        }

        let core = WebRtcServerCore::new(core_config)
            .map_err(|e| Error::from_reason(e.to_string()))?;

        Ok(Self {
            core: Arc::new(core),
            on_peer_connected: Arc::new(Mutex::new(Vec::new())),
            on_peer_disconnected: Arc::new(Mutex::new(Vec::new())),
            on_pipeline_output: Arc::new(Mutex::new(Vec::new())),
            on_data: Arc::new(Mutex::new(Vec::new())),
            on_error: Arc::new(Mutex::new(Vec::new())),
            event_loop_running: Arc::new(AtomicBool::new(false)),
            event_loop_handle: Arc::new(Mutex::new(None)),
        })
    }

    /// Connect to an external signaling server
    ///
    /// @param config - Server configuration with signaling_url set
    /// @returns Promise resolving to WebRtcServer instance
    #[napi(factory)]
    pub async fn connect(config: WebRtcServerConfig) -> Result<Self> {
        let core_config = config.to_core_config()?;

        if core_config.signaling_url.is_none() {
            return Err(Error::from_reason(
                "signaling_url is required for external signaling. Use create() for embedded signaling.",
            ));
        }

        let core = WebRtcServerCore::new(core_config)
            .map_err(|e| Error::from_reason(e.to_string()))?;

        Ok(Self {
            core: Arc::new(core),
            on_peer_connected: Arc::new(Mutex::new(Vec::new())),
            on_peer_disconnected: Arc::new(Mutex::new(Vec::new())),
            on_pipeline_output: Arc::new(Mutex::new(Vec::new())),
            on_data: Arc::new(Mutex::new(Vec::new())),
            on_error: Arc::new(Mutex::new(Vec::new())),
            event_loop_running: Arc::new(AtomicBool::new(false)),
            event_loop_handle: Arc::new(Mutex::new(None)),
        })
    }

    /// Get the server ID
    #[napi(getter)]
    pub fn id(&self) -> String {
        self.core.id().to_string()
    }

    /// Get the current server state
    #[napi(getter)]
    pub async fn state(&self) -> String {
        let state = self.core.state().await;
        match state {
            crate::webrtc::config::ServerState::Created => "created".to_string(),
            crate::webrtc::config::ServerState::Starting => "starting".to_string(),
            crate::webrtc::config::ServerState::Running => "running".to_string(),
            crate::webrtc::config::ServerState::Stopping => "stopping".to_string(),
            crate::webrtc::config::ServerState::Stopped => "stopped".to_string(),
        }
    }

    /// Get connected peers as a Map
    #[napi]
    pub async fn get_peers(&self) -> Result<Vec<PeerInfo>> {
        let peers = self.core.peers().await;
        Ok(peers.into_values().map(Into::into).collect())
    }

    /// Get active sessions
    #[napi]
    pub async fn get_sessions(&self) -> Result<Vec<SessionInfo>> {
        let sessions = self.core.sessions().await;
        Ok(sessions.into_values().map(Into::into).collect())
    }

    /// Register an event listener
    ///
    /// @param event - Event name ('peer_connected', 'peer_disconnected', 'pipeline_output', 'data', 'error')
    /// @param callback - Event handler function
    #[napi]
    pub fn on(&self, env: Env, event: String, callback: JsFunction) -> Result<()> {
        match event.as_str() {
            "peer_connected" => {
                let tsfn: PeerConnectedCallback = callback.create_threadsafe_function(
                    0,
                    |ctx: napi::threadsafe_function::ThreadSafeCallContext<PeerConnectedData>| {
                        Ok(vec![ctx.value])
                    },
                )?;
                let callbacks = self.on_peer_connected.clone();
                tokio::spawn(async move {
                    callbacks.lock().await.push(tsfn);
                });
            }
            "peer_disconnected" => {
                let tsfn: PeerDisconnectedCallback = callback.create_threadsafe_function(
                    0,
                    |ctx: napi::threadsafe_function::ThreadSafeCallContext<PeerDisconnectedData>| {
                        Ok(vec![ctx.value])
                    },
                )?;
                let callbacks = self.on_peer_disconnected.clone();
                tokio::spawn(async move {
                    callbacks.lock().await.push(tsfn);
                });
            }
            "pipeline_output" => {
                let tsfn: PipelineOutputCallback = callback.create_threadsafe_function(
                    0,
                    |ctx: napi::threadsafe_function::ThreadSafeCallContext<PipelineOutputData>| {
                        Ok(vec![ctx.value])
                    },
                )?;
                let callbacks = self.on_pipeline_output.clone();
                tokio::spawn(async move {
                    callbacks.lock().await.push(tsfn);
                });
            }
            "data" => {
                let tsfn: DataCallback = callback.create_threadsafe_function(
                    0,
                    |ctx: napi::threadsafe_function::ThreadSafeCallContext<DataReceivedData>| {
                        Ok(vec![ctx.value])
                    },
                )?;
                let callbacks = self.on_data.clone();
                tokio::spawn(async move {
                    callbacks.lock().await.push(tsfn);
                });
            }
            "error" => {
                let tsfn: ErrorCallback = callback.create_threadsafe_function(
                    0,
                    |ctx: napi::threadsafe_function::ThreadSafeCallContext<ErrorData>| {
                        Ok(vec![ctx.value])
                    },
                )?;
                let callbacks = self.on_error.clone();
                tokio::spawn(async move {
                    callbacks.lock().await.push(tsfn);
                });
            }
            _ => {
                return Err(Error::from_reason(format!(
                    "Unknown event: {}. Valid events: peer_connected, peer_disconnected, pipeline_output, data, error",
                    event
                )));
            }
        }
        Ok(())
    }

    /// Start the server
    ///
    /// For embedded mode: binds to the configured port
    /// For external mode: connects to the signaling server
    #[napi]
    pub async fn start(&self) -> Result<()> {
        // Start the core server
        self.core
            .start()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;

        // Start the event loop
        self.start_event_loop().await?;

        Ok(())
    }

    /// Stop the server gracefully
    ///
    /// Disconnects all peers and cleans up resources.
    #[napi]
    pub async fn shutdown(&self) -> Result<()> {
        // Stop event loop
        self.event_loop_running.store(false, Ordering::SeqCst);

        // Wait for event loop to finish
        if let Some(handle) = self.event_loop_handle.lock().await.take() {
            let _ = handle.await;
        }

        // Shutdown core
        self.core
            .shutdown()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;

        Ok(())
    }

    /// Send data to a specific peer
    ///
    /// @param peer_id - Target peer ID
    /// @param data - Data to send (Buffer)
    #[napi]
    pub async fn send_to_peer(&self, peer_id: String, data: Buffer) -> Result<()> {
        self.core
            .send_to_peer(&peer_id, data.to_vec())
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Broadcast data to all connected peers
    ///
    /// @param data - Data to broadcast (Buffer)
    #[napi]
    pub async fn broadcast(&self, data: Buffer) -> Result<()> {
        self.core
            .broadcast(data.to_vec())
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Disconnect a specific peer
    ///
    /// @param peer_id - Peer to disconnect
    /// @param reason - Optional disconnect reason
    #[napi]
    pub async fn disconnect_peer(&self, peer_id: String, reason: Option<String>) -> Result<()> {
        self.core
            .disconnect_peer(&peer_id, reason.as_deref())
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Create a new session (room)
    ///
    /// @param session_id - Unique session identifier
    /// @param metadata - Optional session metadata as key-value pairs
    #[napi]
    pub async fn create_session(
        &self,
        session_id: String,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<SessionInfo> {
        let session = self
            .core
            .create_session(&session_id, metadata)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;

        Ok(session.into())
    }

    /// Get an existing session
    ///
    /// @param session_id - Session identifier
    /// @returns SessionInfo or undefined
    #[napi]
    pub async fn get_session(&self, session_id: String) -> Result<Option<SessionInfo>> {
        Ok(self.core.get_session(&session_id).await.map(Into::into))
    }

    /// Delete a session
    ///
    /// @param session_id - Session to delete
    #[napi]
    pub async fn delete_session(&self, session_id: String) -> Result<()> {
        self.core
            .delete_session(&session_id)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    // ============ Internal methods ============

    async fn start_event_loop(&self) -> Result<()> {
        if self.event_loop_running.load(Ordering::SeqCst) {
            return Ok(()); // Already running
        }

        // Take the event receiver
        let event_rx = self
            .core
            .take_event_receiver()
            .await
            .ok_or_else(|| Error::from_reason("Event receiver already taken"))?;

        self.event_loop_running.store(true, Ordering::SeqCst);

        // Clone references for the event loop
        let running = self.event_loop_running.clone();
        let on_peer_connected = self.on_peer_connected.clone();
        let on_peer_disconnected = self.on_peer_disconnected.clone();
        let on_pipeline_output = self.on_pipeline_output.clone();
        let on_data = self.on_data.clone();
        let on_error = self.on_error.clone();

        let handle = tokio::spawn(async move {
            let mut event_rx = event_rx;

            while running.load(Ordering::SeqCst) {
                tokio::select! {
                    biased;

                    // Periodic check for shutdown (100ms timeout)
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        // Just check the running flag
                        continue;
                    }
                    Some(event) = event_rx.recv() => {
                        match event {
                            WebRtcEvent::PeerConnected(e) => {
                                let data: PeerConnectedData = e.into();
                                let callbacks = on_peer_connected.lock().await;
                                for tsfn in callbacks.iter() {
                                    let _ = tsfn.call(data.clone(), ThreadsafeFunctionCallMode::NonBlocking);
                                }
                            }
                            WebRtcEvent::PeerDisconnected(e) => {
                                let data: PeerDisconnectedData = e.into();
                                let callbacks = on_peer_disconnected.lock().await;
                                for tsfn in callbacks.iter() {
                                    let _ = tsfn.call(data.clone(), ThreadsafeFunctionCallMode::NonBlocking);
                                }
                            }
                            WebRtcEvent::PipelineOutput(e) => {
                                let data: PipelineOutputData = e.into();
                                let callbacks = on_pipeline_output.lock().await;
                                for tsfn in callbacks.iter() {
                                    let _ = tsfn.call(data.clone(), ThreadsafeFunctionCallMode::NonBlocking);
                                }
                            }
                            WebRtcEvent::DataReceived(e) => {
                                let data: DataReceivedData = e.into();
                                let callbacks = on_data.lock().await;
                                for tsfn in callbacks.iter() {
                                    let _ = tsfn.call(data.clone(), ThreadsafeFunctionCallMode::NonBlocking);
                                }
                            }
                            WebRtcEvent::Error(e) => {
                                let data: ErrorData = e.into();
                                let callbacks = on_error.lock().await;
                                for tsfn in callbacks.iter() {
                                    let _ = tsfn.call(data.clone(), ThreadsafeFunctionCallMode::NonBlocking);
                                }
                            }
                            WebRtcEvent::Session(_) => {
                                // Session events are handled at the session level
                            }
                        }
                    }
                    else => {
                        break;
                    }
                }
            }

            tracing::debug!("WebRTC event loop stopped");
        });

        *self.event_loop_handle.lock().await = Some(handle);

        Ok(())
    }
}
