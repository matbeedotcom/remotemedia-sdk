/**
 * TTS Streaming API Route
 *
 * This route handler connects to the Rust gRPC server using the Node.js client
 * and streams raw audio data back to the browser for direct playback.
 *
 * Uses a persistent gRPC connection pool to enable server-side node caching,
 * dramatically reducing latency for subsequent requests.
 */

import clientPool from '@/lib/grpc-client-pool';
import { NextRequest, NextResponse } from 'next/server';

export const runtime = 'nodejs'; // Use Node.js runtime for gRPC
export const dynamic = 'force-dynamic';

/**
 * POST /api/tts/stream
 *
 * Request body:
 * {
 *   "text": "Hello world",
 *   "language": "en-us",
 *   "voice": "af_bella",
 *   "speed": 1.0
 * }
 *
 * Response: Binary audio stream (PCM float32)
 */
export async function POST(request: NextRequest) {
  // Parse request body
  let body;
  try {
    body = await request.json();
  } catch (error) {
    return new Response('Invalid JSON body', { status: 400 });
  }

  const { text, language, voice, speed } = body;

  if (!text || typeof text !== 'string') {
    return new Response('Missing or invalid "text" field', { status: 400 });
  }

  // Create a ReadableStream that produces audio chunks
  const stream = new ReadableStream({
    async start(controller) {
      // Get persistent client from pool (reuses connection + server-side cache)
      let client;
      try {
        client = await clientPool.getClient();
        console.log('[API] Using persistent gRPC client');

        // Create pipeline manifest
        const manifest = {
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
                language: language || 'en-us',
                voice: voice || 'af_bella',
                speed: speed || 1.0,
              }),
              isStreaming: true,
            },
          ],
          connections: [],
        };

        console.log('[API] Starting TTS streaming pipeline...');

        // Create text data generator for TTS input
        async function* textDataGenerator() {
          // Yield text data for the TTS node to process
          yield [
            'tts',
            {
              type: 'text' as const,
              data: {
                textData: Buffer.from(text, 'utf-8'),
                encoding: 'utf-8',
                language: language || 'en',
              },
              metadata: {},
            },
            0,
          ] as const;
        }

        let totalSamples = 0;
        let latestMetrics: any = null;

        // Stream the pipeline and send raw audio chunks
        for await (const chunk of client.streamPipeline(manifest, textDataGenerator())) {
          console.log(`[API] Chunk keys:`, Object.keys(chunk));

          // Store latest metrics
          if (chunk.metrics) {
            latestMetrics = chunk.metrics;
            console.log(`[API] Received metrics:`, {
              cacheHits: chunk.metrics.cacheHits,
              cacheMisses: chunk.metrics.cacheMisses,
              cachedNodesCount: chunk.metrics.cachedNodesCount,
              cacheHitRate: chunk.metrics.cacheHitRate,
            });
          }

          if (chunk.audioOutput) {
            // Send raw audio buffer directly
            const uint8Array = new Uint8Array(chunk.audioOutput.samples);
            controller.enqueue(uint8Array);

            totalSamples += chunk.audioOutput.numSamples;
            console.log(`[API] Sent chunk ${chunk.sequence} (${chunk.audioOutput.numSamples} samples, ${totalSamples} total)`);
          } else {
            console.log(`[API] No audioOutput in chunk ${chunk.sequence}`);
          }
        }

        console.log(`[API] TTS streaming completed (${totalSamples} total samples)`);

        // DON'T disconnect - keep connection alive for node caching!
        controller.close();

      } catch (error) {
        console.error('[API] TTS streaming error:', error);
        // On error, try to reconnect for next request
        await clientPool.reconnect();
        controller.error(error);
      }
    },
  });

  // Return the streaming response with audio headers
  return new NextResponse(stream, {
    headers: {
      'Content-Type': 'audio/pcm', // Raw PCM audio
      'X-Sample-Rate': '24000',
      'X-Channels': '1',
      'X-Sample-Format': 'float32le',
      'Cache-Control': 'no-cache',
      'Transfer-Encoding': 'chunked',
    },
  });
}
