/**
 * WebSocket message handler (called from custom server)
 *
 * Handles both JSON and binary WebSocket messages
 */

import { NextApiRequest, NextApiResponse } from 'next';
import clientPool from '@/lib/grpc-client-pool';
import sessionManager from '@/lib/grpc-session-manager';
import { createVADS2SPipeline } from '@/lib/pipeline-builder';

export const config = {
  api: {
    bodyParser: {
      sizeLimit: '10mb',
    },
  },
};

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  const connectionId = req.headers['x-connection-id'] as string;

  if (!connectionId) {
    return res.status(400).json({ error: 'Missing connection ID' });
  }

  // Get connection from global store
  const conn = (global as any).wsConnections?.get(connectionId);
  if (!conn) {
    return res.status(404).json({ error: 'Connection not found' });
  }

  const sendJSON = (data: any) => {
    if (conn.ws.readyState === 1) {
      conn.ws.send(JSON.stringify(data));
    }
  };

  try {
    const contentType = req.headers['content-type'];

    // Handle JSON messages
    if (contentType === 'application/json') {
      const message = req.body;

      switch (message.type) {
        case 'init': {
          conn.sessionId =
            message.sessionId ||
            `vad_s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

          console.log(`[WebSocket S2S] Initializing session ${conn.sessionId}`);

          const client = await clientPool.getClient();
          const manifest = createVADS2SPipeline({
            sessionId: conn.sessionId,
            systemPrompt:
              message.systemPrompt ||
              'Respond with interleaved text and audio.',
            maxNewTokens: 4096,
          });

          if (message.reset) {
            await sessionManager.closeSession(conn.sessionId);
          }

          const session = await sessionManager.getOrCreateSession(conn.sessionId, client, manifest);
          console.log(`[WebSocket S2S] Session created: ${conn.sessionId}`);

          sendJSON({
            type: 'ready',
            sessionId: conn.sessionId,
            message: 'Pipeline ready',
          });

          if (!conn.resultListenerActive) {
            conn.resultListenerActive = true;
            await sessionManager.startResultListener(
              conn.sessionId,
              (result) => {
                try {
                  if (result.textOutput) {
                    sendJSON({
                      type: 'text',
                      content: result.textOutput,
                      timestamp: Date.now(),
                    });
                  }

                  if (result.audioOutput && conn.ws.readyState === 1) {
                    conn.ws.send(result.audioOutput.samples);
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
          if (conn.sessionId) {
            console.log(`[WebSocket S2S] Closing session ${conn.sessionId}`);
            await sessionManager.closeSession(conn.sessionId);
            conn.sessionId = null;
            conn.resultListenerActive = false;
          }
          conn.ws.close();
          break;
        }
      }

      return res.status(200).json({ success: true });
    }

    // Handle binary messages (audio)
    if (contentType === 'application/octet-stream') {
      if (!conn.sessionId) {
        sendJSON({ type: 'error', content: 'Session not initialized' });
        return res.status(400).json({ error: 'Session not initialized' });
      }

      const session = sessionManager.getSession(conn.sessionId);
      if (!session) {
        sendJSON({ type: 'error', content: `Session ${conn.sessionId} not found` });
        return res.status(404).json({ error: 'Session not found' });
      }

      // Read binary data
      const chunks: Buffer[] = [];
      await new Promise((resolve, reject) => {
        req.on('data', (chunk) => chunks.push(chunk));
        req.on('end', resolve);
        req.on('error', reject);
      });

      const data = Buffer.concat(chunks);
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
        { sessionId: conn.sessionId }
      );

      return res.status(200).json({ success: true });
    }

    return res.status(400).json({ error: 'Invalid content type' });
  } catch (error) {
    console.error('[WebSocket S2S] Handler error:', error);
    return res.status(500).json({
      error: error instanceof Error ? error.message : 'Unknown error',
    });
  }
}
