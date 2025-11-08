/**
 * Stream Handler
 *
 * Coordinates gRPC streaming with audio playback.
 * Handles the flow: gRPC stream → chunk buffering → audio player
 */

import type { TTSRequest } from '@/types/tts';
import type { AudioChunk, AudioFormat } from '@/types/audio';
import type { StreamResponse, StreamHandle } from './grpc-client';
import { TTSGRPCClient } from './grpc-client';
import { AudioPlayer } from './audio-player';
import { buildTTSPipelineFromRequest } from './tts-pipeline';
import { DEFAULT_AUDIO_FORMAT } from '@/types/audio';

/**
 * Stream handler events
 */
export enum StreamHandlerEvent {
  /** Stream initialized and ready */
  Ready = 'ready',
  /** Audio chunk received */
  ChunkReceived = 'chunk-received',
  /** Stream completed successfully */
  Completed = 'completed',
  /** Stream error occurred */
  Error = 'error',
  /** Stream cancelled */
  Cancelled = 'cancelled',
  /** Playback started */
  PlaybackStarted = 'playback-started',
  /** Buffer health changed */
  BufferHealthChanged = 'buffer-health-changed',
}

/**
 * Stream handler event data
 */
export type StreamHandlerEventData = {
  [StreamHandlerEvent.Ready]: { requestId: string };
  [StreamHandlerEvent.ChunkReceived]: {
    requestId: string;
    sequenceNumber: number;
    duration: number;
    totalChunks: number;
  };
  [StreamHandlerEvent.Completed]: { requestId: string; totalDuration: number };
  [StreamHandlerEvent.Error]: { requestId: string; error: Error };
  [StreamHandlerEvent.Cancelled]: { requestId: string };
  [StreamHandlerEvent.PlaybackStarted]: { requestId: string };
  [StreamHandlerEvent.BufferHealthChanged]: {
    requestId: string;
    status: string;
    bufferedAhead: number;
  };
};

/**
 * Event listener type
 */
type EventListener<E extends StreamHandlerEvent> = (
  data: StreamHandlerEventData[E]
) => void;

/**
 * Stream handler status
 */
export enum StreamHandlerStatus {
  Idle = 'idle',
  Connecting = 'connecting',
  Streaming = 'streaming',
  Completed = 'completed',
  Error = 'error',
  Cancelled = 'cancelled',
}

/**
 * Stream Handler
 *
 * Coordinates streaming TTS audio from gRPC service to audio player.
 */
/**
 * Backpressure configuration
 */
const BACKPRESSURE_CONFIG = {
  /** Maximum chunks to buffer before applying backpressure */
  MAX_BUFFER_CHUNKS: 100,
  /** Resume streaming when buffer drops below this */
  RESUME_THRESHOLD_CHUNKS: 50,
};

export class StreamHandler {
  private grpcClient: TTSGRPCClient;
  private audioPlayer: AudioPlayer;
  private streamHandle: StreamHandle | null = null;
  private currentRequest: TTSRequest | null = null;
  private status: StreamHandlerStatus = StreamHandlerStatus.Idle;
  private chunkCount: number = 0;
  private listeners: Map<StreamHandlerEvent, Set<EventListener<any>>> = new Map();
  private backpressureActive: boolean = false;

  constructor(grpcClient: TTSGRPCClient, audioPlayer: AudioPlayer) {
    this.grpcClient = grpcClient;
    this.audioPlayer = audioPlayer;

    // Listen to audio player events
    this.audioPlayer.on('playback-started', data => {
      this.emit(StreamHandlerEvent.PlaybackStarted, data);
    });

    this.audioPlayer.on('buffer-low', data => {
      this.emit(StreamHandlerEvent.BufferHealthChanged, {
        requestId: data.requestId,
        status: 'low',
        bufferedAhead: data.bufferedAhead,
      });
    });

    this.audioPlayer.on('buffer-critical', data => {
      this.emit(StreamHandlerEvent.BufferHealthChanged, {
        requestId: data.requestId,
        status: 'critical',
        bufferedAhead: data.bufferedAhead,
      });
    });

    this.audioPlayer.on('buffer-healthy', data => {
      this.emit(StreamHandlerEvent.BufferHealthChanged, {
        requestId: data.requestId,
        status: 'healthy',
        bufferedAhead: data.bufferedAhead,
      });
    });

    this.audioPlayer.on('playback-ended', data => {
      if (this.status === StreamHandlerStatus.Streaming) {
        this.status = StreamHandlerStatus.Completed;
        this.emit(StreamHandlerEvent.Completed, {
          requestId: data.requestId,
          totalDuration: data.duration,
        });
      }
    });
  }

