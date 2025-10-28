# Old NodeExecutor Trait & Registry - Archived

**Archived**: 2025-10-28  
**Original Location**: `runtime/src/nodes/mod.rs` (trait definition + simple registry)  
**Size**: ~600 LoC  
**Status**: Preserved for built-in nodes, augmented with multi-tier registry system  
**Reason**: v0.2.1 introduced CompositeRegistry for multi-tier node management

## What's Archived

This directory contains the original `nodes::NodeExecutor` trait and simple NodeRegistry that was used in v0.2.0-v0.2.1:

### 1. Original NodeExecutor Trait (`nodes_mod_trait.rs`)
**Original**: `runtime/src/nodes/mod.rs` (lines ~40-95)

The trait definition that basic nodes implement:

```rust
#[async_trait]
pub trait NodeExecutor: Send + Sync {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()>;
    async fn process(&mut self, input: Value) -> Result<Vec<Value>>;
    async fn cleanup(&mut self) -> Result<()>;
    fn is_streaming(&self) -> bool { false }
    async fn finish_streaming(&mut self) -> Result<Vec<Value>> { Ok(vec![]) }
    fn info(&self) -> NodeInfo { ... }
}
```

### 2. Simple NodeRegistry (`old_node_registry.rs`)
**Original**: `runtime/src/nodes/mod.rs` (lines ~110-147)

Simple HashMap-based registry for built-in nodes:

```rust
pub struct NodeRegistry {
    factories: HashMap<String, NodeFactory>,
}
```

This registry is **still in use** for built-in nodes but has been augmented with the CompositeRegistry system.

## Architecture Evolution

### v0.2.0: Single Simple Registry
- One `nodes::NodeRegistry` with HashMap of factories
- All nodes registered in `Default::default()`
- No runtime selection (Rust vs Python)
- No priority/layering

### v0.2.1: Multi-Tier Registry System
**Problem**: Need to support different tiers of nodes:
- System nodes (built-in: PassThrough, Echo, Calculator)
- Audio nodes (resample, VAD, format conversion)
- User nodes (custom implementations, overrides)

**Solution**: Introduced `CompositeRegistry` that chains multiple `nodes::registry::NodeRegistry` instances:

```rust
pub struct CompositeRegistry {
    registries: Vec<Arc<NodeRegistry>>,  // Searched in order
    names: Vec<String>,                   // For debugging
}
```

**Two Registry Systems Coexist**:
1. **Old `nodes::NodeRegistry`** (simple, HashMap-based)
   - Used for: Built-in nodes (Pass Through, Echo, Calculator, Math nodes)
   - Trait: `nodes::NodeExecutor`
   - Location: `runtime/src/nodes/mod.rs`
   - Scope: Local to executor

2. **New `nodes::registry::NodeRegistry`** (Phase 5, runtime-aware)
   - Used for: Rust/Python factory-based nodes
   - Trait: `executor::node_executor::NodeExecutor`
   - Features: RuntimeHint (Auto/Rust/Python selection)
   - Location: `runtime/src/nodes/registry.rs`
   - Scope: Can be layered in CompositeRegistry

3. **CompositeRegistry** (v0.2.1, multi-tier)
   - Chains multiple `nodes::registry::NodeRegistry` instances
   - Priority order: User → Audio → System
   - Enables override semantics
   - Location: `runtime/src/nodes/registry.rs`

### Current Executor Structure (v0.2.1)

```rust
pub struct Executor {
    config: ExecutorConfig,
    registry: CompositeRegistry,           // NEW: Multi-tier
    builtin_nodes: NodeRegistry,            // OLD: Simple registry
    runtime_selector: RuntimeSelector,
    py_cache: PyObjectCache,
    metrics: Arc<RwLock<PipelineMetrics>>,
}
```

### Node Resolution Order

When creating a node:
1. Check `builtin_nodes` (old simple registry) first
2. If not found, create Python node via `runtime_selector`
3. (Future) Check `registry` (CompositeRegistry) for Rust/Python factories

**Note**: CompositeRegistry not yet used for node creation because it returns `executor::node_executor::NodeExecutor` while built-in nodes use `nodes::NodeExecutor`. Full migration pending.

## Why Two Registries Coexist

### Trait Incompatibility
- `nodes::NodeExecutor` (simple trait, synchronous factories)
- `executor::node_executor::NodeExecutor` (async trait, parameterized factories)

