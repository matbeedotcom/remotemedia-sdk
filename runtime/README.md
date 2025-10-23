# RemoteMedia Runtime

Language-neutral execution engine for distributed AI pipelines.

## Overview

This is the Rust-based runtime that executes RemoteMedia pipelines. It provides:

- **Manifest-based execution**: Pipelines serialized to JSON manifests
- **RustPython VM**: Backward compatibility with existing Python nodes
- **WASM sandbox**: Portable, secure node execution
- **Multiple transports**: gRPC (existing), WebRTC (Phase 2)
- **Capability scheduling**: Automatic executor selection (Phase 4)

## Architecture

```
Python SDK → Manifest → Rust Runtime → [RustPython VM | WASM Sandbox]
```

## Building

Requires Rust 1.70+ toolchain:

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build the runtime
cd runtime
cargo build

# Run tests
cargo test

# Build release version
cargo build --release
```

## Development Status

**Phase 1**: Foundation & MVP (In Progress)
- [x] Project scaffolding
- [x] Manifest schema definition
- [ ] Pipeline executor core
- [ ] RustPython VM integration
- [ ] Python FFI layer
- [ ] Compatibility testing

See `openspec/changes/refactor-language-neutral-runtime/tasks.md` for full implementation checklist.

## Project Structure

```
runtime/
├── src/
│   ├── executor/      # Pipeline execution engine
│   ├── manifest/      # JSON parsing & validation
│   ├── nodes/         # Node type implementations
│   ├── python/        # RustPython integration
│   ├── wasm/          # WASM runtime (Phase 3)
│   ├── transport/     # WebRTC & gRPC (Phase 2)
│   ├── cache/         # Package caching (Phase 4)
│   └── registry/      # OCI registry (Phase 4)
├── tests/             # Integration tests
└── benches/           # Performance benchmarks
```

## License

MIT OR Apache-2.0