  /**
   * Start streaming TTS
   *
   * @param request - TTS request
   * @param autoPlay - Automatically start playback when buffered (default: true)
   */
  async start(request: TTSRequest, autoPlay: boolean = true): Promise<void> {
    if (this.status === StreamHandlerStatus.Streaming) {
      throw new Error('Stream already active');
    }

    this.currentRequest = request;
    this.status = StreamHandlerStatus.Connecting;
    this.chunkCount = 0;

    console.log('[StreamHandler] Starting stream for request:', request.id);

    try {
      // Build pipeline manifest
      const manifest = buildTTSPipelineFromRequest(request);

      // Start gRPC stream
      this.streamHandle = await this.grpcClient.startTTSStream(
        manifest,
        this.handleStreamResponse.bind(this)
      );

      // Send text data to stream
      const textData = new TextEncoder().encode(request.text);
      this.streamHandle.send(textData);

      // Start audio player if autoPlay
      if (autoPlay) {
        await this.audioPlayer.play(request.id, DEFAULT_AUDIO_FORMAT);
      }

      this.status = StreamHandlerStatus.Streaming;

      console.log('[StreamHandler] Stream started');
    } catch (error) {
      this.status = StreamHandlerStatus.Error;
      this.emit(StreamHandlerEvent.Error, {
        requestId: request.id,
        error: error as Error,
      });
      throw error;
    }
  }

  /**
   * Cancel the active stream
   */
  cancel(): void {
    if (
      this.status !== StreamHandlerStatus.Streaming &&
      this.status !== StreamHandlerStatus.Connecting
    ) {
      return;
    }

    console.log('[StreamHandler] Cancelling stream');

    // Close gRPC stream
    if (this.streamHandle) {
      this.streamHandle.close();
      this.streamHandle = null;
    }

    // Stop audio playback
    this.audioPlayer.stop();

    this.status = StreamHandlerStatus.Cancelled;

    if (this.currentRequest) {
      this.emit(StreamHandlerEvent.Cancelled, { requestId: this.currentRequest.id });
    }
  }

  /**
   * Reset handler state
   */
  reset(): void {
    this.cancel();
    this.audioPlayer.reset();
    this.currentRequest = null;
    this.status = StreamHandlerStatus.Idle;
    this.chunkCount = 0;
  }

  /**
   * Get current status
   */
  getStatus(): StreamHandlerStatus {
    return this.status;
  }

  /**
   * Get current request
   */
  getCurrentRequest(): TTSRequest | null {
    return this.currentRequest;
  }

  /**
   * Handle stream response from gRPC
   */
  private handleStreamResponse(response: StreamResponse): void {
    if (!this.currentRequest) {
      console.error('[StreamHandler] Received response without active request');
      return;
    }

    try {
      switch (response.type) {
        case 'ready':
          console.log('[StreamHandler] Stream ready');
          this.emit(StreamHandlerEvent.Ready, { requestId: this.currentRequest.id });
          break;

        case 'data':
          this.handleDataChunk(response);
          break;

        case 'error':
          console.error('[StreamHandler] Stream error:', response.error);
          this.status = StreamHandlerStatus.Error;
          this.emit(StreamHandlerEvent.Error, {
            requestId: this.currentRequest.id,
            error: new Error(response.error?.message || 'Stream error'),
          });
          break;

        case 'closed':
          console.log('[StreamHandler] Stream closed by server');
          this.audioPlayer.markStreamComplete();
          // Let audio player handle completion event
          break;

        default:
          console.warn('[StreamHandler] Unknown response type:', response.type);
      }
    } catch (error) {
      console.error('[StreamHandler] Error handling response:', error);
      this.status = StreamHandlerStatus.Error;
      this.emit(StreamHandlerEvent.Error, {
        requestId: this.currentRequest.id,
        error: error as Error,
      });
    }
  }