These traits are incompatible. Full consolidation requires:
1. Migrating all built-in nodes to `executor::node_executor::NodeExecutor`
2. Updating audio nodes (resample, VAD, format)
3. Updating test nodes (PassThrough, Echo, Calculator, etc.)
4. Extensive testing

### Gradual Migration Path
v0.2.1 preserves both systems:
- **Short term**: Use old registry for built-in nodes (works today)
- **Medium term**: Populate CompositeRegistry with audio/user registries
- **Long term** (v0.3.0): Migrate all nodes, remove old registry

## Multi-Tier Registry API

### Adding Registries

```rust
let mut executor = Executor::new();

// Add system-level nodes (lowest priority)
let system_reg = Arc::new(nodes::registry::NodeRegistry::new());
// ... register system nodes ...
executor.add_system_registry(system_reg);

// Add audio processing nodes (medium priority)
let audio_reg = Arc::new(nodes::registry::NodeRegistry::new());
// ... register audio nodes ...
executor.add_audio_registry(audio_reg);

// Add user custom nodes (highest priority)
let user_reg = Arc::new(nodes::registry::NodeRegistry::new());
// ... register user nodes ...
executor.add_user_registry(user_reg);
```

### Override Semantics

If the same node type is registered in multiple tiers, the **first match wins**:
- User registry checked first → can override system nodes
- Audio registry checked second → isolated from user nodes
- System registry checked last → fallback default

### Listing Nodes

```rust
// All built-in nodes (old registry)
let builtin = executor.builtin_nodes().node_types();

// All nodes from all tiers
let all_nodes = executor.list_all_node_types();
```

## When to Use Each Registry

| Use Case | Registry Type | Example |
|----------|---------------|---------|
| Built-in test nodes | `builtin_nodes` (old) | PassThrough, Echo, Calculator |
| Audio DSP nodes | `audio_registry` (new) | AudioResample, VAD, FormatConverter |
| ML inference nodes | `system_registry` (new) | Whisper, Stable Diffusion |
| Custom user nodes | `user_registry` (new) | MyCustomNode, OverrideEcho |

## Migration Guide (Future v0.3.0)

To fully consolidate to single trait system:

### Step 1: Migrate Built-in Nodes
```rust
// Before (nodes::NodeExecutor)
impl nodes::NodeExecutor for PassThroughNode {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()> { ... }
}

// After (executor::node_executor::NodeExecutor)
impl executor::node_executor::NodeExecutor for PassThroughNode {
    async fn initialize(&mut self, ctx: &NodeContext) -> Result<()> { ... }
}
```

### Step 2: Create Factories
```rust
struct PassThroughFactory;

impl nodes::registry::NodeFactory for PassThroughFactory {
    fn create(&self, _params: Value) -> Result<Box<dyn executor::node_executor::NodeExecutor>> {
        Ok(Box::new(PassThroughNode))
    }
    
    fn node_type(&self) -> &str { "PassThrough" }
}
```

### Step 3: Register in System Registry
```rust
let mut system_reg = nodes::registry::NodeRegistry::new();
system_reg.register_rust(Arc::new(PassThroughFactory));
```

### Step 4: Remove Old Registry
```rust
pub struct Executor {
    registry: CompositeRegistry,  // Only this remains
    // builtin_nodes: NodeRegistry,  // Removed
}
```

## When to Restore

**Generally not needed** - the multi-tier system is superior and preserves the old registry for built-in nodes.

Reference this documentation when:
1. Understanding registry architecture history
2. Planning full trait consolidation (v0.3.0)
3. Implementing new node tier (e.g., network, storage)
4. Debugging node resolution order

## Current Status (v0.2.1)

**In Use**:
- ✅ Old `nodes::NodeRegistry` for built-in nodes
- ✅ `CompositeRegistry` infrastructure exists
- ✅ Multi-tier API (`add_system_registry`, `add_audio_registry`, `add_user_registry`)

**Not Yet Used**:
- ❌ CompositeRegistry not checked during node creation (trait incompatibility)
- ❌ No audio/user registries populated
- ❌ Built-in nodes not migrated to new trait

