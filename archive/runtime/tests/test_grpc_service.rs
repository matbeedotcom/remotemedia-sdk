//! Integration tests for gRPC service
//!
//! Tests ExecutePipeline RPC implementation including:
//! - Contract validation (protobuf schema)
//! - Audio processing (resample, multi-node)
//! - Performance (<5ms p50 latency)
//! - Metrics collection

#![cfg(feature = "grpc-transport")]

mod grpc_integration;
