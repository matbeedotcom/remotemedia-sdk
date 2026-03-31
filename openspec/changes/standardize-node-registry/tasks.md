## 1. Core Infrastructure

- [x] 1.1 Add `inventory` crate dependency to `remotemedia-core`
- [x] 1.2 Create `NodeProvider` trait in `crates/core/src/nodes/provider.rs`
- [x] 1.3 Add `inventory::collect!` for providers in core
- [x] 1.4 Update `create_default_streaming_registry()` to use inventory collection
- [x] 1.5 Add `provider` module to `crates/core/src/nodes/mod.rs`

## 2. Refactor Core Nodes

- [x] 2.1 Create `CoreNodesProvider` in `crates/core/src/nodes/core_provider.rs`
- [x] ~~2.2 Move inline factories from `streaming_registry.rs` to dedicated files~~ (Kept in place for now)
- [x] 2.3 Register `CoreNodesProvider` via `inventory::submit!`
- [x] 2.4 Clean up `streaming_registry.rs` to only contain registry creation logic

## 3. Create Python Nodes Crate (Dynamic Registration)

- [x] 3.1 Create `crates/python-nodes/Cargo.toml` with dependencies
- [x] 3.2 Create dynamic registration system with `PythonNodeConfig` and `PythonNodeRegistry`
- [x] 3.3 Create `DynamicPythonNodeFactory` that creates factories from registry config
- [x] 3.4 Implement `PythonNodesProvider` with `inventory::submit!`
- [x] 3.5 Create Python-side `@streaming_node` decorator in `clients/python/remotemedia/nodes/registration.py`
- [x] 3.6 Add `register_python_node()` function for simple Rust-side registration

## 4. Update Candle Nodes

- [x] 4.1 Implement `NodeProvider` for `CandleNodesProvider`
- [x] 4.2 Replace `register_candle_nodes()` with `inventory::submit!`
- [x] 4.3 Update feature flags to work with provider system

## 5. Testing and Documentation

- [x] 5.1 Add unit tests for `NodeProvider` registration
- [x] 5.2 Add unit tests for dynamic Python node registry
- [ ] 5.3 Update `crates/core/README.md` with provider documentation
- [ ] 5.4 Add example of creating a custom node provider crate

## Architecture Notes

### Dynamic Python Node Registration

Instead of hardcoded factory definitions, Python nodes use a dynamic registry:

**Rust side (`crates/python-nodes/`):**
```rust
// Simple registration
register_python_node(
    PythonNodeConfig::new("KokoroTTSNode")
        .with_multi_output(true)
        .with_category("tts")
        .accepts(["text"])
        .produces(["audio"])
);
```

**Python side (`clients/python/remotemedia/nodes/`):**
```python
from remotemedia.nodes import streaming_node

@streaming_node(
    node_type="KokoroTTSNode",
    multi_output=True,
    category="tts",
    accepts=["text"],
    produces=["audio"]
)
class KokoroTTSNode(MultiprocessNode):
    ...
```

The `DynamicPythonNodeFactory` creates factories on-the-fly from the registry configuration, eliminating boilerplate.
