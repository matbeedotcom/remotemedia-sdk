## Context

RemoteMedia SDK has multiple sources of nodes:
1. **Core Rust nodes** - Audio processing, video flip, text collectors
2. **Python wrapper nodes** - WhisperX, Kokoro TTS, VibeVoice
3. **Candle ML nodes** - Native Whisper, YOLO, LLM (Phi, LLaMA)
4. **User-defined nodes** - External crates

Currently, all registration happens in `create_default_streaming_registry()` which manually calls `registry.register()` for each factory. This doesn't scale and couples everything tightly.

### Stakeholders
- SDK developers adding new node types
- External developers creating custom node crates
- Build system (feature flags for optional nodes)

## Goals / Non-Goals

### Goals
1. Enable auto-registration of nodes when a crate is added to Cargo.toml
2. Support feature-flag-gated nodes (e.g., `features = ["candle-whisper"]`)
3. Clean separation between node crates (core, python, candle)
4. Maintain backward compatibility with existing registry APIs
5. Type-safe registration at compile time (no runtime reflection)

### Non-Goals
1. Dynamic plugin loading at runtime (spec 033 covers this separately)
2. Cross-process node discovery
3. Network-distributed node registry

## Decisions

### Decision 1: Use `inventory` crate for auto-registration

The `inventory` crate provides compile-time collection of items across crates using `#[linkme]` under the hood. This is the idiomatic Rust approach.

**Pattern:**
```rust
// In remotemedia-core
inventory::collect!(Box<dyn NodeProvider>);

pub trait NodeProvider: Send + Sync {
    fn register(&self, registry: &mut StreamingNodeRegistry);
    fn provider_name(&self) -> &'static str;
}

// In each node crate
inventory::submit! {
    Box::new(AudioNodesProvider) as Box<dyn NodeProvider>
}
```

**Alternatives considered:**
- `linkme` crate directly - Lower level, `inventory` provides better ergonomics
- Runtime plugin loading - Already covered by spec 033, adds complexity
- Manual registration functions - Current approach, doesn't scale

### Decision 2: Crate structure with feature flags

```
crates/
├── core/                    # Core traits, registry infrastructure
├── nodes-audio/             # Audio processing nodes (optional feature)
├── nodes-python/            # Python wrapper nodes (optional feature)  
├── nodes-video/             # Video processing nodes (optional feature)
└── candle-nodes/            # ML inference nodes (optional feature)
```

Each node crate:
1. Depends on `remotemedia-core` for traits
2. Implements `NodeProvider`
3. Uses `inventory::submit!` to register its provider

**Rationale:** This allows users to pick only the nodes they need via Cargo features.

### Decision 3: Backward-compatible `create_default_streaming_registry()`

The existing function remains but now collects from all registered providers:

```rust
pub fn create_default_streaming_registry() -> StreamingNodeRegistry {
    let mut registry = StreamingNodeRegistry::new();
    
    // Collect from all registered providers
    for provider in inventory::iter::<Box<dyn NodeProvider>> {
        provider.register(&mut registry);
        tracing::debug!("Loaded node provider: {}", provider.provider_name());
    }
    
    registry
}
```

### Decision 4: Python nodes become a separate crate

The `PythonStreamingNode` wrapper and all Python node factories move to `crates/nodes-python/`:

```
crates/nodes-python/
├── Cargo.toml
└── src/
    ├── lib.rs           # NodeProvider impl, re-exports
    ├── provider.rs      # PythonNodesProvider
    └── factories/
        ├── mod.rs
        ├── whisper.rs   # WhisperXNodeFactory, HFWhisperNodeFactory
        ├── tts.rs       # KokoroTTSNodeFactory, VibeVoiceTTSNodeFactory
        └── ml.rs        # LFM2AudioNodeFactory, SimplePyTorchNodeFactory
```

**Benefits:**
- Core crate doesn't need Python knowledge
- Feature-flag the entire Python subsystem
- Cleaner dependency graph

## Risks / Trade-offs

### Risk: `inventory` adds a dependency
**Mitigation:** `inventory` is lightweight (15KB) and widely used in the Rust ecosystem.

### Risk: Compile-time registration may be confusing
**Mitigation:** Clear documentation and examples. The pattern is common in plugin systems.

### Trade-off: Breaking out Python nodes requires migration
**Mitigation:** Provide a `python-nodes` feature flag in `remotemedia-core` that re-exports everything for backward compatibility.

## Migration Plan

1. **Phase 1:** Add `NodeProvider` trait and `inventory` infrastructure to core
2. **Phase 2:** Create audio nodes provider inline (no crate split yet)
3. **Phase 3:** Create `crates/nodes-python/` and move Python factories
4. **Phase 4:** Update `candle-nodes` to use `NodeProvider`
5. **Phase 5:** Add feature flags and deprecation warnings

**Rollback:** Each phase is independently reversible. The old manual registration can remain as fallback.

## Open Questions

1. Should we split audio nodes into a separate crate too?
   - **Tentative:** No, keep in core for now since they're fundamental
   
2. Should video nodes be separate from audio?
   - **Tentative:** Already gated by `#[cfg(feature = "video")]`, can split later

3. Do we need a versioning scheme for providers?
   - **Tentative:** No, use semver on crate level instead
