//! Custom transport implementation example
//!
//! Demonstrates how to create a custom transport using only runtime-core,
//! without any gRPC, FFI, or other transport dependencies.
//!
//! This example shows a simple console-based transport that:
//! - Reads input from stdin or function arguments
//! - Executes pipeline via PipelineExecutor
//! - Prints output to stdout
//!
//! Total implementation: ~80 lines (well under 100-line target)

use remotemedia_core::transport::{
    PipelineTransport, PipelineExecutor, StreamSession, TransportData,
};
use remotemedia_core::data::RuntimeData;
use remotemedia_core::manifest::Manifest;
use remotemedia_core::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Simple console-based transport
///
/// Uses PipelineExecutor to execute pipelines and logs results to console.
pub struct ConsoleTransport {
    runner: PipelineExecutor,
}

impl ConsoleTransport {
    /// Create new console transport
    pub fn new() -> Result<Self> {
        Ok(Self {
            runner: PipelineExecutor::new()?,
        })
    }
}

#[async_trait]
impl PipelineTransport for ConsoleTransport {
    async fn execute(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        println!("[ConsoleTransport] Executing pipeline (unary mode)");
        println!("[ConsoleTransport] Input type: {}", input.data.data_type());

        let output = self.runner.execute_unary(manifest, input).await?;

        println!("[ConsoleTransport] Output type: {}", output.data.data_type());
        Ok(output)
    }

    async fn stream(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn StreamSession>> {
        println!("[ConsoleTransport] Creating streaming session");
        let session = self.runner.create_stream_session(manifest).await?;
        println!("[ConsoleTransport] Session ID: {}", session.session_id());
        Ok(Box::new(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_console_transport_creation() {
        let transport = ConsoleTransport::new();
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_console_transport_execute() {
        let transport = ConsoleTransport::new().unwrap();

        let manifest_json = r#"{
            "version": "v1",
            "nodes": [],
            "connections": []
        }"#;
        let manifest = Arc::new(Manifest::from_json(manifest_json).unwrap());

        let input = TransportData::new(RuntimeData::Text("Hello from custom transport!".into()));

        let result = transport.execute(manifest, input).await;
        assert!(result.is_ok());
    }
}
