/**
 * PipelineRunner - Execute RemoteMedia pipelines in the browser using WASM
 *
 * This class wraps the WASM runtime and provides a simple API for:
 * - Loading WASM modules
 * - Executing pipelines with manifests
 * - Retrieving results
 *
 * WASI I/O Strategy:
 * - Use browser-native WebAssembly.instantiate()
 * - Use @bjorn3/browser_wasi_shim for WASI polyfill (stdin/stdout)
 * - Pass manifest as JSON via stdin
 * - Capture stdout for results
 */

import { WASI, File, OpenFile, PreopenDirectory, Directory } from '@bjorn3/browser_wasi_shim';
import { loadPythonStdlib } from './python-fs-loader';
import { PyodidePythonExecutor } from './python-executor';

export interface PipelineManifest {
  version: string;
  metadata: {
    name: string;
    description?: string;
  };
  nodes: Array<{
    id: string;
    node_type: string;
    params: Record<string, any>;
  }>;
  connections: Array<{
    from: string;
    to: string;
  }>;
}

export interface PipelineInput {
  manifest: PipelineManifest;
  input_data?: any[];
}

export interface PipelineResult {
  status: 'success' | 'error';
  outputs?: any[];
  error?: string;
  graph_info?: {
    node_count: number;
    sink_count: number;
    source_count: number;
    execution_order: string[];
  };
}

export interface ExecutionMetrics {
  executionTimeMs: number;
  wasmLoadTimeMs: number;
  totalTimeMs: number;
}

export class PipelineRunner {
  private wasmModule: WebAssembly.Module | null = null;
  private wasmBytes: Uint8Array | null = null;
  private pythonStdlib: Map<string, File | Directory> | null = null;
  private pythonExecutor: PyodidePythonExecutor | null = null;

  /**
   * Load Python stdlib filesystem (optional, for Python nodes via WASM)
   * Note: This is only needed for the PyO3 WASM executor (native wasmtime)
   * For browser, use loadPyodideRuntime() instead
   */
  async loadPythonStdlib(): Promise<void> {
    if (this.pythonStdlib) {
      console.log('Python stdlib already loaded');
      return;
    }

    this.pythonStdlib = await loadPythonStdlib();
  }

  /**
   * Load Pyodide runtime for executing Python nodes in the browser
   * This provides full CPython 3.12 compatibility via Pyodide
   */
  async loadPyodideRuntime(): Promise<void> {
    if (this.pythonExecutor) {
      console.log('Pyodide runtime already loaded');
      return;
    }

    this.pythonExecutor = new PyodidePythonExecutor();
    await this.pythonExecutor.initialize();
  }

  /**
   * Load WASM binary from URL or ArrayBuffer
   */
  async loadWasm(source: string | ArrayBuffer): Promise<void> {
    console.log('Loading WASM module...');
    const startTime = performance.now();

    if (typeof source === 'string') {
      // Load from URL
      const response = await fetch(source);
      if (!response.ok) {
        throw new Error(`Failed to fetch WASM: ${response.statusText}`);
      }
      this.wasmBytes = new Uint8Array(await response.arrayBuffer());
    } else {
      // Load from ArrayBuffer
      this.wasmBytes = new Uint8Array(source);
    }

    // Pre-compile the module
    this.wasmModule = await WebAssembly.compile(this.wasmBytes.buffer as ArrayBuffer);
    console.log('WASM module compiled successfully');

    const loadTime = performance.now() - startTime;
    console.log(`WASM module loaded in ${loadTime.toFixed(2)}ms (${this.wasmBytes.length} bytes)`);
  }

  /**
   * Check if the pipeline contains Python nodes that require Pyodide
   */
  private hasPythonNodes(manifest: PipelineManifest): boolean {
    const rustNodes = ['MultiplyNode', 'AddNode', 'Calculator'];

    return manifest.nodes.some(node =>
      node.node_type.endsWith('Node') && !rustNodes.includes(node.node_type)
    );
  }

