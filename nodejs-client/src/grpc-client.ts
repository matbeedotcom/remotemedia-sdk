/**
 * RemoteMedia TypeScript gRPC Client
 * 
 * Modern TypeScript client for the Rust gRPC service (003-rust-grpc-service).
 * Compatible with protocol version v1 (Phases 1-5).
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import * as path from 'path';
import * as fs from 'fs';
import { fileURLToPath } from 'url';
import { dirname } from 'path';
import { DataBuffer } from './data-types';

// ES module compatibility: resolve __dirname
// In ES modules, we need to derive __dirname from import.meta.url
const __filename = fileURLToPath(import.meta.url);
const __dirname_resolved = dirname(__filename);

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
    from: string;
    to: string;
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

export interface StreamMetrics {
  sessionId: string;
  chunksProcessed: number;
  averageLatencyMs: number;
  totalItems: number;
  bufferItems: number;
  chunksDropped: number;
  peakMemoryBytes: number;
  dataTypeBreakdown: Record<string, number>;
  cacheHits: number;
  cacheMisses: number;
  cachedNodesCount: number;
  cacheHitRate: number;
}

export interface ChunkResult {
  sequence: number;
  processingTimeMs: number;
  totalSamplesProcessed: number;
  hasAudioOutput: boolean;
  audioOutput?: AudioBuffer;
  metrics?: StreamMetrics;
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

    // Load proto files - try multiple possible locations
    const possibleProtoPaths = [
      path.join(__dirname_resolved, '../protos'),
      path.join(__dirname_resolved, '../../protos'),
      path.join(__dirname_resolved, '../../runtime/protos'),
      path.join(process.cwd(), 'nodejs-client/protos'),
      path.join(process.cwd(), 'runtime/protos'),
      path.join(process.cwd(), '../../runtime/protos'),
      path.join(process.cwd(), '../../../runtime/protos'),
    ];

    let PROTO_PATH = possibleProtoPaths[0];
    for (const tryPath of possibleProtoPaths) {
      if (fs.existsSync(path.join(tryPath, 'common.proto'))) {
        PROTO_PATH = tryPath;
        console.log(`[RemoteMediaClient] Found protos at: ${PROTO_PATH}`);
        break;
      }
    }

    // Final check - if still not found, throw helpful error
    if (!fs.existsSync(path.join(PROTO_PATH, 'common.proto'))) {
      throw new Error(
        `Proto files not found. Tried paths:\n${possibleProtoPaths.map(p => `  - ${p}`).join('\n')}\n` +
        `Current working directory: ${process.cwd()}\n` +
        `Resolved __dirname: ${__dirname_resolved}`
      );
    }
    
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
          from: conn.from,
          to: conn.to,
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
    dataChunks: AsyncGenerator<[string, DataBuffer, number]>
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
            from: conn.from,
            to: conn.to,
          })),
        },
        client_version: 'v1',
      },
    });

    // Queue for results
    const results: ChunkResult[] = [];
    let resolveNext: ((value: IteratorResult<ChunkResult>) => void) | null = null;
    let rejectNext: ((error: Error) => void) | null = null;
    let streamEnded = false;
    let streamError: Error | null = null;

    // Store latest metrics to attach to next chunk result
    let latestMetrics: StreamMetrics | undefined;

    // Handle incoming responses
    stream.on('data', (response: any) => {
      if (response.result) {
        const hasDataOutput = response.result.data_outputs && Object.keys(response.result.data_outputs).length > 0;

        const chunkResult: ChunkResult = {
          sequence: response.result.sequence,
          processingTimeMs: response.result.processing_time_ms,
          totalSamplesProcessed: response.result.total_items_processed || response.result.total_samples_processed || 0,
          hasAudioOutput: hasDataOutput,
          metrics: latestMetrics, // Attach latest metrics if available
        };

        if (hasDataOutput) {
          // Get the first data output (assuming single node for now)
          const firstOutput = Object.values(response.result.data_outputs)[0] as any;
          console.log('[gRPC Client] Processing data output with type:', firstOutput?.data_type);

          // The protobuf oneof field puts the variant directly on the object
          // Structure: { metadata, audio/video/text/etc, data_type: "audio/video/text/etc" }
          if (firstOutput?.audio) {
            const audioData = firstOutput.audio;
            console.log('[gRPC Client] Found audio:', audioData.num_samples, 'samples,', audioData.sample_rate, 'Hz');
            chunkResult.audioOutput = {
              samples: Buffer.from(audioData.samples),
              sampleRate: audioData.sample_rate,
              channels: audioData.channels,
              format: audioData.format as AudioFormat,
              numSamples: parseInt(audioData.num_samples) || audioData.num_samples,
            };
          } else if (firstOutput?.text) {
            console.log('[gRPC Client] Found text data');
            // Could add text handling here if needed
          } else {
            console.log('[gRPC Client] Unknown data type:', firstOutput?.data_type);
          }
        }

        if (resolveNext) {
          resolveNext({ value: chunkResult, done: false });
          resolveNext = null;
          rejectNext = null;
        } else {
          results.push(chunkResult);
        }
      } else if (response.metrics) {
        // Handle StreamMetrics message
        console.log('[gRPC Client] Received StreamMetrics:', response.metrics);
        latestMetrics = {
          sessionId: response.metrics.session_id || '',
          chunksProcessed: parseInt(response.metrics.chunks_processed) || 0,
          averageLatencyMs: response.metrics.average_latency_ms || 0,
          totalItems: parseInt(response.metrics.total_items) || 0,
          bufferItems: parseInt(response.metrics.buffer_items) || 0,
          chunksDropped: parseInt(response.metrics.chunks_dropped) || 0,
          peakMemoryBytes: parseInt(response.metrics.peak_memory_bytes) || 0,
          dataTypeBreakdown: response.metrics.data_type_breakdown || {},
          cacheHits: parseInt(response.metrics.cache_hits) || 0,
          cacheMisses: parseInt(response.metrics.cache_misses) || 0,
          cachedNodesCount: parseInt(response.metrics.cached_nodes_count) || 0,
          cacheHitRate: response.metrics.cache_hit_rate || 0,
        };
      } else if (response.error) {
        const error = new RemoteMediaError(
          response.error.message,
          response.error.error_type as ErrorType,
          response.error.failing_node_id,
          response.error.context
        );
        if (rejectNext) {
          rejectNext(error);
          resolveNext = null;
          rejectNext = null;
        } else {
          streamError = error;
        }
      }
    });

    stream.on('error', (error: Error) => {
      streamError = error;
      if (rejectNext) {
        rejectNext(error);
        resolveNext = null;
        rejectNext = null;
      }
    });

    stream.on('end', () => {
      streamEnded = true;
      if (resolveNext) {
        resolveNext({ value: undefined, done: true });
        resolveNext = null;
        rejectNext = null;
      }
    });

    // Send data chunks in background
    (async () => {
      try {
        for await (const [nodeId, dataBuffer, sequence] of dataChunks) {
          // Convert DataBuffer to protobuf format
          let protoDataType: any;

          switch (dataBuffer.type) {
            case 'audio':
              protoDataType = {
                audio: {
                  samples: dataBuffer.data.samples,
                  sample_rate: dataBuffer.data.sampleRate,
                  channels: dataBuffer.data.channels,
                  format: dataBuffer.data.format,
                  num_samples: dataBuffer.data.numSamples,
                },
              };
              break;
            case 'video':
              protoDataType = {
                video: {
                  pixel_data: dataBuffer.data.pixelData,
                  width: dataBuffer.data.width,
                  height: dataBuffer.data.height,
                  format: dataBuffer.data.format,
                  frame_number: dataBuffer.data.frameNumber,
                  timestamp_us: dataBuffer.data.timestampUs,
                },
              };
              break;
            case 'tensor':
              protoDataType = {
                tensor: {
                  data: dataBuffer.data.data,
                  shape: dataBuffer.data.shape,
                  dtype: dataBuffer.data.dtype,
                  layout: dataBuffer.data.layout || '',
                },
              };
              break;
            case 'json':
              protoDataType = {
                json: {
                  json_payload: dataBuffer.data.jsonPayload,
                  schema_type: dataBuffer.data.schemaType || '',
                },
              };
              break;
            case 'text':
              protoDataType = {
                text: {
                  text_data: dataBuffer.data.textData,
                  encoding: dataBuffer.data.encoding || 'utf-8',
                  language: dataBuffer.data.language || '',
                },
              };
              break;
            case 'binary':
              protoDataType = {
                binary: {
                  data: dataBuffer.data.data,
                  mime_type: dataBuffer.data.mimeType || 'application/octet-stream',
                },
              };
              break;
          }

          stream.write({
            data_chunk: {
              node_id: nodeId,
              buffer: {
                ...protoDataType,
                metadata: dataBuffer.metadata || {},
              },
              sequence,
            },
          });
        }

        // Send close command
        stream.write({
          control: {
            command: 1, // COMMAND_CLOSE
          },
        });

        stream.end();
      } catch (error) {
        stream.destroy(error as Error);
      }
    })();

    // Yield results as they arrive
    while (!streamEnded || results.length > 0) {
      if (streamError) {
        throw streamError;
      }

      if (results.length > 0) {
        yield results.shift()!;
      } else if (!streamEnded) {
        const result = await new Promise<IteratorResult<ChunkResult>>((resolve, reject) => {
          resolveNext = resolve;
          rejectNext = reject;
        });

        if (!result.done && result.value) {
          yield result.value;
        }
      }
    }
  }
}
