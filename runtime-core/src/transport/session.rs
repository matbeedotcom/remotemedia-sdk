//! Streaming session handle for stateful pipeline interactions

use crate::transport::TransportData;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
        if !self.is_active() {
            return Err(Error::Execution(format!(
                "Session {} is closed",
                self.session_id
            )));
        }

        // Send the core RuntimeData to the router
        self.inner
            .input_tx
            .send(data.data)
            .map_err(|_| {
                self.inner.active.store(false, Ordering::Release);
                Error::Execution(format!("Session {} channel closed", self.session_id))
            })?;

        Ok(())
    }

    async fn recv_output(&mut self) -> Result<Option<TransportData>> {
        let mut output_rx = self.inner.output_rx.lock().await;

        match output_rx.recv().await {
            Some(runtime_data) => {
                // Convert RuntimeData to TransportData
                Ok(Some(TransportData::new(runtime_data)))
            }
            None => {
                // Channel closed, session ended
                self.inner.active.store(false, Ordering::Release);
                Ok(None)
            }
        }
    }

    async fn close(&mut self) -> Result<()> {
        if !self.is_active() {
            // Idempotent: already closed
            return Ok(());
        }

        // Signal shutdown
        if let Some(shutdown_tx) = &self.inner.shutdown_tx {
            let _ = shutdown_tx.send(()).await;
        }

        // Mark inactive
        self.inner.active.store(false, Ordering::Release);

        Ok(())
    }

    fn is_active(&self) -> bool {
        self.inner.active.load(Ordering::Acquire)
    }
}