  /**
   * Execute a pipeline with the given manifest and optional input data
   *
   * This method supports hybrid execution:
   * - Rust nodes are executed via WASM binary
   * - Python nodes are executed via Pyodide (if loaded)
   * - If pipeline has only Rust nodes, uses pure WASM execution
   * - If pipeline has Python nodes, uses Pyodide for those nodes
   */
  async execute(
    manifest: PipelineManifest,
    inputData?: any[]
  ): Promise<{ result: PipelineResult; metrics: ExecutionMetrics }> {
    if (!this.wasmModule) {
      throw new Error('WASM module not loaded. Call loadWasm() first.');
    }

    // Check if we need Pyodide for Python nodes
    const needsPyodide = this.hasPythonNodes(manifest);
    if (needsPyodide && !this.pythonExecutor) {
      console.warn('Pipeline contains Python nodes but Pyodide is not loaded.');
      console.warn('Python nodes will be attempted via WASM (may fail in browser).');
      console.warn('Call loadPyodideRuntime() before execute() to enable Python support.');
    }

    const totalStartTime = performance.now();

    // Prepare input JSON
    const input: PipelineInput = inputData
      ? { manifest, input_data: inputData }
      : { manifest } as any;

    const inputJson = JSON.stringify(input);
    console.log('Executing pipeline:', manifest.metadata.name);
    console.log('Input JSON:', inputJson);

    const execStartTime = performance.now();

    // Check pipeline composition
    const hasPythonNodes = this.hasPythonNodes(manifest);
    const hasRustNodes = manifest.nodes.some(node => !this.pythonExecutor?.isPythonNode(node.node_type));

    // Route to appropriate execution strategy
    if (this.pythonExecutor && hasPythonNodes && !hasRustNodes) {
      // Python-only pipeline - use Pyodide
      console.log('All nodes are Python nodes - using Pyodide-only execution');
      return await this.executePythonOnlyPipeline(manifest, inputData, totalStartTime, execStartTime);
    } else if (hasPythonNodes && hasRustNodes && this.pythonExecutor) {
      // Mixed pipeline - use hybrid execution
      console.log('Mixed Rust+Python pipeline - using hybrid execution');
      return await this.executeHybridPipeline(manifest, inputData, totalStartTime, execStartTime);
    }

    // Rust-only pipeline OR mixed pipeline without Pyodide - use WASM
    if (hasPythonNodes && !this.pythonExecutor) {
      console.warn('Pipeline has Python nodes but Pyodide not loaded - will try via WASM (may fail in browser)');
    }

    try {
      // Create stdin file with manifest JSON
      const stdinContent = new TextEncoder().encode(inputJson);
      const stdinFile = new File(stdinContent);

      // Create stdout file to capture output
      const stdoutFile = new File(new Uint8Array());

      // Prepare preopened directories for Python stdlib
      const fds: Array<OpenFile | PreopenDirectory> = [
        new OpenFile(stdinFile),  // fd 0: stdin
        new OpenFile(stdoutFile), // fd 1: stdout
        new OpenFile(new File(new Uint8Array())), // fd 2: stderr
      ];

      // Mount Python stdlib at /usr if available
      if (this.pythonStdlib) {
        console.log('Mounting Python stdlib at /usr');
        fds.push(new PreopenDirectory('/usr', this.pythonStdlib));
      }

      // Create WASI instance
      const wasi = new WASI([], [], fds);

      // Instantiate WASM with WASI imports
      const instance = await WebAssembly.instantiate(this.wasmModule, {
        wasi_snapshot_preview1: wasi.wasiImport,
      });

      // Run the WASM module
      wasi.start(instance as any);

      const executionTime = performance.now() - execStartTime;
      const totalTime = performance.now() - totalStartTime;

      // Get stdout content (access the File's data property directly)
      const stdoutData = new TextDecoder().decode(stdoutFile.data);

      console.log('WASM execution completed');
      console.log('Stdout:', stdoutData);

      // Parse output
      let pipelineResult: PipelineResult;
      try {
        // Extract JSON from stdout (may have debug output before/after)
        const jsonMatch = stdoutData.match(/\{[\s\S]*\}/);
        if (jsonMatch) {
          pipelineResult = JSON.parse(jsonMatch[0]);
        } else {
          throw new Error('No JSON found in output');
        }
      } catch (parseError) {
        pipelineResult = {
          status: 'error',
          error: `Failed to parse output: ${stdoutData}`,
        };
      }

      const metrics: ExecutionMetrics = {
        executionTimeMs: executionTime,
        wasmLoadTimeMs: 0,
        totalTimeMs: totalTime,
      };

      return { result: pipelineResult, metrics };

    } catch (error) {
      console.error('WASM execution error:', error);

      const errorResult: PipelineResult = {
        status: 'error',
        error: `Execution failed: ${error}`,
      };

      const metrics: ExecutionMetrics = {
        executionTimeMs: 0,
        wasmLoadTimeMs: 0,
        totalTimeMs: performance.now() - totalStartTime,
      };

      return { result: errorResult, metrics };
    }
  }

