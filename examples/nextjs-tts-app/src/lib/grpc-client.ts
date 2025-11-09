/**
 * gRPC Client Wrapper for Browser
 *
 * Provides a browser-compatible interface to the RemoteMedia gRPC service
 * for TTS streaming operations.
 */

import type { PipelineManifest } from '../../../../nodejs-client/src/grpc-client.js';
import type { AudioChunk, AudioFormat } from '@/types/audio';

/**
 * gRPC streaming response from server
 */
export interface StreamResponse {
  /** Response type */
  type: 'ready' | 'data' | 'error' | 'closed';
  /** Sequence number for data chunks */
  sequence?: number;
  /** Audio data (Float32Array for PCM) */
  audioData?: Float32Array;
  /** Error information */
  error?: {
    code: string;
    message: string;
    details?: Record<string, unknown>;
  };
  /** Metadata */
  metadata?: Record<string, unknown>;
}

/**
 * gRPC Client Configuration
 */
export interface GRPCClientConfig {
  /** gRPC server hostname */
  host: string;
  /** gRPC server port */
  port: number;
  /** Enable TLS/SSL */
  ssl: boolean;
  /** Optional API key for authentication */
  apiKey?: string;
  /** Connection timeout in milliseconds */
  timeout?: number;
}

/**
 * Stream control handle
 */
export interface StreamHandle {
  /** Send data to the stream */
  send: (data: Uint8Array) => void;
  /** Close the stream */
  close: () => void;
  /** Get current stream status */
  getStatus: () => 'connecting' | 'ready' | 'streaming' | 'closed' | 'error';
}

/**
 * Browser-compatible gRPC Client for RemoteMedia TTS
 *
 * This is a lightweight wrapper that will connect to the Rust gRPC service
 * via a gRPC-Web proxy or direct gRPC connection (depending on deployment).
 *
 * For Phase 2, this provides the interface. Full implementation will be
 * completed in Phase 3 when integrating with the actual gRPC service.
 */
export class TTSGRPCClient {
  private config: GRPCClientConfig;
  private connected: boolean = false;
  private connectionPromise: Promise<void> | null = null;

  constructor(config: Partial<GRPCClientConfig> = {}) {
    this.config = {
      host: config.host || process.env.NEXT_PUBLIC_GRPC_HOST || 'localhost',
      port: config.port || parseInt(process.env.NEXT_PUBLIC_GRPC_PORT || '50051', 10),
      ssl: config.ssl ?? (process.env.NEXT_PUBLIC_GRPC_SSL === 'true'),
      apiKey: config.apiKey,
      timeout: config.timeout || 30000,
    };
  }

  /**
   * Connect to the gRPC service
   */
  async connect(): Promise<void> {
    if (this.connected) {
      return;
    }

    if (this.connectionPromise) {
      return this.connectionPromise;
    }

    this.connectionPromise = this._doConnect();
    return this.connectionPromise;
  }

  private async _doConnect(): Promise<void> {
    // TODO: Phase 3 - Implement actual gRPC connection
    // For now, validate configuration
    const address = `${this.config.ssl ? 'https' : 'http'}://${this.config.host}:${this.config.port}`;

    console.log(`[TTSGRPCClient] Connecting to ${address}...`);

    // Validate configuration
    if (!this.config.host || !this.config.port) {
      throw new Error('Invalid gRPC configuration: host and port are required');
    }

    // Mark as connected (actual connection will be established in Phase 3)
    this.connected = true;
    console.log(`[TTSGRPCClient] Connected to ${address}`);
  }

  /**
   * Disconnect from the gRPC service
   */
  async disconnect(): Promise<void> {
    if (!this.connected) {
      return;
    }

    // TODO: Phase 3 - Implement actual disconnection
    console.log('[TTSGRPCClient] Disconnecting...');
    this.connected = false;
    this.connectionPromise = null;
  }

  /**
   * Check if client is connected
   */
  isConnected(): boolean {
    return this.connected;
  }

