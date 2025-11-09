/**
 * PyodidePythonExecutor - Execute Python nodes in the browser using Pyodide
 *
 * This adapter integrates Pyodide (CPython compiled to WASM) to run Python nodes
 * in the browser, working alongside the Rust WASM executor for a hybrid runtime.
 *
 * Features:
 * - Full CPython 3.12 compatibility via Pyodide
 * - Loads remotemedia Python package into Pyodide environment
 * - Executes Python nodes with proper input/output handling
 * - Caches Pyodide instance for fast subsequent executions
 */

import { loadPyodide, PyodideInterface } from 'pyodide';

export interface PythonNodeConfig {
  [key: string]: any;
}

export class PyodidePythonExecutor {
  private pyodide: PyodideInterface | null = null;
  private initialized: boolean = false;

  /**
   * Initialize Pyodide and load remotemedia package
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      console.log('Pyodide already initialized');
      return;
    }

    console.log('Loading Pyodide...');
    const startTime = performance.now();

    this.pyodide = await loadPyodide({
      indexURL: 'https://cdn.jsdelivr.net/pyodide/v0.29.0/full/'
    });

    const loadTime = performance.now() - startTime;
    console.log(`Pyodide loaded in ${loadTime.toFixed(2)}ms`);

    // Load remotemedia package into Pyodide
    await this.loadRemoteMediaPackage();

    this.initialized = true;
  }

  /**
   * Load remotemedia Python package into Pyodide environment
   *
   * This loads a minimal implementation of remotemedia nodes that work in the browser.
   * For full feature parity, we could:
   * - Load from python-stdlib.json
   * - Install from PyPI (if published)
   * - Use inline Python code (current approach for simplicity)
   */
  private async loadRemoteMediaPackage(): Promise<void> {
    console.log('Loading remotemedia package into Pyodide...');

    const remoteMediaCode = `
# Browser-compatible remotemedia package
# Minimal implementation of core nodes for browser execution

class Node:
    """Base node class for all remotemedia nodes"""
    def __init__(self, name: str, config: dict):
        self.name = name
        self.config = config

    def process(self, data):
        """Process input data and return output"""
        raise NotImplementedError(f"{self.__class__.__name__} must implement process()")


class TextProcessorNode(Node):
    """Process text with various operations (uppercase, lowercase, word count, etc.)"""

    def process(self, data):
        text = data.get('text', '') if isinstance(data, dict) else str(data)
        operations = self.config.get('operations', ['uppercase'])

        results = {}

        for op in operations:
            if op == 'uppercase':
                results['uppercase'] = text.upper()
            elif op == 'lowercase':
                results['lowercase'] = text.lower()
            elif op == 'word_count':
                results['word_count'] = len(text.split())
            elif op == 'char_count':
                results['char_count'] = len(text)
            elif op == 'reverse':
                results['reverse'] = text[::-1]

        return {
            'original_text': text,
            'results': results,
            'processed_by': f'TextProcessorNode[{self.name}]'
        }


class DataTransformNode(Node):
    """Transform data with various operations"""

    def process(self, data):
        operation = self.config.get('operation', 'passthrough')

        if operation == 'passthrough':
            return data
        elif operation == 'double':
            if isinstance(data, (int, float)):
                return data * 2
            elif isinstance(data, list):
                return [x * 2 if isinstance(x, (int, float)) else x for x in data]
        elif operation == 'increment':
            if isinstance(data, (int, float)):
                return data + 1
            elif isinstance(data, list):
                return [x + 1 if isinstance(x, (int, float)) else x for x in data]

        return {
            'operation': operation,
            'input': data,
            'processed_by': f'DataTransformNode[{self.name}]'
        }


# Registry for available node types
NODE_REGISTRY = {
    'TextProcessorNode': TextProcessorNode,
    'DataTransformNode': DataTransformNode,
    'Node': Node,
}


def create_node(node_type: str, node_name: str, config: dict):
    """Factory function to create node instances"""
    if node_type not in NODE_REGISTRY:
        raise ValueError(f"Unknown node type: {node_type}")

    return NODE_REGISTRY[node_type](node_name, config)
`;

    await this.pyodide!.runPythonAsync(remoteMediaCode);
    console.log('remotemedia package loaded successfully');
  }

  /**
   * Execute a Python node with given configuration and input data
   *
   * @param nodeType - The Python class name (e.g., "TextProcessorNode")
   * @param nodeName - Instance name for this node
   * @param config - Node configuration parameters
   * @param data - Input data to process
   * @returns Processed output data
   */
  async executeNode(
    nodeType: string,
    nodeName: string,
    config: PythonNodeConfig,
    data: any
  ): Promise<any> {
    if (!this.pyodide) {
      throw new Error('Pyodide not initialized. Call initialize() first.');
    }

    console.log(`Executing Python node: ${nodeType}[${nodeName}]`);

    try {
      // Convert JS objects to Python using toPy() to ensure proper dict conversion
      const pyConfig = this.pyodide.toPy(config);
      const pyData = this.pyodide.toPy(data);

      // Set as Python globals
      this.pyodide.globals.set('node_config', pyConfig);
      this.pyodide.globals.set('input_data', pyData);

      // Execute Python code to create node and process data
      const pythonCode = `
# Create node instance
node = create_node("${nodeType}", "${nodeName}", node_config)

# Process input data
result = node.process(input_data)

# Return result
result
`;

      const result = await this.pyodide.runPythonAsync(pythonCode);

      // Convert Python object to JavaScript
      const jsResult = result.toJs({ dict_converter: Object.fromEntries });

      console.log(`Python node executed successfully:`, jsResult);
      return jsResult;

    } catch (error) {
      console.error(`Python execution error in ${nodeType}[${nodeName}]:`, error);
      throw new Error(`Python node execution failed: ${error}`);
    }
  }

  /**
   * Check if a node type is a Python node
   *
   * @param nodeType - The node type to check
   * @returns true if this is a Python node type
   */
  isPythonNode(nodeType: string): boolean {
    // Python nodes end with "Node" and are not Rust nodes
    const rustNodes = ['MultiplyNode', 'AddNode', 'Calculator'];

    return nodeType.endsWith('Node') && !rustNodes.includes(nodeType);
  }

  /**
   * Get information about the Pyodide runtime
   */
  getInfo(): { version: string; initialized: boolean } | null {
    if (!this.pyodide) {
      return null;
    }

    return {
      version: this.pyodide.version,
      initialized: this.initialized,
    };
  }

  /**
   * Cleanup Pyodide resources
   */
  destroy(): void {
    this.pyodide = null;
    this.initialized = false;
    console.log('Pyodide executor destroyed');
  }
}
