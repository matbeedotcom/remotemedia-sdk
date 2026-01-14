import { ExecutionResponse, ExecutionOptions, StreamHandle, NodeInfo } from './base';
import { NodeConfigMap } from './config-types';
import { NodeType } from './node-types';

/**
 * RemoteMedia Processing Client Interface
 */
export interface RemoteExecutionClient {
  /**
   * Execute a node with type-safe configuration
   */
  executeNode<T extends NodeType>(
    nodeType: T,
    config: NodeConfigMap[T],
    inputData: any,
    options?: ExecutionOptions
  ): Promise<ExecutionResponse>;

  /**
   * List all available nodes
   */
  listAvailableNodes(): Promise<NodeInfo[]>;

  /**
   * Stream data through a node
   */
  streamNode<T extends NodeType>(
    nodeType: T,
    config: NodeConfigMap[T],
    onData: (data: any) => void,
    onError?: (error: Error) => void
  ): StreamHandle;

  /**
   * Close the client connection
   */
  close(): Promise<void>;
}