  /**
   * Execute a hybrid Rust+Python pipeline
   * Routes each node to the appropriate runtime based on type
   */
  private async executeHybridPipeline(
    manifest: PipelineManifest,
    inputData: any[] | undefined,
    totalStartTime: number,
    execStartTime: number
  ): Promise<{ result: PipelineResult; metrics: ExecutionMetrics }> {
    try {
      // Build execution order from connections
      const executionOrder = this.topologicalSort(manifest);
      console.log('Execution order:', executionOrder);

      const outputs: any[] = [];

      // For each input data item, execute through the pipeline
      const dataItems = inputData && inputData.length > 0 ? inputData : [{}];

      for (const data of dataItems) {
        let currentData = data;
        const nodeOutputs = new Map<string, any>();

        // Execute nodes in topological order
        for (const nodeId of executionOrder) {
          const node = manifest.nodes.find(n => n.id === nodeId);
          if (!node) continue;

          // Get input for this node (either initial data or output from previous node)
          const incomingConnections = manifest.connections.filter(c => c.to === nodeId);
          if (incomingConnections.length > 0) {
            // Use output from connected node
            const fromNode = incomingConnections[0].from;
            currentData = nodeOutputs.get(fromNode) || currentData;
          }

          // Execute node via appropriate runtime
          let nodeOutput: any;
          if (this.pythonExecutor!.isPythonNode(node.node_type)) {
            console.log(`Executing ${node.id} (${node.node_type}) via Pyodide with input:`, currentData);
            nodeOutput = await this.pythonExecutor!.executeNode(
              node.node_type,
              node.id,
              node.params,
              currentData
            );
          } else {
            console.log(`Executing ${node.id} (${node.node_type}) via WASM with input:`, currentData);
            nodeOutput = await this.executeRustNode(node, currentData);
          }

          console.log(`Node ${node.id} output:`, nodeOutput);
          nodeOutputs.set(nodeId, nodeOutput);
          currentData = nodeOutput;
        }

        // The final output is from the last node in execution order
        outputs.push(currentData);
      }

      const executionTime = performance.now() - execStartTime;
      const totalTime = performance.now() - totalStartTime;

      const result: PipelineResult = {
        status: 'success',
        outputs: outputs,
        graph_info: {
          node_count: manifest.nodes.length,
          sink_count: 1,
          source_count: 1,
          execution_order: executionOrder,
        },
      };

      const metrics: ExecutionMetrics = {
        executionTimeMs: executionTime,
        wasmLoadTimeMs: 0,
        totalTimeMs: totalTime,
      };

      return { result, metrics };

    } catch (error) {
      console.error('Hybrid execution error:', error);

      const errorResult: PipelineResult = {
        status: 'error',
        error: `Hybrid execution failed: ${error}`,
      };

      const metrics: ExecutionMetrics = {
        executionTimeMs: 0,
        wasmLoadTimeMs: 0,
        totalTimeMs: performance.now() - totalStartTime,
      };

      return { result: errorResult, metrics };
    }
  }

