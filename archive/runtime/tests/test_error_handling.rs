//! Integration tests for error handling and context propagation
//!
//! Tests T117: Error propagation with rich context, stack traces, metadata

use remotemedia_runtime::error::ErrorContext;
use remotemedia_runtime::executor::error::ExecutionErrorExt;
use remotemedia_runtime::{Error, Result};

/// Test: Error context builder pattern
#[test]
fn test_error_context_builder() {
    let ctx = ErrorContext::new()
        .with_node_id("audio_processor")
        .with_operation("resample")
        .with_stack_frame("pipeline::execute")
        .with_stack_frame("node::process")
        .with_metadata("input_rate", "44100")
        .with_metadata("output_rate", "16000");

    assert_eq!(ctx.node_id.as_ref().unwrap(), "audio_processor");
    assert_eq!(ctx.operation.as_ref().unwrap(), "resample");
    assert_eq!(ctx.stack_trace.len(), 2);
    assert_eq!(ctx.metadata.len(), 2);
}

/// Test: Error with context display formatting
#[test]
fn test_error_with_context_display() {
    let ctx = ErrorContext::new()
        .with_node_id("vad_node")
        .with_operation("detect_voice")
        .with_metadata("frame_size", "480");

    let err = Error::execution_with_context("Failed to detect voice activity", ctx);

    let error_string = err.to_string();
    assert!(error_string.contains("Execution error"));
    assert!(error_string.contains("Failed to detect voice activity"));
    assert!(error_string.contains("[node: vad_node]"));
    assert!(error_string.contains("[operation: detect_voice]"));
}

/// Test: Error context with stack trace
#[test]
fn test_error_with_stack_trace() {
    let ctx = ErrorContext::new()
        .with_node_id("pipeline_executor")
        .with_stack_frame("execute_pipeline() at executor.rs:123")
        .with_stack_frame("execute_node() at executor.rs:456")
        .with_stack_frame("process_audio() at audio.rs:789");

    let err = Error::execution_with_context("Processing failed", ctx);

    let error_string = err.to_string();
    assert!(error_string.contains("Stack trace:"));
    assert!(error_string.contains("execute_pipeline()"));
    assert!(error_string.contains("execute_node()"));
    assert!(error_string.contains("process_audio()"));
}

/// Test: Error context retrieval
#[test]
fn test_error_context_retrieval() {
    let ctx = ErrorContext::new()
        .with_node_id("test_node")
        .with_operation("test_op");

    let err = Error::execution_with_context("test error", ctx);

    let retrieved_ctx = err.context().unwrap();
    assert_eq!(retrieved_ctx.node_id.as_ref().unwrap(), "test_node");
    assert_eq!(retrieved_ctx.operation.as_ref().unwrap(), "test_op");
}

/// Test: Adding context to existing errors
#[test]
fn test_add_context_to_error() {
    let err = Error::execution("Original error message");

    let ctx = ErrorContext::new()
        .with_node_id("modified_node")
        .with_operation("modified_op");

    let err_with_ctx = err.with_context(ctx);

    let retrieved_ctx = err_with_ctx.context().unwrap();
    assert_eq!(retrieved_ctx.node_id.as_ref().unwrap(), "modified_node");
    assert_eq!(retrieved_ctx.operation.as_ref().unwrap(), "modified_op");
}

/// Test: Error propagation through function calls
#[tokio::test]
async fn test_error_propagation_with_context() {
    async fn inner_function() -> Result<()> {
        let ctx = ErrorContext::new()
            .with_node_id("inner_node")
            .with_operation("inner_operation")
            .with_stack_frame("inner_function() at line 100");

        Err(Error::execution_with_context("Inner function failed", ctx))
    }

    async fn outer_function() -> Result<()> {
        inner_function().await.map_err(|e| {
            let mut ctx = e.context().cloned().unwrap_or_default();
            ctx = ctx.with_stack_frame("outer_function() at line 200");
            e.with_context(ctx)
        })
    }

    let result = outer_function().await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let ctx = err.context().unwrap();
    assert_eq!(ctx.stack_trace.len(), 2);
    assert!(err.to_string().contains("inner_function()"));
    assert!(err.to_string().contains("outer_function()"));
}

