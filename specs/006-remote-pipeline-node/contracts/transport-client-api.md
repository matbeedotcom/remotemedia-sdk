# Transport Client API

Contract Version: v0.4.0 | Feature: 006-remote-pipeline-node

## PipelineClient Trait

See data-model.md for trait definition.

Transport clients must implement execute_unary, create_stream_session, and health_check methods.

## ClientStreamSession Trait

Provides send, receive, and close methods for streaming.

## Implementation Requirements

- Respect ExecutionContext timeout
- Pass auth tokens from context
- Return retryable errors for transient failures
- Thread-safe (Send + Sync)
