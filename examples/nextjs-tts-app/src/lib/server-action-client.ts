/**
 * Client wrapper for TTS Server Actions
 *
 * This provides a similar interface to the gRPC-Web client but uses
 * Next.js Server Actions under the hood.
 */

import { executeTTS, type TTSRequest, type TTSChunk } from '@/app/actions/tts';
import type { AudioChunk } from '@/types/audio';

export interface TTSServerActionClientConfig {
  // No config needed since Server Actions handle the connection
}

export interface StreamCallbacks {
  onReady?: () => void;
  onChunk?: (chunk: AudioChunk) => void;
  onComplete?: () => void;
  onError?: (error: Error) => void;
}

/**
 * TTS Client using Next.js Server Actions
 */
export class TTSServerActionClient {
  constructor(config?: TTSServerActionClientConfig) {
    console.log('[TTSServerActionClient] Initialized');
  }

  /**
   * Execute TTS and get complete audio
   */
  async executeTTS(
    text: string,
    voiceConfig: { language: string; voice: string; speed: number }
  ): Promise<AudioBuffer | null> {
    const request: TTSRequest = {
      text,
      language: voiceConfig.language,
      voice: voiceConfig.voice,
      speed: voiceConfig.speed,
    };

    console.log('[TTSServerActionClient] Executing TTS via Server Action...');

    const response = await executeTTS(request);

    if (!response.success || !response.audioData) {
      console.error('[TTSServerActionClient] TTS failed:', response.error);
      throw new Error(response.error || 'TTS execution failed');
    }

    // Decode base64 audio data
    const binaryString = atob(response.audioData);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }

    // Convert to Float32Array
    const float32Array = new Float32Array(bytes.buffer);

    console.log('[TTSServerActionClient] TTS completed, samples:', float32Array.length);

    // Return as Web Audio API AudioBuffer-compatible format
    // Note: We'll need to create an actual AudioBuffer in the AudioPlayer
    return {
      sampleRate: response.sampleRate || 24000,
      numberOfChannels: response.channels || 1,
      length: float32Array.length,
      duration: float32Array.length / (response.sampleRate || 24000),
      getChannelData: (channel: number) => float32Array,
      copyFromChannel: () => {},
      copyToChannel: () => {},
    } as unknown as AudioBuffer;
  }

  /**
   * Start TTS with streaming callbacks
   *
   * For now, this executes the full TTS and simulates streaming by
   * chunking the result. True streaming requires more complex setup.
   */
  async startTTSStream(
    text: string,
    voiceConfig: { language: string; voice: string; speed: number },
    callbacks: StreamCallbacks
  ): Promise<AbortController> {
    const abortController = new AbortController();

    // Execute in background
    (async () => {
      try {
        callbacks.onReady?.();

        const request: TTSRequest = {
          text,
          language: voiceConfig.language,
          voice: voiceConfig.voice,
          speed: voiceConfig.speed,
        };

        console.log('[TTSServerActionClient] Starting TTS stream via Server Action...');

        const response = await executeTTS(request);

        if (abortController.signal.aborted) {
          return;
        }

        if (!response.success || !response.audioData) {
          callbacks.onError?.(new Error(response.error || 'TTS execution failed'));
          return;
        }

        // Decode base64 audio data
        const binaryString = atob(response.audioData);
        const bytes = new Uint8Array(binaryString.length);
        for (let i = 0; i < binaryString.length; i++) {
          bytes[i] = binaryString.charCodeAt(i);
        }

        // Convert to Float32Array
        const float32Array = new Float32Array(bytes.buffer);

        // Simulate streaming by chunking the audio
        const chunkSize = 4800; // 200ms at 24kHz
        const numChunks = Math.ceil(float32Array.length / chunkSize);

        console.log('[TTSServerActionClient] Streaming', numChunks, 'chunks...');

        for (let i = 0; i < numChunks; i++) {
          if (abortController.signal.aborted) {
            return;
          }

          const start = i * chunkSize;
          const end = Math.min(start + chunkSize, float32Array.length);
          const chunkData = float32Array.slice(start, end);

          const chunk: AudioChunk = {
            sequenceNumber: i,
            data: chunkData,
            duration: chunkData.length / (response.sampleRate || 24000),
            receivedAt: new Date(),
            metadata: {
              sampleRate: response.sampleRate || 24000,
              channels: response.channels || 1,
            },
          };

          callbacks.onChunk?.(chunk);

          // Small delay to simulate streaming
          await new Promise(resolve => setTimeout(resolve, 10));
        }

        callbacks.onComplete?.();
        console.log('[TTSServerActionClient] Stream completed');

      } catch (error) {
        console.error('[TTSServerActionClient] Stream error:', error);
        callbacks.onError?.(error as Error);
      }
    })();

    return abortController;
  }

  /**
   * Create a pipeline manifest (compatibility method)
   */
  createTTSManifest(
    text: string,
    voiceConfig: { language: string; voice: string; speed: number }
  ) {
    return {
      version: 'v1',
      metadata: {
        name: 'tts-streaming',
        description: `TTS: ${text.substring(0, 50)}`,
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
   * Dispose (no-op for Server Actions)
   */
  dispose(): void {
    console.log('[TTSServerActionClient] Disposing');
  }
}
