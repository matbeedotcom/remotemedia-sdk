//! Streaming session handle for stateful pipeline interactions

use crate::transport::TransportData;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Streaming session handle for stateful pipeline interactions
///
/// Represents an active streaming session between a transport and the core
/// runtime. Provides bidirectional communication: transport sends inputs,
/// core returns outputs.
///
/// # Lifecycle
///
/// 1. Created by `PipelineTransport::stream()` or `PipelineRunner::create_stream_session()`
/// 2. Active: `send_input()` and `recv_output()` called repeatedly
/// 3. Closed: `close()` called or error occurs
/// 4. Terminal: session cannot be reused after close
///
/// # Thread Safety
///
/// Implementations must be Send + Sync. Multiple tasks may share access
/// to a session (though typically only one task drives I/O).
#[async_trait]
pub trait StreamSession: Send + Sync {
    /// Get unique identifier for this session
    ///
    /// Returns a UUID string that uniquely identifies this session across
    /// all sessions in the runtime. Used for logging, metrics, and debugging.
    fn session_id(&self) -> &str;

    /// Send input data to the pipeline
    ///
    /// Submits data to the pipeline for processing. The data flows through
    /// the node graph defined in the manifest. Outputs are received via
    /// `recv_output()`.
    ///
    /// # Arguments
    ///
    /// * `data` - Input data with optional sequence number and metadata
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Data accepted and queued for processing
    /// * `Err(Error)` - Send failed (session closed, invalid data, etc.)
    ///
    /// # Errors
    ///
    /// * `Error::Execution` - Session closed or internal error
    async fn send_input(&mut self, data: TransportData) -> Result<()>;

    /// Receive output data from the pipeline
    ///
    /// Blocks until output is available or session closes. Returns None
    /// when the session has been closed and all pending outputs have been
    /// delivered.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(TransportData))` - Output data from pipeline
    /// * `Ok(None)` - Session closed, no more outputs
    /// * `Err(Error)` - Receive failed (internal error)
    async fn recv_output(&mut self) -> Result<Option<TransportData>>;

    /// Close the session gracefully
    ///
    /// Signals that no more inputs will be sent. The core runtime will:
    /// 1. Finish processing any pending inputs
    /// 2. Deliver any remaining outputs via `recv_output()`
    /// 3. Clean up session resources (router, executor state)
    /// 4. Return None from subsequent `recv_output()` calls
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Close initiated successfully
    /// * `Err(Error)` - Close failed (already closed, internal error)
    async fn close(&mut self) -> Result<()>;

    /// Check if session is still active
    ///
    /// Returns true if the session is open and can accept inputs/outputs.
    /// Returns false if `close()` has been called or an error occurred.
    fn is_active(&self) -> bool;
}

/// Concrete implementation of StreamSession provided by runtime-core
///
/// This struct is the actual handle returned from
/// `PipelineRunner::create_stream_session()`. It wraps internal channels
/// and state to communicate with the SessionRouter.
pub struct StreamSessionHandle {
    /// Unique session identifier (UUID)
    session_id: String,

    /// Internal state (channels, router handle, etc.)
    inner: Arc<StreamSessionInner>,
}

impl StreamSessionHandle {
    /// Create new session handle (internal use only)
    ///
    /// This is called by PipelineRunner, not by transport implementations.
    pub(crate) fn new(
        session_id: String,
        input_tx: mpsc::UnboundedSender<crate::data::RuntimeData>,
        output_rx: mpsc::UnboundedReceiver<crate::data::RuntimeData>,
        shutdown_tx: mpsc::Sender<()>,
    ) -> Self {
        Self {
            session_id,
            inner: Arc::new(StreamSessionInner {
                input_tx,
                output_rx: Mutex::new(output_rx),
                shutdown_tx: Some(shutdown_tx),
                active: AtomicBool::new(true),
            }),
        }
    }
}

/// Internal session state (not exposed to transports)
struct StreamSessionInner {
    /// Input channel to SessionRouter
    input_tx: mpsc::UnboundedSender<crate::data::RuntimeData>,

    /// Output channel from SessionRouter
    output_rx: Mutex<mpsc::UnboundedReceiver<crate::data::RuntimeData>>,

    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,

    /// Active flag
    active: AtomicBool,
}

#[async_trait]
impl StreamSession for StreamSessionHandle {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn send_input(&mut self, data: TransportData) -> Result<()> {
        tracing::debug!("Session {} send_input called, is_active={}", self.session_id, self.is_active());

        if !self.is_active() {
            tracing::error!("Session {} send_input rejected: session is not active", self.session_id);
            return Err(Error::Execution(format!(
                "Session {} is closed",
                self.session_id
            )));
        }

        // Send the core RuntimeData to the router
        tracing::debug!("Session {} sending data to input channel", self.session_id);
        self.inner.input_tx.send(data.data).map_err(|_| {
            tracing::error!("Session {} input channel closed, marking session inactive", self.session_id);
            self.inner.active.store(false, Ordering::Release);
            Error::Execution(format!("Session {} channel closed", self.session_id))
        })?;

        tracing::debug!("Session {} data sent successfully to input channel", self.session_id);
        Ok(())
    }

    async fn recv_output(&mut self) -> Result<Option<TransportData>> {
        let mut output_rx = self.inner.output_rx.lock().await;

        tracing::trace!("Session {} recv_output awaiting data from output channel", self.session_id);
        match output_rx.recv().await {
            Some(runtime_data) => {
                tracing::debug!("Session {} recv_output received data", self.session_id);
                // Convert RuntimeData to TransportData
                Ok(Some(TransportData::new(runtime_data)))
            }
            None => {
                // Channel closed - this means the session task has ended
                // Don't mark inactive here - let explicit shutdown handle that
                // This allows the session to remain active between pipeline executions
                tracing::debug!("Session {} recv_output got None (channel empty/closed)", self.session_id);
                Ok(None)
            }
        }
    }

    async fn close(&mut self) -> Result<()> {
        if !self.is_active() {
            // Idempotent: already closed
            tracing::debug!("Session {} close called but already closed", self.session_id);
            return Ok(());
        }

        tracing::info!("Session {} closing, sending shutdown signal", self.session_id);

        // Signal shutdown
        if let Some(shutdown_tx) = &self.inner.shutdown_tx {
            let _ = shutdown_tx.send(()).await;
        }

        // Mark inactive
        self.inner.active.store(false, Ordering::Release);
        tracing::info!("Session {} marked inactive", self.session_id);

        Ok(())
    }

    fn is_active(&self) -> bool {
        let active = self.inner.active.load(Ordering::Acquire);
        tracing::trace!("Session {} is_active check: {}", self.session_id, active);
        active
    }
}