**Roadmap**:
1. **v0.2.1** (current): Dual registry system, old registry active ✅
2. **v0.2.2**: Populate audio registry, use CompositeRegistry for audio nodes
3. **v0.3.0**: Migrate built-in nodes, remove old registry

## Questions?

For questions about the registry architecture:
1. Review archived files in this directory
2. Check `runtime/src/nodes/registry.rs` for CompositeRegistry implementation
3. Check `runtime/src/executor/mod.rs` for registry usage
4. Consult `docs/ARCHIVAL_GUIDE.md` for general guidance

---

**Note**: This documents the evolution from simple single-tier registry to multi-tier CompositeRegistry system. Both coexist in v0.2.1 to support gradual migration and maintain backwards compatibility.

## What's Archived

This directory contains the original `nodes::NodeExecutor` trait and its adapter layer that was used in v0.2.0:

### 1. Original NodeExecutor Trait (`nodes_mod.rs`)
**Original**: `runtime/src/nodes/mod.rs` (lines ~40-95)

The trait definition that all nodes implemented before consolidation:

```rust
#[async_trait]
pub trait NodeExecutor: Send + Sync {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()>;
    async fn process(&mut self, input: Value) -> Result<Vec<Value>>;
    async fn cleanup(&mut self) -> Result<()>;
    fn is_streaming(&self) -> bool { false }
    async fn finish_streaming(&mut self) -> Result<Vec<Value>> { Ok(vec![]) }
    fn info(&self) -> NodeInfo { ... }
}
```

### 2. CPython Node Adapter (`cpython_node.rs`)
**Original**: `runtime/src/python/cpython_node.rs`

Adapter that bridged `CPythonNodeExecutor` (implementing `nodes::NodeExecutor`) to work with `NodeRegistry` (expecting `executor::node_executor::NodeExecutor`):

```rust
pub struct CPythonNodeAdapter {
    executor: CPythonNodeExecutor,
}

impl ExecutorNodeExecutor for CPythonNodeAdapter {
    // Adapts between the two trait definitions
}
```

### 3. Failing Tests (if any)
Any Rust tests that failed after consolidation due to trait changes.

## Why Archived

### Duplicate Trait Definitions
v0.2.0 had **two nearly identical** NodeExecutor traits:
- `nodes::NodeExecutor` (in `src/nodes/mod.rs`)
- `executor::node_executor::NodeExecutor` (in `src/executor/node_executor.rs`)

This caused:
- **Confusion**: Which trait should nodes implement?
- **Adapter complexity**: `cpython_node.rs` bridged between them
- **Error propagation**: Changes to Error enum affected 62 files (now ~15)
- **Maintenance burden**: Keep two traits in sync

### Consolidation Benefits
v0.2.1 uses only `executor::node_executor::NodeExecutor`:
- **Single source of truth**: One trait definition
- **No adapters needed**: Direct implementation
- **Reduced file impact**: Error enum changes affect ~15 files (was 62)
- **Clearer architecture**: Executor module owns the trait
- **Simpler node implementation**: No trait disambiguation needed

### Implementation Changes
**Before** (v0.2.0):
```rust
// CPythonNodeExecutor implements nodes::NodeExecutor
impl nodes::NodeExecutor for CPythonNodeExecutor { ... }

// NodeRegistry expects executor::node_executor::NodeExecutor
// So we need an adapter
impl ExecutorNodeExecutor for CPythonNodeAdapter { ... }
```

**After** (v0.2.1):
```rust
// CPythonNodeExecutor directly implements executor::node_executor::NodeExecutor
impl executor::node_executor::NodeExecutor for CPythonNodeExecutor { ... }

// No adapter needed!
```

## When to Restore

Consider referencing (not restoring) if:

1. **Understanding history**: Why did we have two traits?
2. **Adapter pattern**: How was trait bridging implemented?
3. **Migration reference**: Porting nodes to new trait
4. **Debugging**: Investigating trait-related issues

**Note**: This is reference documentation only. The consolidated trait in `executor::node_executor` is the current standard.

## Technical Details

### Trait Differences
The two traits were almost identical, with minor naming differences:

