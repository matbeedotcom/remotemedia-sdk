/**
 * VAD S2S Results Streaming Endpoint
 *
 * Server-Sent Events endpoint that streams pipeline results (text, audio)
 * back to the client for a specific session.
 */

import sessionManager from '@/lib/grpc-session-manager';
import { NextRequest, NextResponse } from 'next/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

/**
 * GET /api/s2s/results?sessionId=xxx
 *
 * Establishes an SSE connection and streams pipeline results for a session.
 */
export async function GET(request: NextRequest) {
  const searchParams = request.nextUrl.searchParams;
  const sessionId = searchParams.get('sessionId');

  if (!sessionId) {
    return NextResponse.json({ error: 'sessionId is required' }, { status: 400 });
  }

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
        console.log(`[Results API] Looking for session: ${sessionId}`);

        // Get session
        const session = sessionManager.getSession(sessionId);
        console.log(`[Results API] Session lookup result: ${session ? 'FOUND' : 'NOT FOUND'}`);

        if (!session) {
          // Log all available sessions for debugging
          console.log(`[Results API] Available sessions:`, sessionManager.listSessions());

          sendSSE({
            type: 'error',
            content: `Session ${sessionId} not found`,
          });
          controller.close();
          return;
        }

        // Send connected event
        sendSSE({ type: 'connected', sessionId });

        // Start result listener if not already active
        await sessionManager.startResultListener(
          sessionId,
          (result) => {
            try {
              console.log('[Results API] Received result:', result);

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
              console.error('[Results API] Error processing result:', error);
              sendSSE({
                type: 'error',
                content: error instanceof Error ? error.message : 'Unknown error',
              });
            }
          },
          (error) => {
            console.error('[Results API] Listener error:', error);
            sendSSE({
              type: 'error',
              content: error.message,
            });
            controller.close();
          }
        );

        // Keep connection alive with heartbeat
        const heartbeatInterval = setInterval(() => {
          try {
            sendSSE({ type: 'heartbeat', timestamp: Date.now() });
          } catch (error) {
            console.error('[Results API] Heartbeat error:', error);
            clearInterval(heartbeatInterval);
          }
        }, 30000); // Every 30 seconds

        // Cleanup on connection close
        request.signal.addEventListener('abort', () => {
          console.log(`[Results API] Client disconnected from session ${sessionId}`);
          clearInterval(heartbeatInterval);
          controller.close();
        });
      } catch (error) {
        console.error('[Results API] Error:', error);
        sendSSE({
          type: 'error',
          content: error instanceof Error ? error.message : 'Unknown error',
        });
        controller.close();
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
