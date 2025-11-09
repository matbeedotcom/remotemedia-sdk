# .rmpkg Package Format Specification

## Overview

The `.rmpkg` format is a standardized package format for distributing RemoteMedia pipelines for browser execution. It bundles the manifest, WASM runtime binary, and optional dependencies into a single ZIP archive.

## File Structure

```
package.rmpkg (ZIP archive)
├── manifest.json          # Pipeline manifest with runtime metadata
├── runtime.wasm          # WASM runtime binary (pipeline_executor_wasm.wasm)
├── metadata.json         # Package metadata (optional)
└── deps/                 # Optional dependencies directory
    └── ...               # Additional resources
```

## manifest.json

The manifest must include a `runtime` section specifying the target platform:

```json
{
  "version": "v1",
  "metadata": {
    "name": "my-pipeline",
    "description": "Example pipeline",
    "author": "Your Name",
    "version": "1.0.0"
  },
  "runtime": {
    "target": "wasm32-wasi",
    "min_version": "0.1.0"
  },
  "nodes": [
    {
      "id": "multiply",
      "node_type": "MultiplyNode",
      "params": { "multiplier": 2 }
    }
  ],
  "connections": []
}
```

### Runtime Configuration

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `runtime.target` | string | Yes | Target platform: `"wasm32-wasi"` for browser execution |
| `runtime.min_version` | string | No | Minimum runtime version required |
| `runtime.features` | string[] | No | Required features: `["python"]`, `["rust-only"]` |

### Runtime Target Values

- `"wasm32-wasi"` - Browser WASM execution (current implementation)
- `"native"` - Native execution (not applicable for .rmpkg)
- Future: `"wasm32-unknown-unknown"`, `"wasm32-emscripten"`

## metadata.json (Optional)

Additional package metadata for discovery and documentation:

```json
{
  "package_name": "my-pipeline",
  "package_version": "1.0.0",
  "author": "Your Name",
  "license": "MIT",
  "repository": "https://github.com/user/repo",
  "tags": ["audio", "transcription", "ml"],
  "created_at": "2025-01-15T12:00:00Z",
  "runtime_info": {
    "wasm_size_bytes": 20971520,
    "has_python_nodes": false,
    "has_rust_nodes": true,
    "node_types": ["MultiplyNode", "AddNode"]
  },
  "performance_hints": {
    "expected_load_time_ms": 50,
    "expected_execution_time_ms": 10,
    "memory_requirement_mb": 50
  }
}
```

## deps/ Directory (Optional)

For future extensions, packages can include:
- Custom Python modules
- Model files (e.g., Whisper models)
- Static assets (audio samples, images)
- Configuration files

**Note**: Currently not implemented. The hybrid Pyodide runtime handles Python dependencies via Pyodide's package system.

## File Size Guidelines

| Component | Typical Size | Notes |
|-----------|--------------|-------|
| manifest.json | <10 KB | Keep lean |
| runtime.wasm | 15-20 MB | Release build with wasm-opt |
| metadata.json | <5 KB | Optional |
| Total .rmpkg | 15-20 MB | Without additional deps |

## Creating a .rmpkg Package

### Using the packaging script:

```bash
cd browser-demo
npm run package -- \
  --manifest ../runtime/tests/simple_calc_wasm.json \
  --wasm ../runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm \
  --output my-pipeline.rmpkg
```

### Manual creation (ZIP):

```bash
# Create directory structure
mkdir -p rmpkg-build
cp manifest.json rmpkg-build/
cp pipeline_executor_wasm.wasm rmpkg-build/runtime.wasm

# Optional: Add metadata
cat > rmpkg-build/metadata.json <<EOF
{
  "package_name": "my-pipeline",
  "package_version": "1.0.0"
}
EOF

# Create ZIP
cd rmpkg-build
zip -r ../my-pipeline.rmpkg .
```

## Loading .rmpkg in Browser

The browser demo automatically extracts and validates .rmpkg files:

```typescript
// User uploads .rmpkg file
const file = document.getElementById('pkg-upload').files[0];

// Extract and load
const pkg = await RemoteMediaPackage.fromFile(file);
await pkg.load();

// Execute pipeline
const result = await pkg.execute(inputData);
```

## Validation

When loading a .rmpkg, the browser validates:

1. **Archive integrity**: Valid ZIP structure
2. **Required files**: `manifest.json` and `runtime.wasm` present
3. **Manifest schema**: Conforms to RemoteMedia manifest v1 spec
4. **Runtime compatibility**: `runtime.target` matches browser capabilities
5. **Size limits**: Total uncompressed size < 100 MB (configurable)

## Security Considerations

.rmpkg files execute arbitrary WebAssembly code in the browser. Best practices:

- ✅ **Sandboxed execution**: WASM runs in browser sandbox
- ✅ **No network access**: WASI has no network capabilities
- ✅ **File system isolation**: WASI filesystem is virtual (in-memory)
- ⚠️ **Size limits**: Prevent DoS via large packages
- ⚠️ **Validation**: Verify manifest before execution
- ⚠️ **User consent**: Show package info before loading

**Trust model**: Users should only load .rmpkg files from trusted sources, similar to installing browser extensions.

## Versioning

.rmpkg format version: `1.0.0`

Future versions may add:
- Compression algorithms (gzip, brotli)
- Digital signatures for verification
- Dependency resolution
- Multi-runtime support (select WASM vs native)

## Examples

### Minimal Package (Rust-only nodes)

```
calculator.rmpkg
├── manifest.json         # 2 KB
└── runtime.wasm          # 20 MB
Total: 20 MB
```

### Full Package (with metadata)

```
audio-pipeline.rmpkg
├── manifest.json         # 5 KB
├── runtime.wasm          # 20 MB
├── metadata.json         # 3 KB
└── deps/
    └── whisper-tiny.bin  # 75 MB
Total: 95 MB
```

## Compatibility

| Browser | Support | Notes |
|---------|---------|-------|
| Chrome 90+ | ✅ Full | Recommended |
| Firefox 88+ | ✅ Full | Tested |
| Safari 15+ | ⚠️ Partial | WASI support limited |
| Edge 90+ | ✅ Full | Chromium-based |

## Migration from Manual Loading

**Before** (.rmpkg):
1. User manually uploads `pipeline_executor_wasm.wasm`
2. User pastes manifest JSON
3. Click "Load Runtime"
4. Click "Run Pipeline"

**After** (.rmpkg):
1. User uploads `pipeline.rmpkg`
2. Click "Load Package" (auto-extracts manifest + WASM)
3. Click "Run Pipeline"

Improved UX: Single file upload vs. multiple steps.

## Future Extensions

### Planned for v2.0:

- **Compressed runtime**: Store `runtime.wasm.gz` to reduce package size by 60-70%
- **Multi-file manifests**: Split large manifests across multiple JSON files
- **Asset bundling**: Include audio/video samples for demos
- **Python wheels**: Bundle Python dependencies as .whl files
- **Model quantization metadata**: Specify model precision (fp16, int8)

### Potential for v3.0:

- **Streaming execution**: Load and execute pipeline chunks progressively
- **Differential updates**: Only download changed components
- **Content addressing**: Use hash-based filenames for better caching

## Reference Implementation

See:
- `browser-demo/scripts/create-package.js` - Packaging tool
- `browser-demo/src/package-loader.ts` - Browser extraction
- `browser-demo/README.md` - Usage examples

## License

This specification is part of the RemoteMedia SDK and follows the same license.
