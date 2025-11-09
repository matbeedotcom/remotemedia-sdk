# RemoteMedia Transports

This directory contains transport implementation crates that depend on `remotemedia-runtime-core`.

## Available Transports

- **remotemedia-grpc**: gRPC transport for remote pipeline execution
- **remotemedia-ffi**: Python FFI transport for Python SDK integration
- **remotemedia-webrtc**: WebRTC transport (placeholder for future development)

## Architecture

Each transport is an independent crate that:
1. Depends on `remotemedia-runtime-core`
2. Implements the `PipelineTransport` trait
3. Handles its own serialization format
4. Can be independently versioned and deployed

See `docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md` for details.
