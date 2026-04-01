# Managed Python Environments via Bundled uv

**Date:** 2026-03-31
**Status:** Draft

## Problem

Users of the RemoteMedia SDK must manually configure Python environments, install dependencies, and ensure the correct Python version is available before running multiprocess pipelines. This creates friction for new users and makes reproducibility difficult across machines.

## Solution

Integrate `uv` (Astral's Rust-based Python package manager) into the SDK to provide zero-config Python environment management. The SDK bundles the `uv` binary via Cargo feature flags and manages virtual environments automatically based on node dependency declarations.

## Release Tiers

Three tiers controlled by Cargo features in `crates/core/Cargo.toml`:

| Tier | Feature Flags | Behavior |
|------|--------------|----------|
| **Bare** | `multiprocess` (existing) | Uses system Python only. User manages their own environment. Current behavior preserved. |
| **Complete** | `multiprocess` + `bundled-uv` | Bundles `uv` binary. Auto-creates and caches virtual environments for pipeline nodes. |
| **Embedded** | `multiprocess` + `bundled-uv` + `embedded-python` | Same as Complete, but also auto-downloads a standalone Python via `uv python install` if no system Python is found. |

### Feature Flag Details

**`bundled-uv`:**
- At runtime, on first use: detects `uv` on PATH, or downloads the platform-appropriate `uv` binary from GitHub releases to `~/.config/remotemedia/bin/uv`
- `build.rs` verifies the download URL and embeds the expected SHA256 checksum for the pinned uv version (exact pin, e.g. `0.6.14`)
- Cross-platform: downloads correct binary for linux-x64, linux-arm64, darwin-x64, darwin-arm64, windows-x64
- For air-gapped/offline environments: set `UV_BINARY_PATH` env var to point to a pre-provided uv binary, skipping the download
- Optional sub-feature `bundled-uv-embedded` uses `include_bytes!()` to embed the uv binary directly in the SDK binary (~30-50MB increase) for true offline use

**`embedded-python`:**
- Implies `bundled-uv`
- Enables the `ManagedWithPython` env mode
- On first run, calls `uv python install <version>` to download a standalone Python build (from python-build-standalone)
- Python is cached at `~/.config/remotemedia/python/`
- Does NOT bundle Python in the SDK binary itself — downloads on first use (~30MB)

### Fallback Behavior

When `bundled-uv` is NOT enabled:
- `PythonEnvMode::System` is the only available mode
- Behaves identically to current SDK (full backward compatibility)
- `python_executable` config and `PYTHON_EXECUTABLE` env var work as today

When `bundled-uv` IS enabled but `uv` extraction fails:
- Falls back to `python -m venv` + `pip install` (system backend)
- Logs a warning about degraded performance

## Architecture

### PythonEnvManager

New module: `crates/core/src/python/env_manager.rs`

```
┌─────────────────────────────────────────────────────┐
│  PythonEnvManager                                    │
│                                                      │
│  ┌─────────────┐   ┌──────────────┐                │
│  │ UvBackend   │   │ SystemBackend│                │
│  │ (bundled-uv)│   │ (fallback)   │                │
│  └──────┬──────┘   └──────┬───────┘                │
│         │                  │                         │
│         ▼                  ▼                         │
│  ┌─────────────────────────────────┐                │
│  │  EnvBackend trait               │                │
│  │  - ensure_python() -> PathBuf   │                │
│  │  - create_venv() -> VenvInfo    │                │
│  │  - install_deps() -> Result     │                │
│  │  - resolve_python() -> PathBuf  │                │
│  └─────────────────────────────────┘                │
│                                                      │
│  ┌─────────────────────────────────┐                │
│  │  VenvCache                      │                │
│  │  ~/.config/remotemedia/envs/    │                │
│  │  - hash(deps) → cached venv    │                │
│  │  - LRU cleanup                  │                │
│  └─────────────────────────────────┘                │
└─────────────────────────────────────────────────────┘
```

### EnvBackend Trait

```rust
#[async_trait]
pub trait EnvBackend: Send + Sync {
    /// Ensure a Python interpreter is available, return its path
    async fn ensure_python(&self, version: &str) -> Result<PathBuf>;

    /// Create a virtual environment with the given dependencies
    async fn create_venv(&self, python: &Path, deps: &[String], cache_key: &str) -> Result<VenvInfo>;

    /// Install dependencies into an existing venv
    async fn install_deps(&self, venv: &VenvInfo, deps: &[String]) -> Result<()>;

    /// Get the python executable path for a venv
    fn resolve_python(&self, venv: &VenvInfo) -> PathBuf;
}
```

**UvBackend** — Shells out to the `uv` CLI (detected on PATH or downloaded):
- `ensure_python()` → `uv python install 3.11` (only in `embedded-python` mode)
- `create_venv()` → `uv venv --python <version> <path>`
- `install_deps()` → `uv pip sync <requirements.txt>` (sync ensures exact match, not additive)
- `resolve_python()` → `<venv>/bin/python` (Unix) or `<venv>/Scripts/python.exe` (Windows)

**SystemBackend** — Uses stdlib tools:
- `ensure_python()` → validates `python3` exists on PATH
- `create_venv()` → `python -m venv <path>`
- `install_deps()` → `pip install <deps...>`
- `resolve_python()` → `<venv>/bin/python` (Unix) or `<venv>/Scripts/python.exe` (Windows)

### Venv Caching

Environments are cached at `~/.config/remotemedia/envs/{cache_key}/`.

**Cache key computation:**
```
cache_key = sha256(python_version + "\0" + sorted(all_deps).join("\0"))[..16]
```

This means:
- Same deps + same Python version = reuse existing venv (instant startup)
- Any dep change = new venv (correctness over speed)
- Old venvs cleaned up via LRU when `max_cached_envs` exceeded (default: 10)

**Cache metadata:** Each venv dir contains a `remotemedia-env.json`:
```json
{
  "python_version": "3.11.9",
  "deps": ["torch>=2.0", "openai-whisper"],
  "created_at": "2026-03-31T12:00:00Z",
  "last_used_at": "2026-03-31T14:30:00Z",
  "cache_key": "a1b2c3d4e5f6g7h8"
}
```

## Dependency Declaration

### Merged Model

Dependencies come from two sources, merged at resolution time:

**1. Node-level (Python decorator):**
```python
@register_node("WhisperNode")
@python_requires(["torch>=2.0", "openai-whisper"])
class WhisperNode(MultiprocessNode):
    ...
```

The `@python_requires` decorator stores requirements in the node's registry metadata. When a Python process is spawned, it reports its requirements via the control channel before the READY signal.

**2. Manifest-level (pipeline YAML):**
```yaml
python_env:
  python_version: "3.11"
  scope: per-pipeline            # global | per-pipeline | per-node
  extra_deps: ["custom-lib==1.0"]

nodes:
  - id: whisper
    node_type: WhisperNode
    python_deps:
      - torch==2.1.0             # overrides node's torch>=2.0
```

### Resolution Order

1. Collect base deps from `@python_requires` on the node class
2. Apply `python_deps` from manifest (overrides matching packages by name)
3. Append `extra_deps` from manifest `python_env` section
4. Deduplicate by **normalized package name** (PEP 503: lowercased, hyphens/underscores equivalent). Extras like `torch[cuda]` match base package `torch`. Manifest version wins over node default.
5. Sort and compute cache key

## Configuration

### MultiprocessConfig Extensions

```rust
pub struct MultiprocessConfig {
    // Existing fields (unchanged)
    pub max_processes_per_session: Option<usize>,
    pub channel_capacity: usize,
    pub init_timeout_secs: u64,
    pub python_executable: PathBuf,        // Override: if set, skips env management
    pub enable_backpressure: bool,
    pub docker_fallback_policy: DockerFallbackPolicy,
    pub python_path: Vec<PathBuf>,

    // NEW fields
    pub python_env_mode: PythonEnvMode,
    pub python_version: Option<String>,     // e.g. "3.11", default: "3.11"
    pub env_scope: EnvScope,
    pub env_cache_dir: Option<PathBuf>,     // Override default cache location
    pub max_cached_envs: usize,             // LRU limit, default: 10
}

pub enum PythonEnvMode {
    /// Use python_executable as-is. Current behavior. Default.
    System,
    /// Use uv to create/manage venvs. Requires bundled-uv feature.
    Managed,
    /// Use uv to manage both Python and venvs. Requires embedded-python feature.
    ManagedWithPython,
}

pub enum EnvScope {
    /// One venv shared across all pipelines and nodes
    Global,
    /// One venv per pipeline manifest (deps merged from all nodes)
    PerPipeline,
    /// Each node type gets its own venv (maximum isolation)
    PerNode,
}
```

### runtime.toml

```toml
python_env_mode = "managed"        # "system" | "managed" | "managed_with_python"
python_version = "3.11"
env_scope = "per-pipeline"         # "global" | "per-pipeline" | "per-node"
env_cache_dir = "~/.config/remotemedia/envs"
max_cached_envs = 10
```

### Backward Compatibility

- Default `PythonEnvMode::System` preserves all current behavior
- Explicit `python_executable` in config always takes precedence over managed envs
- `PYTHON_EXECUTABLE` env var still works as the highest-priority override
- No existing tests or workflows change unless the user opts into managed mode

## Integration Points

### ProcessManager Changes

The `PythonEnvManager` is created **once** at `MultiprocessExecutor` construction (not per `spawn_node()` call) and shared across all node spawns. This prevents races when multiple nodes in the same pipeline resolve to the same venv.

```rust
pub struct MultiprocessExecutor {
    // ... existing fields ...
    env_manager: Option<Arc<PythonEnvManager>>,  // None when PythonEnvMode::System
}

impl MultiprocessExecutor {
    pub fn new(config: MultiprocessConfig) -> Self {
        let env_manager = if config.python_env_mode != PythonEnvMode::System {
            Some(Arc::new(PythonEnvManager::new(&config).expect("env manager init")))
        } else {
            None
        };
        // ...
    }
}
```

In `process_manager.rs`, `spawn_node()` uses the shared manager:

```rust
async fn spawn_node(..., env_manager: Option<&PythonEnvManager>) -> Result<ProcessHandle> {
    let python_exe = if let Some(mgr) = env_manager {
        let deps = self.collect_node_deps(node_type, manifest_deps)?;
        let venv = mgr.ensure_env(deps).await?;
        mgr.resolve_python(&venv)
    } else {
        self.spawn_config.python_executable.clone()
    };

    let mut command = Command::new(&python_exe);
    // ... rest unchanged ...
}
```

The `PythonEnvManager` uses an internal `tokio::sync::Mutex` around venv creation to serialize concurrent requests for the same cache key.

### Control Channel Protocol Extension

The Python runner reports its deps as a **separate iceoryx2 sample** before the READY sample:

```
Current: Python → Rust: sample(b"READY")
New:     Python → Rust: sample(b"DEPS:" + json(["torch>=2.0", "whisper"]))
         Python → Rust: sample(b"READY")
```

The Rust side poll loop recognizes `DEPS:` as a known prefix (not a warning), stores the parsed deps, then continues waiting for `READY`. Older Python runners that don't send `DEPS:` still work — the Rust side treats a missing DEPS message as "deps unknown" and skips validation. This preserves backward compatibility.

### build.rs for uv Checksums

```rust
// crates/core/build.rs (when bundled-uv feature is enabled)
fn main() {
    #[cfg(feature = "bundled-uv")]
    emit_uv_metadata();

    #[cfg(feature = "bundled-uv-embedded")]
    download_and_embed_uv();
}

fn emit_uv_metadata() {
    // Pin exact version and checksums for runtime download verification
    let uv_version = "0.6.14";
    println!("cargo:rustc-env=UV_VERSION={uv_version}");

    // Per-platform SHA256 checksums (verified against GitHub release)
    let checksum = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "abc123...",
        ("linux", "aarch64") => "def456...",
        ("macos", "x86_64") => "ghi789...",
        ("macos", "aarch64") => "jkl012...",
        ("windows", "x86_64") => "mno345...",
        _ => panic!("Unsupported platform for bundled-uv"),
    };
    println!("cargo:rustc-env=UV_CHECKSUM={checksum}");

    // UV_BINARY_PATH env var allows skipping download for air-gapped envs
    println!("cargo:rerun-if-env-changed=UV_BINARY_PATH");
}
```

The `bundled-uv-embedded` sub-feature additionally downloads and `include_bytes!()` embeds the binary at build time for true offline use.

## Execution Flow

### First Run (Complete tier, new machine)

```
1. User runs: remotemedia-sdk run pipeline.yaml
2. SDK reads manifest → finds WhisperNode needs torch, openai-whisper
3. PythonEnvManager checks PythonEnvMode::Managed
4. Extracts bundled uv to ~/.config/remotemedia/bin/uv (if not already there)
5. Computes cache_key = sha256("3.11\0openai-whisper\0torch>=2.0")
6. Cache miss → creates venv: uv venv --python 3.11 ~/.config/remotemedia/envs/{key}/
7. Installs deps: uv pip install torch>=2.0 openai-whisper
8. Spawns Python process with ~/.config/remotemedia/envs/{key}/bin/python
9. Node sends DEPS + READY, pipeline runs normally
```

### Subsequent Runs (cache hit)

```
1. SDK reads manifest → computes same cache_key
2. Cache hit → venv exists at ~/.config/remotemedia/envs/{key}/
3. Updates last_used_at in metadata
4. Spawns Python directly with cached venv python — no uv calls needed
```

### First Run (Embedded tier, no Python on system)

```
1-4. Same as Complete
5. ensure_python("3.11") → no system Python found
6. Runs: uv python install 3.11
7. Python downloaded to ~/.config/remotemedia/python/cpython-3.11.9-...
8-9. Same as Complete, using the managed Python
```

## Error Handling

### Venv Creation Failures

If venv creation or dependency installation fails (network error, resolution conflict):

1. **Partial venvs are cleaned up** — the cache directory entry is deleted on failure
2. **Error propagates as `Error::Execution`** with a clear message: which dep failed, what uv reported
3. **No automatic retry** — the user fixes deps in manifest/node and re-runs
4. **Fallback to SystemBackend** only when uv binary itself is unavailable, not for dep resolution failures (those indicate a real problem)

### Concurrent Access

Multiple SDK processes sharing the same `env_cache_dir` (common in CI):
- Venv creation uses filesystem advisory locks (`flock` on Unix, `LockFileEx` on Windows) on a `.lock` file in the cache directory
- LRU eviction skips venvs that are currently locked
- If a lock cannot be acquired within 60s, the operation fails with a clear error

### Docker + Managed Envs

Docker execution mode (`ExecutionMode::Docker`) and managed Python envs are **mutually exclusive**. Docker containers manage their own Python environment via Dockerfile. If both are configured, the Docker mode takes precedence and the env manager is skipped. This is documented as expected behavior.

## Manifest Schema Changes

The `Manifest` struct in `crates/core/src/manifest.rs` gains new optional fields:

```rust
pub struct Manifest {
    // ... existing fields ...
    pub python_env: Option<PythonEnvConfig>,
}

pub struct PythonEnvConfig {
    pub python_version: Option<String>,
    pub scope: Option<EnvScope>,
    pub extra_deps: Vec<String>,
}

pub struct NodeManifest {
    // ... existing fields ...
    pub python_deps: Option<Vec<String>>,
}
```

These fields are always parsed (not gated behind feature flags) but ignored when `bundled-uv` is not enabled. This allows manifest files to be portable across SDK tiers.

## pack-pipeline Integration: `--bundle-python`

The existing `tools/pack-pipeline` tool creates self-contained Python wheels from pipeline manifests. Today it bundles Python bytecode and the `remotemedia` runtime, but expects the user to have Python and deps available at install time. A new `--bundle-python` flag extends this to create **fully portable** packages with pre-provisioned Python environments.

### New CLI Flags

```bash
cargo run -p remotemedia-pack -- python pipeline.yaml \
    --bundle-python                    # Pre-install Python + deps into package
    --python-version 3.11              # Target Python version (default: 3.11)
    --target linux-x86_64              # Cross-platform target (default: current)
```

### What `--bundle-python` Does

1. **Resolves all Python deps** — collects `@python_requires` from nodes + manifest `python_deps` + `extra_deps`
2. **Downloads standalone Python** — uses `uv python install 3.11 --install-dir <pack_dir>/python/`
3. **Creates a pre-populated venv** — `uv venv --python <bundled_python> <pack_dir>/venv/`
4. **Pre-installs all deps** — `uv pip sync` into the bundled venv
5. **Embeds env config** — writes a `runtime-env.json` that tells the SDK to use the bundled Python/venv instead of system or managed mode

### Generated Package Structure (with `--bundle-python`)

```
my_pipeline/
├── Cargo.toml
├── pyproject.toml
├── src/
│   ├── lib.rs
│   ├── pipeline.yaml
│   └── nodes/*.pyc
├── python/
│   ├── my_pipeline/
│   └── remotemedia/
├── bundled_env/                 # NEW: pre-provisioned environment
│   ├── python/                  # Standalone Python installation
│   │   └── cpython-3.11.9-.../ 
│   ├── venv/                    # Pre-populated virtualenv
│   │   ├── bin/python           # Symlink to bundled Python
│   │   └── lib/python3.11/site-packages/
│   └── runtime-env.json         # Points SDK to bundled env
└── README.md
```

### runtime-env.json

```json
{
  "python_env_mode": "bundled",
  "python_executable": "bundled_env/venv/bin/python",
  "python_version": "3.11.9",
  "deps": ["torch>=2.0", "openai-whisper"],
  "bundled_at": "2026-03-31T12:00:00Z"
}
```

When the SDK loads a packed pipeline and finds `runtime-env.json`, it uses the bundled Python directly — no environment resolution, no downloads, no uv needed at runtime. This is the fastest possible startup path.

### Cross-Platform Bundling

The `--target` flag allows packing for a different platform than the build machine:

```bash
# On macOS, pack for Linux deployment
remotemedia-pack python pipeline.yaml --bundle-python --target linux-x86_64
```

This downloads the target platform's Python build and creates a venv with platform-appropriate wheels. Requires `uv` which handles cross-platform resolution natively.

### Relationship to Feature Tiers

| Scenario | What Happens |
|----------|-------------|
| Pack without `--bundle-python` | Current behavior — user needs Python at install time |
| Pack with `--bundle-python` | Bundled env, no Python/uv needed at runtime |
| Run packed pipeline (bundled) | SDK reads `runtime-env.json`, uses bundled Python directly |
| Run packed pipeline (not bundled) | SDK falls back to normal env resolution (System/Managed/ManagedWithPython) |

### Implementation Notes

- `pack-pipeline` gains a dependency on the `env_manager` module from `remotemedia-core`
- Reuses `UvBackend.ensure_python()` and `UvBackend.create_venv()` for the provisioning
- The bundled env adds ~100-500MB to the package depending on deps (torch alone is ~2GB)
- Consider adding `--bundle-python-slim` that uses `uv pip install --no-cache` and strips `.pyc`/test files

## File Structure

New files:
- `crates/core/src/python/env_manager.rs` — PythonEnvManager, EnvBackend trait, VenvCache
- `crates/core/src/python/env_manager/uv_backend.rs` — UvBackend implementation
- `crates/core/src/python/env_manager/system_backend.rs` — SystemBackend (venv+pip fallback)
- `crates/core/build.rs` — uv binary download logic (behind `bundled-uv` feature)

Modified files:
- `crates/core/Cargo.toml` — new feature flags: `bundled-uv`, `bundled-uv-embedded`, `embedded-python`
- `crates/core/src/python/multiprocess/mod.rs` — re-export PythonEnvMode, EnvScope
- `crates/core/src/python/multiprocess/multiprocess_executor.rs` — MultiprocessConfig new fields, env_manager field
- `crates/core/src/python/multiprocess/process_manager.rs` — env resolution before spawn
- `crates/core/src/manifest.rs` — PythonEnvConfig, python_deps on NodeManifest
- `clients/python/remotemedia/core/multiprocessing/node.py` — `@python_requires` decorator
- `clients/python/remotemedia/core/multiprocessing/runner.py` — DEPS reporting via control channel

## Testing

- Unit tests for VenvCache (hash computation, LRU eviction)
- Unit tests for dependency resolution (merge, override, dedup)
- Integration test: spawn node with `PythonEnvMode::Managed`, verify venv created and reused
- Integration test: `PythonEnvMode::System` works identically to current behavior
- Feature flag test: `bundled-uv` disabled → `Managed` mode returns error
- Cross-platform CI: verify uv binary download works on linux, macos, windows
