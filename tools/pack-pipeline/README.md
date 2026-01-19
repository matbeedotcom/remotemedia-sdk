# Pack Pipeline

Create **self-contained Python wheels** from RemoteMedia pipeline manifests.

## Overview

The `pack-pipeline` tool compiles a pipeline YAML manifest into a standalone Python wheel that:

- **Pre-compiles Python nodes** to bytecode at pack time (not runtime)
- **Bundles the `remotemedia` runtime** so no external installation is needed
- **Includes native Rust acceleration** via PyO3 bindings
- **Packages all dependencies** from `requirements.txt`

The resulting wheel can be distributed and installed anywhere without requiring users to install RemoteMedia or configure the runtime.

## Usage

### Basic Packaging

```bash
# Pack a pipeline YAML into a Python package
cargo run -p remotemedia-pack -- python pipeline.yaml --output ./dist

# Build the wheel with maturin
cd ./dist/my_pipeline
maturin build --release

# Install the wheel
pip install target/wheels/my_pipeline-0.1.0-*.whl
```

### CLI Options

```bash
cargo run -p remotemedia-pack -- python <PIPELINE_YAML> [OPTIONS]

Arguments:
  <PIPELINE_YAML>    Path to the pipeline YAML manifest

Options:
  -o, --output <DIR>     Output directory (default: ./dist)
  -n, --name <NAME>      Override package name (default: from manifest)
  --version <VERSION>    Package version (default: 0.1.0)
  -v, --verbose          Enable verbose logging
  --build                Build the wheel after generating
  --release              Build in release mode (with --build)
  --test                 Run import tests after building
```

### Example

```bash
# Pack the TTS pipeline with full build and test
cargo run -p remotemedia-pack -- python examples/shared-pipelines/tts.yaml \
    --output /tmp/packed \
    --build \
    --release \
    --test
```

## Using Packed Pipelines

### Python API

```python
import tts  # Your packed pipeline
import asyncio

async def main():
    # Create a session
    session = tts.TtsSession()
    
    # Get pipeline info
    print(f"Version: {tts.get_version()}")
    print(f"Pipeline: {tts.get_pipeline_yaml()}")
    
    # Send input and receive output
    result = await session.send({"type": "text", "data": "Hello, world!"})
    print(f"Result: {result}")
    
    # Cleanup
    session.close()

asyncio.run(main())
```

### Available Functions

| Function | Description |
|----------|-------------|
| `get_version()` | Returns the package version |
| `get_pipeline_yaml()` | Returns the embedded pipeline YAML |
| `{Name}Session()` | Creates a new pipeline session |
| `process(data)` | One-shot pipeline execution |

### Session Methods

| Method | Description |
|--------|-------------|
| `send(data)` | Send input data (async) |
| `recv()` | Receive output data (async) |
| `has_output()` | Check if output is pending |
| `info()` | Get session info dict |
| `close()` | Close and cleanup session |

## How It Works

### 1. Pipeline Analysis

The tool analyzes your pipeline YAML against the node registry to determine:
- Which nodes are Rust (built-in)
- Which nodes are Python (need embedding)
- Any missing or unregistered nodes

### 2. Python Bytecode Compilation

For Python nodes:
1. Creates a temporary virtualenv with `remotemedia` installed
2. Compiles each `.py` node file to bytecode using `marshal.dumps()`
3. Embeds bytecode via Rust's `include_bytes!()`

### 3. Runtime Bundling

The tool copies the entire `remotemedia` Python package into the wheel, making it fully self-contained.

### 4. Code Generation

Generates:
- `Cargo.toml` with proper dependencies
- `pyproject.toml` with Python dependencies
- `lib.rs` with PyO3 bindings and bytecode loading
- `__init__.py` with Python re-exports

### 5. Wheel Building

Uses [maturin](https://github.com/PyO3/maturin) to build a mixed Rust/Python wheel.

## Generated Package Structure

```
my_pipeline/
├── Cargo.toml           # Rust dependencies
├── pyproject.toml       # Python metadata & deps
├── README.md            # Auto-generated docs
├── src/
│   ├── lib.rs           # PyO3 bindings
│   ├── pipeline.yaml    # Embedded manifest
│   └── nodes/
│       └── *.pyc        # Compiled Python bytecode
└── python/
    ├── my_pipeline/
    │   └── __init__.py  # Python re-exports
    └── remotemedia/     # Bundled runtime
        └── ...
```

## Requirements

- **Rust 1.87+** - For building the native extension
- **Python 3.10+** - For bytecode compilation and runtime
- **maturin** - For wheel building (`pip install maturin`)

## Limitations

- Python bytecode is version-specific (compiled for Python 3.10+)
- Native extension is platform-specific (separate wheel per OS/arch)
- All Python dependencies must be pip-installable

## Troubleshooting

### "ModuleNotFoundError: No module named 'X'"

A dependency is missing. Check that all imports in your Python nodes are listed in `clients/python/requirements.txt`.

### "Failed to compile Python node"

Syntax error in the Python node. The error message will show the file and line number.

### "Maturin build failed"

Ensure maturin is installed (`pip install maturin`) and Rust is properly configured.
