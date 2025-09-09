/**
 * Helper utilities and convenience functions for RemoteMedia Node.js Client
 */

import { RemoteProxyClient } from './client';
import { RemoteExecutorConfig, RemoteNodeProxy, NodeConfig } from './types';

/**
 * Execute function interface for simplified node execution
 */
export interface ExecuteFunction {
  /**
   * Execute a node with the given type and configuration
   * @param nodeType The type of node to execute
   * @param config Node configuration parameters
   * @param data Input data to process
   * @returns Promise resolving to the processed output
   */
  <T extends import('../generated-types').NodeType>(
    nodeType: T,
    config: Partial<import('../generated-types').NodeMap[T]>,
    data: any
  ): Promise<any>;

  /**
   * Execute a node with the given type and configuration (legacy string-based)
   * @deprecated Use NodeType enum instead of string for better type safety
   */
  (nodeType: string, config: NodeConfig, data: any): Promise<any>;
}

/**
 * Helper function for Python-style async with pattern - original interface
 * 
 * @example
 * ```typescript
 * await withRemoteProxy({ host: 'localhost', port: 50052 }, async (client) => {
 *   const node = await client.createNodeProxy('CalculatorNode');
 *   const result = await node.process({ operation: 'add', args: [1, 2] });
 * });
 * ```
 */
export async function withRemoteProxy<T>(
  config: RemoteExecutorConfig,
  callback: (client: RemoteProxyClient) => Promise<T>
): Promise<T> {
  const client = new RemoteProxyClient(config);
  try {
    await client.connect();
    return await callback(client);
  } finally {
    await client.close();
  }
}

/**
 * Helper function for simplified node execution interface
 * 
 * @example
 * ```typescript
 * await withRemoteExecutor({ host: 'localhost', port: 50052 }, async (execute) => {
 *   const result = await execute(NodeType.TransformersPipelineNode, {
 *     task: 'sentiment-analysis',
 *     model: 'distilbert-base-uncased-finetuned-sst-2-english'
 *   }, "This is amazing!");
 * });
 * ```
 */
export async function withRemoteExecutor<T>(
  config: RemoteExecutorConfig,
  callback: (execute: ExecuteFunction) => Promise<T>
): Promise<T> {
  const client = new RemoteProxyClient(config);
  try {
    await client.connect();

    // Create the simplified execute function
    const execute: ExecuteFunction = async (nodeType: any, nodeConfig: any, data: any) => {
      const proxy = await client.createNodeProxy(nodeType, nodeConfig);
      return await proxy.process(data);
    };

    return await callback(execute);
  } finally {
    await client.close();
  }
}

/**
 * Convenience class for common node types
 */
export class RemoteNodes {
  constructor(private client: RemoteProxyClient) { }

  /**
   * Create an audio transformation node
   */
  async audioTransform(config: {
    sampleRate?: number;
    channels?: number;
    dtype?: string;
  } = {}): Promise<RemoteNodeProxy> {
    return this.client.createNodeProxy('AudioTransform', config);
  }

  /**
   * Create a Hugging Face transformers pipeline node
   */
  async transformersPipeline(config: {
    task: string;
    model?: string;
    device?: string | number;
    model_kwargs?: Record<string, any>;
  }): Promise<RemoteNodeProxy> {
    return this.client.createNodeProxy('TransformersPipelineNode', config);
  }

  /**
   * Create a text processor node
   */
  async textProcessor(config: {} = {}): Promise<RemoteNodeProxy> {
    return this.client.createNodeProxy('TextProcessorNode', config);
  }

  /**
   * Create a calculator node
   */
  async calculator(config: {
    operation?: string;
    factor?: number;
  } = {}): Promise<RemoteNodeProxy> {
    return this.client.createNodeProxy('CalculatorNode', config);
  }

  /**
   * Create a code executor node (WARNING: Security risk!)
   */
  async codeExecutor(config: {
    code: string;
    entry_point?: string;
  }): Promise<RemoteNodeProxy> {
    return this.client.createNodeProxy('CodeExecutorNode', config);
  }

  /**
   * Create an audio buffer node
   */
  async audioBuffer(config: {
    target_size?: number;
    target_duration?: number;
    sample_rate?: number;
  } = {}): Promise<RemoteNodeProxy> {
    return this.client.createNodeProxy('AudioBuffer', config);
  }

  /**
   * Create a video transform node
   */
  async videoTransform(config: NodeConfig = {}): Promise<RemoteNodeProxy> {
    return this.client.createNodeProxy('VideoTransform', config);
  }

  /**
   * Create a format converter node
   */
  async formatConverter(config: {
    input_format?: string;
    output_format?: string;
  } = {}): Promise<RemoteNodeProxy> {
    return this.client.createNodeProxy('FormatConverter', config);
  }
}

/**
 * Create a pipeline of nodes that process data sequentially
 */
export class NodePipeline {
  private nodes: RemoteNodeProxy[] = [];

  /**
   * Add a node to the pipeline
   */
  add(node: RemoteNodeProxy): NodePipeline {
    this.nodes.push(node);
    return this;
  }

  /**
   * Process data through all nodes in the pipeline
   */
  async process(data: any): Promise<any> {
    let result = data;
    for (const node of this.nodes) {
      result = await node.process(result);
    }
    return result;
  }

  /**
   * Get the number of nodes in the pipeline
   */
  get length(): number {
    return this.nodes.length;
  }

  /**
   * Clear all nodes from the pipeline
   */
  clear(): void {
    this.nodes = [];
  }
}

/**
 * Batch process multiple items through a node
 */
export async function batchProcess<T, R>(
  node: RemoteNodeProxy,
  items: T[],
  options: {
    batchSize?: number;
    parallel?: boolean;
    onProgress?: (completed: number, total: number) => void;
  } = {}
): Promise<R[]> {
  const { batchSize = 10, parallel = true, onProgress } = options;
  const results: R[] = [];

  if (parallel) {
    // Process in parallel batches
    for (let i = 0; i < items.length; i += batchSize) {
      const batch = items.slice(i, i + batchSize);
      const batchResults = await Promise.all(
        batch.map(item => node.process(item))
      );
      results.push(...batchResults);

      if (onProgress) {
        onProgress(results.length, items.length);
      }
    }
  } else {
    // Process sequentially
    for (let i = 0; i < items.length; i++) {
      const result = await node.process(items[i]);
      results.push(result);

      if (onProgress) {
        onProgress(i + 1, items.length);
      }
    }
  }

  return results;
}

/**
 * Retry a node operation with exponential backoff
 */
export async function retryOperation<T>(
  operation: () => Promise<T>,
  options: {
    maxAttempts?: number;
    initialDelay?: number;
    maxDelay?: number;
    backoffMultiplier?: number;
    shouldRetry?: (error: Error) => boolean;
  } = {}
): Promise<T> {
  const {
    maxAttempts = 3,
    initialDelay = 1000,
    maxDelay = 30000,
    backoffMultiplier = 2,
    shouldRetry = () => true
  } = options;

  let lastError: Error;
  let delay = initialDelay;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await operation();
    } catch (error: any) {
      lastError = error;

      if (attempt === maxAttempts || !shouldRetry(error)) {
        throw error;
      }

      // Wait before retrying
      await new Promise(resolve => setTimeout(resolve, delay));

      // Increase delay for next attempt
      delay = Math.min(delay * backoffMultiplier, maxDelay);
    }
  }

  throw lastError!;
}