/**
 * Speech-to-Speech Streaming API Route
 *
 * This route handler connects to the Rust gRPC server for conversational speech-to-speech
 * using the LFM2-Audio-1.5B model. It maintains session state for multi-turn conversations.
 *
 * Features:
 * - Audio input → Text + Audio output (interleaved)
 * - Session-based conversation history
 * - Persistent gRPC connection with server-side caching
 */

import clientPool from '@/lib/grpc-client-pool';
import sessionManager from '@/lib/grpc-session-manager';
import { createSimpleS2SPipeline } from '@/lib/pipeline-builder';
import { NextRequest, NextResponse } from 'next/server';

export const runtime = 'nodejs'; // Use Node.js runtime for gRPC
export const dynamic = 'force-dynamic';

/**
 * POST /api/s2s/stream
 *
 * Request body:
 * {
 *   "audio": base64-encoded PCM audio data,
 *   "sampleRate": 24000,
 *   "sessionId": "optional-session-id", // For conversation continuity
 *   "systemPrompt": "Optional system prompt override"
 * }
 *
 * Response: JSON stream with interleaved text and audio chunks
 * {
 *   "type": "text" | "audio" | "metrics",
 *   "content": string | base64-audio,
 *   "sequence": number
 * }
 */
export async function POST(request: NextRequest) {
  // Parse request body
  let body;
  try {
    body = await request.json();
  } catch (error) {
    return new Response('Invalid JSON body', { status: 400 });
  }

  const { audio, sampleRate, sessionId, systemPrompt, reset } = body;

  if (!audio || typeof audio !== 'string') {
    return new Response('Missing or invalid "audio" field (expected base64 string)', {
      status: 400,
    });
  }

  // Generate or use provided session ID
  const actualSessionId = sessionId || `s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

  // Create a ReadableStream that produces response chunks
  const stream = new ReadableStream({
    async start(controller) {
      // Get persistent client from pool
      let client;
      let session;
      try {
        client = await clientPool.getClient();
        console.log(`[S2S API] Using persistent gRPC client for session ${actualSessionId}`);

        // Handle session reset
        if (reset) {
          console.log(`[S2S API] Resetting session ${actualSessionId}`);
          // Close existing session to reset conversation history
          await sessionManager.closeSession(actualSessionId);
        }

        // Create pipeline manifest using pipeline builder
        const manifest = createSimpleS2SPipeline({
          sessionId: actualSessionId,
          systemPrompt:
            systemPrompt ||
            'You are a helpful AI assistant. Respond naturally and conversationally.',
          audioTemperature: 1.0,
          audioTopK: 4,
          maxNewTokens: 512,
        });

        console.log(`[S2S API] Getting or creating persistent session: ${actualSessionId}...`);

        // Get or create persistent streaming session
        session = await sessionManager.getOrCreateSession(actualSessionId, client, manifest);

        // Decode base64 audio to buffer
        const audioBuffer = Buffer.from(audio, 'base64');
        const numSamples = audioBuffer.length / 4; // float32 = 4 bytes per sample

        console.log(`[S2S API] Sending audio chunk to session: ${actualSessionId}`);

        // Send audio chunk to persistent stream
        await session.stream.sendChunk(
          'lfm2_audio',
          {
            type: 'audio' as const,
            data: {
              samples: audioBuffer,
              sampleRate: sampleRate || 24000,
              numChannels: 1,
              numSamples: numSamples,
              format: 'float32le',
            },
          },
          {
            sessionId: actualSessionId,
            reset: reset ? 'true' : 'false',
          }
        );

        let sequenceNum = 0;
        let receivedResults = false;

        // Process responses from persistent stream until we get all outputs for this turn
        // We continue until we get text or audio output, indicating the turn is complete
        for await (const chunk of session.stream.getResults()) {
          console.log(`[S2S API] Chunk ${sequenceNum} keys:`, Object.keys(chunk));
          receivedResults = true;

          // Handle metrics
          if (chunk.metrics) {
            const metricsData = JSON.stringify({
              type: 'metrics',
              sequence: sequenceNum++,
              content: {
                sessionId: actualSessionId,
                cacheHits: chunk.metrics.cacheHits,
                cacheMisses: chunk.metrics.cacheMisses,
                cachedNodesCount: chunk.metrics.cachedNodesCount,
                cacheHitRate: chunk.metrics.cacheHitRate,
                averageLatencyMs: chunk.metrics.averageLatencyMs,
              },
            });
            controller.enqueue(new TextEncoder().encode(metricsData + '\n'));
          }

          // Handle text output
          if (chunk.textOutput) {
            // Strip special tokens like <|text_end|>, <|audio_end|>, etc.
            const cleanedText = chunk.textOutput
              .replace(/<\|text_end\|>/g, '')
              .replace(/<\|audio_end\|>/g, '')
              .replace(/<\|[^|]+\|>/g, '') // Remove any other special tokens
              .trim();

            if (cleanedText) {
              const textData = JSON.stringify({
                type: 'text',
                sequence: sequenceNum++,
                content: cleanedText,
                sessionId: actualSessionId,
              });
              controller.enqueue(new TextEncoder().encode(textData + '\n'));
              console.log(`[S2S API] Sent text response: ${cleanedText.substring(0, 50)}...`);
            }
          }

          // Handle audio output
          if (chunk.audioOutput) {
            // Convert audio buffer to base64 for JSON transport
            const audioBase64 = Buffer.from(chunk.audioOutput.samples).toString('base64');
            const audioData = JSON.stringify({
              type: 'audio',
              sequence: sequenceNum++,
              content: audioBase64,
              sampleRate: chunk.audioOutput.sampleRate || 24000,
              numSamples: chunk.audioOutput.numSamples,
              sessionId: actualSessionId,
            });
            controller.enqueue(new TextEncoder().encode(audioData + '\n'));
            console.log(
              `[S2S API] Sent audio chunk (${chunk.audioOutput.numSamples} samples)`,
            );

            // For now, break after receiving audio output (TTS completed)
            // This keeps the session alive for the next request
            break;
          }
        }

        console.log(`[S2S API] S2S streaming completed for session ${actualSessionId}, keeping session alive`);

        // Send completion marker
        const completeData = JSON.stringify({
          type: 'complete',
          sequence: sequenceNum,
          sessionId: actualSessionId,
        });
        controller.enqueue(new TextEncoder().encode(completeData + '\n'));

        // Close HTTP response (but keep gRPC session alive)
        controller.close();
      } catch (error) {
        console.error('[S2S API] Speech-to-speech streaming error:', error);

        // On critical error, close the session so it can be recreated
        if (session) {
          try {
            await sessionManager.closeSession(actualSessionId);
            console.log(`[S2S API] Closed session due to error: ${actualSessionId}`);
          } catch (closeError) {
            console.error('[S2S API] Error closing session:', closeError);
          }
        }

        // Try to send error to client
        try {
          const errorData = JSON.stringify({
            type: 'error',
            content: error instanceof Error ? error.message : 'Unknown error',
            sessionId: actualSessionId,
          });
          controller.enqueue(new TextEncoder().encode(errorData + '\n'));
        } catch (e) {
          // Ignore if controller is already closed
        }
        controller.error(error);
      }
    },
  });

  // Return the streaming response
  return new NextResponse(stream, {
    headers: {
      'Content-Type': 'application/x-ndjson', // Newline-delimited JSON
      'X-Session-ID': actualSessionId,
      'Cache-Control': 'no-cache',
      'Transfer-Encoding': 'chunked',
    },
  });
}

/**
 * GET /api/s2s/stream?sessionId=xxx
 *
 * Get information about a session
 */
export async function GET(request: NextRequest) {
  const searchParams = request.nextUrl.searchParams;
  const sessionId = searchParams.get('sessionId');

  if (!sessionId) {
    return NextResponse.json({ error: 'Missing sessionId parameter' }, { status: 400 });
  }

  // TODO: Implement session info retrieval
  // For now, return a placeholder
  return NextResponse.json({
    sessionId,
    status: 'active',
    message: 'Session info endpoint - to be implemented',
  });
}

/**
 * DELETE /api/s2s/stream?sessionId=xxx
 *
 * Delete/reset a session
 */
export async function DELETE(request: NextRequest) {
  const searchParams = request.nextUrl.searchParams;
  const sessionId = searchParams.get('sessionId');

  if (!sessionId) {
    return NextResponse.json({ error: 'Missing sessionId parameter' }, { status: 400 });
  }

  console.log(`[S2S API] Deleting session ${sessionId}`);

  // TODO: Implement session deletion
  // This would send a reset signal to the LFM2AudioNode

  return NextResponse.json({
    sessionId,
    message: 'Session deleted successfully',
  });
}
