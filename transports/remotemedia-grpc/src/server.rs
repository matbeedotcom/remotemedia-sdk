//! Tonic server setup and configuration for gRPC service
//!
//! Implements server builder with middleware stack (auth, metrics, logging).
//! Provides graceful shutdown and health check support.

use crate::{
    auth::AuthConfig,
    execution::ExecutionServiceImpl,
    generated::{
        pipeline_execution_service_server::PipelineExecutionServiceServer,
        streaming_pipeline_service_server::StreamingPipelineServiceServer,
    },
    metrics::ServiceMetrics,
    streaming::StreamingServiceImpl,
    ServiceConfig,
};
use remotemedia_runtime_core::transport::PipelineRunner;
use std::sync::Arc;
use tonic::{service::LayerExt as _, transport::Server};
use tracing::{info, warn};

/// gRPC server builder with middleware
pub struct GrpcServer {
    config: ServiceConfig,
    metrics: Arc<ServiceMetrics>,
    runner: Arc<PipelineRunner>,
}

impl GrpcServer {
    /// Create new server with configuration and pipeline runner
    ///
    /// The runner encapsulates all executor and node registry details.
    /// The server is only responsible for the gRPC transport layer.
    pub fn new(
        config: ServiceConfig,
        runner: Arc<PipelineRunner>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let metrics = Arc::new(ServiceMetrics::with_default_registry()?);

        Ok(Self {
            config,
            metrics,
            runner,
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
    
    /// Get pipeline runner for creating model workers
    pub fn runner(&self) -> Arc<PipelineRunner> {
        Arc::clone(&self.runner)
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

        // Create service implementations with PipelineRunner
        let execution_service = ExecutionServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&self.runner),
        );

        let streaming_service = StreamingServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&self.runner),
        );

