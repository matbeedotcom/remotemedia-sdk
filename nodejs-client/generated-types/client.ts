import { ExecutionResponse, ExecutionOptions, StreamHandle, NodeInfo } from './base';
import { NodeMap } from './config-types';
import { NodeType } from './node-types';

/**
 * RemoteMedia Processing Client Interface
 */
export interface RemoteExecutionClient {
  /**
   * Execute a node with type-safe configuration
   * 
   * @param nodeType - The type of node to instantiate and execute
   * @param config - Configuration object with constructor args for the node
   * @param inputData - Data to process with the node
   * @param options - Execution options
   */
  executeNode<T extends NodeType>(
    nodeType: T,
    config: Partial<NodeMap[T]>,
    inputData: any,
    options?: ExecutionOptions
  ): Promise<ExecutionResponse>;

  /**
   * List all available nodes
   */
  listAvailableNodes(): Promise<NodeInfo[]>;

  /**
   * Stream data through a node
   * 
   * @param nodeType - The type of node to instantiate and use for streaming
   * @param config - Configuration object with constructor args for the node
   * @param onData - Callback for processed data
   * @param onError - Callback for errors
   */
  streamNode<T extends NodeType>(
    nodeType: T,
    config: Partial<NodeMap[T]>,
    onData: (data: any) => void,
    onError?: (error: Error) => void
  ): StreamHandle;

  /**
   * Close the client connection
   */
  close(): Promise<void>;
}
