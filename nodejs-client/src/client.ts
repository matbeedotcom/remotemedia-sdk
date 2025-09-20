/**
 * RemoteProxyClient - Execute any server node remotely
 * 
 * This client provides a transparent proxy pattern for executing
 * nodes registered on a RemoteMedia processing server.
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import { promisify } from 'util';
import * as path from 'path';
import {
  RemoteExecutorConfig,
  NodeConfig,
  ExecutionOptions,
  RemoteNodeProxy,
  StreamHandle,
  NodeInfo,
  ServerStatus,
  RemoteExecutionError
} from './types';
import {
  NodeType,
  NodeMap
} from '../generated-types';

// Default configuration values
const DEFAULT_CONFIG: Partial<RemoteExecutorConfig> = {
  protocol: 'grpc',
  timeout: 30,
  sslEnabled: false,
  maxMessageSize: 4 * 1024 * 1024, // 4MB
  retry: {
    maxAttempts: 3,
    initialBackoff: 1000,
    maxBackoff: 5000,
    backoffMultiplier: 1.5
  }
};

/**
 * Main client class for remote node execution
 */
export class RemoteProxyClient {
  private client: any;
  private config: RemoteExecutorConfig;
  private connected: boolean = false;
  private packageDefinition: any;
  private remoteMedia: any;

  // Promisified methods
  private executeNode: any;
  private listNodesMethod: any;
  private getStatusMethod: any;

  constructor(config: RemoteExecutorConfig) {
    this.config = { ...DEFAULT_CONFIG, ...config };

    // Load protobuf definition
    const protoPath = this.getProtoPath();
    this.packageDefinition = protoLoader.loadSync(protoPath, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
      includeDirs: [path.dirname(protoPath)]
    });

