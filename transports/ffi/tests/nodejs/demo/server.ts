/**
 * RemoteMedia WebRTC Demo Server
 *
 * This demo demonstrates the WebRTC FFI bindings with:
 * - WebRTC server with embedded gRPC signaling (JSON-RPC 2.0 over WebSocket)
 * - HTTP server serving the client webpage
 *
 * The browser connects directly to the WebRTC server's signaling endpoint.
 *
 * Run with: npx ts-node demo/server.ts
 */

import * as http from 'http';
import * as fs from 'fs';
import * as path from 'path';

// Load native bindings
const native = require('@remotemedia/native');

// Configuration (override via environment variables)
const HTTP_PORT = parseInt(process.env.HTTP_PORT || '8080', 10);
const WEBRTC_PORT = parseInt(process.env.WEBRTC_PORT || '50100', 10);
const STUN_SERVERS = ['stun:stun.l.google.com:19302', 'stun:stun1.l.google.com:19302'];

// Types for WebRTC events
interface PeerConnectedData {
  peer_id: string;
  capabilities: { audio: boolean; video: boolean; data: boolean };
  metadata: Record<string, string>;
}

interface PeerDisconnectedData {
  peer_id: string;
  reason?: string;
}

interface PipelineOutputData {
  peer_id: string;
  data_type: string;
  data: string;
  timestamp: number;
}

interface DataReceivedData {
  peer_id: string;
  size: number;
  timestamp: number;
}

interface ErrorData {
  code: string;
  message: string;
  peer_id?: string;
}

// WebRTC server instance
let webrtcServer: any = null;

/**
 * Create and start the WebRTC server with embedded signaling
 */
async function startWebRtcServer(): Promise<void> {
  if (!native.WebRtcServer) {
    console.error('WebRTC bindings not available. Make sure to build with --features napi-webrtc');
    process.exit(1);
  }

  console.log('Creating WebRTC server with embedded signaling...');

  // Create a simple passthrough pipeline manifest (v1 format)
  const manifest = {
    version: '1.0.0',
    metadata: {
      name: 'webrtc-demo-passthrough',
      description: 'Simple audio passthrough pipeline for WebRTC demo',
    },
    nodes: [
      {
        id: 'input',
        node_type: 'AudioInput',
        params: {},
      },
      {
        id: 'output',
        node_type: 'AudioOutput',
        params: {},
      },
    ],
    connections: [{ from: 'input', to: 'output' }],
  };

  try {
    webrtcServer = await native.WebRtcServer.create({
      port: WEBRTC_PORT,
      manifest: JSON.stringify(manifest),
      stunServers: STUN_SERVERS,
      maxPeers: 10,
      audioCodec: 'opus',
      videoCodec: 'vp9',
    });

    console.log(`WebRTC server created with ID: ${webrtcServer.id}`);

    // Register event handlers
    webrtcServer.on('peer_connected', (data: PeerConnectedData) => {
      console.log(`[EVENT] Peer connected: ${data.peer_id}`);
      console.log(`        Capabilities: audio=${data.capabilities.audio}, video=${data.capabilities.video}, data=${data.capabilities.data}`);
    });

    webrtcServer.on('peer_disconnected', (data: PeerDisconnectedData) => {
      console.log(`[EVENT] Peer disconnected: ${data.peer_id} (reason: ${data.reason || 'none'})`);
    });

    webrtcServer.on('pipeline_output', (data: PipelineOutputData) => {
      console.log(`[EVENT] Pipeline output for peer ${data.peer_id}: ${data.data_type}`);
    });

    webrtcServer.on('data', (data: DataReceivedData) => {
      console.log(`[EVENT] Data received from peer ${data.peer_id}: ${data.size} bytes`);
    });

    webrtcServer.on('error', (data: ErrorData) => {
      console.error(`[ERROR] WebRTC error [${data.code}]: ${data.message}`);
      if (data.peer_id) {
        console.error(`        Related peer: ${data.peer_id}`);
      }
    });

    // Start the WebSocket signaling server explicitly on a dedicated thread
    // This ensures the server is actually listening before we continue
    console.log(`Starting WebSocket signaling server on port ${WEBRTC_PORT}...`);
    await webrtcServer.startSignalingServer(WEBRTC_PORT);
    console.log(`Signaling WebSocket: ws://localhost:${WEBRTC_PORT}/ws`);
    console.log(`Server state: ${await webrtcServer.state}`);
  } catch (err) {
    console.error('Failed to start WebRTC server:', err);
    throw err;
  }
}