/// Test: Different error types with context
#[test]
fn test_different_error_types_with_context() {
    let ctx = ErrorContext::new().with_node_id("test_node");

    let manifest_err = Error::Manifest {
        message: "Invalid manifest".to_string(),
        source: None,
        context: ctx.clone(),
    };
    assert!(manifest_err.context().is_some());

    let exec_err = Error::execution_with_context("Execution failed", ctx.clone());
    assert!(exec_err.context().is_some());

    let transport_err = Error::Transport {
        message: "Network error".to_string(),
        source: None,
        context: ctx.clone(),
    };
    assert!(transport_err.context().is_some());
}

/// Test: Error context metadata preservation
#[test]
fn test_error_metadata_preservation() {
    let ctx = ErrorContext::new()
        .with_metadata("retry_count", "3")
        .with_metadata("last_error", "timeout")
        .with_metadata("duration_ms", "5000");

    let err = Error::execution_with_context("Operation failed after retries", ctx);

    let retrieved_ctx = err.context().unwrap();
    assert_eq!(retrieved_ctx.metadata.get("retry_count").unwrap(), "3");
    assert_eq!(retrieved_ctx.metadata.get("last_error").unwrap(), "timeout");
    assert_eq!(retrieved_ctx.metadata.get("duration_ms").unwrap(), "5000");
}

/// Test: Error context display with all fields
#[test]
fn test_complete_error_context_display() {
    let ctx = ErrorContext::new()
        .with_node_id("complete_node")
        .with_operation("complete_operation")
        .with_stack_frame("frame_1")
        .with_stack_frame("frame_2")
        .with_metadata("key1", "value1")
        .with_metadata("key2", "value2");

    let display = ctx.to_string();
    assert!(display.contains("[node: complete_node]"));
    assert!(display.contains("[operation: complete_operation]"));
    assert!(display.contains("Stack trace:"));
    assert!(display.contains("frame_1"));
    assert!(display.contains("frame_2"));
    assert!(display.contains("Metadata:"));
    assert!(display.contains("key1: value1"));
    assert!(display.contains("key2: value2"));
}

/// Test: Chaining context operations
#[test]
fn test_context_chaining() {
    let ctx = ErrorContext::default()
        .with_node_id("chain_node")
        .with_operation("chain_op")
        .with_stack_frame("frame1")
        .with_metadata("m1", "v1");

    // Context should be fully populated
    assert!(ctx.node_id.is_some());
    assert!(ctx.operation.is_some());
    assert_eq!(ctx.stack_trace.len(), 1);
    assert_eq!(ctx.metadata.len(), 1);
}

/// Test: Error retryability with context
#[test]
fn test_error_retryability_with_context() {
    let ctx = ErrorContext::new().with_node_id("retry_test_node");

    // Retryable errors
    let exec_err = Error::execution_with_context("Retryable execution error", ctx.clone());
    assert!(exec_err.is_retryable());

    let transport_err = Error::Transport {
        message: "Network timeout".to_string(),
        source: None,
        context: ctx.clone(),
    };
    assert!(transport_err.is_retryable());

    // Non-retryable errors
    let manifest_err = Error::Manifest {
        message: "Invalid schema".to_string(),
        source: None,
        context: ctx.clone(),
    };
    assert!(!manifest_err.is_retryable());
}

/// Test: ExecutionErrorExt trait helpers
#[test]
fn test_execution_error_ext_helpers() {
    let ctx = ErrorContext::new()
        .with_node_id("helper_test")
        .with_operation("test_operation");

    // Test execution helper
    let exec_err = Error::execution("Simple execution error");
    assert!(exec_err.is_retryable());

    // Test timeout helper
    let timeout_err = Error::timeout("Operation timed out");
    assert!(timeout_err.to_string().contains("Timeout:"));
    assert!(timeout_err.is_retryable());

    // Test execution with context
    let ctx_err = Error::execution_ctx("Error with context", ctx);
    assert!(ctx_err.context().is_some());
    assert!(ctx_err.is_retryable());
}
