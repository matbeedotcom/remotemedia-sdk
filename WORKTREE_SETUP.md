# Git Worktree Setup

## Active Worktrees

This repository has multiple git worktrees for parallel development:

### 1. Main Development (`remotemedia-sdk/`)
- **Location:** `C:/Users/mail/dev/personal/remotemedia-sdk`
- **Branch:** `006-model-sharing` (or current branch)
- **Purpose:** Main development work, feature branches

### 2. Python/Rust Reorganization (`remotemedia-sdk-python-rust-reorg/`)
- **Location:** `C:/Users/mail/dev/personal/remotemedia-sdk-python-rust-reorg`
- **Branch:** `python-rust-reorg`
- **Base:** `003-transport-decoupling`
- **Purpose:** Major codebase reorganization to cleanly separate Python and Rust code

### 3. WebRTC Multi-Peer Transport (`remotemedia-sdk-webrtc/`)
- **Location:** `C:/Users/mail/dev/personal/remotemedia-sdk-webrtc`
- **Branch:** `webrtc-multi-peer-transport`
- **Base:** `003-transport-decoupling`
- **Purpose:** Implement production WebRTC transport with multi-peer mesh, audio/video/data channels
- **Design:** See `transports/remotemedia-webrtc/DESIGN.md`

## Worktree Management

### List all worktrees
```bash
git worktree list
```

### Switch between worktrees
```bash
# Just navigate to the directory
cd ../remotemedia-sdk-python-rust-reorg

# Or use your IDE to open the other directory
```

### Remove a worktree (when done)
```bash
# First, navigate back to main worktree
cd ../remotemedia-sdk

# Remove the worktree
git worktree remove ../remotemedia-sdk-python-rust-reorg

# Optionally delete the branch
git branch -D python-rust-reorg
```

## Python/Rust Reorganization Plan

### Current State (After Archive)
```
remotemedia-sdk/
├── runtime-core/              # ✅ Pure Rust, zero transport deps
├── transports/                # ✅ Rust transport implementations
│   ├── remotemedia-grpc/
│   ├── remotemedia-ffi/
│   └── remotemedia-webrtc/
├── runtime/                   # ⚠️ DEPRECATED: Mixed Rust/Python, embedded protobuf
├── python-client/             # 🐍 Python SDK
├── archive/                   # 📦 Archived legacy code
└── examples/                  # Mixed examples

# Python spread across:
- python-client/               # Main Python SDK
- runtime/src/python/          # Python multiprocess support
- examples/*.py                # Python examples
```

### Proposed Reorganization

**Option A: Language-First Structure**
```
remotemedia-sdk/
├── rust/                      # All Rust code
│   ├── runtime-core/
│   ├── transports/
│   │   ├── grpc/
│   │   ├── ffi/
│   │   └── webrtc/
│   └── examples/
│
├── python/                    # All Python code
│   ├── sdk/                   # Main SDK (was python-client/)
│   ├── nodes/                 # Python node implementations
│   ├── examples/
│   └── tests/
│
├── docs/
├── specs/
└── archive/
```

**Option B: Component-First Structure** (Current, with cleanup)
```
remotemedia-sdk/
├── core/                      # Core runtime (pure Rust)
│   ├── runtime-core/
│   └── nodes/                 # Core Rust nodes
│
├── transports/                # Transport layer (Rust)
│   ├── grpc/
│   ├── ffi/
│   └── webrtc/
│
├── sdk/                       # SDK layer
│   ├── python/               # Python SDK (was python-client/)
│   └── rust/                 # Rust SDK (future)
│
├── python-runtime/           # Python-specific runtime
│   ├── multiprocess/         # From runtime/src/python/
│   └── nodes/                # Python node implementations
│
├── examples/
│   ├── rust/
│   └── python/
│
├── docs/
├── specs/
└── archive/
```

**Option C: Minimal Disruption** (Recommended for gradual migration)
```
remotemedia-sdk/
├── runtime-core/              # Keep as-is ✅
├── transports/                # Keep as-is ✅
├── python/                    # Consolidate all Python
│   ├── sdk/                   # Move from python-client/
│   ├── runtime/               # Move from runtime/src/python/
│   ├── nodes/                 # Python nodes
│   └── examples/              # Move *.py from examples/
│
├── examples/                  # Rust examples only
├── docs/
├── specs/
└── archive/
```

### Benefits of Reorganization

1. **Clear Separation:** Python vs Rust code clearly separated
2. **Easier Navigation:** Find Python or Rust code quickly
3. **Independent Builds:** Build Rust without Python dependencies (already achieved)
4. **Better Tooling:** Language-specific tooling doesn't conflict
5. **Cleaner Archive:** Legacy code clearly separated

### Migration Strategy

1. **Phase 1:** Create worktree (✅ Done)
2. **Phase 2:** Choose structure (A, B, or C)
3. **Phase 3:** Move directories in worktree
4. **Phase 4:** Update all import paths
5. **Phase 5:** Update build systems (Cargo.toml, setup.py)
6. **Phase 6:** Update CI/CD pipelines
7. **Phase 7:** Update documentation
8. **Phase 8:** Test thoroughly
9. **Phase 9:** Merge back to main

### Key Considerations

- **Breaking Changes:** This will be a breaking change for users
- **Version Bump:** Should be v0.5.0 or v1.0.0
- **Migration Guide:** Need comprehensive migration docs
- **Deprecation Period:** Consider 1-2 version overlap with old paths
- **CI/CD:** All workflows need updating
- **Import Paths:** Python imports will change significantly

### Next Steps

Work in the `remotemedia-sdk-python-rust-reorg/` worktree to:
1. Decide on structure (A, B, or C)
2. Create target directory structure
3. Move files systematically
4. Update all references
5. Verify builds and tests

### Resources

- [Git Worktree Docs](https://git-scm.com/docs/git-worktree)
- [Cargo Workspace Docs](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [Python Package Structure](https://packaging.python.org/en/latest/tutorials/packaging-projects/)

## Questions?

See:
- [LEGACY_ARCHIVE_SUMMARY.md](LEGACY_ARCHIVE_SUMMARY.md) - Current archive status
- [runtime/DEPRECATION_NOTICE.md](runtime/DEPRECATION_NOTICE.md) - Legacy runtime info
- [specs/003-transport-decoupling/](specs/003-transport-decoupling/) - Transport decoupling details
