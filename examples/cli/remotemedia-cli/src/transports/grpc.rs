//! gRPC transport wrapper

use anyhow::Result;

/// gRPC client wrapper
pub struct GrpcClient {
    endpoint: String,
    #[allow(dead_code)]
    auth_token: Option<String>,
}

impl GrpcClient {
    /// Create a new gRPC client
    pub fn new(endpoint: &str, auth_token: Option<String>) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            auth_token,
        }
    }

    /// Connect to the gRPC server
    pub async fn connect(&self) -> Result<()> {
        // TODO: Use remotemedia-grpc transport
        tracing::info!("Connecting to gRPC server: {}", self.endpoint);
        Ok(())
    }

    /// Execute a pipeline
    pub async fn execute(&self, _manifest: &str, _input: Option<Vec<u8>>) -> Result<Vec<u8>> {
        // TODO: Implement using remotemedia-grpc
        anyhow::bail!("gRPC execution not yet implemented")
    }
}
