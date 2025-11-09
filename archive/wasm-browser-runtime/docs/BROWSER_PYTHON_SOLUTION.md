# Browser Python Execution: Hybrid Architecture Solution

## Problem Summary

After extensive investigation, we've identified a fundamental incompatibility:

**CPython's stdlib causes stack overflow in browser when compiled to WASM via PyO3/wlr-libpy**

### Root Cause
- Browser JavaScript VMs have hard call stack limits (~10,000 frames in Chrome)
- CPython's stdlib modules (`typing`, `logging`, `json`, etc.) have deep recursion during import
- Standard CPython compiled to WASM hits these limits immediately
- Even Pyodide's patched stdlib doesn't work with our PyO3 WASM build (requires Pyodide's custom CPython build)

### What We Tried
1. ✅ Removed typing imports from asyncio → Still failed on logging
2. ✅ Minified stdlib with python-minifier (70% size reduction) → Still failed
3. ✅ Disabled logging in remotemedia → Still failed on json
4. ✅ Used Pyodide's pre-patched stdlib → Still failed (requires Pyodide's CPython build)
5. ❌ Every complex stdlib module causes stack overflow in our PyO3 WASM build

## Recommended Solution: Hybrid Architecture

Use **different Python runtimes for different targets**:

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ RemoteMedia Runtime                                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Native (Linux/Mac/Windows via wasmtime):                  │
│  ┌───────────────────────────────────────────────┐         │
│  │ Rust Executor + PyO3/wlr-libpy                │         │
│  │ - Full CPython 3.12 support                   │         │
│  │ - Rust nodes work ✅                           │         │
│  │ - Python nodes work ✅                         │         │
│  │ - Streaming nodes work ✅                      │         │
│  └───────────────────────────────────────────────┘         │
│                                                             │
│  Browser (WASM):                                           │
│  ┌───────────────────────────────────────────────┐         │
│  │ OPTION A: Rust-only (Current - WORKING!)     │         │
│  │ - Rust Executor only                          │         │
│  │ - Rust nodes work ✅                           │         │
│  │ - Python nodes: NOT SUPPORTED ⚠️              │         │
│  │ - 20MB WASM binary                            │         │
│  └───────────────────────────────────────────────┘         │
│                                                             │
│  ┌───────────────────────────────────────────────┐         │
│  │ OPTION B: Pyodide Integration (RECOMMENDED)  │         │
│  │ - Rust Executor for Rust nodes                │         │
│  │ - Pyodide API for Python nodes                │         │
│  │ - Rust nodes work ✅                           │         │
│  │ - Python nodes work ✅                         │         │
│  │ - Streaming nodes work ✅                      │         │
│  │ - Initial load: ~30-40MB (cached after first) │         │
│  └───────────────────────────────────────────────┘         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Implementation Plan: Option B (Pyodide Integration)

### Phase 1: Install Pyodide
```bash
cd browser-demo
npm install pyodide
```

### Phase 2: Create Python Executor Adapter
Create `browser-demo/src/python-executor.ts`:

```typescript
import { loadPyodide, PyodideInterface } from 'pyodide';

export class PyodidePythonExecutor {
  private pyodide: PyodideInterface | null = null;

  async initialize(): Promise<void> {
    console.log('Loading Pyodide...');
    this.pyodide = await loadPyodide({
      indexURL: 'https://cdn.jsdelivr.net/pyodide/v0.29.0/full/'
    });

    // Load remotemedia package into Pyodide
    await this.loadRemoteMediaPackage();
  }

  async loadRemoteMediaPackage(): Promise<void> {
    // Option A: Load from python-stdlib.json (our custom package)
    // Option B: Install from PyPI if published
    // Option C: Load inline Python code

    const remoteMediaCode = `
# Minimal remotemedia nodes for browser
class Node:
    def __init__(self, name, config):
        self.name = name
        self.config = config

    def process(self, data):
        raise NotImplementedError()

class TextProcessorNode(Node):
    def process(self, data):
        text = data.get('text', '')
        operations = data.get('operations', ['uppercase'])
        results = {}

        for op in operations:
            if op == 'uppercase':
                results['uppercase'] = text.upper()
            elif op == 'lowercase':
                results['lowercase'] = text.lower()
            elif op == 'word_count':
                results['word_count'] = len(text.split())

        return {
            'original_text': text,
            'results': results,
            'processed_by': f'TextProcessorNode[{self.name}]'
        }
`;

    await this.pyodide!.runPythonAsync(remoteMediaCode);
  }

  async executeNode(nodeType: string, nodeName: string, config: any, data: any): Promise<any> {
    if (!this.pyodide) {
      throw new Error('Pyodide not initialized');
    }

    // Create node instance
    this.pyodide.globals.set('node_config', config);
    this.pyodide.globals.set('input_data', data);

    const result = await this.pyodide.runPythonAsync(`
