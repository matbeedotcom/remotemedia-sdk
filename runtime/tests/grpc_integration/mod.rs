//! Integration test module for gRPC execution service
//!
//! This module aggregates all integration tests for the ExecutePipeline RPC.

#![cfg(feature = "grpc-transport")]

mod test_execution_contract;
mod test_execution_resample;
mod test_execution_multi_node;
mod test_execution_performance;
mod test_execution_metrics;

// Phase 4: Concurrent support tests (T031-T035)
mod test_concurrent_load;
mod test_concurrent_isolation;
mod test_concurrent_performance;
mod test_concurrent_connections;
mod test_concurrent_memory;

// Phase 5: Streaming tests
mod test_streaming_contract;
mod test_streaming_vad;
mod test_streaming_latency;
mod test_streaming_backpressure;
mod test_streaming_lifecycle;

// Phase 3: Generic streaming tests (Feature 004)
mod test_generic_streaming;

// Test helpers
mod test_helpers;