  /**
   * Execute a Python-only pipeline via Pyodide
   * This is used when all nodes in the pipeline are Python nodes
   */
  private async executePythonOnlyPipeline(
    manifest: PipelineManifest,
    inputData: any[] | undefined,
    totalStartTime: number,
    execStartTime: number
  ): Promise<{ result: PipelineResult; metrics: ExecutionMetrics }> {
    try {
      // For now, we support simple single-node Python pipelines
      // Full pipeline execution with connections would require implementing graph traversal
      if (manifest.nodes.length !== 1) {
        throw new Error('Python-only execution currently supports single-node pipelines only');
      }

      const node = manifest.nodes[0];
      const outputs: any[] = [];

      // Process each input data item through the node
      if (inputData && inputData.length > 0) {
        for (const data of inputData) {
          const output = await this.pythonExecutor!.executeNode(
            node.node_type,
            node.id,
            node.params,
            data
          );
          outputs.push(output);
        }
      } else {
        // No input data - just execute once with empty input
        const output = await this.pythonExecutor!.executeNode(
          node.node_type,
          node.id,
          node.params,
          {}
        );
        outputs.push(output);
      }

      const executionTime = performance.now() - execStartTime;
      const totalTime = performance.now() - totalStartTime;

      const result: PipelineResult = {
        status: 'success',
        outputs: outputs,
        graph_info: {
          node_count: manifest.nodes.length,
          sink_count: 1,
          source_count: 1,
          execution_order: [node.id],
        },
      };

      const metrics: ExecutionMetrics = {
        executionTimeMs: executionTime,
        wasmLoadTimeMs: 0,
        totalTimeMs: totalTime,
      };

      return { result, metrics };

    } catch (error) {
      console.error('Python-only execution error:', error);

      const errorResult: PipelineResult = {
        status: 'error',
        error: `Python execution failed: ${error}`,
      };

      const metrics: ExecutionMetrics = {
        executionTimeMs: 0,
        wasmLoadTimeMs: 0,
        totalTimeMs: performance.now() - totalStartTime,
      };

      return { result: errorResult, metrics };
    }
  }

  /**
   * Topological sort to determine execution order
   */
  private topologicalSort(manifest: PipelineManifest): string[] {
    const nodes = manifest.nodes.map(n => n.id);
    const edges = manifest.connections;

    // Build adjacency list and in-degree count
    const adj = new Map<string, string[]>();
    const inDegree = new Map<string, number>();

    for (const node of nodes) {
      adj.set(node, []);
      inDegree.set(node, 0);
    }

    for (const edge of edges) {
      adj.get(edge.from)?.push(edge.to);
      inDegree.set(edge.to, (inDegree.get(edge.to) || 0) + 1);
    }

    // Kahn's algorithm
    const queue: string[] = [];
    for (const [node, degree] of inDegree.entries()) {
      if (degree === 0) queue.push(node);
    }

    const result: string[] = [];
    while (queue.length > 0) {
      const node = queue.shift()!;
      result.push(node);

      for (const neighbor of adj.get(node) || []) {
        const newDegree = (inDegree.get(neighbor) || 0) - 1;
        inDegree.set(neighbor, newDegree);
        if (newDegree === 0) queue.push(neighbor);
      }
    }

    return result;
  }

