# Custom Transport Example

This example demonstrates how to create a custom transport implementation using **only** `remotemedia-runtime-core`, without any gRPC, FFI, or other transport dependencies.

## Purpose

Shows that developers can:
- Use runtime-core as a standalone library
- Implement the `PipelineTransport` trait
- Execute pipelines without transport dependencies
- Create custom transports in ~80 lines of code

## Implementation

The `ConsoleTransport` is a minimal transport that:
- Reads input from function arguments
- Executes pipelines via `PipelineRunner`
- Prints output to console/logs

**Total code**: ~80 lines (well under the 100-line success criterion)

## Running

### Unary Execution Example

```bash
cargo run --bin custom-transport
```

This demonstrates:
- Creating a `ConsoleTransport`
- Executing text and audio data through unary mode
- Zero transport dependencies

### Streaming Example

```bash
cargo run --example streaming
```

This demonstrates:
- Creating a streaming session
- Sending multiple chunks with sequence numbers
- Receiving outputs continuously
- Session lifecycle (close, is_active)

## Verification

### Check Dependencies

```bash
cd examples/custom-transport
cargo tree | grep -E '(tonic|prost|pyo3|tower|hyper)'
```

**Expected**: No matches (zero transport dependencies)

### Build Time

```bash
cargo clean
time cargo build --release
```

**Expected**: Fast build since only depends on runtime-core

### Run Tests

```bash
cargo test
```

**Expected**: All tests pass

## Code Structure

```
examples/custom-transport/
├── Cargo.toml              # Only depends on runtime-core
├── src/
│   ├── lib.rs              # ConsoleTransport implementation (~80 lines)
│   └── main.rs             # Unary execution demo
└── examples/
    └── streaming.rs        # Streaming execution demo
```

## Key Takeaways

1. **Minimal dependencies**: Only `remotemedia-runtime-core` + tokio
2. **Simple implementation**: `PipelineTransport` trait has just 2 methods
3. **Full functionality**: Supports both unary and streaming modes
4. **No transport overhead**: No network, serialization, or FFI complexity
5. **Production-ready pattern**: This pattern works for real custom transports (Kafka, Redis, etc.)

## Next Steps

- See `docs/CUSTOM_TRANSPORT_GUIDE.md` for detailed implementation guide
- Review contracts in `specs/003-transport-decoupling/contracts/`
- Check `docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md` for architecture overview
