//! Probe backends — pluggable execution engines for testing manifests

pub mod direct;

#[cfg(feature = "probe-grpc")]
pub mod grpc;

#[cfg(feature = "probe-webrtc")]
pub mod webrtc;

#[cfg(feature = "probe-http")]
pub mod http;

use crate::report::ProbeResult;
use async_trait::async_trait;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::manifest::Manifest;
use remotemedia_manifest_analyzer::AnalysisResult;
use std::sync::Arc;
use std::time::Duration;

/// Specification for which probe to run
#[derive(Debug, Clone)]
pub enum ProbeSpec {
    /// Direct in-process execution via PipelineExecutor
    Direct,
    /// gRPC transport probe
    Grpc { port: Option<u16> },
    /// WebRTC transport probe
    WebRtc { signal_port: Option<u16> },
    /// HTTP transport probe
    Http { port: Option<u16> },
}

/// Context passed to each probe
pub struct ProbeContext {
    pub manifest: Arc<Manifest>,
    pub analysis: Arc<AnalysisResult>,
    pub test_data: Vec<RuntimeData>,
    pub timeout: Duration,
    pub skip_ml: bool,
}

/// Trait for pluggable test probe backends
#[async_trait]
pub trait ProbeBackend: Send + Sync {
    /// Human-readable name for this probe
    fn name(&self) -> &str;

    /// Run the probe and return results
    async fn probe(&self, ctx: &ProbeContext) -> ProbeResult;
}
