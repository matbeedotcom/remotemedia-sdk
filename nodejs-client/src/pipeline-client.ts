/**
 * Pipeline Client for RemoteMedia Processing SDK
 * 
 * This module provides TypeScript client for managing and executing
 * pipelines on the remote gRPC service.
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import { EventEmitter } from 'events';
import path from 'path';

// Pipeline types
export interface PipelineDefinition {
  name: string;
  nodes: NodeDefinition[];
  connections?: PipelineConnection[];
  config?: Record<string, string>;
  metadata?: Record<string, string>;
}

export interface NodeDefinition {
  nodeId: string;
  nodeType: string;
  config?: Record<string, any>;
  isRemote?: boolean;
  remoteEndpoint?: string;
  isStreaming?: boolean;
  isSource?: boolean;
  isSink?: boolean;
}

export interface PipelineConnection {
  fromNode: string;
  toNode: string;
  outputPort?: string;
  inputPort?: string;
}

export interface PipelineInfo {
  pipelineId: string;
  name: string;
  category: string;
  description: string;
  registeredTimestamp: number;
  usageCount: number;
  metadata?: Record<string, string>;
}

export interface PipelineMetrics {
  totalExecutions: number;
  totalErrors: number;
  averageExecutionTimeMs: number;
  lastExecutionTimestamp?: number;
}

export interface RegisterPipelineOptions {
  metadata?: Record<string, string>;
  dependencies?: string[];
  autoExport?: boolean;
}

export interface ExecutePipelineOptions {
  runtimeConfig?: Record<string, string>;
  timeout?: number;
}

export interface StreamPipelineOptions {
  runtimeConfig?: Record<string, string>;
  bidirectional?: boolean;
}

// Pipeline client class
export class PipelineClient extends EventEmitter {
  private client: any;
  private connected: boolean = false;

  constructor(
    private host: string = 'localhost',
    private port: number = 50052,
    private credentials?: grpc.ChannelCredentials
  ) {
    super();
  }

  /**
   * Connect to the gRPC service
   */
  async connect(): Promise<void> {
    // Use import.meta.url to get current file path in ES modules
    const currentDir = path.dirname(new URL(import.meta.url).pathname);
    const protoPath = path.join(currentDir, '../../../remote_service/protos/execution.proto');
    
    const packageDefinition = protoLoader.loadSync(protoPath, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true
    });

    const proto = grpc.loadPackageDefinition(packageDefinition) as any;
    
    this.client = new proto.remotemedia.execution.RemoteExecutionService(
      `${this.host}:${this.port}`,
      this.credentials || grpc.credentials.createInsecure()
    );

    this.connected = true;
    this.emit('connected');
  }

  /**
   * Register a pipeline definition
   */
  async registerPipeline(
    name: string,
    definition: PipelineDefinition,
    options?: RegisterPipelineOptions
  ): Promise<string> {
    if (!this.connected) {
      throw new Error('Not connected to service');
    }

    return new Promise((resolve, reject) => {
      const request = {
        pipeline_name: name,
        definition: this.convertDefinitionToProto(definition),
        metadata: options?.metadata || {},
        dependencies: options?.dependencies || [],
        auto_export: options?.autoExport || false
      };

      this.client.RegisterPipeline(request, (error: any, response: any) => {
        if (error) {
          reject(error);
          return;
        }

        if (response.status !== 'EXECUTION_STATUS_SUCCESS') {
          reject(new Error(response.error_message));
          return;
        }

        resolve(response.pipeline_id);
      });
    });
  }

  /**
   * Unregister a pipeline
   */
  async unregisterPipeline(pipelineId: string): Promise<void> {
    if (!this.connected) {
      throw new Error('Not connected to service');
    }

    return new Promise((resolve, reject) => {
      this.client.UnregisterPipeline({ pipeline_id: pipelineId }, (error: any, response: any) => {
        if (error) {
          reject(error);
          return;
        }

        if (response.status !== 'EXECUTION_STATUS_SUCCESS') {
          reject(new Error(response.error_message));
          return;
        }

        resolve();
      });
    });
  }

  /**
   * List registered pipelines
   */
  async listPipelines(category?: string, includeDefinitions?: boolean): Promise<PipelineInfo[]> {
    if (!this.connected) {
      throw new Error('Not connected to service');
    }

    return new Promise((resolve, reject) => {
      const request = {
        category: category || '',
        include_definitions: includeDefinitions || false
      };

      this.client.ListPipelines(request, (error: any, response: any) => {
        if (error) {
          reject(error);
          return;
        }

        const pipelines = response.pipelines.map((p: any) => this.convertPipelineInfoFromProto(p));
        resolve(pipelines);
      });
    });
  }

  /**
   * Get detailed pipeline information
   */
  async getPipelineInfo(
    pipelineId: string,
    includeDefinition?: boolean,
    includeMetrics?: boolean
  ): Promise<{ info: PipelineInfo; metrics?: PipelineMetrics }> {
    if (!this.connected) {
      throw new Error('Not connected to service');
    }

    return new Promise((resolve, reject) => {
      const request = {
        pipeline_id: pipelineId,
        include_definition: includeDefinition || false,
        include_metrics: includeMetrics || false
      };

      this.client.GetPipelineInfo(request, (error: any, response: any) => {
        if (error) {
          reject(error);
          return;
        }

        if (response.status !== 'EXECUTION_STATUS_SUCCESS') {
          reject(new Error(response.error_message));
          return;
        }

        const result: any = {
          info: this.convertPipelineInfoFromProto(response.pipeline_info)
        };

        if (response.metrics) {
          result.metrics = {
            totalExecutions: response.metrics.total_executions,
            totalErrors: response.metrics.total_errors,
            averageExecutionTimeMs: response.metrics.average_execution_time_ms,
            lastExecutionTimestamp: response.metrics.last_execution_timestamp
          };
        }

        resolve(result);
      });
    });
  }

  /**
   * Execute a pipeline with input data
   */
  async executePipeline<T = any>(
    pipelineId: string,
    inputData: any,
    options?: ExecutePipelineOptions
  ): Promise<T> {
    if (!this.connected) {
      throw new Error('Not connected to service');
    }

    return new Promise((resolve, reject) => {
      const request = {
        pipeline_id: pipelineId,
        input_data: Buffer.from(JSON.stringify(inputData)),
        serialization_format: 'json',
        runtime_config: options?.runtimeConfig || {}
      };

      const deadline = options?.timeout
        ? Date.now() + options.timeout
        : undefined;

      this.client.ExecutePipeline(
        request,
        { deadline },
        (error: any, response: any) => {
          if (error) {
            reject(error);
            return;
          }

          if (response.status !== 'EXECUTION_STATUS_SUCCESS') {
            reject(new Error(response.error_message));
            return;
          }

          try {
            const result = JSON.parse(response.output_data.toString());
            resolve(result);
          } catch (parseError) {
            reject(new Error(`Failed to parse response: ${parseError}`));
          }
        }
      );
    });
  }

  /**
   * Stream data through a pipeline
   */
  streamPipeline(
    pipelineId: string,
    options?: StreamPipelineOptions
  ): PipelineStream {
    if (!this.connected) {
      throw new Error('Not connected to service');
    }

    const stream = new PipelineStream(
      this.client,
      pipelineId,
      options
    );

    return stream;
  }

  /**
   * Close the client connection
   */
  close(): void {
    if (this.connected) {
      this.connected = false;
      this.emit('disconnected');
    }
  }

  // Helper methods

  private convertDefinitionToProto(def: PipelineDefinition): any {
    return {
      name: def.name,
      nodes: def.nodes.map(n => ({
        node_id: n.nodeId,
        node_type: n.nodeType,
        config: n.config || {},
        is_remote: n.isRemote || false,
        remote_endpoint: n.remoteEndpoint || '',
        is_streaming: n.isStreaming || false,
        is_source: n.isSource || false,
        is_sink: n.isSink || false
      })),
      connections: (def.connections || []).map(c => ({
        from_node: c.fromNode,
        to_node: c.toNode,
        output_port: c.outputPort || 'default',
        input_port: c.inputPort || 'default'
      })),
      config: def.config || {},
      metadata: def.metadata || {}
    };
  }

  private convertPipelineInfoFromProto(info: any): PipelineInfo {
    return {
      pipelineId: info.pipeline_id,
      name: info.name,
      category: info.category,
      description: info.description,
      registeredTimestamp: parseInt(info.registered_timestamp),
      usageCount: info.usage_count,
      metadata: info.metadata || {}
    };
  }
}

