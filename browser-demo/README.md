# RemoteMedia Browser Demo - Hybrid Python Runtime

Execute RemoteMedia pipelines directly in your browser using a **hybrid Rust (WASM) + Python (Pyodide)** runtime.

## Features

- 🚀 **Rust Node Execution**: Execute Rust-compiled nodes via WebAssembly (20MB, <1ms per node)
- 🐍 **Python Node Execution**: Execute Python nodes via Pyodide (CPython 3.12 in WASM, ~30-40MB)
- 🔀 **Hybrid Architecture**: Automatically route nodes to appropriate runtime
- 📝 **Manifest Editor**: Create and edit pipeline manifests with live preview
- 🧪 **Example Pipelines**: Pre-built examples for calculator, text processing, and mixed pipelines
- 📊 **Performance Metrics**: Real-time execution metrics
- 🎨 **Modern UI**: Responsive design with dark theme

## What's New: Pyodide Integration ✨

We've implemented **Option B: Hybrid Pyodide Integration** from [BROWSER_PYTHON_SOLUTION.md](../docs/BROWSER_PYTHON_SOLUTION.md):

- ✅ **Python nodes now work in browser** via Pyodide (CPython 3.12 in WASM)
- ✅ **Rust nodes work in browser** via our custom WASM binary (wasm32-wasi)
- ✅ **Hybrid execution**: Mix Rust and Python nodes in same pipeline with automatic routing
- ✅ **Full CPython 3.12 stdlib** available (via Pyodide)
- ✅ **Battle-tested** solution using production-ready Pyodide (~15M downloads/month)
- ✅ **Progressive enhancement**: Load Pyodide only when Python nodes needed
- ✅ **Data flow**: Seamless data passing between Rust WASM ↔ JavaScript ↔ Pyodide

**Architecture**: Two separate WASM runtimes working together:
1. **Rust WASM** (20MB) - Executes Rust-native nodes via @bjorn3/browser_wasi_shim
2. **Pyodide WASM** (~30-40MB, CDN cached) - Executes Python nodes via JavaScript API

See [PYODIDE_IMPLEMENTATION.md](../docs/PYODIDE_IMPLEMENTATION.md) for complete implementation details.

## Getting Started

### Prerequisites

- Node.js 18+ and npm
- The WASM binary from the runtime (`pipeline_executor_wasm.wasm`)

### Building the WASM Binary

To build the WASM runtime binary:

```bash
cd runtime
cargo build --target wasm32-wasip1 \
    --bin pipeline_executor_wasm \
    --no-default-features \
    --features wasm \
    --release
```

Then copy to browser demo:

```bash
cp runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm browser-demo/public/
```

**Important**: Use `--no-default-features` to exclude wasmtime and other native-only dependencies that don't compile to WASM.

### Installation

```bash
cd browser-demo
npm install
```

### Development

```bash
npm run dev
```

This will start a local development server at `http://localhost:5173` (or another port if 5173 is busy).

### Building for Production

```bash
npm run build
```

The built files will be in the `dist/` directory.

## Usage

1. **Load WASM Runtime**
   - Click "Choose WASM file..." and select `pipeline_executor_wasm.wasm`
   - Click "Load Runtime" to load the WASM module

2. **Configure Pipeline**
   - Choose an example from the "Examples" tab, or
   - Write your own manifest in the "Custom Manifest" tab
   - Optionally provide input data as JSON array

3. **Execute**
   - Click "Run Pipeline" to execute
   - View results and performance metrics below

## Example Manifests

### Calculator (Rust Nodes)

```json
{
  "version": "v1",
  "metadata": {
    "name": "calculator-demo"
  },
  "nodes": [
    { "id": "multiply", "node_type": "MultiplyNode", "params": { "multiplier": 2 } },
    { "id": "add", "node_type": "AddNode", "params": { "addend": 10 } }
  ],
  "connections": [
    { "from": "multiply", "to": "add" }
  ]
}
```

Input: `[5, 7, 3]`
Output: `[20, 24, 16]` (5×2+10=20, 7×2+10=24, 3×2+10=16)

### Text Processor (Python Node)

```json
{
  "version": "v1",
  "metadata": {
    "name": "text-processor-demo"
  },
  "nodes": [
    { "id": "text1", "node_type": "TextProcessorNode", "params": {} }
  ],
  "connections": []
}
```

Input:
```json
[
  { "text": "Hello WASM", "operations": ["uppercase", "word_count"] }
]
```

## Architecture

```
┌─────────────────────────────────────────┐
│  Browser (TypeScript + Vite)            │
│  ├─ PipelineRunner (Wasmer SDK)         │
│  ├─ Manifest Editor                     │
│  └─ Results Display                     │
├─────────────────────────────────────────┤
│  WASM Runtime (pipeline_executor_wasm)  │
│  ├─ Rust Executor                       │
│  ├─ Embedded CPython 3.12               │
│  └─ Python Nodes                        │
└─────────────────────────────────────────┘
```

