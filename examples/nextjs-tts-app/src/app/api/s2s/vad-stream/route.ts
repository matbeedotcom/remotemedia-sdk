/**
 * VAD-based Speech-to-Speech Streaming API Route
 *
 * This route accepts continuous audio chunks from the client and uses VAD to detect
 * speech segments, which are then processed through LFM2-Audio for conversation.
 *
 * Pipeline: Audio Stream → AudioChunker (512 samples) → VAD → Buffer → LFM2-Audio → Text + Audio Response
 *
 * Features:
 * - Continuous audio streaming (like a real conversation)
 * - Audio chunking for optimal VAD processing (512 samples required by Silero VAD)
 * - VAD triggers processing only when speech is detected
 * - Session-based conversation history
 * - Low latency with server-side caching
 */

import clientPool from '@/lib/grpc-client-pool';
import sessionManager from '@/lib/grpc-session-manager';
import { createVADS2SPipeline } from '@/lib/pipeline-builder';
import { NextRequest, NextResponse } from 'next/server';
import { AudioFormat } from '../../../../../../../nodejs-client/dist/src/data-types.js';

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
        const { sessionId, systemPrompt, reset, audio, sampleRate } = body;

        const actualSessionId =
          sessionId || `vad_s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

        // Check if this is a chunk send (has audio) vs session init
        if (audio) {
          // This is an audio chunk - send to existing session
          // console.log(`[VAD S2S API] [Client->Server] Received audio chunk for session ${actualSessionId}: ${audio.length} bytes`);

          const session = sessionManager.getSession(actualSessionId);
          if (!session) {
            sendSSE({
              type: 'error',
              content: `Session ${actualSessionId} not found - please initialize first`,
            });
            controller.close();
            return;
          }

          // Decode audio and send to gRPC stream
          const audioBuffer = Buffer.from(audio, 'base64');
          const numSamples = audioBuffer.length / 4;
          // console.log(`[VAD S2S API] [Client->Server] Sending audio chunk to session ${actualSessionId}: ${numSamples} samples at ${sampleRate || 16000} Hz`);
          await session.stream.sendChunk(
            'input_chunker', // First node in VAD pipeline (chunks for resampler)
            {
              type: 'audio' as const,
              data: {
                samples: audioBuffer,
                sampleRate: sampleRate || 16000,
                channels: 1,
                numSamples: numSamples,
                format: AudioFormat.F32,
              },
            },
            {
              sessionId: actualSessionId,
            }
          );

          // Don't wait for results - just acknowledge the chunk was sent
          sendSSE({ type: 'chunk_sent', sessionId: actualSessionId });
          controller.close();
          return;
        }

        // Otherwise, this is session initialization
        sendSSE({ type: 'session', sessionId: actualSessionId });

        // Get gRPC client
        const client = await clientPool.getClient();
        console.log(`[VAD S2S API] Using persistent gRPC client for session ${actualSessionId}`);

        // Create VAD-based streaming pipeline using pipeline builder
        const manifest = createVADS2SPipeline({
          sessionId: actualSessionId,
          systemPrompt:
            systemPrompt ||
            'Respond with interleaved text and audio.',
          maxNewTokens: 4096,
        });

        console.log('[VAD S2S API] ========================================');
        console.log('[VAD S2S API] MANIFEST BEING SENT TO GRPC SERVER:');
        console.log('[VAD S2S API] ========================================');
        console.log(JSON.stringify(manifest, null, 2));
        console.log('[VAD S2S API] ========================================');
        console.log('[VAD S2S API] Pipeline nodes:');
        manifest.nodes.forEach((node, idx) => {
          console.log(`[VAD S2S API]   ${idx + 1}. ${node.id} (${node.nodeType})`);
        });
        console.log('[VAD S2S API] ========================================');

        // Handle session reset
        if (reset) {
          console.log(`[VAD S2S API] Resetting session ${actualSessionId}`);
          await sessionManager.closeSession(actualSessionId);
        }

        // Get or create persistent streaming session
        const session = await sessionManager.getOrCreateSession(actualSessionId, client, manifest);
        console.log(`[VAD S2S API] Session created and stored: ${actualSessionId}`);

        // Verify session is stored
        const verifySession = sessionManager.getSession(actualSessionId);
        console.log(`[VAD S2S API] Session verification: ${verifySession ? 'FOUND' : 'NOT FOUND'}`);

        // Send ready event
        sendSSE({
          type: 'ready',
          sessionId: actualSessionId,
          message: 'VAD-based streaming pipeline ready. Send audio chunks to process.',
        });

        // Start result listener and stream results through this SSE connection
        console.log(`[VAD S2S API] Starting result listener for session ${actualSessionId}`);

        // Keep the SSE connection open and stream results
        await sessionManager.startResultListener(
          actualSessionId,
          (result) => {
            try {
              console.log('[VAD S2S API] Received result:', result);

              // Send text output
              if (result.textOutput) {
                sendSSE({
                  type: 'text',
                  content: result.textOutput,
                  timestamp: Date.now(),
                });
              }

              // Send audio output
              if (result.audioOutput) {
                sendSSE({
                  type: 'audio',
                  content: result.audioOutput.samples.toString('base64'),
                  sampleRate: result.audioOutput.sampleRate,
                  channels: result.audioOutput.channels,
                  format: result.audioOutput.format,
                  timestamp: Date.now(),
                });
              }

              // Send JSON output
              if (result.jsonOutput) {
                sendSSE({
                  type: 'json',
                  content: result.jsonOutput,
                  timestamp: Date.now(),
                });
              }
            } catch (error) {
              console.error('[VAD S2S API] Error processing result:', error);
              sendSSE({
                type: 'error',
                content: error instanceof Error ? error.message : 'Unknown error',
              });
            }
          },
          (error) => {
            console.error('[VAD S2S API] Result listener error:', error);
            sendSSE({
              type: 'error',
              content: error instanceof Error ? error.message : 'Unknown error',
            });
            controller.close();
          }
        );

        // Keep connection open - don't close it here
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
