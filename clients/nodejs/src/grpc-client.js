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
// ES module compatibility: resolve __dirname
// In ES modules, we need to derive __dirname from import.meta.url
const __filename = fileURLToPath(import.meta.url);
const __dirname_resolved = dirname(__filename);
// ============================================================================
// Types and Interfaces
// ============================================================================
export var AudioFormat;
(function (AudioFormat) {
    AudioFormat["F32"] = "AUDIO_FORMAT_F32";
    AudioFormat["I16"] = "AUDIO_FORMAT_I16";
    AudioFormat["I32"] = "AUDIO_FORMAT_I32";
})(AudioFormat || (AudioFormat = {}));
export var ErrorType;
(function (ErrorType) {
    ErrorType["VALIDATION"] = "ERROR_TYPE_VALIDATION";
    ErrorType["NODE_EXECUTION"] = "ERROR_TYPE_NODE_EXECUTION";
    ErrorType["RESOURCE_LIMIT"] = "ERROR_TYPE_RESOURCE_LIMIT";
    ErrorType["AUTHENTICATION"] = "ERROR_TYPE_AUTHENTICATION";
    ErrorType["VERSION_MISMATCH"] = "ERROR_TYPE_VERSION_MISMATCH";
    ErrorType["INTERNAL"] = "ERROR_TYPE_INTERNAL";
})(ErrorType || (ErrorType = {}));
export class RemoteMediaError extends Error {
    constructor(message, errorType, failingNodeId, context) {
        super(message);
        this.errorType = errorType;
        this.failingNodeId = failingNodeId;
        this.context = context;
        this.name = 'RemoteMediaError';
    }
}
// ============================================================================
// Main Client Class
// ============================================================================
export class RemoteMediaClient {
    constructor(address = 'localhost:50051', apiKey) {
        this.connected = false;
        this.address = address;
        this.apiKey = apiKey;
    }
    async connect() {
        if (this.connected) {
            return;
        }
        // Load proto files - try multiple possible locations
        const possibleProtoPaths = [
            path.join(__dirname_resolved, '../protos'),
            path.join(__dirname_resolved, '../../protos'),
            path.join(process.cwd(), 'nodejs-client/protos'),
            path.join(process.cwd(), 'runtime/protos'),
        ];
        let PROTO_PATH = possibleProtoPaths[0];
        for (const tryPath of possibleProtoPaths) {
            if (fs.existsSync(path.join(tryPath, 'common.proto'))) {
                PROTO_PATH = tryPath;
                break;
            }
        }
        const packageDefinition = protoLoader.loadSync([
            path.join(PROTO_PATH, 'common.proto'),
            path.join(PROTO_PATH, 'execution.proto'),
            path.join(PROTO_PATH, 'streaming.proto'),
        ], {
            keepCase: true,
            longs: String,
            enums: String,
            defaults: true,
            oneofs: true,
            includeDirs: [PROTO_PATH],
        });
        this.packageDef = grpc.loadPackageDefinition(packageDefinition);
        // Create clients
        const credentials = grpc.credentials.createInsecure();
        this.client = new this.packageDef.remotemedia.v1.PipelineExecutionService(this.address, credentials);
        this.streamingClient = new this.packageDef.remotemedia.v1.StreamingPipelineService(this.address, credentials);
        this.connected = true;
    }
    async disconnect() {
        if (this.client) {
            grpc.closeClient(this.client);
        }
        if (this.streamingClient) {
            grpc.closeClient(this.streamingClient);
        }
        this.connected = false;
    }
    getMetadata() {
        const metadata = new grpc.Metadata();
        if (this.apiKey) {
            metadata.add('authorization', `Bearer ${this.apiKey}`);
        }
        return metadata;
    }
    async getVersion() {
        if (!this.connected) {
            await this.connect();
        }
        return new Promise((resolve, reject) => {
            this.client.GetVersion({ clientVersion: 'v1' }, this.getMetadata(), (error, response) => {
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
            });
        });
    }
    async executePipeline(manifest, audioInputs = {}, dataInputs = {}) {
        if (!this.connected) {
            await this.connect();
        }
        // Convert to proto format
        const request = {
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
            this.client.ExecutePipeline(request, this.getMetadata(), (error, response) => {
                if (error) {
                    reject(new RemoteMediaError(`Execution failed: ${error.message}`));
                    return;
                }
                if (response.error) {
                    reject(new RemoteMediaError(response.error.message, response.error.error_type, response.error.failing_node_id, response.error.context));
                    return;
                }
                const result = response.result;
                // Convert audio outputs
                const audioOutputs = {};
                if (result.audio_outputs) {
                    for (const [nodeId, audio] of Object.entries(result.audio_outputs)) {
                        audioOutputs[nodeId] = {
                            samples: Buffer.from(audio.samples),
                            sampleRate: audio.sample_rate,
                            channels: audio.channels,
                            format: audio.format,
                            numSamples: audio.num_samples,
                        };
                    }
                }
                // Convert node metrics
                const nodeMetrics = {};
                if (result.metrics?.node_metrics) {
                    for (const [nodeId, metrics] of Object.entries(result.metrics.node_metrics)) {
                        nodeMetrics[nodeId] = {
                            executionTimeMs: metrics.execution_time_ms,
                            memoryUsedBytes: metrics.memory_used_bytes,
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
            });
        });
    }
    async *streamPipeline(manifest, audioChunks) {
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
        const results = [];
        let resolveNext = null;
        let streamEnded = false;
        stream.on('data', (response) => {
            if (response.chunk_result) {
                const result = {
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
                        format: response.chunk_result.audio_output.format,
                        numSamples: response.chunk_result.audio_output.num_samples,
                    };
                }
                if (resolveNext) {
                    resolveNext(result);
                    resolveNext = null;
                }
                else {
                    results.push(result);
                }
            }
            else if (response.error) {
                throw new RemoteMediaError(response.error.message, response.error.error_type, response.error.failing_node_id, response.error.context);
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
                yield results.shift();
            }
            else {
                await new Promise((resolve) => {
                    resolveNext = (result) => {
                        results.push(result);
                        resolve();
                    };
                });
            }
        }
    }
}
//# sourceMappingURL=grpc-client.js.map