/**
 * Pipeline streaming interface
 */
export class PipelineStream extends EventEmitter {
  private call: any;
  private sessionId?: string;
  private isReady: boolean = false;

  constructor(
    private client: any,
    private pipelineId: string,
    private options?: StreamPipelineOptions
  ) {
    super();
    this.initializeStream();
  }

  private initializeStream(): void {
    this.call = this.client.StreamPipeline();

    // Handle responses
    this.call.on('data', (response: any) => {
      if (response.ack) {
        this.sessionId = response.ack.session_id;
        this.isReady = response.ack.ready;
        this.emit('ready', this.sessionId);
      } else if (response.data) {
        try {
          const data = JSON.parse(response.data.toString());
          this.emit('data', data);
        } catch (error) {
          this.emit('error', new Error(`Failed to parse data: ${error}`));
        }
      } else if (response.error) {
        this.emit('error', new Error(response.error));
      } else if (response.status) {
        this.emit('status', {
          sessionId: response.status.session_id,
          itemsProcessed: response.status.items_processed,
          bytesProcessed: response.status.bytes_processed,
          isActive: response.status.is_active
        });
      }
    });

    this.call.on('error', (error: any) => {
      this.emit('error', error);
    });

    this.call.on('end', () => {
      this.emit('end');
    });

    // Send initialization message
    const initMessage = {
      init: {
        pipeline_id: this.pipelineId,
        serialization_format: 'json',
        runtime_config: this.options?.runtimeConfig || {},
        bidirectional: this.options?.bidirectional || false
      }
    };

    this.call.write(initMessage);
  }

