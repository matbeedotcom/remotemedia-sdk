# Old NodeExecutor Trait - Archived

**Archived**: 2025-10-27  
**Original Location**: `runtime/src/nodes/mod.rs` (trait definition), `runtime/src/python/cpython_node.rs` (adapter)  
**Size**: ~2,000 LoC  
**Status**: Superseded by unified `executor::node_executor::NodeExecutor` trait  
**Reason**: v0.2.1 consolidated to single NodeExecutor trait architecture

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
