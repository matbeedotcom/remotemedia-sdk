//! Mock gRPC server for testing RemotePipelineNode
//!
//! This module provides a simple mock gRPC server that echoes back inputs.
//! It's designed for testing remote pipeline execution without requiring a full server.
//!
//! # Features
//!
//! - Implements PipelineExecutionService
//! - Returns echo responses (input = output)
//! - Runs on random available port
//! - Can be started/stopped for tests
//!
//! # Usage
//!
//! ```
//! let server = MockGrpcServer::start().await?;
//! let endpoint = server.endpoint();
//!
//! // Use endpoint in tests...
//!
//! server.shutdown().await?;
//! ```

use remotemedia_grpc::generated::{
    pipeline_execution_service_server::{PipelineExecutionService, PipelineExecutionServiceServer},
    ExecuteRequest, ExecuteResponse, ExecutionResult, ExecutionStatus, VersionInfo, VersionRequest,
    VersionResponse,
};
use std::net::SocketAddr;
use tokio::sync::oneshot;
use tonic::{transport::Server, Request, Response, Status};

/// Mock gRPC server for testing
pub struct MockGrpcServer {
    /// Server endpoint (e.g., "127.0.0.1:12345")
    endpoint: String,
    /// Shutdown signal sender
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Server task handle
    server_handle: Option<tokio::task::JoinHandle<Result<(), tonic::transport::Error>>>,
}

impl MockGrpcServer {
    /// Start the mock server on a random available port
    ///
    /// # Returns
    ///
    /// * `Ok(MockGrpcServer)` - Server started successfully
    /// * `Err(anyhow::Error)` - Failed to start server
    pub async fn start() -> anyhow::Result<Self> {
        // Bind to random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let endpoint = addr.to_string();

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Create mock service
        let service = MockPipelineService;

        // Spawn server task
        let server_handle = tokio::spawn(async move {
            Server::builder()
                .add_service(PipelineExecutionServiceServer::new(service))
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    async {
                        shutdown_rx.await.ok();
                    },
                )
                .await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(Self {
            endpoint,
            shutdown_tx: Some(shutdown_tx),
            server_handle: Some(server_handle),
        })
    }

    /// Get the server endpoint
    ///
    /// # Returns
    ///
    /// Server endpoint string (e.g., "127.0.0.1:12345")
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Shutdown the server
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Server shutdown successfully
    /// * `Err(anyhow::Error)` - Failed to shutdown server
    pub async fn shutdown(mut self) -> anyhow::Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if let Some(handle) = self.server_handle.take() {
            handle.await??;
        }

        Ok(())
    }
}

/// Mock implementation of PipelineExecutionService
///
/// This service echoes back the input data as output.
struct MockPipelineService;

#[tonic::async_trait]
impl PipelineExecutionService for MockPipelineService {
    /// Execute a pipeline (echo implementation)
    ///
    /// This simply echoes back the input data as output, keyed by the first node ID
    /// in the manifest.
    async fn execute_pipeline(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<ExecuteResponse>, Status> {
        let req = request.into_inner();

        // Extract manifest
        let manifest = req
            .manifest
            .ok_or_else(|| Status::invalid_argument("Missing manifest"))?;

        // Get first node ID from manifest
        let first_node = manifest
            .nodes
            .first()
            .ok_or_else(|| Status::invalid_argument("Manifest has no nodes"))?;
        let node_id = first_node.id.clone();

        // Echo back the first input
        let data_outputs = if let Some((_input_key, input_data)) = req.data_inputs.iter().next() {
            vec![(node_id, input_data.clone())].into_iter().collect()
        } else {
            std::collections::HashMap::new()
        };

        // Build response
        let result = ExecutionResult {
            data_outputs,
            metrics: None,
            node_results: vec![],
            status: ExecutionStatus::Success as i32,
        };

        let response = ExecuteResponse {
            outcome: Some(remotemedia_grpc::generated::execute_response::Outcome::Result(result)),
        };

        Ok(Response::new(response))
    }

    /// Get service version
    async fn get_version(
        &self,
        _request: Request<VersionRequest>,
    ) -> Result<Response<VersionResponse>, Status> {
        let version_info = VersionInfo {
            protocol_version: "v1".to_string(),
            runtime_version: "mock-0.1.0".to_string(),
            supported_node_types: vec!["PassThrough".to_string()],
            supported_protocols: vec!["v1".to_string()],
            build_timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let response = VersionResponse {
            version_info: Some(version_info),
            compatible: true,
            compatibility_message: "Mock server accepts all versions".to_string(),
        };

        Ok(Response::new(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_start_stop() {
        let server = MockGrpcServer::start().await.unwrap();
        let endpoint = server.endpoint().to_string();

        // Verify endpoint is valid
        assert!(!endpoint.is_empty());
        assert!(endpoint.contains(":"));

        // Shutdown
        server.shutdown().await.unwrap();
    }
}
