//! Tonic server setup and configuration for gRPC service
//!
//! Implements server builder with middleware stack (auth, metrics, logging).
//! Provides graceful shutdown and health check support.

#![cfg(feature = "grpc-transport")]

use crate::grpc_service::{
    auth::AuthConfig,
    execution::ExecutionServiceImpl,
    streaming::StreamingServiceImpl,
    generated::{
        pipeline_execution_service_server::PipelineExecutionServiceServer,
        streaming_pipeline_service_server::StreamingPipelineServiceServer,
    },
    metrics::ServiceMetrics,
    ServiceConfig,
};
use crate::executor::Executor;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::{info, warn};

/// gRPC server builder with middleware
pub struct GrpcServer {
    config: ServiceConfig,
    metrics: Arc<ServiceMetrics>,
    executor: Arc<Executor>,
}

impl GrpcServer {
    /// Create new server with configuration and executor
    pub fn new(config: ServiceConfig, executor: Arc<Executor>) -> Result<Self, Box<dyn std::error::Error>> {
        let metrics = Arc::new(ServiceMetrics::with_default_registry()?);
        
        Ok(Self { 
            config, 
            metrics,
            executor,
        })
    }

    /// Get metrics for use in service implementations
    pub fn metrics(&self) -> Arc<ServiceMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Get auth config for use in service implementations
    pub fn auth_config(&self) -> &AuthConfig {
        &self.config.auth
    }

    /// Build and run the server
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr: std::net::SocketAddr = self.config.bind_address.parse()?;
        
        info!(
            %addr,
            auth_required = self.config.auth.require_auth,
            max_memory_mb = self.config.limits.max_memory_bytes / 1_000_000,
            "Starting gRPC server"
        );

        // Create service implementations
        let execution_service = ExecutionServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            self.config.version.clone(),
            Arc::clone(&self.metrics),
        );

        // Phase 5: Streaming service (T058)
        let streaming_service = StreamingServiceImpl::new(
            self.config.clone(),
            Arc::clone(&self.executor),
            Arc::clone(&self.metrics),
        );

        let server = Server::builder()
            .trace_fn(|_| tracing::info_span!("grpc_request"))
            .add_service(PipelineExecutionServiceServer::new(execution_service))
            .add_service(StreamingPipelineServiceServer::new(streaming_service))
            ;

        // Graceful shutdown on Ctrl+C
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for Ctrl+C");
            info!("Received shutdown signal");
            let _ = tx.send(());
        });

        info!("gRPC server listening on {}", addr);
        
        server
            .serve_with_shutdown(addr, async {
                rx.await.ok();
                info!("Graceful shutdown complete");
            })
            .await?;

        Ok(())
    }

    /// Expose Prometheus metrics as HTTP endpoint
    ///
    /// Returns metrics text for /metrics endpoint
    pub fn metrics_text(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.metrics.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let config = ServiceConfig::default();
        let server = GrpcServer::new(config);
        assert!(server.is_ok());
    }

    #[test]
    fn test_metrics_access() {
        let config = ServiceConfig::default();
        let server = GrpcServer::new(config).unwrap();
        let metrics = server.metrics();
        
        // Test metrics are accessible
        metrics.active_connections.inc();
        assert_eq!(metrics.active_connections.get(), 1);
    }

    #[test]
    fn test_metrics_text_export() {
        let config = ServiceConfig::default();
        let server = GrpcServer::new(config).unwrap();
        
        let text = server.metrics_text();
        assert!(text.contains("remotemedia_grpc"));
    }

    #[test]
    fn test_auth_config_access() {
        let mut config = ServiceConfig::default();
        config.auth.require_auth = false;
        
        let server = GrpcServer::new(config).unwrap();
        assert!(!server.auth_config().require_auth);
    }
}