| Aspect | `nodes::NodeExecutor` | `executor::node_executor::NodeExecutor` |
|--------|----------------------|----------------------------------------|
| Context type | `NodeContext` | `NodeContext` (different module) |
| Initialize | `initialize(&mut self, context: &NodeContext)` | `initialize(&mut self, ctx: &NodeContext)` |
| Process | `process(&mut self, input: Value)` | `process(&mut self, input: Value)` |
| Cleanup | `cleanup(&mut self)` | `cleanup(&mut self)` |
| Streaming | `is_streaming()`, `finish_streaming()` | Same |
| Info | `info() -> NodeInfo` | Not present |

### Files Affected by Consolidation

**Removed**:
- `runtime/src/python/cpython_node.rs` (adapter, 200 LoC)

**Updated** (to use `executor::node_executor::NodeExecutor` directly):
- `runtime/src/python/cpython_executor.rs`
- `runtime/src/nodes/audio/resample.rs`
- `runtime/src/nodes/audio/vad.rs`
- `runtime/src/nodes/audio/format_converter.rs`
- `runtime/src/nodes/registry.rs`

**Archived** (trait definition moved here):
- Lines ~40-95 from `runtime/src/nodes/mod.rs`

### Error Enum Impact Reduction

**Before Consolidation** (v0.2.0):
- Both traits use `crate::Error` and `crate::Result`
- Error enum changes propagate through both trait hierarchies
- Affected files: 62

**After Consolidation** (v0.2.1):
- Single trait uses `crate::Error` and `crate::Result`
- Error enum changes propagate through one hierarchy
- Affected files: ~15 (76% reduction)

## Restoration Instructions

**Generally not needed** - the consolidated trait is superior.

If you need to reference the old implementation:

```bash
# View archived trait definition
cat archive/old-node-executor/nodes_mod.rs

# View adapter implementation
cat archive/old-node-executor/cpython_node.rs

# View historical implementation in git
git log --follow --all -- runtime/src/python/cpython_node.rs
git show <commit>:runtime/src/python/cpython_node.rs
```

If you truly need to restore (extremely rare):

```bash
# Copy archived files back
cp archive/old-node-executor/cpython_node.rs runtime/src/python/

# Restore trait definition to nodes/mod.rs
# (Manual edit required - can't just copy whole file)

# Update imports in affected files
# Change from:
use crate::executor::node_executor::NodeExecutor;
# Back to:
use crate::nodes::NodeExecutor;

# Recompile
cargo build --release
```

**Warning**: Restoring this will reintroduce:
- Adapter complexity
- 62-file error propagation
- Trait disambiguation issues
- Maintenance burden

## Migration Guide (v0.2.0 → v0.2.1)

For nodes implementing the old trait:

### Step 1: Update Imports
```rust
// Before
use crate::nodes::{NodeExecutor, NodeContext};

// After
use crate::executor::node_executor::{NodeExecutor, NodeContext};
```

### Step 2: Update Implementation (if needed)
```rust
// Before
impl nodes::NodeExecutor for MyNode {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()> {
        // ...
    }
}

// After
impl executor::node_executor::NodeExecutor for MyNode {
    async fn initialize(&mut self, ctx: &NodeContext) -> Result<()> {
        // Note: 'context' → 'ctx' parameter name
        // ...
    }
}
```

### Step 3: Remove Adapters
If your node used `CPythonNodeAdapter`, you can now implement `executor::node_executor::NodeExecutor` directly.

### Step 4: Update Registry
```rust
// Before (with adapter)
Box::new(CPythonNodeAdapter::new("MyNode"))

// After (direct implementation)
Box::new(CPythonNodeExecutor::new("MyNode"))
```

## Impact on v0.2.1

**Before Consolidation** (v0.2.0):
- NodeExecutor traits: 2 (nodes::, executor::)
- Adapter files: 1 (cpython_node.rs, 200 LoC)
- Error enum impact: 62 files
- Trait disambiguation: Required in many places

**After Consolidation** (v0.2.1):
- NodeExecutor traits: 1 (executor::)
- Adapter files: 0
- Error enum impact: ~15 files (-76%)
- Trait disambiguation: Not needed

## Questions?

For questions about the old NodeExecutor trait:
1. Review archived files in this directory
2. Check git history for trait evolution
3. Consult `docs/ARCHIVAL_GUIDE.md` for general guidance
4. Open GitHub issue with `archival` label

---

**Note**: This trait was archived in v0.2.1 to simplify the codebase architecture. The consolidated `executor::node_executor::NodeExecutor` trait is the current standard and should be used for all new nodes.