  /**
   * Handle data chunk from stream
   */
  private handleDataChunk(response: StreamResponse): void {
    if (!response.audioData || !this.currentRequest) {
      return;
    }

    const sequenceNumber = response.sequence ?? this.chunkCount;
    const sampleRate = DEFAULT_AUDIO_FORMAT.sampleRate;
    const duration = response.audioData.length / sampleRate;

    const chunk: AudioChunk = {
      sequenceNumber,
      data: response.audioData,
      duration,
      receivedAt: new Date(),
      metadata: response.metadata,
    };

    // Add chunk to audio player
    this.audioPlayer.addChunk(chunk);

    this.chunkCount++;

    console.log('[StreamHandler] Received chunk', sequenceNumber, ':', duration.toFixed(3), 's');

    this.emit(StreamHandlerEvent.ChunkReceived, {
      requestId: this.currentRequest.id,
      sequenceNumber,
      duration,
      totalChunks: this.chunkCount,
    });

    // Backpressure handling: pause stream if buffer is too full
    this.checkAndApplyBackpressure();
  }

  /**
   * Check buffer size and apply backpressure if needed
   */
  private checkAndApplyBackpressure(): void {
    const bufferHealth = this.audioPlayer.getBufferHealth();

    // Calculate buffer size in chunks (approximate)
    const avgChunkDuration = 0.2; // Average ~200ms per chunk
    const bufferedChunks = Math.floor(bufferHealth.totalBuffered / avgChunkDuration);

    // Apply backpressure if buffer is too full
    if (
      !this.backpressureActive &&
      bufferedChunks >= BACKPRESSURE_CONFIG.MAX_BUFFER_CHUNKS
    ) {
      this.backpressureActive = true;
      console.warn(
        `[StreamHandler] Backpressure activated: ${bufferedChunks} chunks buffered (max: ${BACKPRESSURE_CONFIG.MAX_BUFFER_CHUNKS})`
      );
      // TODO: Pause gRPC stream when actual implementation supports it
      // this.streamHandle?.pause();
    }

    // Resume streaming if buffer has drained sufficiently
    if (
      this.backpressureActive &&
      bufferedChunks <= BACKPRESSURE_CONFIG.RESUME_THRESHOLD_CHUNKS
    ) {
      this.backpressureActive = false;
      console.log(
        `[StreamHandler] Backpressure released: ${bufferedChunks} chunks buffered (resume: ${BACKPRESSURE_CONFIG.RESUME_THRESHOLD_CHUNKS})`
      );
      // TODO: Resume gRPC stream when actual implementation supports it
      // this.streamHandle?.resume();
    }
  }

  // ============================================================================
  // Event Management
  // ============================================================================

  /**
   * Add event listener
   */
  on<E extends StreamHandlerEvent>(event: E, listener: EventListener<E>): void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(listener);
  }

  /**
   * Remove event listener
   */
  off<E extends StreamHandlerEvent>(event: E, listener: EventListener<E>): void {
    const listeners = this.listeners.get(event);
    if (listeners) {
      listeners.delete(listener);
    }
  }

  /**
   * Remove all event listeners
   */
  removeAllListeners(event?: StreamHandlerEvent): void {
    if (event) {
      this.listeners.delete(event);
    } else {
      this.listeners.clear();
    }
  }

  /**
   * Emit event
   */
  private emit<E extends StreamHandlerEvent>(
    event: E,
    data: StreamHandlerEventData[E]
  ): void {
    const listeners = this.listeners.get(event);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(data);
        } catch (e) {
          console.error('[StreamHandler] Error in event listener:', e);
        }
      });
    }
  }
}

/**
 * Create a configured stream handler instance
 */
export function createStreamHandler(
  grpcClient?: TTSGRPCClient,
  audioPlayer?: AudioPlayer
): StreamHandler {
  const client = grpcClient || new TTSGRPCClient();
  const player = audioPlayer || new AudioPlayer();
  return new StreamHandler(client, player);
}
