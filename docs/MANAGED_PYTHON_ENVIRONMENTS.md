# Managed Python Environments

The SDK can automatically create and cache Python virtual environments with the right dependencies for each pipeline node. This eliminates the need for users to manually install Python packages before running pipelines.

## Quick Start

```bash
# Run a pipeline with automatic dependency management
PYTHON_ENV_MODE=managed cargo run -p remotemedia-manifest-test -- pipeline.yaml

# Or set it in runtime.toml
echo 'python_env_mode = "managed"' >> runtime.toml
```

On first run, the SDK will:
1. Create a virtual environment at `~/.config/remotemedia/envs/<hash>/`
2. Install the Python dependencies declared by each node
3. Cache the environment for instant reuse on subsequent runs

## How It Works

### Environment Modes

| Mode | Env Var / Config Value | Behavior |
|------|----------------------|----------|
| **System** (default) | `system` | Uses whatever `python` is on PATH. No venv management. Current behavior. |
| **Managed** | `managed` | Creates venvs with `--system-site-packages`, installs node-specific deps via pip. Falls back to system pip when uv isn't available. |
| **ManagedWithPython** | `managed_with_python` | Same as Managed, but also downloads a standalone Python via `uv python install` if none is found on the system. Requires `bundled-uv` feature. |

### Configuration

**Environment variables** (highest priority):
```bash
PYTHON_ENV_MODE=managed          # system | managed | managed_with_python
PYTHON_VERSION=3.11              # Target Python version
PYTHON_EXECUTABLE=/usr/bin/python3  # Override: skip env management entirely
```

**runtime.toml** (loaded from current directory):
```toml
[python_env]
mode = "managed"
python_version = "3.11"
scope = "global"                 # global | per_pipeline | per_node
max_cached_envs = 8
cache_dir = "~/.config/remotemedia/envs"
```

### Declaring Dependencies

Dependencies can be declared at two levels, which are merged at resolution time.

**1. Python node class** (via `@python_requires` decorator):

```python
from remotemedia.core.multiprocessing import register_node, python_requires

@register_node("WhisperNode")
@python_requires(["torch>=2.0", "openai-whisper"])
class WhisperNode(MultiprocessNode):
    ...
```

**2. Pipeline manifest** (per-node or pipeline-wide):

```yaml
# Pipeline-level extra deps (added to all nodes)
python_env:
  python_version: "3.11"
  extra_deps: ["custom-lib==1.0"]

nodes:
  - id: kokoro
    node_type: KokoroTTSNode
    is_streaming: true
    params:
      voice: af_bella
    # Node-specific deps (override/extend decorator deps)
    python_deps:
      - "kokoro>=0.9.4"
      - soundfile
```

**Merge order**: Node `@python_requires` < manifest `python_deps` (overrides by package name) < manifest `extra_deps` (appended). Package names are normalized per PEP 503 (`My_Package` and `my-package` are treated as the same package).

### Venv Caching

Environments are cached by a SHA-256 hash of `(python_version, sorted_deps)`:

```
~/.config/remotemedia/envs/
  9104b31befb91350/          # hash of "3.11" + ["kokoro>=0.9.4", "soundfile"]
    bin/python               # venv python (symlink to system)
    lib/python3.12/site-packages/
      kokoro/                # installed deps
      soundfile/
    remotemedia-env.json     # metadata (deps, timestamps)
```

- **Same deps + same Python version** = instant cache hit (~25us)
- **Any dep change** = new venv created
- **LRU eviction** when cache exceeds `max_cached_envs` (default: 8)

### DEPS Control Channel

When a Python node starts, it reports its `@python_requires` dependencies to the Rust runtime via the iceoryx2 control channel (as a `DEPS:` message before the `READY` signal). This allows the runtime to validate that the venv has the correct packages.

```
Python -> Rust: sample(b"DEPS:["kokoro>=0.9.4", "soundfile"]")
Python -> Rust: sample(b"READY")
```

Older Python runners that don't send `DEPS:` still work - backward compatible.

## Cargo Feature Flags

| Feature | What it enables |
|---------|----------------|
| (default) | `PythonEnvMode::System` only. No env management. |
| `bundled-uv` | Enables uv-based backend. Downloads uv binary on first use for fast venv creation. |
| `bundled-uv-embedded` | Embeds uv binary in the SDK via `include_bytes!()` for offline use. |
| `embedded-python` | Enables `ManagedWithPython` mode. Auto-downloads Python via `uv python install`. |