  /**
   * Execute a single Rust node via WASM
   */
  private async executeRustNode(
    node: { id: string; node_type: string; params: Record<string, any> },
    data: any
  ): Promise<any> {
    // Create a single-node manifest for this Rust node
    const singleNodeManifest: PipelineManifest = {
      version: 'v1',
      metadata: {
        name: `single-node-${node.id}`,
        description: `Execute ${node.node_type} via WASM`,
      },
      nodes: [node],
      connections: [],
    };

    // Execute via WASM
    const input: PipelineInput = {
      manifest: singleNodeManifest,
      input_data: [data],
    };

    const inputJson = JSON.stringify(input);
    console.log(`Single-node WASM input for ${node.id}:`, inputJson);
    const stdinContent = new TextEncoder().encode(inputJson);
    const stdinFile = new File(stdinContent);
    const stdoutFile = new File(new Uint8Array());

    const fds = [
      new OpenFile(stdinFile),
      new OpenFile(stdoutFile),
      new OpenFile(new File(new Uint8Array())),
    ];

    const wasi = new WASI([], [], fds);

    const instance = await WebAssembly.instantiate(this.wasmModule!, {
      wasi_snapshot_preview1: wasi.wasiImport,
    });

    wasi.start(instance as any);

    const stdoutData = new TextDecoder().decode(stdoutFile.data);
    console.log(`WASM stdout for ${node.id}:`, stdoutData);

    const jsonMatch = stdoutData.match(/\{[\s\S]*\}/);

    if (!jsonMatch) {
      throw new Error(`No JSON output from Rust node ${node.id}. Stdout: ${stdoutData}`);
    }

    const result = JSON.parse(jsonMatch[0]);
    console.log(`Parsed WASM result for ${node.id}:`, result);

    if (result.status !== 'success') {
      throw new Error(`Rust node ${node.id} failed: ${result.error || 'Unknown error'}`);
    }

    if (!result.outputs) {
      throw new Error(`Rust node ${node.id} produced no outputs`);
    }

    // Handle both array and single object outputs
    let output = result.outputs;
    if (Array.isArray(result.outputs)) {
      if (result.outputs.length === 0) {
        throw new Error(`Rust node ${node.id} produced empty outputs array`);
      }
      output = result.outputs[0];
    }

    console.log(`Extracting output for ${node.id}:`, output);
    return output;
  }

  /**
   * Execute a single node (either via WASM or Pyodide)
   * This is used for hybrid execution mode
   * TODO: Implement for true hybrid Rust+Python pipelines
   */
  // private async executeNode(
  //   nodeType: string,
  //   nodeName: string,
  //   params: Record<string, any>,
  //   data: any
  // ): Promise<any> {
  //   // Check if this is a Python node and Pyodide is available
  //   if (this.pythonExecutor && this.pythonExecutor.isPythonNode(nodeType)) {
  //     console.log(`Routing ${nodeType} to Pyodide`);
  //     return await this.pythonExecutor.executeNode(nodeType, nodeName, params, data);
  //   } else {
  //     // For Rust nodes or when Pyodide is not available, use WASM
  //     // This would require executing individual nodes via WASM
  //     // For now, we'll note that this path needs implementation
  //     throw new Error(
  //       `Hybrid node-by-node execution not yet implemented for ${nodeType}. ` +
  //       `Use pure WASM execution (Rust-only) or Pyodide execution (Python-only) for now.`
  //     );
  //   }
  // }

  /**
   * Get information about the loaded WASM module
   */
  getModuleInfo(): { size: number; loaded: boolean } | null {
    if (!this.wasmBytes) {
      return null;
    }

    return {
      size: this.wasmBytes.length,
      loaded: this.wasmModule !== null,
    };
  }

  /**
   * Get information about loaded runtimes
   */
  getRuntimeInfo(): {
    wasm: { size: number; loaded: boolean } | null;
    pyodide: { version: string; initialized: boolean } | null;
  } {
    return {
      wasm: this.getModuleInfo(),
      pyodide: this.pythonExecutor ? this.pythonExecutor.getInfo() : null,
    };
  }

  /**
   * Unload the WASM module and free resources
   */
  unload(): void {
    this.wasmModule = null;
    this.wasmBytes = null;
    if (this.pythonExecutor) {
      this.pythonExecutor.destroy();
      this.pythonExecutor = null;
    }
    console.log('WASM module and Python executor unloaded');
  }
}