    this.remoteMedia = grpc.loadPackageDefinition(this.packageDefinition).remotemedia as any;
  }

  /**
   * Get the path to the proto file
   * Handles both development and production environments
   */
  private getProtoPath(): string {
    // Try multiple possible locations
    const possiblePaths = [
      // Production: proto file bundled with the package
      path.join(__dirname, '../protos/execution.proto'),
      // Development: relative to the project root
      path.join(__dirname, '../../service/protos/execution.proto'),
      // Alternative development path
      path.join(__dirname, '../../../service/protos/execution.proto')
    ];

    // Find the first existing path
    const fs = require('fs');
    for (const protoPath of possiblePaths) {
      if (fs.existsSync(protoPath)) {
        return protoPath;
      }
    }

    throw new Error(
      'Could not find execution.proto file. Tried paths: ' +
      possiblePaths.join(', ')
    );
  }

  /**
   * Connect to the remote service
   */
  async connect(): Promise<void> {
    if (this.connected) return;

    const address = `${this.config.host}:${this.config.port}`;
    const channelOptions: any = {
      'grpc.max_receive_message_length': this.config.maxMessageSize,
      'grpc.max_send_message_length': this.config.maxMessageSize,
    };

    // Add retry configuration
    if (this.config.retry) {
      channelOptions['grpc.service_config'] = JSON.stringify({
        methodConfig: [{
          name: [{ service: 'remotemedia.execution.RemoteExecutionService' }],
          retryPolicy: {
            maxAttempts: this.config.retry.maxAttempts,
            initialBackoff: `${this.config.retry.initialBackoff ?? 1000 / 1000}s`,
            maxBackoff: `${this.config.retry.maxBackoff ?? 5000 / 1000}s`,
            backoffMultiplier: this.config.retry.backoffMultiplier,
            retryableStatusCodes: ['UNAVAILABLE', 'DEADLINE_EXCEEDED']
          }
        }]
      });
    }

    // Create credentials
    let credentials;
    if (this.config.sslEnabled) {
      if (this.config.sslCredentials) {
        credentials = this.config.sslCredentials;
      } else {
        credentials = grpc.credentials.createSsl();
      }
    } else {
      credentials = grpc.credentials.createInsecure();
    }

    // Create client
    this.client = new this.remoteMedia.execution.RemoteExecutionService(
      address,
      credentials,
      channelOptions
    );

    // Promisify methods
    this.executeNode = promisify(this.client.ExecuteNode.bind(this.client));
    this.listNodesMethod = promisify(this.client.ListNodes.bind(this.client));
    this.getStatusMethod = promisify(this.client.GetStatus.bind(this.client));

    this.connected = true;
  }

  /**
   * Create a proxy for a specific node type with full type safety
   * 
   * @param nodeType - The type of node to create from NodeType enum
   * @param config - Node configuration parameters (type-safe based on node type)
   * @param options - Execution options
   * @returns A proxy object with a process method
   */
  async createNodeProxy<T extends NodeType>(
    nodeType: T,
    config?: Partial<NodeMap[T]>,
    options?: ExecutionOptions
  ): Promise<RemoteNodeProxy>;

  /**
   * Create a proxy for a specific node type (legacy string-based)
   * @deprecated Use NodeType enum instead of string for better type safety
   */
  async createNodeProxy(
    nodeType: string,
    config?: NodeConfig,
    options?: ExecutionOptions
  ): Promise<RemoteNodeProxy>;

  async createNodeProxy<T extends NodeType>(
    nodeType: T | string,
    config: Partial<NodeMap[T]> | NodeConfig = {},
    options: ExecutionOptions = {}
  ): Promise<RemoteNodeProxy> {
    if (!this.connected) {
      await this.connect();
    }

    // Convert config to string map for gRPC
    const configMap: Record<string, string> = {};
    for (const [key, value] of Object.entries(config)) {
      if (typeof value === 'object') {
        configMap[key] = JSON.stringify(value);
      } else {
        configMap[key] = String(value);
      }
    }

    // Create proxy object
    const proxy: RemoteNodeProxy = {
      process: async (data: any): Promise<any> => {
        const request = {
          node_type: nodeType,
          config: configMap,
          input_data: Buffer.from(JSON.stringify(data)),
          serialization_format: 'json',
          options: {
            timeout: options.timeout || this.config.timeout || 30.0,
            enable_gpu: options.enable_gpu || false,
            priority: options.priority || 'normal',
            metadata: options.metadata || {}
          }
        };

        try {
          const response = await this.executeNode(request);

          if (response.status === 'EXECUTION_STATUS_SUCCESS') {
            return JSON.parse(response.output_data.toString());
          } else {
            const error: RemoteExecutionError = new Error(
              response.error_message || 'Node execution failed'
            );
            error.code = response.status;
            error.details = response.error_details;
            throw error;
          }
        } catch (error: any) {
          // Wrap gRPC errors
          if (error.code !== undefined && error.details === undefined) {
            const wrappedError: RemoteExecutionError = new Error(error.message);
            wrappedError.code = grpc.status[error.code] || String(error.code);
            wrappedError.details = error.metadata?.getMap() || {};
            throw wrappedError;
          }
          throw error;
        }
      },

      processStream: (onData: (data: any) => void, onError?: (error: Error) => void): StreamHandle => {
        const stream = this.client.StreamNode();
        let sessionId = '';

        // Send initialization
        stream.write({
          init: {
            node_type: nodeType,
            config: configMap,
            serialization_format: 'json',
            options: {
              timeout: options.timeout || this.config.timeout || 30.0,
              enable_gpu: options.enable_gpu || false,
              priority: options.priority || 'normal'
            }
          }
        });

        // Handle responses
        stream.on('data', (response: any) => {
          if (response.session_id && !sessionId) {
            sessionId = response.session_id;
          }

          if (response.error_message) {
            const error: RemoteExecutionError = new Error(response.error_message);
            error.code = response.status;
            if (onError) {
              onError(error);
            }
          } else if (response.data) {
            try {
              const data = JSON.parse(response.data.toString());
              onData(data);
            } catch (e) {
              if (onError) {
                onError(e as Error);
              }
            }
          }
        });

        stream.on('error', (error: Error) => {
          if (onError) {
            onError(error);
          }
        });

        return {
          send: async (data: any) => {
            const serialized = Buffer.from(JSON.stringify(data));
            stream.write({ data: serialized });
          },
          close: async () => {
            stream.end();
          },
          sessionId: sessionId || 'stream-' + Date.now()
        };
      }
    };

    return proxy;
  }

  /**
   * Get list of available nodes from the server
   * 
   * @param category - Optional category filter
   * @returns Array of node information
   */
  async listNodes(category?: string): Promise<NodeInfo[]> {
    if (!this.connected) {
      await this.connect();
    }

    const response = await this.listNodesMethod({ category: category || '' });
    return response.available_nodes || [];
  }

  /**
   * Get server status and health information
   * 
   * @param includeMetrics - Include performance metrics
   * @param includeSessions - Include active session information
   * @returns Server status object
   */
  async getStatus(includeMetrics = true, includeSessions = false): Promise<ServerStatus> {
    if (!this.connected) {
      await this.connect();
    }

    const response = await this.getStatusMethod({
      include_metrics: includeMetrics,
      include_sessions: includeSessions
    });

    return response;
  }

  /**
   * Check if the client is connected
   */
  isConnected(): boolean {
    return this.connected;
  }

  /**
   * Close the connection
   */
  async close(): Promise<void> {
    this.connected = false;
    if (this.client) {
      this.client.close();
    }
  }

  /**
   * Python-style context manager support
   */
  async __aenter__(): Promise<RemoteProxyClient> {
    await this.connect();
    return this;
  }

  async __aexit__(): Promise<void> {
    await this.close();
  }
}