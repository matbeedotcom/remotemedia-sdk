/**
 * Server Actions for TTS Streaming
 *
 * These actions run on the Next.js server and use the Node.js gRPC client
 * to communicate with the Rust gRPC server. Results are streamed back to
 * the browser via Server-Sent Events or similar.
 */

'use server';

import { RemoteMediaClient } from '../../../../../nodejs-client/src/grpc-client';

/**
 * TTS request parameters
 */
export interface TTSRequest {
  text: string;
  language: string;
  voice: string;
  speed: number;
}

/**
 * Audio chunk response
 */
export interface TTSChunk {
  sequenceNumber: number;
  audioData: number[]; // Float32Array as array for serialization
  sampleRate: number;
  channels: number;
  duration: number;
}

/**
 * Execute TTS and return audio chunks
 *
 * This server action connects to the gRPC server using the Node.js client
 * and returns the complete audio as base64-encoded data.
 */
export async function executeTTS(request: TTSRequest): Promise<{
  success: boolean;
  audioData?: string; // Base64-encoded audio
  sampleRate?: number;
  channels?: number;
  error?: string;
}> {
  const client = new RemoteMediaClient(
    process.env.GRPC_HOST || 'localhost:50051'
  );

  try {
    await client.connect();
    console.log('[Server Action] Connected to gRPC server');

    // Create pipeline manifest for TTS
    const manifest = {
      version: 'v1',
      metadata: {
        name: 'tts-streaming',
        description: `TTS: ${request.text.substring(0, 50)}`,
        createdAt: new Date().toISOString(),
      },
      nodes: [
        {
          id: 'tts',
          nodeType: 'KokoroTTSNode',
          params: JSON.stringify({
            text: request.text,
            language: request.language,
            voice: request.voice,
            speed: request.speed,
          }),
          isStreaming: false, // Use non-streaming for now
        },
      ],
      connections: [],
    };

    console.log('[Server Action] Executing TTS pipeline...');
    const result = await client.executePipeline(manifest);

    // Extract audio output
    const audioOutputs = Object.values(result.audioOutputs);
    if (audioOutputs.length === 0) {
      return {
        success: false,
        error: 'No audio output from TTS pipeline',
      };
    }

    const audioBuffer = audioOutputs[0];

    // Convert Buffer to base64 for transport to client
    const audioBase64 = audioBuffer.samples.toString('base64');

    console.log('[Server Action] TTS completed successfully');
    await client.disconnect();

    return {
      success: true,
      audioData: audioBase64,
      sampleRate: audioBuffer.sampleRate,
      channels: audioBuffer.channels,
    };
  } catch (error) {
    console.error('[Server Action] TTS error:', error);
    await client.disconnect();

    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}

/**
 * Stream TTS audio chunks
 *
 * This uses an async generator to stream chunks back to the client.
 * Note: Next.js Server Actions with streaming require special handling.
 */
export async function* streamTTS(request: TTSRequest): AsyncGenerator<TTSChunk, void, unknown> {
  const client = new RemoteMediaClient(
    process.env.GRPC_HOST || 'localhost:50051'
  );

  try {
    await client.connect();
    console.log('[Server Action] Connected to gRPC server for streaming');

    // Create pipeline manifest for TTS
    const manifest = {
      version: 'v1',
      metadata: {
        name: 'tts-streaming',
        description: `TTS: ${request.text.substring(0, 50)}`,
        createdAt: new Date().toISOString(),
      },
      nodes: [
        {
          id: 'tts',
          nodeType: 'SimplePyTorchNode',
          params: JSON.stringify({
            text: request.text,
            language: request.language,
            voice: request.voice,
            speed: request.speed,
          }),
          isStreaming: true,
        },
      ],
      connections: [],
    };

    console.log('[Server Action] Starting TTS streaming...');

    // For streaming, we need to use a different approach
    // Create an empty async generator that yields audio chunks
    async function* emptyAudioGenerator() {
      // No input audio needed for TTS
      yield ['tts', { samples: Buffer.alloc(0), sampleRate: 24000, channels: 1, format: 'AUDIO_FORMAT_F32', numSamples: 0 }, 0] as const;
    }

    // Stream the pipeline
    let sequence = 0;
    for await (const chunk of client.streamPipeline(manifest, emptyAudioGenerator())) {
      if (chunk.audioOutput) {
        // Convert Float32Array to regular array for JSON serialization
        const audioArray = Array.from(new Float32Array(
          chunk.audioOutput.samples.buffer,
          chunk.audioOutput.samples.byteOffset,
          chunk.audioOutput.samples.byteLength / 4
        ));

        yield {
          sequenceNumber: sequence++,
          audioData: audioArray,
          sampleRate: chunk.audioOutput.sampleRate,
          channels: chunk.audioOutput.channels,
          duration: chunk.audioOutput.numSamples / chunk.audioOutput.sampleRate,
        };
      }
    }

    console.log('[Server Action] TTS streaming completed');
    await client.disconnect();
  } catch (error) {
    console.error('[Server Action] TTS streaming error:', error);
    await client.disconnect();
    throw error;
  }
}
