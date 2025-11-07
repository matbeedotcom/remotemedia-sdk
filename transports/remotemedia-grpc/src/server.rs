//! Tonic server setup and configuration for gRPC service
//!
//! Implements server builder with middleware stack (auth, metrics, logging).
//! Provides graceful shutdown and health check support.

#![cfg(feature = "grpc-transport")]

use crate::{
    auth::AuthConfig,
    execution::ExecutionServiceImpl,
    streaming::StreamingServiceImpl,
    generated::{
        pipeline_execution_service_server::PipelineExecutionServiceServer,
        streaming_pipeline_service_server::StreamingPipelineServiceServer,
    },
    metrics::ServiceMetrics,
    executor_registry::{ExecutorRegistry, ExecutorType, PatternRule},
    ServiceConfig,
};
use remotemedia_runtime_core::executor::Executor;
use std::sync::Arc;
use tonic::{service::LayerExt as _, transport::Server};
use tracing::{info, warn};

/// gRPC server builder with middleware
pub struct GrpcServer {
    config: ServiceConfig,
    metrics: Arc<ServiceMetrics>,
    executor: Arc<Executor>,
    executor_registry: Arc<ExecutorRegistry>,
}

impl GrpcServer {
    /// Create new server with configuration and executor
    pub fn new(config: ServiceConfig, executor: Arc<Executor>) -> Result<Self, Box<dyn std::error::Error>> {
        let metrics = Arc::new(ServiceMetrics::with_default_registry()?);

        // Initialize executor registry with default mappings
        let executor_registry = Arc::new(Self::initialize_executor_registry()?);

        Ok(Self {
            config,
            metrics,
            executor,
            executor_registry,
        })
    }

