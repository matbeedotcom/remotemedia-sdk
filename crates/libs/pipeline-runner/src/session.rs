//! Pipeline session for streaming execution
//!
//! Provides the `PipelineSession` abstraction for streaming pipeline execution,
//! used by both the ingest-srt service and stream-health-demo CLI.

use std::sync::Arc;
use std::time::Duration;

use remotemedia_core::data::RuntimeData;
use remotemedia_core::manifest::Manifest;
use remotemedia_core::transport::{PipelineExecutor, SessionHandle, TransportData};
use thiserror::Error;

/// Errors that can occur during pipeline session operations
#[derive(Debug, Error)]
pub enum PipelineSessionError {
    /// Failed to create the pipeline executor
    #[error("Failed to create pipeline executor: {0}")]
    ExecutorCreation(String),

    /// Failed to create the session
    #[error("Failed to create session: {0}")]
    SessionCreation(String),

    /// Failed to send data to the pipeline
    #[error("Failed to send data: {0}")]
    Send(String),

    /// Failed to receive data from the pipeline
    #[error("Failed to receive data: {0}")]
    Receive(String),

    /// Failed to close the session
    #[error("Failed to close session: {0}")]
    Close(String),

    /// Session is already closed
    #[error("Session is closed")]
    Closed,
}

/// A streaming pipeline session
///
/// Wraps the core's session handling to provide a clean interface
/// for sending `RuntimeData` to a pipeline and receiving processed outputs.
///
/// # Example
///
/// ```ignore
/// use remotemedia_pipeline_runner::PipelineSession;
/// use remotemedia_core::data::RuntimeData;
///
/// // Create session with a manifest
/// let mut session = PipelineSession::new(manifest).await?;
///
/// // Send audio data
/// session.send(RuntimeData::Audio { ... }).await?;
///
/// // Receive processed outputs
/// while let Some(output) = session.try_recv()? {
///     println!("Got output: {:?}", output);
/// }
///
/// // Clean shutdown
/// session.close().await?;
/// ```
pub struct PipelineSession {
    /// The pipeline executor (owns the registry and runs nodes)
    executor: PipelineExecutor,

    /// The session handle for streaming I/O
    handle: Option<SessionHandle>,

    /// The manifest this session was created with
    manifest: Arc<Manifest>,
}

impl PipelineSession {
    /// Create a new pipeline session with a manifest
    ///
    /// This creates a new `PipelineExecutor` and session. For processing
    /// multiple streams with the same manifest, consider using
    /// `PipelineSession::with_executor()` to reuse the executor.
    pub async fn new(manifest: Arc<Manifest>) -> Result<Self, PipelineSessionError> {
        let executor = PipelineExecutor::new()
            .map_err(|e| PipelineSessionError::ExecutorCreation(e.to_string()))?;

        Self::with_executor(executor, manifest).await
    }

    /// Create a new pipeline session with an existing executor
    ///
    /// Allows reusing a `PipelineExecutor` across multiple sessions,
    /// which is more efficient as node factories don't need to be
    /// re-registered.
    pub async fn with_executor(
        executor: PipelineExecutor,
        manifest: Arc<Manifest>,
    ) -> Result<Self, PipelineSessionError> {
        let handle = executor
            .create_session(manifest.clone())
            .await
            .map_err(|e| PipelineSessionError::SessionCreation(e.to_string()))?;

        Ok(Self {
            executor,
            handle: Some(handle),
            manifest,
        })
    }

    /// Get the manifest this session was created with
    pub fn manifest(&self) -> &Arc<Manifest> {
        &self.manifest
    }

    /// Get a reference to the underlying executor
    ///
    /// This can be used to access executor-level functionality like
    /// registering additional nodes or getting metrics.
    pub fn executor(&self) -> &PipelineExecutor {
        &self.executor
    }

    /// Send data to the pipeline for processing
    pub async fn send(&mut self, data: RuntimeData) -> Result<(), PipelineSessionError> {
        let handle = self
            .handle
            .as_mut()
            .ok_or(PipelineSessionError::Closed)?;

        let transport_data = TransportData::new(data);
        handle
            .send_input(transport_data)
            .await
            .map_err(|e| PipelineSessionError::Send(e.to_string()))
    }

    /// Receive output from the pipeline (blocking)
    ///
    /// Returns `None` when the session has ended or the output channel is closed.
    pub async fn recv(&mut self) -> Result<Option<RuntimeData>, PipelineSessionError> {
        let handle = self
            .handle
            .as_mut()
            .ok_or(PipelineSessionError::Closed)?;

        match handle.recv_output().await {
            Ok(Some(transport_data)) => Ok(Some(transport_data.data)),
            Ok(None) => Ok(None),
            Err(e) => Err(PipelineSessionError::Receive(e.to_string())),
        }
    }

    /// Try to receive output without blocking
    ///
    /// Returns `Ok(None)` if no output is immediately available.
    pub fn try_recv(&mut self) -> Result<Option<RuntimeData>, PipelineSessionError> {
        let handle = self
            .handle
            .as_mut()
            .ok_or(PipelineSessionError::Closed)?;

        match handle.try_recv_output() {
            Ok(Some(transport_data)) => Ok(Some(transport_data.data)),
            Ok(None) => Ok(None),
            Err(e) => Err(PipelineSessionError::Receive(e.to_string())),
        }
    }

    /// Receive output with a timeout
    ///
    /// Returns `Ok(None)` if no output is available within the timeout.
    pub async fn recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<RuntimeData>, PipelineSessionError> {
        let handle = self
            .handle
            .as_mut()
            .ok_or(PipelineSessionError::Closed)?;

        match tokio::time::timeout(timeout, handle.recv_output()).await {
            Ok(Ok(Some(transport_data))) => Ok(Some(transport_data.data)),
            Ok(Ok(None)) => Ok(None),
            Ok(Err(e)) => Err(PipelineSessionError::Receive(e.to_string())),
            Err(_) => Ok(None), // Timeout
        }
    }

    /// Close the session and release resources
    pub async fn close(mut self) -> Result<(), PipelineSessionError> {
        if let Some(mut handle) = self.handle.take() {
            handle
                .close()
                .await
                .map_err(|e| PipelineSessionError::Close(e.to_string()))?;
        }
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use remotemedia_core::manifest::{ManifestMetadata, NodeManifest};

    fn test_manifest() -> Arc<Manifest> {
        Arc::new(Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "test".to_string(),
                description: None,
                created_at: None,
                auto_negotiate: false,
            },
            nodes: vec![],
            connections: vec![],
        })
    }

    #[tokio::test]
    async fn test_session_creation() {
        let manifest = test_manifest();
        let session = PipelineSession::new(manifest.clone()).await;
        // Should succeed even with empty manifest
        assert!(session.is_ok());
    }

    #[tokio::test]
    async fn test_session_close() {
        let manifest = test_manifest();
        let session = PipelineSession::new(manifest).await.unwrap();
        let result = session.close().await;
        assert!(result.is_ok());
    }
}