## WASI I/O Implementation

The browser demo uses `@bjorn3/browser_wasi_shim` for lightweight WASI support:

```typescript
import { WASI, File, OpenFile } from '@bjorn3/browser_wasi_shim';

// Create stdin file with manifest JSON
const stdinContent = new TextEncoder().encode(inputJson);
const stdinFile = new File(stdinContent);

// Create stdout file to capture output
const stdoutFile = new File(new Uint8Array());

// Create WASI instance with file descriptors
const wasi = new WASI([], [], [
  new OpenFile(stdinFile),  // fd 0: stdin
  new OpenFile(stdoutFile), // fd 1: stdout
  new OpenFile(new File(new Uint8Array())), // fd 2: stderr
]);

// Instantiate and run WASM with WASI
const instance = await WebAssembly.instantiate(wasmModule, {
  wasi_snapshot_preview1: wasi.wasiImport,
});
wasi.start(instance);

// Read output from stdout
const stdoutData = new TextDecoder().decode(stdoutFile.data);
const pipelineResult = JSON.parse(stdoutData);
```

**Why not Wasmer SDK?**
- `@bjorn3/browser_wasi_shim` is **lightweight** (~1KB vs 2MB+)
- Uses **browser-native** `WebAssembly.instantiate()` API
- **Simple** WASI polyfill - perfect for stdin/stdout
- **No heavy runtime** - our WASM binary is self-contained

This provides:
- ✅ **stdin**: Pass manifest JSON to WASM binary
- ✅ **stdout**: Receive execution results as JSON
- ✅ **stderr**: Capture error messages
- ✅ **Exit codes**: Detect execution failures (via WASI return)

## Architecture FAQ

### Why Two Separate WASM Runtimes?

**Q: Could we compile Pyodide into our WASM binary?**

**A**: Not practical. Here's why:

**Current Approach** (Optimal):
```
Browser loads:
├─ our_pipeline.wasm (20MB, wasm32-wasi) - Rust nodes
└─ pyodide from CDN (40MB, Emscripten) - Python nodes (cached globally)
Total first load: 60MB
Total subsequent: 20MB (Pyodide cached)
```

**Bundled Approach** (Not recommended):
```
mega_pipeline.wasm (60MB+)
├─ Our Rust code
├─ PyO3 + embedded CPython (stack overflow issue)
└─ Pyodide WASM (incompatible Emscripten vs wasm32-wasi)
Total: 60MB every time, no CDN benefit
```

**Problems with bundling**:
1. **Toolchain incompatibility**: Emscripten (Pyodide) ≠ wasm32-wasi (our WASM)
2. **No CDN caching**: Pyodide is already cached by millions of websites
3. **Larger downloads**: Users always get 60MB instead of progressive 20MB → 60MB
4. **Embedded CPython still fails**: Stack overflow issue remains unsolved

