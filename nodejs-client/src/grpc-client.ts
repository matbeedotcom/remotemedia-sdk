/**
 * RemoteMedia TypeScript gRPC Client
 * 
 * Modern TypeScript client for the Rust gRPC service (003-rust-grpc-service).
 * Compatible with protocol version v1 (Phases 1-5).
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import * as path from 'path';

// ============================================================================
// Types and Interfaces
// ============================================================================

export enum AudioFormat {
  F32 = 'AUDIO_FORMAT_F32',
  I16 = 'AUDIO_FORMAT_I16',
  I32 = 'AUDIO_FORMAT_I32',
}

export enum ErrorType {
  VALIDATION = 'ERROR_TYPE_VALIDATION',
  NODE_EXECUTION = 'ERROR_TYPE_NODE_EXECUTION',
  RESOURCE_LIMIT = 'ERROR_TYPE_RESOURCE_LIMIT',
  AUTHENTICATION = 'ERROR_TYPE_AUTHENTICATION',
  VERSION_MISMATCH = 'ERROR_TYPE_VERSION_MISMATCH',
  INTERNAL = 'ERROR_TYPE_INTERNAL',
}

export interface AudioBuffer {
  samples: Buffer;
  sampleRate: number;
  channels: number;
  format: AudioFormat;
  numSamples: number;
}

export interface PipelineManifest {
  version: string;
  metadata: {
    name: string;
    description?: string;
    createdAt?: string;
  };
  nodes: Array<{
    id: string;
    nodeType: string;
    params: string;
    isStreaming?: boolean;
  }>;
  connections?: Array<{
    fromNode: string;
    fromOutput: string;
    toNode: string;
    toInput: string;
  }>;
}

export interface ExecutionMetrics {
  wallTimeMs: number;
  cpuTimeMs: number;
  memoryUsedBytes: number;
  nodeMetrics: Record<string, NodeMetrics>;
}

export interface NodeMetrics {
  executionTimeMs: number;
  memoryUsedBytes: number;
}

export interface ExecutionResult {
  audioOutputs: Record<string, AudioBuffer>;
  dataOutputs: Record<string, string>;
  metrics: ExecutionMetrics;
}

export interface VersionInfo {
  protocolVersion: string;
  runtimeVersion: string;
  supportedNodeTypes: string[];
  supportedProtocols: string[];
  buildTimestamp?: string;
}

export interface ChunkResult {
  sequence: number;
  processingTimeMs: number;
  totalSamplesProcessed: number;
  hasAudioOutput: boolean;
  audioOutput?: AudioBuffer;
}

export class RemoteMediaError extends Error {
  constructor(
    message: string,
    public errorType?: ErrorType,
    public failingNodeId?: string,
    public context?: Record<string, string>
  ) {
    super(message);
    this.name = 'RemoteMediaError';
  }
}

// ============================================================================
// Main Client Class
// ============================================================================

export class RemoteMediaClient {
  private client: any;
  private streamingClient: any;
  private packageDef: any;
  private address: string;
  private apiKey?: string;
  private connected: boolean = false;

  constructor(address: string = 'localhost:50051', apiKey?: string) {
    this.address = address;
    this.apiKey = apiKey;
  }

  async connect(): Promise<void> {
    if (this.connected) {
      return;
    }

    // Load proto files
    const PROTO_PATH = path.join(__dirname, '../../protos');
    
    const packageDefinition = protoLoader.loadSync(
      [
        path.join(PROTO_PATH, 'common.proto'),
        path.join(PROTO_PATH, 'execution.proto'),
        path.join(PROTO_PATH, 'streaming.proto'),
      ],
      {
        keepCase: true,
        longs: String,
        enums: String,
        defaults: true,
        oneofs: true,
        includeDirs: [PROTO_PATH],
      }
    );

    this.packageDef = grpc.loadPackageDefinition(packageDefinition);

    // Create clients
    const credentials = grpc.credentials.createInsecure();
    
    this.client = new this.packageDef.remotemedia.v1.PipelineExecutionService(
      this.address,
      credentials
    );

    this.streamingClient = new this.packageDef.remotemedia.v1.StreamingPipelineService(
      this.address,
      credentials
    );

    this.connected = true;
  }

  async disconnect(): Promise<void> {
    if (this.client) {
      grpc.closeClient(this.client);
    }
    if (this.streamingClient) {
      grpc.closeClient(this.streamingClient);
    }
    this.connected = false;
  }

  private getMetadata(): grpc.Metadata {
    const metadata = new grpc.Metadata();
    if (this.apiKey) {
      metadata.add('authorization', `Bearer ${this.apiKey}`);
    }
    return metadata;
  }

  async getVersion(): Promise<VersionInfo> {
    if (!this.connected) {
      await this.connect();
    }

    return new Promise((resolve, reject) => {
      this.client.GetVersion(
        { clientVersion: 'v1' },
        this.getMetadata(),
        (error: any, response: any) => {
          if (error) {
            reject(new RemoteMediaError(`Failed to get version: ${error.message}`));
            return;
          }

          resolve({
            protocolVersion: response.version_info.protocol_version,
            runtimeVersion: response.version_info.runtime_version,
            supportedNodeTypes: response.version_info.supported_node_types || [],
            supportedProtocols: response.version_info.supported_protocols || [],
            buildTimestamp: response.version_info.build_timestamp,
          });
        }
      );
    });
  }

  async executePipeline(
    manifest: PipelineManifest,
    audioInputs: Record<string, AudioBuffer> = {},
    dataInputs: Record<string, string> = {}
  ): Promise<ExecutionResult> {
    if (!this.connected) {
      await this.connect();
    }

    // Convert to proto format
    const request: any = {
      manifest: {
        version: manifest.version,
        metadata: {
          name: manifest.metadata.name,
          description: manifest.metadata.description || '',
          created_at: manifest.metadata.createdAt || new Date().toISOString(),
        },
        nodes: manifest.nodes.map(node => ({
          id: node.id,
          node_type: node.nodeType,
          params: node.params,
          is_streaming: node.isStreaming || false,
        })),
        connections: (manifest.connections || []).map(conn => ({
          from_node: conn.fromNode,
          from_output: conn.fromOutput,
          to_node: conn.toNode,
          to_input: conn.toInput,
        })),
      },
      audio_inputs: {},
      data_inputs: dataInputs,
      client_version: 'v1',
    };

    // Add audio inputs
    for (const [nodeId, audio] of Object.entries(audioInputs)) {
      request.audio_inputs[nodeId] = {
        samples: audio.samples,
        sample_rate: audio.sampleRate,
        channels: audio.channels,
        format: audio.format,
        num_samples: audio.numSamples,
      };
    }

    return new Promise((resolve, reject) => {
      this.client.ExecutePipeline(
        request,
        this.getMetadata(),
        (error: any, response: any) => {
          if (error) {
            reject(new RemoteMediaError(`Execution failed: ${error.message}`));
            return;
          }

          if (response.error) {
            reject(
              new RemoteMediaError(
                response.error.message,
                response.error.error_type as ErrorType,
                response.error.failing_node_id,
                response.error.context
              )
            );
            return;
          }

          const result = response.result;
          
          // Convert audio outputs
          const audioOutputs: Record<string, AudioBuffer> = {};
          if (result.audio_outputs) {
            for (const [nodeId, audio] of Object.entries(result.audio_outputs as any)) {
              audioOutputs[nodeId] = {
                samples: Buffer.from((audio as any).samples),
                sampleRate: (audio as any).sample_rate,
                channels: (audio as any).channels,
                format: (audio as any).format as AudioFormat,
                numSamples: (audio as any).num_samples,
              };
            }
          }

          // Convert node metrics
          const nodeMetrics: Record<string, NodeMetrics> = {};
          if (result.metrics?.node_metrics) {
            for (const [nodeId, metrics] of Object.entries(result.metrics.node_metrics as any)) {
              nodeMetrics[nodeId] = {
                executionTimeMs: (metrics as any).execution_time_ms,
                memoryUsedBytes: (metrics as any).memory_used_bytes,
              };
            }
          }

          resolve({
            audioOutputs,
            dataOutputs: result.data_outputs || {},
            metrics: {
              wallTimeMs: result.metrics.wall_time_ms,
              cpuTimeMs: result.metrics.cpu_time_ms,
              memoryUsedBytes: result.metrics.memory_used_bytes,
              nodeMetrics,
            },
          });
        }
      );
    });
  }

  async *streamPipeline(
    manifest: PipelineManifest,
    audioChunks: AsyncGenerator<[string, AudioBuffer, number]>
  ): AsyncGenerator<ChunkResult> {
    if (!this.connected) {
      await this.connect();
    }

    const stream = this.streamingClient.StreamPipeline(this.getMetadata());

    // Send init message
    stream.write({
      init: {
        manifest: {
          version: manifest.version,
          metadata: {
            name: manifest.metadata.name,
            description: manifest.metadata.description || '',
            created_at: manifest.metadata.createdAt || new Date().toISOString(),
          },
          nodes: manifest.nodes.map(node => ({
            id: node.id,
            node_type: node.nodeType,
            params: node.params,
            is_streaming: node.isStreaming || false,
          })),
          connections: (manifest.connections || []).map(conn => ({
            from_node: conn.fromNode,
            from_output: conn.fromOutput,
            to_node: conn.toNode,
            to_input: conn.toInput,
          })),
        },
        client_version: 'v1',
      },
    });

    // Set up response handling
    const results: ChunkResult[] = [];
    let resolveNext: ((value: ChunkResult) => void) | null = null;
    let streamEnded = false;

    stream.on('data', (response: any) => {
      if (response.chunk_result) {
        const result: ChunkResult = {
          sequence: response.chunk_result.sequence,
          processingTimeMs: response.chunk_result.processing_time_ms,
          totalSamplesProcessed: response.chunk_result.total_samples_processed,
          hasAudioOutput: response.chunk_result.has_audio_output,
        };

        if (response.chunk_result.audio_output) {
          result.audioOutput = {
            samples: Buffer.from(response.chunk_result.audio_output.samples),
            sampleRate: response.chunk_result.audio_output.sample_rate,
            channels: response.chunk_result.audio_output.channels,
            format: response.chunk_result.audio_output.format as AudioFormat,
            numSamples: response.chunk_result.audio_output.num_samples,
          };
        }

        if (resolveNext) {
          resolveNext(result);
          resolveNext = null;
        } else {
          results.push(result);
        }
      } else if (response.error) {
        throw new RemoteMediaError(
          response.error.message,
          response.error.error_type as ErrorType,
          response.error.failing_node_id,
          response.error.context
        );
      }
    });

    stream.on('end', () => {
      streamEnded = true;
    });

    // Send audio chunks
    (async () => {
      for await (const [nodeId, audio, sequence] of audioChunks) {
        stream.write({
          chunk: {
            node_id: nodeId,
            audio_data: {
              samples: audio.samples,
              sample_rate: audio.sampleRate,
              channels: audio.channels,
              format: audio.format,
              num_samples: audio.numSamples,
            },
            sequence,
          },
        });
      }

      // Send close command
      stream.write({
        chunk: {
          node_id: manifest.nodes[0].id,
          sequence: 999999,
          command: 'CHUNK_COMMAND_CLOSE',
        },
      });

      stream.end();
    })();

    // Yield results as they arrive
    while (!streamEnded || results.length > 0) {
      if (results.length > 0) {
        yield results.shift()!;
      } else {
        await new Promise<void>((resolve) => {
          resolveNext = (result: ChunkResult) => {
            results.push(result);
            resolve();
          };
        });
      }
    }
  }
}