```bash
# Build with uv support
cargo build --features bundled-uv

# Build with full embedded Python support
cargo build --features embedded-python
```

Without `bundled-uv`, the `Managed` mode falls back to system `python -m venv` + `pip install` (slower but works everywhere).

## Architecture

```
PythonEnvManager (created once per MultiprocessExecutor)
    |
    +-- EnvBackend trait
    |     +-- UvBackend (fast, feature-gated behind bundled-uv)
    |     +-- SystemBackend (fallback: venv + pip)
    |
    +-- VenvCache
          +-- cache_key = sha256(version + deps)
          +-- LRU eviction
          +-- tokio::sync::Mutex (prevents races)
```

**Integration flow:**

1. `PythonStreamingNode::ensure_initialized()` loads `MultiprocessConfig::from_default_file()`
2. `MultiprocessExecutor::new(config)` creates `PythonEnvManager` if mode != System
3. Before `spawn_node()`, the executor calls `env_mgr.ensure_env(deps)` 
4. On cache miss: creates venv, installs deps, writes metadata
5. On cache hit: returns cached venv path (~25us)
6. `spawn_config.python_executable` is updated to the venv's python
7. Node spawns in the managed environment

## Key Files

| File | Purpose |
|------|---------|
| `crates/core/src/python/env_manager/mod.rs` | PythonEnvManager, EnvBackend trait, VenvCache, dep merge/normalize |
| `crates/core/src/python/env_manager/uv_backend.rs` | UvBackend (uv CLI integration) |
| `crates/core/src/python/env_manager/system_backend.rs` | SystemBackend (venv + pip fallback) |
| `crates/core/src/python/multiprocess/multiprocess_executor.rs` | MultiprocessConfig.python_env, env resolution before spawn |
| `crates/core/src/python/multiprocess/process_manager.rs` | spawn_config() accessor for updating python executable |
| `crates/core/src/transport/session_router.rs` | Injects python_deps from manifest into node params |
| `crates/core/src/manifest.rs` | ManifestPythonEnv, NodeManifest.python_deps |
| `clients/python/remotemedia/core/multiprocessing/__init__.py` | @python_requires decorator, get_node_requirements() |
| `clients/python/remotemedia/core/multiprocessing/runner.py` | DEPS control channel reporting |

## pack-pipeline Integration

The `remotemedia-pack` tool supports `--bundle-python` to create fully portable packages:

```bash
# Pack pipeline with pre-installed Python environment
cargo run -p remotemedia-pack -- python pipeline.yaml \
    --bundle-python \
    --python-version 3.11 \
    --output ./dist
```

This creates a `bundled_env/` directory in the package containing:
- A standalone Python installation (via `uv python install`)
- A pre-populated venv with all dependencies
- A `runtime-env.json` that the SDK reads at startup to skip env resolution

The resulting package needs no system Python at runtime.

## Performance

| Scenario | Env Resolution Time |
|----------|-------------------|
| System mode (no env management) | 0 |
| Managed, cache hit | ~25us |
| Managed, cache miss (pip install) | 10s - 5min (depends on deps) |
| Managed + bundled-uv, cache miss | 2s - 2min (uv is 10-100x faster than pip) |

## Troubleshooting

**"ModuleNotFoundError: No module named 'remotemedia.core'"**

The managed venv can't find the remotemedia package. The venv is created with `--system-site-packages` so it should inherit system-installed packages. Ensure remotemedia is installed: `pip install -e clients/python/`

**"Failed to create venv"**

Python's `venv` module might not be installed. On Ubuntu/Debian: `sudo apt install python3-venv`

**"pip install failed"**

Check network connectivity. Some ML packages (torch, etc.) are large and need a stable connection. Consider using `bundled-uv` feature for faster, more reliable installs.

**Clearing the cache:**

```bash
rm -rf ~/.config/remotemedia/envs/
```

**Inspecting a cached environment:**

```bash
cat ~/.config/remotemedia/envs/<hash>/remotemedia-env.json
# Shows: deps, python_version, created_at, last_used_at
```
