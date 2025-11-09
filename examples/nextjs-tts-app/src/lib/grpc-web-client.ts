/**
 * Browser gRPC-Web Client for RemoteMedia TTS
 *
 * This client uses gRPC-Web protocol to communicate with the Rust gRPC server
 * from the browser. It wraps the generated protobuf-ts clients.
 */

import { GrpcWebFetchTransport } from '@protobuf-ts/grpcweb-transport';
import { StreamingPipelineServiceClient } from '@/generated/streaming.client';
import type { PipelineManifest, StreamRequest } from '@/generated/streaming';
import type { AudioBuffer as ProtoAudioBuffer } from '@/generated/common';
import type { AudioChunk } from '@/types/audio';

/**
 * Configuration for the gRPC-Web client
 */
export interface GrpcWebClientConfig {
  /** Base URL of the gRPC server (e.g., 'http://localhost:50051') */
  baseUrl: string;
  /** Optional API key for authentication */
  apiKey?: string;
  /** Request timeout in milliseconds */
  timeout?: number;
}

/**
 * Stream response handler callbacks
 */
export interface StreamCallbacks {
  onReady?: () => void;
  onChunk?: (chunk: AudioChunk) => void;
  onComplete?: () => void;
  onError?: (error: Error) => void;
}

/**
 * Browser-compatible gRPC-Web client for RemoteMedia TTS streaming
 */
export class TTSGrpcWebClient {
  private transport: GrpcWebFetchTransport;
  private client: StreamingPipelineServiceClient;
  private config: Required<GrpcWebClientConfig>;

  constructor(config: GrpcWebClientConfig) {
    this.config = {
      baseUrl: config.baseUrl || process.env.NEXT_PUBLIC_GRPC_HOST || 'http://localhost:50051',
      apiKey: config.apiKey,
      timeout: config.timeout || 30000,
    };

    // Create gRPC-Web transport
    this.transport = new GrpcWebFetchTransport({
      baseUrl: this.config.baseUrl,
      format: 'binary', // Use binary format for efficiency
      timeout: this.config.timeout,
    });

    // Create streaming client
    this.client = new StreamingPipelineServiceClient(this.transport);

    console.log(`[TTSGrpcWebClient] Initialized with baseUrl: ${this.config.baseUrl}`);
  }

  /**
   * Start a streaming TTS pipeline
   *
   * @param manifest - Pipeline manifest with TTS configuration
   * @param callbacks - Callbacks for stream events
   * @returns Abort controller to cancel the stream
   */
  async startTTSStream(
    manifest: PipelineManifest,
    callbacks: StreamCallbacks
  ): Promise<AbortController> {
    console.log('[TTSGrpcWebClient] Starting TTS stream...', manifest);

    // Create abort controller for cancellation
    const abortController = new AbortController();

    try {
      // Create init request
      const initRequest: StreamRequest = {
        message: {
          oneofKind: 'init',
          init: {
            manifest,
            clientVersion: 'v1',
            dataInputs: {}, // No data inputs for TTS
          },
        },
      };

      // Start bidirectional stream
      const call = this.client.streamPipeline({
        abort: abortController.signal,
      });

      // Handle responses
      (async () => {
        try {
          for await (const response of call.responses) {
            if (response.message.oneofKind === 'result') {
              const result = response.message.result;

              // Extract audio output
              if (result.audioOutputs) {
                const audioBuffers = Object.values(result.audioOutputs);
                if (audioBuffers.length > 0) {
                  const audioBuffer = audioBuffers[0] as ProtoAudioBuffer;

                  // Convert to AudioChunk
                  const chunk: AudioChunk = {
                    sequenceNumber: result.sequence,
                    data: this.convertToFloat32Array(audioBuffer.samples),
                    duration: audioBuffer.numSamples / audioBuffer.sampleRate,
                    receivedAt: new Date(),
                    metadata: {
                      sampleRate: audioBuffer.sampleRate,
                      channels: audioBuffer.channels,
                      format: audioBuffer.format,
                    },
                  };

                  callbacks.onChunk?.(chunk);
                }
              }
            } else if (response.message.oneofKind === 'error') {
              const error = new Error(response.message.error.message);
              callbacks.onError?.(error);
              break;
            }
          }

          // Stream completed successfully
          callbacks.onComplete?.();
        } catch (error) {
          console.error('[TTSGrpcWebClient] Stream error:', error);
          callbacks.onError?.(error as Error);
        }
      })();

      // Send init request
      await call.requests.send(initRequest);
      callbacks.onReady?.();

      // Close the request stream (server will process and respond)
      await call.requests.complete();

      return abortController;
    } catch (error) {
      console.error('[TTSGrpcWebClient] Failed to start stream:', error);
      callbacks.onError?.(error as Error);
      throw error;
    }
  }

  /**
   * Create a pipeline manifest for TTS
   */
  createTTSManifest(
    text: string,
    voiceConfig: { language: string; voice: string; speed: number }
  ): PipelineManifest {
    return {
      version: 'v1',
      metadata: {
        name: 'tts-streaming',
        description: `TTS: ${text.substring(0, 50)}${text.length > 50 ? '...' : ''}`,
        createdAt: new Date().toISOString(),
      },
      nodes: [
        {
          id: 'tts',
          nodeType: 'KokoroTTSNode',
          params: JSON.stringify({
            text,
            language: voiceConfig.language,
            voice: voiceConfig.voice,
            speed: voiceConfig.speed,
          }),
          isStreaming: true,
        },
      ],
      connections: [],
    };
  }

  /**
   * Convert Uint8Array to Float32Array
   * The samples are stored as bytes, need to convert to float32
   */
  private convertToFloat32Array(bytes: Uint8Array): Float32Array {
    // Create a Float32Array view of the byte buffer
    const float32Array = new Float32Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 4);
    return float32Array;
  }

  /**
   * Dispose of the client and cleanup resources
   */
  dispose(): void {
    console.log('[TTSGrpcWebClient] Disposing client');
    // Transport cleanup is automatic
  }
}

/**
 * Create a singleton instance of the gRPC-Web client
 */
let clientInstance: TTSGrpcWebClient | null = null;

export function getGrpcWebClient(config?: GrpcWebClientConfig): TTSGrpcWebClient {
  if (!clientInstance) {
    clientInstance = new TTSGrpcWebClient(config || {
      baseUrl: process.env.NEXT_PUBLIC_GRPC_HOST || 'http://localhost:50051',
    });
  }
  return clientInstance;
}

export function resetGrpcWebClient(): void {
  if (clientInstance) {
    clientInstance.dispose();
    clientInstance = null;
  }
}