    /// Initialize executor registry with default node type mappings
    fn initialize_executor_registry() -> Result<ExecutorRegistry, Box<dyn std::error::Error>> {
        let mut registry = ExecutorRegistry::new();

        // Register native Rust audio nodes
        registry.register_explicit("AudioChunkerNode", ExecutorType::Native);
        registry.register_explicit("FastResampleNode", ExecutorType::Native);
        registry.register_explicit("SileroVADNode", ExecutorType::Native);
        registry.register_explicit("AudioBufferAccumulatorNode", ExecutorType::Native);

        // Register pattern for Python multiprocess nodes
        #[cfg(feature = "multiprocess")]
        {
            // Pattern: Node types ending with "Node" that are not explicitly registered
            // go to multiprocess executor (for WhisperNode, LFM2Node, VibeVoiceNode, etc.)
            let python_pattern = PatternRule::new(
                r"^(Whisper|LFM2|VibeVoice|HF.*)Node$",
                ExecutorType::Multiprocess,
                100,
                "Python AI model nodes",
            )?;
            registry.register_pattern(python_pattern);
        }

        // Set default executor to Native
        registry.set_default(ExecutorType::Native);

        let summary = registry.summary();
        info!(
            "Executor registry initialized: {} explicit mappings, {} pattern rules, default: {:?}",
            summary.explicit_mappings_count,
            summary.pattern_rules_count,
            summary.default_executor
        );

        Ok(registry)
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

        // Create multiprocess executor (spec 002)
        #[cfg(feature = "multiprocess")]
        let multiprocess_executor = {
            use remotemedia_runtime_core::python::multiprocess::{MultiprocessExecutor, MultiprocessConfig};

            let config = MultiprocessConfig::from_default_file()
                .map_err(|e| format!("Failed to load multiprocess config: {}", e))?;

            Arc::new(MultiprocessExecutor::new(config))
        };

        // Create service implementations
        #[cfg(feature = "multiprocess")]
        let execution_service = ExecutionServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            self.config.version.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&self.executor),
            Arc::clone(&self.executor_registry),
            multiprocess_executor.clone(),
        );

        #[cfg(not(feature = "multiprocess"))]
        let execution_service = ExecutionServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            self.config.version.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&self.executor),
            Arc::clone(&self.executor_registry),
        );

        // Phase 5: Streaming service (T058)
        #[cfg(feature = "multiprocess")]
        let streaming_service = {
            let mut service = StreamingServiceImpl::new(
                self.config.clone(),
                Arc::clone(&self.executor),
                Arc::clone(&self.metrics),
            );
            service.set_multiprocess_executor(multiprocess_executor.clone());
            service
        };

        #[cfg(not(feature = "multiprocess"))]
        let streaming_service = StreamingServiceImpl::new(
            self.config.clone(),
            Arc::clone(&self.executor),
            Arc::clone(&self.metrics),
        );

        // Wrap services with gRPC-Web and CORS support using tower ServiceBuilder
        let execution_service = tower::ServiceBuilder::new()
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tonic_web::GrpcWebLayer::new())
            .into_inner()
            .named_layer(
                PipelineExecutionServiceServer::new(execution_service)
                    .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
                    .max_encoding_message_size(10 * 1024 * 1024) // 10MB
            );

        let streaming_service = tower::ServiceBuilder::new()
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tonic_web::GrpcWebLayer::new())
            .into_inner()
            .named_layer(
                StreamingPipelineServiceServer::new(streaming_service)
                    .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
                    .max_encoding_message_size(10 * 1024 * 1024) // 10MB
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
            .add_service(streaming_service)
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
            "Starting gRPC server"
        );

        // Create multiprocess executor (spec 002)
        #[cfg(feature = "multiprocess")]
        let multiprocess_executor_2 = {
            use remotemedia_runtime_core::python::multiprocess::{MultiprocessExecutor, MultiprocessConfig};

            let config = MultiprocessConfig::from_default_file()
                .map_err(|e| format!("Failed to load multiprocess config: {}", e))?;

            Arc::new(MultiprocessExecutor::new(config))
        };

        // Create service implementations
        #[cfg(feature = "multiprocess")]
        let execution_service = ExecutionServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            self.config.version.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&self.executor),
            Arc::clone(&self.executor_registry),
            multiprocess_executor_2.clone(),
        );

        #[cfg(not(feature = "multiprocess"))]
        let execution_service = ExecutionServiceImpl::new(
            self.config.auth.clone(),
            self.config.limits.clone(),
            self.config.version.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&self.executor),
            Arc::clone(&self.executor_registry),
        );

        // Phase 5: Streaming service (T058)
        #[cfg(feature = "multiprocess")]
        let streaming_service = {
            let mut service = StreamingServiceImpl::new(
                self.config.clone(),
                Arc::clone(&self.executor),
                Arc::clone(&self.metrics),
            );
            service.set_multiprocess_executor(multiprocess_executor_2.clone());
            service
        };

        #[cfg(not(feature = "multiprocess"))]
        let streaming_service = StreamingServiceImpl::new(
            self.config.clone(),
            Arc::clone(&self.executor),
            Arc::clone(&self.metrics),
        );

        // Wrap services with gRPC-Web and CORS support using tower ServiceBuilder
        let execution_service = tower::ServiceBuilder::new()
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tonic_web::GrpcWebLayer::new())
            .into_inner()
            .named_layer(
                PipelineExecutionServiceServer::new(execution_service)
                    .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
                    .max_encoding_message_size(10 * 1024 * 1024) // 10MB
            );

        let streaming_service = tower::ServiceBuilder::new()
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tonic_web::GrpcWebLayer::new())
            .into_inner()
            .named_layer(
                StreamingPipelineServiceServer::new(streaming_service)
                    .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
                    .max_encoding_message_size(10 * 1024 * 1024) // 10MB
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
            .add_service(streaming_service)
            ;

        info!("gRPC server listening on {}", addr);
        
        // Monitor shutdown flag and trigger graceful shutdown
        // Poll frequently (10ms) to ensure responsive shutdown
        let shutdown_future = async move {
            let mut check_count = 0u64;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                check_count += 1;
                
                if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    info!("[SHUTDOWN] Flag detected after {} checks ({}ms)", check_count, check_count * 10);
                    info!("[SHUTDOWN] Initiating graceful shutdown of gRPC server...");
                    break;
                }
                
                // Log periodically to show we're still checking
                if check_count % 6000 == 0 {  // Every minute
                    info!("[HEALTH] Server running, checked shutdown flag {} times", check_count);
                }
            }
            info!("[SHUTDOWN] Shutdown future completed, server will now close connections");
        };
        
        info!("[SERVER] Calling serve_with_shutdown, will block until shutdown signal...");
        let serve_result = server.serve_with_shutdown(addr, shutdown_future).await;
        info!("[SERVER] serve_with_shutdown returned: {:?}", serve_result.is_ok());
        
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
        let executor = Arc::new(Executor::new());
        let server = GrpcServer::new(config, executor);
        assert!(server.is_ok());
    }

    #[test]
    fn test_metrics_access() {
        let config = ServiceConfig::default();
        let executor = Arc::new(Executor::new());
        let server = GrpcServer::new(config, executor).unwrap();
        let metrics = server.metrics();

        // Test metrics are accessible
        metrics.active_connections.inc();
        assert_eq!(metrics.active_connections.get(), 1);
    }

    #[test]
    fn test_metrics_text_export() {
        let config = ServiceConfig::default();
        let executor = Arc::new(Executor::new());
        let server = GrpcServer::new(config, executor).unwrap();

        let text = server.metrics_text();
        assert!(text.contains("remotemedia_grpc"));
    }

    #[test]
    fn test_auth_config_access() {
        let mut config = ServiceConfig::default();
        config.auth.require_auth = false;

        let executor = Arc::new(Executor::new());
        let server = GrpcServer::new(config, executor).unwrap();
        assert!(!server.auth_config().require_auth);
    }
}
