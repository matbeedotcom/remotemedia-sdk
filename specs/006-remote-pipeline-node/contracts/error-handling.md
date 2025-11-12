# Error Handling

Contract Version: v0.4.0 | Feature: 006-remote-pipeline-node

## Error Types

### RemoteExecutionFailed
Server execution error

### RemoteTimeout
Execution exceeded timeout

### CircuitBreakerOpen
Endpoint circuit breaker triggered

### AllEndpointsFailed
All endpoints exhausted

### ManifestFetchFailed
Failed to fetch remote manifest

## Error Code Mapping

See data-model.md and research.md for detailed error handling strategy.

## Diagnostic Messages

Format: [CODE] Description | Endpoint | Node | Troubleshooting steps
