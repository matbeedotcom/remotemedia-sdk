// Contract: StreamSession Trait
// Location: runtime-core/src/transport/session.rs
// Status: Design (to be implemented in Phase 2)

use crate::{Error, Result};
use async_trait::async_trait;

/// Streaming session handle for stateful pipeline interactions
///
/// Represents an active streaming session between a transport and the core
/// runtime. Provides bidirectional communication: transport sends inputs,
/// core returns outputs.
///
/// # Lifecycle
///
/// 1. Created by `PipelineTransport::stream()`
/// 2. Active: `send_input()` and `recv_output()` called repeatedly
/// 3. Closed: `close()` called or error occurs
/// 4. Terminal: session cannot be reused after close
///
/// # Thread Safety
///
/// Implementations must be Send + Sync. Multiple tasks may share access
/// to a session (though typically only one task drives I/O).
///
/// # State Management
///
/// - `is_active()` returns true between creation and close
/// - Operations fail with error after `close()` called
/// - Core manages internal session state (router, executor, metrics)
#[async_trait]
pub trait StreamSession: Send + Sync {
    /// Get unique identifier for this session
    ///
    /// Returns a UUID string that uniquely identifies this session across
    /// all sessions in the runtime. Used for logging, metrics, and debugging.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let session_id = session.session_id();
    /// tracing::info!("Processing data for session: {}", session_id);
    /// ```
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
    /// * `Error::SessionClosed` - Cannot send after close() called
    /// * `Error::InvalidData` - Data format incompatible with pipeline
    /// * `Error::ResourceLimit` - Internal buffer full (backpressure)
    ///
    /// # Backpressure
    ///
    /// If the pipeline cannot keep up with inputs, this method may:
    /// - Block until buffer space available (await yields)
    /// - Return ResourceLimit error if policy is fail-fast
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let data = TransportData::new(RuntimeData::Audio { ... })
    ///     .with_sequence(1);
    /// session.send_input(data).await?;
    /// ```
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
    ///
    /// # Errors
    ///
    /// * `Error::NodeExecutionFailed` - A node in pipeline failed
    /// * `Error::InternalError` - Core runtime error
    ///
    /// # Blocking Behavior
    ///
    /// This method blocks (awaits) until:
    /// - Output is available from a node
    /// - Session is closed
    /// - Error occurs
    ///
    /// For non-blocking behavior, use tokio::select! with timeout.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// while let Some(output) = session.recv_output().await? {
    ///     tracing::debug!("Received output: {:?}", output.data);
    ///     // Process output
    /// }
    /// // None returned: session closed
    /// ```
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
    ///
    /// # Errors
    ///
    /// * `Error::SessionClosed` - Already closed (idempotent, not fatal)
    /// * `Error::InternalError` - Cleanup failed
    ///
    /// # Idempotency
    ///
    /// Calling `close()` multiple times is safe and has no effect after
    /// the first call.
    ///
    /// # Cleanup
    ///
    /// Core automatically cleans up:
    /// - Session router task
    /// - Node executor instances
    /// - IPC threads (for multiprocess nodes)
    /// - Metrics and logging state
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Send all inputs
    /// for data in inputs {
    ///     session.send_input(data).await?;
    /// }
    ///
    /// // Signal done
    /// session.close().await?;
    ///
    /// // Drain remaining outputs
    /// while let Some(output) = session.recv_output().await? {
    ///     // Process final outputs
    /// }
    /// ```
    async fn close(&mut self) -> Result<()>;

    /// Check if session is still active
    ///
    /// Returns true if the session is open and can accept inputs/outputs.
    /// Returns false if `close()` has been called or an error occurred.
    ///
    /// # Returns
    ///
    /// * `true` - Session is active
    /// * `false` - Session is closed or errored
    ///
    /// # Note
    ///
    /// This is a snapshot of state at call time. The session may become
    /// inactive immediately after this returns true (e.g., if another task
    /// calls close() concurrently).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if session.is_active() {
    ///     session.send_input(data).await?;
    /// } else {
    ///     tracing::warn!("Session {} already closed", session.session_id());
    /// }
    /// ```
    fn is_active(&self) -> bool;
}

/// Concrete implementation of StreamSession provided by runtime-core
///
/// This struct is the actual handle returned from
/// `PipelineRunner::create_stream_session()`. It wraps internal channels
/// and state to communicate with the SessionRouter.
///
/// # Implementation Note
///
/// This is an opaque type. Transports interact only through the
/// `StreamSession` trait methods. Internal structure is subject to change.
pub struct StreamSessionHandle {
    /// Unique session identifier (UUID)
    session_id: String,

    /// Internal state (channels, router handle, etc.)
    /// Wrapped in Arc for cheap cloning if needed
    inner: std::sync::Arc<StreamSessionInner>,
}

// Internal state (not exposed to transports)
struct StreamSessionInner {
    // Input channel to SessionRouter
    input_tx: tokio::sync::mpsc::UnboundedSender<DataPacket>,

    // Output channel from SessionRouter
    output_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<DataPacket>>,

    // Shutdown signal
    shutdown_tx: Option<tokio::sync::mpsc::Sender<()>>,

    // Active flag
    active: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl StreamSession for StreamSessionHandle {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn send_input(&mut self, data: TransportData) -> Result<()> {
        if !self.is_active() {
            return Err(Error::SessionClosed(self.session_id.clone()));
        }

        // Convert TransportData to internal DataPacket
        let packet = DataPacket::from_transport_data(
            data,
            &self.session_id,
        );

        self.inner.input_tx.send(packet)
            .map_err(|_| Error::SessionClosed(self.session_id.clone()))?;

        Ok(())
    }

    async fn recv_output(&mut self) -> Result<Option<TransportData>> {
        let mut output_rx = self.inner.output_rx.lock().await;

        match output_rx.recv().await {
            Some(packet) => {
                // Convert internal DataPacket to TransportData
                Ok(Some(packet.to_transport_data()))
            }
            None => {
                // Channel closed, session ended
                self.inner.active.store(false, std::sync::atomic::Ordering::Release);
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
        self.inner.active.store(false, std::sync::atomic::Ordering::Release);

        Ok(())
    }

    fn is_active(&self) -> bool {
        self.inner.active.load(std::sync::atomic::Ordering::Acquire)
    }
}

// DataPacket is an internal type (not part of public API)
// Used for communication between transports and SessionRouter
struct DataPacket {
    // Fields match existing SessionRouter::DataPacket
    // Implementation details omitted (internal to runtime-core)
}
