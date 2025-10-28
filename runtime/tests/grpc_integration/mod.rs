//! Integration test module for gRPC execution service
//!
//! This module aggregates all integration tests for the ExecutePipeline RPC.

#![cfg(feature = "grpc-transport")]

mod test_execution_contract;
mod test_execution_resample;
mod test_execution_multi_node;
mod test_execution_performance;
mod test_execution_metrics;

// Phase 5: Streaming tests
mod test_streaming_contract;
