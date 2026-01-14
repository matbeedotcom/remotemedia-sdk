# Test Fixtures for Remote Pipeline Node

## Mock Servers

The `mock_server.rs` file contains mock gRPC and HTTP server implementations for testing.

### Current Status

**Mock Infrastructure**: ✅ Implemented
**Integration Tests**: ⏸️ Pending protobuf conversion utilities

### Blocked Tests (6)

The following tests are marked `#[ignore]` and require protobuf conversion:

1. `test_single_remote_node` - Needs RuntimeData ↔ DataBuffer conversion
2. `test_remote_timeout` - Needs mock server with configurable delay
3. `test_remote_retry` - Needs mock server that can simulate failures
4. `test_multi_transport_pipeline` - Needs both gRPC and HTTP mock servers
5. `test_remote_manifest_loading` - Needs HTTP server serving manifests
6. `test_manifest_name_resolution` - Needs HTTP /manifests/{name} endpoint

### What's Needed

The mock servers exist but need:
- Protobuf conversion utilities from `remotemedia-grpc`
- RuntimeData → DataBuffer serialization
- DataBuffer → RuntimeData deserialization

These utilities should live in the `remotemedia-grpc` transport crate, not runtime-core.

### Running Available Tests

```bash
cd runtime-core
cargo test --features grpc-client --test test_remote_node
```

**Current Results**: 10 passed, 0 failed, 6 ignored

### Example Manifests

- `local-vad-remote-tts.json` - Local VAD → Remote TTS (US1)
- `passthrough-remote-echo.json` - Simple passthrough test
- `microservices-composition.json` - Multi-service composition (US3)
