//! Mock transport implementation for testing
//!
//! Provides a simple MockTransport that implements PipelineTransport
//! for testing core functionality without real transport overhead.

use async_trait::async_trait;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::transport::{
    PipelineRunner, PipelineTransport, StreamSession, TransportData,
};
use remotemedia_runtime_core::{Error, Result};
use std::sync::Arc;

/// Mock transport for testing
///
/// This transport uses the real PipelineRunner but is useful for
/// testing transport-level logic without network overhead.
pub struct MockTransport {
    runner: PipelineRunner,
    /// Optional transformation to apply to outputs
    transform_output: Option<Box<dyn Fn(RuntimeData) -> RuntimeData + Send + Sync>>,
}

impl MockTransport {
    /// Create new mock transport
    pub fn new() -> Result<Self> {
        Ok(Self {
            runner: PipelineRunner::new()?,
            transform_output: None,
        })
    }

    /// Create mock transport with output transformation
    #[allow(dead_code)]
    pub fn with_transform<F>(mut self, f: F) -> Self
    where
        F: Fn(RuntimeData) -> RuntimeData + Send + Sync + 'static,
    {
        self.transform_output = Some(Box::new(f));
        self
    }
}

#[async_trait]
impl PipelineTransport for MockTransport {
    async fn execute(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        let mut output = self.runner.execute_unary(manifest, input).await?;

        // Apply optional transformation
        if let Some(ref transform) = self.transform_output {
            output.data = transform(output.data);
        }

        Ok(output)
    }

    async fn stream(&self, manifest: Arc<Manifest>) -> Result<Box<dyn StreamSession>> {
        let session = self.runner.create_stream_session(manifest).await?;
        Ok(Box::new(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_transport_creation() {
        let transport = MockTransport::new();
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_mock_transport_execute() {
        let transport = MockTransport::new().unwrap();

        let manifest_json = r#"{
            "version": "v1",
            "nodes": [],
            "connections": []
        }"#;
        let manifest = Arc::new(serde_json::from_str::<Manifest>(manifest_json).unwrap());

        let input = TransportData::new(RuntimeData::Text("test input".into()));

        let result = transport.execute(manifest, input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        // In Phase 2, runner echoes input
        assert_eq!(output.data, RuntimeData::Text("test input".into()));
    }

    #[tokio::test]
    async fn test_mock_transport_stream() {
        let transport = MockTransport::new().unwrap();

        let manifest_json = r#"{
            "version": "v1",
            "nodes": [],
            "connections": []
        }"#;
        let manifest = Arc::new(Manifest::from_json(manifest_json).unwrap());

        let result = transport.stream(manifest).await;
        assert!(result.is_ok());

        let mut session = result.unwrap();
        assert!(session.is_active());
        assert!(!session.session_id().is_empty());

        // Test send/receive
        let input = TransportData::new(RuntimeData::Text("hello".into()));
        session.send_input(input).await.unwrap();

        let output = session.recv_output().await.unwrap();
        assert!(output.is_some());
        assert_eq!(output.unwrap().data, RuntimeData::Text("hello".into()));

        // Close session
        session.close().await.unwrap();
        assert!(!session.is_active());
    }
}