node = ${nodeType}("${nodeName}", node_config)
node.process(input_data)
`);

    return result.toJs({ dict_converter: Object.fromEntries });
  }
}
```

### Phase 3: Update Pipeline Runner

Modify `browser-demo/src/pipeline-runner.ts`:

```typescript
import { PyodidePythonExecutor } from './python-executor';

export class PipelineRunner {
  private pythonExecutor: PyodidePythonExecutor | null = null;

  async loadPythonRuntime(): Promise<void> {
    this.pythonExecutor = new PyodidePythonExecutor();
    await this.pythonExecutor.initialize();
  }

  async execute(manifest: PipelineManifest, inputData?: any[]): Promise<any> {
    // For Rust nodes: use WASM binary (current implementation)
    // For Python nodes: use Pyodide

    for (const node of manifest.nodes) {
      if (this.isPythonNode(node.node_type)) {
        // Execute via Pyodide
        const result = await this.pythonExecutor!.executeNode(
          node.node_type,
          node.id,
          node.params,
          data
        );
      } else {
        // Execute via Rust WASM (current implementation)
        // ... existing code ...
      }
    }
  }

  private isPythonNode(nodeType: string): boolean {
    return nodeType.endsWith('Node') &&
           !['MultiplyNode', 'AddNode'].includes(nodeType);
  }
}
```

## Benefits of Hybrid Approach

### ✅ Advantages
1. **Rust nodes work perfectly** - Already proven with Calculator example
2. **Python nodes work in browser** - Via Pyodide (battle-tested solution)
3. **Native performance** - wasmtime execution unchanged
4. **Progressive enhancement** - Load Pyodide only when Python nodes needed
5. **Maintainability** - Leverage Pyodide's active development

### ⚠️ Trade-offs
1. **Larger initial download** - ~30-40MB for Pyodide (vs 20MB current)
   - Mitigated by CDN caching
   - Only loads when Python nodes are detected
2. **Two Python runtimes** - PyO3 for native, Pyodide for browser
   - Clear separation of concerns
   - Both use CPython 3.12
3. **Different node implementations** - May need browser-specific node code
   - Can share most logic
   - Browser nodes can be simplified versions

## Alternative: Rust-Only Nodes (Current Status)

If Python in browser is not critical:

### What Works Now ✅
- **Rust-native nodes**: Perfect execution in browser
- **Calculator example**: [5,7,3] → [20,24,16] ✅
- **WASM load time**: 16ms (20MB)
- **All Rust features**: Fully supported

### What Doesn't Work ❌
- **Python nodes**: Stack overflow on stdlib imports
- **TextProcessor**: Fails during json/logging import

### Recommendation
**Document as limitation** and provide Rust implementations of common nodes:
- TextProcessorNode → RustTextProcessorNode
- DataTransformNode → RustTransformNode
- etc.

## Decision Matrix

| Criterion | Rust-Only | Hybrid (Pyodide) |
|-----------|-----------|------------------|
| **Implementation Time** | ✅ Done (0 hours) | ⚠️ 4-8 hours |
| **Browser Bundle Size** | ✅ 20MB | ⚠️ 30-40MB |
| **Python Node Support** | ❌ None | ✅ Full |
| **Maintenance Burden** | ✅ Low | ⚠️ Medium |
| **Native Performance** | ✅ Same | ✅ Same |
| **Browser Performance** | ✅ Excellent | ✅ Good |
| **Streaming Nodes** | ❌ No Python | ✅ Yes |

## Recommendation

**For MVP**: Use Rust-only nodes in browser (current status)
- Document Python nodes as "Native only" feature
- Provide Rust alternatives for common operations

**For Full Feature Parity**: Implement Hybrid Pyodide approach
- Estimated effort: 1-2 days
- Provides complete Python compatibility
- Leverages battle-tested Pyodide solution

## Next Steps

1. ✅ Document current status (Rust nodes work, Python nodes don't)
2. ✅ Update README with browser limitations
3. ⏳ Decide: Rust-only or Hybrid approach?
4. ⏳ If Hybrid: Implement Pyodide integration
5. ⏳ If Rust-only: Create Rust implementations of common nodes

## Current Status

**Phase 2.5: WASI Filesystem Mounting** - ⚠️ **PARTIAL**
- ✅ WASI filesystem working
- ✅ Rust nodes execute perfectly in browser
- ✅ Python nodes execute perfectly in native wasmtime
- ❌ Python nodes fail in browser due to stack overflow
- **Recommendation**: Use hybrid Pyodide approach OR document as Rust-only for browser
