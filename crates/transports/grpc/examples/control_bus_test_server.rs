//! Minimal test-helper binary for the Python control-bus integration
//! tests.
//!
//! Starts a gRPC server bound to an ephemeral port, creates a live
//! pipeline session with a single `CalculatorNode`, and prints one line
//! to stdout:
//!
//!     READY <port> <session_id>
//!
//! The server runs until killed. Python drives the data plane by
//! continuing to call `SessionHandle::send_input` via a second RPC
//! client (not needed for this test) or by using `publish` on the
//! control bus.
//!
//! Intended to be spawned by `clients/python/tests/test_control_bus_grpc.py`.

use remotemedia_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use remotemedia_core::transport::PipelineExecutor;
use remotemedia_grpc::control::ControlServiceImpl;
use remotemedia_grpc::generated::{
    pipeline_control_server::PipelineControlServer,
    pipeline_execution_service_server::PipelineExecutionServiceServer,
    streaming_pipeline_service_server::StreamingPipelineServiceServer,
};
use remotemedia_grpc::{
    metrics::ServiceMetrics, ExecutionServiceImpl, ServiceConfig, StreamingServiceImpl,
};
use std::io::Write;
use std::sync::Arc;
use tonic::transport::Server;

fn calc_manifest() -> Manifest {
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "control-bus-test-pipeline".to_string(),
            ..Default::default()
        },
        nodes: vec![NodeManifest {
            id: "calc".to_string(),
            node_type: "CalculatorNode".to_string(),
            params: serde_json::json!({}),
            ..Default::default()
        }],
        connections: Vec::<Connection>::new(),
        python_env: None,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let executor = Arc::new(PipelineExecutor::new()?);

    // Pre-create a session so the Python test can attach without needing
    // to drive StreamPipeline itself.
    let session = executor
        .create_session(Arc::new(calc_manifest()))
        .await?;
    let session_id = session.session_id.clone();
    // Leak the session handle so the router task keeps running for the
    // test's lifetime. Dropping it would close the input channel and
    // tear down the session.
    std::mem::forget(session);

    // Build services.
    let config = ServiceConfig::default();
    let metrics = Arc::new(ServiceMetrics::with_default_registry()?);
    let execution =
        ExecutionServiceImpl::new(config.auth.clone(), config.limits.clone(), metrics.clone(), executor.clone());
    let streaming =
        StreamingServiceImpl::new(config.auth, config.limits, metrics, executor.clone());
    let control = ControlServiceImpl::new(executor.control_bus());

    // Signal ready to the Python test runner.
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "READY {port} {session_id}")?;
    stdout.flush()?;
    drop(stdout);

    Server::builder()
        .add_service(PipelineExecutionServiceServer::new(execution))
        .add_service(StreamingPipelineServiceServer::new(streaming))
        .add_service(PipelineControlServer::new(control))
        .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
        .await?;

    Ok(())
}