  /**
   * Get server version and capabilities
   */
  async getVersion(): Promise<{
    protocolVersion: string;
    runtimeVersion: string;
    supportedNodeTypes: string[];
  }> {
    if (!this.connected) {
      await this.connect();
    }

    // TODO: Phase 3 - Implement actual GetVersion RPC
    return {
      protocolVersion: 'v1',
      runtimeVersion: '0.2.0',
      supportedNodeTypes: ['KokoroTTSNode', 'AudioInputNode', 'AudioOutputNode'],
    };
  }

  /**
   * Start a streaming TTS pipeline
   *
   * @param manifest - Pipeline manifest with TTS configuration
   * @param onResponse - Callback for each stream response
   * @returns Stream handle for controlling the stream
   */
  async startTTSStream(
    manifest: PipelineManifest,
    onResponse: (response: StreamResponse) => void
  ): Promise<StreamHandle> {
    if (!this.connected) {
      await this.connect();
    }

    console.log('[TTSGRPCClient] Starting TTS stream...', manifest);

    // TODO: Phase 3 - Implement actual streaming
    // For now, return a mock handle
    let status: 'connecting' | 'ready' | 'streaming' | 'closed' | 'error' = 'connecting';

    // Simulate connection
    setTimeout(() => {
      status = 'ready';
      onResponse({ type: 'ready' });
    }, 100);

    const handle: StreamHandle = {
      send: (data: Uint8Array) => {
        console.log('[TTSGRPCClient] Sending data chunk:', data.length, 'bytes');
        status = 'streaming';
        // TODO: Phase 3 - Send to actual gRPC stream
      },
      close: () => {
        console.log('[TTSGRPCClient] Closing stream');
        status = 'closed';
        onResponse({ type: 'closed' });
      },
      getStatus: () => status,
    };

    return handle;
  }

  /**
   * Helper: Create a pipeline manifest for TTS
   *
   * @param text - Text to synthesize
   * @param voiceConfig - Voice configuration (language, voice, speed)
   * @returns Pipeline manifest
   */
  createTTSManifest(
    text: string,
    voiceConfig: { language: string; voice: string; speed: number }
  ): PipelineManifest {
    // Create manifest based on contracts/tts-streaming-protocol.md
    const manifest: PipelineManifest = {
      version: 'v1',
      metadata: {
        name: 'tts-streaming',
        description: 'Real-time text-to-speech synthesis',
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

    return manifest;
  }

  /**
   * Helper: Convert audio format from server to client format
   */
  convertAudioFormat(serverFormat: string): AudioFormat {
    // Map server audio format to client AudioFormat type
    const formatMap: Record<string, AudioFormat> = {
      AUDIO_FORMAT_F32: {
        sampleRate: 24000,
        channels: 1,
        bitDepth: 32,
        encoding: 'pcm_f32le',
      },
      AUDIO_FORMAT_I16: {
        sampleRate: 24000,
        channels: 1,
        bitDepth: 16,
        encoding: 'pcm_s16le',
      },
    };

    return formatMap[serverFormat] || formatMap.AUDIO_FORMAT_F32;
  }

  /**
   * Helper: Parse audio chunk from server response
   */
  parseAudioChunk(
    response: StreamResponse,
    sequenceNumber: number
  ): AudioChunk | null {
    if (response.type !== 'data' || !response.audioData) {
      return null;
    }

    // Calculate duration (samples / sample_rate)
    const sampleRate = 24000; // Kokoro TTS output
    const duration = response.audioData.length / sampleRate;

    return {
      sequenceNumber,
      data: response.audioData,
      duration,
      receivedAt: new Date(),
      metadata: response.metadata,
    };
  }
}

/**
 * Singleton instance for the application
 */
let clientInstance: TTSGRPCClient | null = null;

/**
 * Get or create the global gRPC client instance
 */
export function getGRPCClient(config?: Partial<GRPCClientConfig>): TTSGRPCClient {
  if (!clientInstance) {
    clientInstance = new TTSGRPCClient(config);
  }
  return clientInstance;
}

/**
 * Reset the global client instance (useful for testing)
 */
export function resetGRPCClient(): void {
  if (clientInstance) {
    clientInstance.disconnect();
    clientInstance = null;
  }
}
