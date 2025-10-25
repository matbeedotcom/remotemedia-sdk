# Package Creation Scripts

## create-package.js

Creates `.rmpkg` package files for RemoteMedia browser execution.

### Prerequisites

```bash
npm install  # Install dependencies (archiver)
```

### Usage

#### Basic Usage

```bash
npm run package -- \
  --manifest examples/calculator.rmpkg.json \
  --output calculator.rmpkg
```

This will:
1. Use the default WASM binary from `../runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm`
2. Read the manifest from `examples/calculator.rmpkg.json`
3. Auto-generate metadata from the manifest
4. Create `calculator.rmpkg` package

#### Custom WASM Binary

```bash
npm run package -- \
  --manifest examples/calculator.rmpkg.json \
  --wasm path/to/custom.wasm \
  --output calculator.rmpkg
```

#### With Custom Metadata

```bash
npm run package -- \
  --manifest examples/calculator.rmpkg.json \
  --metadata examples/calculator-metadata.json \
  --output calculator.rmpkg
```

### Command Line Options

| Option | Short | Description | Required |
|--------|-------|-------------|----------|
| `--manifest` | `-m` | Path to manifest JSON file | ✓ |
| `--output` | `-o` | Output .rmpkg file path | ✓ |
| `--wasm` | `-w` | Path to WASM runtime binary | (default path) |
| `--metadata` | | Optional metadata.json file | |
| `--help` | `-h` | Show help message | |

### Building WASM Binary

Before creating packages, you need to build the WASM runtime:

```bash
cd ../runtime
cargo build --target wasm32-wasip1 \
  --bin pipeline_executor_wasm \
  --no-default-features \
  --features wasm \
  --release
```

This creates: `runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm` (~20 MB)

### Example Workflow

```bash
# 1. Build WASM runtime (one-time)
cd ../runtime
cargo build --target wasm32-wasip1 --bin pipeline_executor_wasm --release --no-default-features --features wasm

# 2. Create packages
cd ../browser-demo

# Calculator package
npm run package -- \
  --manifest examples/calculator.rmpkg.json \
  --output calculator.rmpkg

# Text processor package
npm run package -- \
  --manifest examples/text-processor.rmpkg.json \
  --output text-processor.rmpkg
```

### Package Contents

The created `.rmpkg` file is a ZIP archive containing:

```
my-pipeline.rmpkg (ZIP)
├── manifest.json         # Pipeline manifest with runtime metadata
├── runtime.wasm          # WASM runtime binary
└── metadata.json         # Package metadata (auto-generated or custom)
```

### Manifest Requirements

The manifest must include a `runtime` section:

```json
{
  "version": "v1",
  "metadata": {
    "name": "my-pipeline",
    "description": "Pipeline description",
    "author": "Your Name",
    "version": "1.0.0"
  },
  "runtime": {
    "target": "wasm32-wasi",
    "min_version": "0.1.0",
    "features": ["rust-only"]
  },
  "nodes": [...],
  "connections": [...]
}
```

### Auto-Generated Metadata

If you don't provide a metadata file, the script auto-generates one with:

- Package name, version, author (from manifest)
- Created timestamp
- Runtime info (WASM size, node types, counts)
- Performance hints (estimated load/execution times)

### Output Example

```
Creating .rmpkg package...
  Manifest: C:\...\examples\calculator.rmpkg.json
  WASM:     C:\...\runtime\target\wasm32-wasip1\release\pipeline_executor_wasm.wasm
  Output:   C:\...\calculator.rmpkg

Package contents:
  manifest.json:  486 B
  runtime.wasm:   19.85 MB
  metadata.json:  312 B (auto-generated)

✅ Package created successfully!
  Output: C:\...\calculator.rmpkg
  Size:   19.85 MB
  Nodes:  2 (MultiplyNode, AddNode)
```

### Optimizing Package Size

For production packages, optimize the WASM binary:

```bash
# Install wasm-opt (from binaryen toolkit)
# https://github.com/WebAssembly/binaryen/releases

wasm-opt -O3 \
  -o runtime/target/wasm32-wasip1/release/pipeline_executor_wasm_opt.wasm \
  runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm

# Then create package with optimized binary
npm run package -- \
  --manifest examples/calculator.rmpkg.json \
  --wasm runtime/target/wasm32-wasip1/release/pipeline_executor_wasm_opt.wasm \
  --output calculator-optimized.rmpkg
```

This can reduce size by 20-40% (20 MB → 12-15 MB).

### Troubleshooting

**Error: WASM file not found**
- Build the WASM runtime first (see "Building WASM Binary" above)
- Or specify a custom path with `--wasm`

**Error: Invalid JSON in manifest**
- Validate your JSON syntax
- Ensure all required fields are present

**Warning: Manifest missing runtime.target**
- Add `"runtime": {"target": "wasm32-wasi"}` to your manifest
- The script will add it automatically with a warning

**Package is too large**
- Use release build (`--release`)
- Run wasm-opt optimizer
- Consider using `--features wasm` without default features