        // Wrap services with gRPC-Web and CORS support using tower ServiceBuilder
        let execution_service = tower::ServiceBuilder::new()
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tonic_web::GrpcWebLayer::new())
            .into_inner()
            .named_layer(
                PipelineExecutionServiceServer::new(execution_service)
                    .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
                    .max_encoding_message_size(10 * 1024 * 1024), // 10MB
            );

        let streaming_service = tower::ServiceBuilder::new()
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tonic_web::GrpcWebLayer::new())
            .into_inner()
            .named_layer(
                StreamingPipelineServiceServer::new(streaming_service)
                    .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
                    .max_encoding_message_size(10 * 1024 * 1024), // 10MB
            );

        // T037: Configure connection pooling and HTTP/2 keepalive for concurrent clients
        let server = Server::builder()
            // Allow many concurrent requests per connection
            .concurrency_limit_per_connection(256)
            // TCP keepalive to detect dead connections
            .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
            .tcp_nodelay(true)
            // HTTP/2 keepalive ping to keep connections alive
            .http2_keepalive_interval(Some(std::time::Duration::from_secs(30)))
            .http2_keepalive_timeout(Some(std::time::Duration::from_secs(10)))
            // Connection timeouts
            .timeout(std::time::Duration::from_secs(60))
            // Enable HTTP/1.1 for gRPC-Web
            .accept_http1(true)
            // Tracing
            .trace_fn(|_| tracing::info_span!("grpc_request"))
            .add_service(execution_service)
            .add_service(streaming_service);

        // TODO: Add graceful shutdown on Ctrl+C
        // Requires tokio signal feature which may not be available on all platforms
        info!("gRPC server listening on {}", addr);

        server.serve(addr).await?;

        Ok(())
    }

    /// Build and run the server with external shutdown flag (for robust Ctrl+C handling)
    pub async fn serve_with_shutdown_flag(
        self,
        shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let addr: std::net::SocketAddr = self.config.bind_address.parse()?;

        info!(
            %addr,
            auth_required = self.config.auth.require_auth,
            max_memory_mb = self.config.limits.max_memory_bytes / 1_000_000,
            "Starting gRPC server with shutdown flag"
        );

        // Create service implementations with PipelineRunner
        let execution_service = ExecutionServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&self.runner),
        );

        let streaming_service = StreamingServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&self.runner),
        );

        // Wrap services with gRPC-Web and CORS support using tower ServiceBuilder
        let execution_service = tower::ServiceBuilder::new()
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tonic_web::GrpcWebLayer::new())
            .into_inner()
            .named_layer(
                PipelineExecutionServiceServer::new(execution_service)
                    .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
                    .max_encoding_message_size(10 * 1024 * 1024), // 10MB
            );

        let streaming_service = tower::ServiceBuilder::new()
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tonic_web::GrpcWebLayer::new())
            .into_inner()
            .named_layer(
                StreamingPipelineServiceServer::new(streaming_service)
                    .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
                    .max_encoding_message_size(10 * 1024 * 1024), // 10MB
            );

        // T037: Configure connection pooling and HTTP/2 keepalive for concurrent clients
        let server = Server::builder()
            // Allow many concurrent requests per connection
            .concurrency_limit_per_connection(256)
            // TCP keepalive to detect dead connections
            .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
            .tcp_nodelay(true)
            // HTTP/2 keepalive ping to keep connections alive
            .http2_keepalive_interval(Some(std::time::Duration::from_secs(30)))
            .http2_keepalive_timeout(Some(std::time::Duration::from_secs(10)))
            // Connection timeouts
            .timeout(std::time::Duration::from_secs(60))
            // Enable HTTP/1.1 for gRPC-Web
            .accept_http1(true)
            // Tracing
            .trace_fn(|_| tracing::info_span!("grpc_request"))
            .add_service(execution_service)
            .add_service(streaming_service);

        info!("gRPC server listening on {}", addr);

        // Monitor shutdown flag and trigger graceful shutdown
        // Poll frequently (10ms) to ensure responsive shutdown
        let shutdown_future = async move {
            let mut check_count = 0u64;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                check_count += 1;

                if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    info!(
                        "[SHUTDOWN] Flag detected after {} checks ({}ms)",
                        check_count,
                        check_count * 10
                    );
                    info!("[SHUTDOWN] Initiating graceful shutdown of gRPC server...");
                    break;
                }

                // Log periodically to show we're still checking
                if check_count % 6000 == 0 {
                    // Every minute
                    info!(
                        "[HEALTH] Server running, checked shutdown flag {} times",
                        check_count
                    );
                }
            }
            info!("[SHUTDOWN] Shutdown future completed, server will now close connections");
        };

        info!("[SERVER] Calling serve_with_shutdown, will block until shutdown signal...");
        let serve_result = server.serve_with_shutdown(addr, shutdown_future).await;
        info!(
            "[SERVER] serve_with_shutdown returned: {:?}",
            serve_result.is_ok()
        );

        serve_result?;

        info!("[SHUTDOWN] gRPC server shutdown complete");

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
        let runner = Arc::new(PipelineRunner::new().unwrap());
        let server = GrpcServer::new(config, runner);
        assert!(server.is_ok());
    }

    #[test]
    fn test_metrics_access() {
        let config = ServiceConfig::default();
        let runner = Arc::new(PipelineRunner::new().unwrap());
        let server = GrpcServer::new(config, runner).unwrap();
        let metrics = server.metrics();

        // Test metrics are accessible
        metrics.active_connections.inc();
        assert_eq!(metrics.active_connections.get(), 1);
    }

    #[test]
    fn test_metrics_text_export() {
        let config = ServiceConfig::default();
        let runner = Arc::new(PipelineRunner::new().unwrap());
        let server = GrpcServer::new(config, runner).unwrap();

        let text = server.metrics_text();
        assert!(text.contains("remotemedia_grpc"));
    }

    #[test]
    fn test_auth_config_access() {
        let mut config = ServiceConfig::default();
        config.auth.require_auth = false;

        let runner = Arc::new(PipelineRunner::new().unwrap());
        let server = GrpcServer::new(config, runner).unwrap();
        assert!(!server.auth_config().require_auth);
    }
}
