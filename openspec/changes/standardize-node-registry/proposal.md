## Why

The current node registry system is fragmented across multiple locations with inconsistent patterns:
- `streaming_registry.rs` is a 1400+ line monolithic file with inline factory definitions
- Python node factories are mixed with Rust node factories
- Candle nodes have their own `register_candle_nodes()` pattern
- No standardized way to add nodes from external crates via Cargo.toml dependencies

This makes it difficult to:
1. Extend the system with new node crates
2. Maintain separation of concerns
3. Enable conditional compilation based on features
4. Support plugin-like node libraries

## What Changes

- **Add `NodeProvider` trait** for standardized node registration
- **Create `inventory`-based auto-registration** using the `inventory` crate for compile-time collection
- **Break out Python nodes** into a separate `remotemedia-nodes-python` crate
- **Refactor `streaming_registry.rs`** to use the new provider pattern
- **Update Candle nodes** to use the standardized `NodeProvider` trait
- **Add feature flags** for optional node providers in Cargo.toml

## Impact

- Affected specs: node-registry (new capability)
- Affected code:
  - `crates/core/src/nodes/streaming_registry.rs` - Major refactor
  - `crates/core/src/nodes/mod.rs` - Add NodeProvider trait
  - `crates/candle-nodes/src/registry.rs` - Update to use NodeProvider
  - `crates/nodes-python/` - New crate (break out from core)
  - `Cargo.toml` - Add feature flags and dependencies
