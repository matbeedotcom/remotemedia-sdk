//! Transport detection — determines which transports apply to a pipeline

use crate::ExecutionMode;
use remotemedia_core::nodes::schema::RuntimeDataType;
use serde::{Deserialize, Serialize};

/// A transport that can be used to test this pipeline
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicableTransport {
    /// Direct in-process execution (no transport layer)
    Direct,
    /// gRPC streaming
    GrpcStreaming,
    /// gRPC unary
    GrpcUnary,
    /// WebRTC (audio/video tracks)
    WebRtc,
    /// HTTP POST (unary only)
    Http,
}

/// Detect which transports are applicable based on execution mode and data types
pub fn detect(
    execution_mode: &ExecutionMode,
    source_types: &[RuntimeDataType],
    sink_types: &[RuntimeDataType],
) -> Vec<ApplicableTransport> {
    let mut transports = vec![ApplicableTransport::Direct];

    let has_media = source_types
        .iter()
        .chain(sink_types.iter())
        .any(|t| matches!(t, RuntimeDataType::Audio | RuntimeDataType::Video));

    match execution_mode {
        ExecutionMode::Streaming => {
            transports.push(ApplicableTransport::GrpcStreaming);
            if has_media {
                transports.push(ApplicableTransport::WebRtc);
            }
        }
        ExecutionMode::Unary => {
            transports.push(ApplicableTransport::GrpcUnary);
            transports.push(ApplicableTransport::Http);
        }
    }

    transports
}
