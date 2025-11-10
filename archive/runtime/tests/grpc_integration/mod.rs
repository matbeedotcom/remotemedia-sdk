//! Integration test module for gRPC execution service
//!
//! This module aggregates all integration tests for the ExecutePipeline RPC.

#![cfg(feature = "grpc-transport")]

mod test_execution_contract;
mod test_execution_metrics;
mod test_execution_multi_node;
mod test_execution_performance;
mod test_execution_resample;

// Phase 4: Concurrent support tests (T031-T035)
mod test_concurrent_connections;
mod test_concurrent_isolation;
mod test_concurrent_load;
mod test_concurrent_memory;
mod test_concurrent_performance;

// Phase 5: Streaming tests
mod test_streaming_backpressure;
mod test_streaming_contract;
mod test_streaming_latency;
mod test_streaming_lifecycle;
mod test_streaming_timing;
mod test_streaming_vad;

// Phase 3: Generic streaming tests (Feature 004)
mod test_generic_streaming;

// Mixed Rust + Python pipeline tests
mod test_python_rust_mixed_pipeline;

// LFM2-Audio node tests
mod test_lfm2_audio;
mod test_lfm2_audio_grpc_simple;
mod test_lfm2_audio_real;
// mod test_lfm2_audio_grpc_e2e; // TODO: Fix client import issues

// Test helpers
mod test_helpers;
