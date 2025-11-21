//! Mock transport plugin for testing

use async_trait::async_trait;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::transport::runner::PipelineRunner;
use remotemedia_runtime_core::transport::{
    ClientConfig, ClientStreamSession, PipelineClient, PipelineTransport, ServerConfig,
    StreamSession, TransportData, TransportPlugin,
};
use remotemedia_runtime_core::Result;
use std::sync::Arc;

pub struct MockTransportPlugin;

#[async_trait]
impl TransportPlugin for MockTransportPlugin {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn create_client(&self, _config: &ClientConfig) -> Result<Box<dyn PipelineClient>> {
        Ok(Box::new(MockClient))
    }

    async fn create_server(
        &self,
        _config: &ServerConfig,
        _runner: Arc<PipelineRunner>,
    ) -> Result<Box<dyn PipelineTransport>> {
        Ok(Box::new(MockServer))
    }

    fn validate_config(&self, _extra_config: &serde_json::Value) -> Result<()> {
        Ok(())
    }
}

// MockClient - echoes input as output
struct MockClient;

#[async_trait]
impl PipelineClient for MockClient {
    async fn execute_unary(
        &self,
        _manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        // Echo input as output
        Ok(input)
    }

    async fn create_stream_session(
        &self,
        _manifest: Arc<Manifest>,
    ) -> Result<Box<dyn ClientStreamSession>> {
        Ok(Box::new(MockStreamSession::new()))
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

// MockServer - similar echo behavior
struct MockServer;

#[async_trait]
impl PipelineTransport for MockServer {
    async fn execute(
        &self,
        _manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        Ok(input)
    }

    async fn stream(&self, _manifest: Arc<Manifest>) -> Result<Box<dyn StreamSession>> {
        Ok(Box::new(MockServerStreamSession::new()))
    }
}

// MockStreamSession for client
struct MockStreamSession {
    session_id: String,
    buffer: Vec<TransportData>,
}

impl MockStreamSession {
    fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            buffer: Vec::new(),
        }
    }
}

#[async_trait]
impl ClientStreamSession for MockStreamSession {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn send(&mut self, data: TransportData) -> Result<()> {
        self.buffer.push(data);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<TransportData>> {
        Ok(self.buffer.pop())
    }

    async fn close(&mut self) -> Result<()> {
        self.buffer.clear();
        Ok(())
    }

    fn is_active(&self) -> bool {
        true
    }
}

// MockServerStreamSession for server
struct MockServerStreamSession {
    session_id: String,
}

impl MockServerStreamSession {
    fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

#[async_trait]
impl StreamSession for MockServerStreamSession {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn send_input(&mut self, _data: TransportData) -> Result<()> {
        Ok(())
    }

    async fn recv_output(&mut self) -> Result<Option<TransportData>> {
        Ok(None)
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }

    fn is_active(&self) -> bool {
        true
    }
}
