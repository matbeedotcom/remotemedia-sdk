/**
 * WebSocket-based Speech-to-Speech API (Pages Router)
 *
 * Bidirectional WebSocket for real-time S2S:
 * - Text frames: JSON messages
 * - Binary frames: Audio data
 */

import { NextApiRequest } from 'next';
import { WebSocketServer, WebSocket } from 'ws';
import clientPool from '@/lib/grpc-client-pool';
import sessionManager from '@/lib/grpc-session-manager';
import { createVADS2SPipeline, createVibeVoiceTTSPipeline } from '@/lib/pipeline-builder';
console.log('[WS] Imported functions:', { createVADS2SPipeline: typeof createVADS2SPipeline, createVibeVoiceTTSPipeline: typeof createVibeVoiceTTSPipeline });

const wss = new WebSocketServer({ noServer: true });

wss.on('connection', (ws: WebSocket) => {
  console.log('[WebSocket S2S] Client connected');

  let sessionId: string | null = null;
  let resultListenerActive = false;

  const sendJSON = (data: any) => {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(data));
    }
  };

  ws.on('message', async (data: Buffer) => {
    try {
      // Check if it's JSON (starts with '{')
      if (data[0] === 0x7b) {
        const message = JSON.parse(data.toString());

        switch (message.type) {
          case 'init': {
            sessionId =
              message.sessionId ||
              `vad_s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

            console.log(`[WebSocket S2S] Initializing session ${sessionId}`);

            const client = await clientPool.getClient();
            console.log('[WS] About to call createVADS2SPipeline, function is:', createVADS2SPipeline.name);
            const manifest = createVADS2SPipeline({
              sessionId,
              systemPrompt:
                message.systemPrompt ||
                'Respond with interleaved text and audio.',
              maxNewTokens: 4096,
            });
            console.log('[WS] Called function, checking manifest last node:', manifest.nodes[manifest.nodes.length - 1]?.nodeType);

            if (message.reset) {
              await sessionManager.closeSession(sessionId);
            }

            const session = await sessionManager.getOrCreateSession(sessionId, client, manifest);
            console.log(`[WebSocket S2S] Session created: ${sessionId}`);

            sendJSON({
              type: 'ready',
              sessionId,
              message: 'Pipeline ready',
            });

            if (!resultListenerActive) {
              resultListenerActive = true;
              await sessionManager.startResultListener(
                sessionId,
                (result) => {
                  try {
                    if (result.textOutput) {
                      sendJSON({
                        type: 'text',
                        content: result.textOutput,
                        timestamp: Date.now(),
                      });
                    }

                    if (result.audioOutput && ws.readyState === WebSocket.OPEN) {
                      ws.send(result.audioOutput.samples);
                    }

                    if (result.jsonOutput) {
                      sendJSON({
                        type: 'json',
                        content: result.jsonOutput,
                        timestamp: Date.now(),
                      });
                    }
                  } catch (error) {
                    console.error('[WebSocket S2S] Error processing result:', error);
                    sendJSON({
                      type: 'error',
                      content: error instanceof Error ? error.message : 'Unknown error',
                    });
                  }
                },
                (error) => {
                  console.error('[WebSocket S2S] Result listener error:', error);
                  sendJSON({
                    type: 'error',
                    content: error instanceof Error ? error.message : 'Unknown error',
                  });
                }
              );
            }
            break;
          }

          case 'close': {
            if (sessionId) {
              console.log(`[WebSocket S2S] Closing session ${sessionId}`);
              await sessionManager.closeSession(sessionId);
              sessionId = null;
              resultListenerActive = false;
            }
            ws.close();
            break;
          }
        }
      } else {
        // Binary frame - audio input
        if (!sessionId) {
          sendJSON({ type: 'error', content: 'Session not initialized' });
          return;
        }

        const session = sessionManager.getSession(sessionId);
        if (!session) {
          sendJSON({ type: 'error', content: `Session ${sessionId} not found` });
          return;
        }

        const numSamples = data.length / 4;
        await session.stream.sendChunk(
          'input_chunker',
          {
            type: 'audio' as const,
            data: {
              samples: data,
              sampleRate: 48000,
              numChannels: 1,
              numSamples: numSamples,
              format: 'float32le',
            },
          },
          { sessionId }
        );
      }
    } catch (error) {
      console.error('[WebSocket S2S] Error:', error);
      sendJSON({
        type: 'error',
        content: error instanceof Error ? error.message : 'Unknown error',
      });
    }
  });

  ws.on('close', () => {
    console.log('[WebSocket S2S] Client disconnected');
  });

  ws.on('error', (error: Error) => {
    console.error('[WebSocket S2S] Error:', error);
  });
});

export default async function handler(req: NextApiRequest, res: any) {
  if (!res.socket.server.wss) {
    console.log('[WebSocket S2S] Initializing WebSocket server');
    res.socket.server.wss = wss;

    res.socket.server.on('upgrade', (request: any, socket: any, head: any) => {
      if (request.url === '/api/s2s/ws') {
        wss.handleUpgrade(request, socket, head, (ws) => {
          wss.emit('connection', ws, request);
        });
      }
    });
  }

  // Send success response for HTTP requests
  res.status(200).json({ message: 'WebSocket server running' });
}