**Benefits of hybrid approach**:
- ✅ Progressive loading (start with 20MB Rust-only)
- ✅ CDN caching (Pyodide cached across all websites)
- ✅ Battle-tested (both runtimes independently proven)
- ✅ Clean separation (each does what it's best at)

### How Do The Two WASM Runtimes Communicate?

**A**: They don't! JavaScript acts as the orchestrator:

```
Input [5] → PipelineRunner (TypeScript)
              ↓
           MultiplyNode (Rust WASM via WASI)
              ↓
           JavaScript object (15)
              ↓
           TextProcessorNode (Pyodide WASM via JS API)
              ↓
           JavaScript object {text: "15", ...}
```

See [PYODIDE_IMPLEMENTATION.md](../docs/PYODIDE_IMPLEMENTATION.md) for complete data flow diagrams.

## Current Limitations

**Phase 2.5: Pyodide Hybrid Runtime** ✅ **COMPLETE**

**What Works**:
- ✅ **Rust nodes**: MultiplyNode, AddNode execute perfectly (<1ms)
- ✅ **Python nodes**: TextProcessorNode, DataTransformNode work in browser (5-20ms)
- ✅ **Mixed pipelines**: Rust → Python data flow seamless
- ✅ **Three runtimes**: RustPython (native), CPython PyO3 (native), Pyodide (browser)

**Known Constraints**:
- ⚠️ **Python-only pipelines**: Currently limited to single-node (multi-node graph traversal in Pyodide planned)
- ⚠️ **Initial load time**: Pyodide takes ~1.5s first load (then cached)
- ⚠️ **Bundle size**: Total 60MB (20MB Rust + 40MB Pyodide), but CDN cached after first visit
- ⚠️ **Performance**: Python nodes ~10x slower than native (but still real-time capable)

## Quick Start

### 1. Build the WASM Runtime

```bash
cd runtime
cargo build --target wasm32-wasip1 \
    --bin pipeline_executor_wasm \
    --no-default-features \
    --features wasm \
    --release
```

**Build time**: ~15 seconds (incremental)
**Output**: `runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm` (20MB)

### 2. Copy WASM Binary to Browser Demo

```bash
cp runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm browser-demo/public/
```

### 3. Start Dev Server

```bash
cd browser-demo
npm install  # First time only
npm run dev
```

**Dev server**: http://localhost:5173

### 4. Test in Browser

1. Open http://localhost:5173 in your browser
2. Click "Choose WASM file..." and select `pipeline_executor_wasm.wasm` from `browser-demo/public/`
3. Click "Load Runtime" (should load in ~10ms)
4. Select the "Calculator" example (Rust nodes)
5. Click "Run Pipeline"
6. **Expected output**: `[20, 24, 16]` for inputs `[5, 7, 3]`

### 5. Local Testing with Wasmtime (Optional)

Test the WASM binary locally before browser deployment:

```bash
cd runtime

# Test Rust-native nodes
echo '{"manifest":{"version":"v1","metadata":{"name":"calc"},"nodes":[{"id":"multiply","node_type":"MultiplyNode","params":{"multiplier":2}},{"id":"add","node_type":"AddNode","params":{"addend":10}}],"connections":[{"from":"multiply","to":"add"}]},"input_data":[5,7,3]}' | \
  wasmtime run --dir=target/wasm32-wasi/wasi-deps/usr::/usr \
  target/wasm32-wasip1/release/pipeline_executor_wasm.wasm
```

**Expected output**:
```json
{
  "status": "success",
  "outputs": [20, 24, 16],
  "graph_info": {
    "node_count": 2,
    "sink_count": 1,
    "source_count": 1,
    "execution_order": ["multiply", "add"]
  }
}
```

## Roadmap

- [x] Phase 2.1: Basic browser demo structure
- [x] Phase 2.2: PipelineRunner implementation
- [x] Phase 2.3: HTML/CSS interface
- [x] Phase 2.4: WASI stdin/stdout implementation ✅ **COMPLETE**
- [x] Phase 2.4.1: Lazy Python initialization (Rust nodes work without Python)
- [x] Phase 2.4.2: Browser execution with @bjorn3/browser_wasi_shim
- [ ] Phase 2.5: WASI filesystem mounting for Python stdlib (NEXT)
- [ ] Phase 2.6: .rmpkg packaging format
- [ ] Phase 2.7: Production optimization (wasm-opt, code splitting)
- [ ] Phase 2.8: Deploy to GitHub Pages/Vercel

## Troubleshooting

### CORS Errors

The demo requires COOP and COEP headers for SharedArrayBuffer support (needed by Wasmer). The Vite dev server is configured to add these headers automatically.

If you're deploying to a static host, add these headers:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

### Module Not Found

If you see "Cannot find module '@wasmer/sdk'":

```bash
npm install
```

### WASM Execution Fails

**Rust Nodes**: Should work perfectly. If you see errors:
- Check browser console for detailed error messages
- Verify the WASM binary is the correct one (from `runtime/target/wasm32-wasip1/release/`)
- Ensure COOP/COEP headers are set (check Network tab in DevTools)

**Python Nodes**: Will fail with "module not found" errors because the Python stdlib filesystem is not yet mounted. This is expected and will be fixed in Phase 2.5.

## Testing

### Test 1: Calculator (Rust Nodes) ✅ **WORKING**

1. Load `pipeline_executor_wasm.wasm` (20MB, loads in ~10ms)
2. Select the "Calculator" example
3. Input data is pre-filled: `[5, 7, 3]`
4. Click "Run Pipeline"
5. **Expected output**:
```json
{
  "status": "success",
  "outputs": [20, 24, 16],
  "graph_info": {
    "node_count": 2,
    "sink_count": 1,
    "source_count": 1,
    "execution_order": ["multiply", "add"]
  }
}
```

**Verification**:
- `5 × 2 + 10 = 20` ✓
- `7 × 2 + 10 = 24` ✓
- `3 × 2 + 10 = 16` ✓

This tests:
- ✅ WASM loading and compilation
- ✅ WASI stdin (manifest + input data)
- ✅ WASI stdout (JSON results)
- ✅ Manifest parsing
- ✅ Rust-native node execution (MultiplyNode → AddNode)
- ✅ Data marshaling (JSON → Rust → JSON)
- ✅ Graph execution order

**Performance**:
- WASM load time: ~10ms
- Pipeline execution: <50ms
- Total: ~60ms

### Test 2: Text Processor (Python Node) ⏸️ **Pending Phase 2.5**

Currently blocked on Python stdlib filesystem mounting. Will work after implementing WASI filesystem in browser.

### Test 3: Error Handling ✅ **WORKING**

Try these to test error handling:
1. **Invalid JSON**: Edit manifest to have syntax errors
2. **Unknown node type**: Change `"node_type": "FakeNode"`
3. **Missing connections**: Remove a connection
4. **Invalid input data**: Provide wrong data format

All errors should be caught and displayed with helpful messages.

## License

Same as the main RemoteMedia SDK project.