  /**
   * Send data to the pipeline
   */
  async send(data: any): Promise<void> {
    if (!this.isReady) {
      throw new Error('Stream not ready');
    }

    const message = {
      data: Buffer.from(JSON.stringify(data))
    };

    this.call.write(message);
  }

  /**
   * Send control message
   */
  async control(type: 'PAUSE' | 'RESUME' | 'FLUSH' | 'CLOSE'): Promise<void> {
    const typeMap = {
      'PAUSE': 0,
      'RESUME': 1,
      'FLUSH': 2,
      'CLOSE': 3
    };

    const message = {
      control: {
        type: typeMap[type]
      }
    };

    this.call.write(message);

    if (type === 'CLOSE') {
      this.call.end();
    }
  }

  /**
   * Close the stream
   */
  async close(): Promise<void> {
    await this.control('CLOSE');
  }
}

/**
 * Pipeline builder for creating pipeline definitions
 */
export class PipelineBuilder {
  private definition: PipelineDefinition;

  constructor(name: string) {
    this.definition = {
      name,
      nodes: [],
      connections: [],
      config: {},
      metadata: {}
    };
  }

  /**
   * Add a node to the pipeline
   */
  addNode(node: NodeDefinition): PipelineBuilder {
    this.definition.nodes.push(node);
    
    // Auto-connect to previous node if linear pipeline
    if (this.definition.nodes.length > 1) {
      const prevNode = this.definition.nodes[this.definition.nodes.length - 2];
      this.connect(prevNode.nodeId, node.nodeId);
    }
    
    return this;
  }

  /**
   * Add a source node
   */
  addSource(nodeId: string, config?: Record<string, any>): PipelineBuilder {
    return this.addNode({
      nodeId,
      nodeType: 'DataSourceNode',
      config,
      isSource: true,
      isStreaming: true
    });
  }

  /**
   * Add a sink node
   */
  addSink(nodeId: string, config?: Record<string, any>): PipelineBuilder {
    return this.addNode({
      nodeId,
      nodeType: 'DataSinkNode',
      config,
      isSink: true
    });
  }

  /**
   * Connect two nodes
   */
  connect(
    fromNode: string,
    toNode: string,
    outputPort: string = 'default',
    inputPort: string = 'default'
  ): PipelineBuilder {
    this.definition.connections = this.definition.connections || [];
    this.definition.connections.push({
      fromNode,
      toNode,
      outputPort,
      inputPort
    });
    return this;
  }

  /**
   * Set pipeline configuration
   */
  setConfig(config: Record<string, string>): PipelineBuilder {
    this.definition.config = config;
    return this;
  }

  /**
   * Set pipeline metadata
   */
  setMetadata(metadata: Record<string, string>): PipelineBuilder {
    this.definition.metadata = metadata;
    return this;
  }

  /**
   * Build the pipeline definition
   */
  build(): PipelineDefinition {
    return this.definition;
  }
}

// Export for convenience
export default PipelineClient;