/**
 * Start the HTTP server to serve the client webpage
 */
function startHttpServer(): http.Server {
  const server = http.createServer(async (req, res) => {
    const url = req.url || '/';

    // Add CORS headers for local development
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

    if (req.method === 'OPTIONS') {
      res.writeHead(204);
      res.end();
      return;
    }

    if (url === '/' || url === '/index.html') {
      // Serve the WebRTC client page
      const clientPath = path.join(__dirname, 'client.html');
      fs.readFile(clientPath, 'utf8', (err, content) => {
        if (err) {
          res.writeHead(500);
          res.end('Error loading client page');
          return;
        }
        // Inject configuration
        const configuredContent = content
          .replace(/\{\{SIGNALING_URL\}\}/g, `ws://localhost:${WEBRTC_PORT}/ws`)
          .replace(/\{\{STUN_SERVERS\}\}/g, JSON.stringify(STUN_SERVERS));

        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(configuredContent);
      });
    } else if (url === '/api/config') {
      // Server configuration endpoint
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          signaling_url: `ws://localhost:${WEBRTC_PORT}/ws`,
          stun_servers: STUN_SERVERS,
          server_id: webrtcServer?.id || null,
        })
      );
    } else if (url === '/api/status') {
      // Server status endpoint
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          webrtc: webrtcServer
            ? {
                id: webrtcServer.id,
                port: WEBRTC_PORT,
                state: await webrtcServer.state,
              }
            : null,
        })
      );
    } else if (url === '/api/peers') {
      // List connected peers
      if (webrtcServer) {
        try {
          const peers = await webrtcServer.getPeers();
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ peers }));
        } catch (err: any) {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: err.message }));
        }
      } else {
        res.writeHead(503, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'WebRTC server not running' }));
      }
    } else if (url === '/api/sessions') {
      // List sessions
      if (webrtcServer) {
        try {
          const sessions = await webrtcServer.getSessions();
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ sessions }));
        } catch (err: any) {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: err.message }));
        }
      } else {
        res.writeHead(503, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'WebRTC server not running' }));
      }
    } else {
      res.writeHead(404);
      res.end('Not Found');
    }
  });

  return server;
}

/**
 * Graceful shutdown
 */
async function shutdown(): Promise<void> {
  console.log('\nShutting down...');

  // Shutdown WebRTC server
  if (webrtcServer) {
    try {
      await webrtcServer.shutdown();
      console.log('WebRTC server shutdown complete');
    } catch (err) {
      console.error('Error shutting down WebRTC server:', err);
    }
  }

  process.exit(0);
}

/**
 * Main entry point
 */
async function main(): Promise<void> {
  console.log('='.repeat(60));
  console.log('RemoteMedia WebRTC Demo Server');
  console.log('='.repeat(60));

  // Handle shutdown signals
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);

  try {
    // Start WebRTC server with embedded signaling
    await startWebRtcServer();

    // Start HTTP server for serving the client page
    const httpServer = startHttpServer();

    // Listen on HTTP port
    httpServer.listen(HTTP_PORT, () => {
      console.log('');
      console.log('Server is running!');
      console.log(`  Web Client:     http://localhost:${HTTP_PORT}`);
      console.log(`  Signaling WS:   ws://localhost:${WEBRTC_PORT}/ws`);
      console.log(`  STUN Servers:   ${STUN_SERVERS.join(', ')}`);
      console.log('');
      console.log(`Open http://localhost:${HTTP_PORT} in your browser to test WebRTC`);
      console.log('');
      console.log('The browser will connect directly to the signaling server');
      console.log('using JSON-RPC 2.0 over WebSocket.');
      console.log('');
      console.log('Press Ctrl+C to stop');
      console.log('');
    });
  } catch (err) {
    console.error('Failed to start server:', err);
    process.exit(1);
  }
}

main();
