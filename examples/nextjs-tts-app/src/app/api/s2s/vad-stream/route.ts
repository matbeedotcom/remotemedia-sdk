/**
 * VAD-based Speech-to-Speech Streaming API Route
 *
 * This route accepts continuous audio chunks from the client and uses VAD to detect
 * speech segments, which are then processed through LFM2-Audio for conversation.
 *
 * Pipeline: Audio Stream → VAD → Buffer → LFM2-Audio → Text + Audio Response
 *
 * Features:
 * - Continuous audio streaming (like a real conversation)
 * - VAD triggers processing only when speech is detected
 * - Session-based conversation history
 * - Low latency with server-side caching
 */

import clientPool from '@/lib/grpc-client-pool';
import { createVADS2SPipeline } from '@/lib/pipeline-builder';
import { NextRequest, NextResponse } from 'next/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

/**
 * POST /api/s2s/vad-stream
 *
 * Accepts continuous audio stream, uses VAD to detect speech, and processes through LFM2-Audio.
 *
 * Request body (multipart or streaming):
 * - Continuous stream of audio chunks
 * - Each chunk: { audio: base64, sequence: number, sessionId: string }
 *
 * Response: Server-Sent Events (SSE) stream
 * - VAD events: { type: 'vad', hasSpeech: boolean }
 * - Text responses: { type: 'text', content: string }
 * - Audio responses: { type: 'audio', content: base64 }
 */

interface AudioChunkMessage {
  audio: string; // base64
  sequence: number;
  sessionId?: string;
  sampleRate?: number;
  isLast?: boolean;
}

export async function POST(request: NextRequest) {
  const encoder = new TextEncoder();

  // Create SSE stream
  const stream = new ReadableStream({
    async start(controller) {
      // Helper to send SSE message
      const sendSSE = (data: any) => {
        const message = `data: ${JSON.stringify(data)}\n\n`;
        controller.enqueue(encoder.encode(message));
      };

      try {
        // Parse request body
        const body = await request.json();
        const { sessionId, systemPrompt, reset } = body;

        const actualSessionId =
          sessionId || `vad_s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

        sendSSE({ type: 'session', sessionId: actualSessionId });

        // Get gRPC client
        const client = await clientPool.getClient();
        console.log(`[VAD S2S API] Using persistent gRPC client for session ${actualSessionId}`);

        // Create VAD-based streaming pipeline using pipeline builder
        const manifest = createVADS2SPipeline({
          sessionId: actualSessionId,
          systemPrompt:
            systemPrompt ||
            'You are a helpful AI assistant. Respond naturally and conversationally.',
          audioTemperature: 1.0,
          audioTopK: 4,
          maxNewTokens: 512,
          vadEnergyThreshold: 0.02,
          minSpeechDuration: 0.8,
          maxSpeechDuration: 10.0,
          silenceDuration: 1.0,
        });

        console.log('[VAD S2S API] Created pipeline manifest:', JSON.stringify(manifest, null, 2));

        // This is a placeholder for now - we need to implement the actual streaming pipeline
        // For the MVP, we'll document the approach:

        /*
         * IMPLEMENTATION APPROACH:
         *
         * 1. Client opens WebSocket or uses fetch with ReadableStream
         * 2. Client continuously sends audio chunks (e.g., every 100ms)
         * 3. Server streams chunks to gRPC pipeline
         * 4. VAD detects speech segments
         * 5. Complete speech segments trigger LFM2-Audio processing
         * 6. Server streams back text + audio responses via SSE
         *
         * For now, return instructions:
         */

        sendSSE({
          type: 'info',
          message:
            'VAD-based streaming pipeline is under construction. Use /api/s2s/stream for single-shot processing.',
        });

        sendSSE({
          type: 'complete',
          sessionId: actualSessionId,
        });

        controller.close();
      } catch (error) {
        console.error('[VAD S2S API] Error:', error);
        sendSSE({
          type: 'error',
          content: error instanceof Error ? error.message : 'Unknown error',
        });
        controller.error(error);
      }
    },
  });

  return new NextResponse(stream, {
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      Connection: 'keep-alive',
    },
  });
}
