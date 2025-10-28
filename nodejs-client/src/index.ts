/**
 * RemoteMedia Node.js Client
 * 
 * A TypeScript/JavaScript client for executing remote nodes on a RemoteMedia Processing server.
 * 
 * @packageDocumentation
 */

// Export main gRPC client (new)
export { RemoteMediaClient, AudioFormat, ErrorType, RemoteMediaError } from './grpc-client.js';
export type { AudioBuffer, PipelineManifest, ExecutionResult, VersionInfo, ChunkResult } from './grpc-client.js';

// Export legacy proxy client
export { RemoteProxyClient } from './client.js';

// Export helper functions and classes
export {
  withRemoteProxy,
  withRemoteExecutor,
  ExecuteFunction,
  RemoteNodes,
  NodePipeline,
  batchProcess,
  retryOperation
} from './helpers.js';

// Export all types
export * from './types.js';

// Export generated types for convenience
export { NodeType, NodeMap } from '../generated-types/index.js';

// Version information
export const VERSION = '0.2.0';