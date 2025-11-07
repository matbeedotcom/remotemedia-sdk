/**
 * WebRTC Signaling API Route
 *
 * Handles WebRTC signaling via gRPC WebRtcSignalingService.
 *
 * POST /api/webrtc/signal
 * - Forwards SDP offer/answer between browser and RemoteMedia server
 * - Exchanges ICE candidates
 *
 * This is a bridge between browser WebRTC and gRPC signaling.
 */

import { NextRequest, NextResponse } from 'next/server';
import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import { v4 as uuidv4 } from 'uuid';
import path from 'path';

// gRPC client configuration
const GRPC_HOST = process.env.NEXT_PUBLIC_GRPC_HOST || 'localhost';
const GRPC_PORT = process.env.NEXT_PUBLIC_GRPC_PORT || '50051';
const GRPC_URL = `${GRPC_HOST}:${GRPC_PORT}`;

// Peer ID for this Next.js server instance
const SERVER_PEER_ID = `nextjs-server-${Date.now()}`;

// Type definitions for gRPC proto
interface WebRtcSignalingClient extends grpc.Client {
  Signal(metadata?: grpc.Metadata): grpc.ClientDuplexStream<any, any>;
  GetPeers(
    request: any,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
}

// Global gRPC client (singleton)
let signalingClient: WebRtcSignalingClient | null = null;
let signalingStream: grpc.ClientDuplexStream<any, any> | null = null;

// Pending requests map (request_id → Promise resolve)
const pendingRequests = new Map<
  string,
  {
    resolve: (value: any) => void;
    reject: (reason: any) => void;
    timeout: NodeJS.Timeout;
  }
>();

/**
 * Initialize gRPC client and signaling stream
 */
async function initializeGrpcClient(): Promise<void> {
  if (signalingClient && signalingStream) {
    return; // Already initialized
  }

  try {
    // Load protobuf definition
    const PROTO_PATH = path.resolve(
      process.cwd(),
      '../../transports/remotemedia-grpc/protos/webrtc_signaling.proto'
    );

    const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
    });

    const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
    const remotemedia = protoDescriptor.remotemedia as any;
    const WebRtcSignalingService = remotemedia.v1.webrtc.WebRtcSignaling;

    // Create gRPC client
    signalingClient = new WebRtcSignalingService(
      GRPC_URL,
      grpc.credentials.createInsecure()
    ) as WebRtcSignalingClient;

    console.log(`[Signaling] gRPC client initialized: ${GRPC_URL}`);

    // Create bidirectional stream
    signalingStream = signalingClient.Signal();

    // Handle incoming messages from server
    signalingStream.on('data', (response: any) => {
      console.log('[Signaling] Received response:', response);

      const requestId = response.request_id;

      // Handle response types
      if (response.ack) {
        // Acknowledgment response
        const pending = pendingRequests.get(requestId);
        if (pending) {
          clearTimeout(pending.timeout);
          pending.resolve(response.ack);
          pendingRequests.delete(requestId);
        }
      } else if (response.peer_list) {
        // Peer list response
        const pending = pendingRequests.get(requestId);
        if (pending) {
          clearTimeout(pending.timeout);
          pending.resolve(response.peer_list);
          pendingRequests.delete(requestId);
        }
      } else if (response.notification) {
        // Server notification (offer, answer, ICE candidate, etc.)
        handleServerNotification(response.notification);
      } else if (response.error) {
        // Error response
        console.error('[Signaling] Server error:', response.error);
        const pending = pendingRequests.get(requestId);
        if (pending) {
          clearTimeout(pending.timeout);
          pending.reject(new Error(response.error.message));
          pendingRequests.delete(requestId);
        }
      }
    });

    signalingStream.on('error', (error: Error) => {
      console.error('[Signaling] Stream error:', error);
      // Reject all pending requests
      for (const [requestId, pending] of pendingRequests.entries()) {
        clearTimeout(pending.timeout);
        pending.reject(error);
        pendingRequests.delete(requestId);
      }
      // Reset client
      signalingStream = null;
      signalingClient = null;
    });

    signalingStream.on('end', () => {
      console.log('[Signaling] Stream ended');
      signalingStream = null;
    });

    // Announce this server as a peer
    await sendAnnounce();

    console.log('[Signaling] Stream initialized and peer announced');
  } catch (error) {
    console.error('[Signaling] Failed to initialize gRPC client:', error);
    throw error;
  }
}

/**
 * Send announce message to register this server as a peer
 */
async function sendAnnounce(): Promise<void> {
  const requestId = uuidv4();

  const announceRequest = {
    request_id: requestId,
    announce: {
      peer_id: SERVER_PEER_ID,
      capabilities: {
        audio: true,
        video: false,
        data: true,
      },
      metadata: {
        type: 'nextjs-server',
        version: '1.0.0',
      },
    },
  };

  await sendSignalingRequest(requestId, announceRequest);
}

/**
 * Send signaling request and wait for response
 */
function sendSignalingRequest(requestId: string, request: any): Promise<any> {
  return new Promise((resolve, reject) => {
    if (!signalingStream) {
      reject(new Error('Signaling stream not initialized'));
      return;
    }

    // Set timeout (10 seconds)
    const timeout = setTimeout(() => {
      pendingRequests.delete(requestId);
      reject(new Error('Request timeout'));
    }, 10000);

    // Store pending request
    pendingRequests.set(requestId, { resolve, reject, timeout });

    // Send request
    signalingStream.write(request, (error: Error | null | undefined) => {
      if (error) {
        clearTimeout(timeout);
        pendingRequests.delete(requestId);
        reject(error);
      }
    });
  });
}

/**
 * Handle server notifications (incoming offers, answers, ICE candidates)
 */
function handleServerNotification(notification: any): void {
  console.log('[Signaling] Server notification:', notification);

  // TODO: Forward notifications to browser clients
  // This would require WebSocket or SSE connection to browser
  // For now, we'll handle this via polling or stateful session storage

  if (notification.offer_received) {
    console.log(
      '[Signaling] Received offer from peer:',
      notification.offer_received.from_peer_id
    );
    // Store offer for browser to retrieve
  } else if (notification.answer_received) {
    console.log(
      '[Signaling] Received answer from peer:',
      notification.answer_received.from_peer_id
    );
    // Store answer for browser to retrieve
  } else if (notification.ice_candidate_received) {
    console.log(
      '[Signaling] Received ICE candidate from peer:',
      notification.ice_candidate_received.from_peer_id
    );
    // Store ICE candidate for browser to retrieve
  } else if (notification.peer_joined) {
    console.log('[Signaling] Peer joined:', notification.peer_joined.peer_id);
  } else if (notification.peer_left) {
    console.log('[Signaling] Peer left:', notification.peer_left.peer_id);
  }
}

/**
 * Handle POST requests for WebRTC signaling
 */
export async function POST(request: NextRequest) {
  try {
    // Initialize gRPC client if not already done
    await initializeGrpcClient();

    const body = await request.json();

    // Handle different signaling message types
    switch (body.type) {
      case 'offer':
        return await handleOffer(body);

      case 'answer':
        return await handleAnswer(body);

      case 'ice-candidate':
        return await handleIceCandidate(body);

      case 'get-peers':
        return await handleGetPeers();

      default:
        return NextResponse.json(
          { error: 'Unknown message type' },
          { status: 400 }
        );
    }
  } catch (error) {
    console.error('Error in WebRTC signaling:', error);
    return NextResponse.json(
      {
        error: 'Internal server error',
        message: error instanceof Error ? error.message : 'Unknown error',
      },
      { status: 500 }
    );
  }
}

/**
 * Handle SDP offer from browser
 *
 * Flow:
 * 1. Receive offer from browser
 * 2. Forward to RemoteMedia server via gRPC
 * 3. Wait for answer from server
 * 4. Return answer to browser
 */
async function handleOffer(body: any): Promise<NextResponse> {
  try {
    console.log('[Signaling] Handling WebRTC offer');

    const requestId = uuidv4();
    const toPeerId = body.to_peer_id || 'remotemedia-server'; // Default target

    const offerRequest = {
      request_id: requestId,
      offer: {
        to_peer_id: toPeerId,
        sdp: body.sdp,
        type: body.offer_type || 'offer',
      },
    };

    // Send offer to gRPC signaling service
    const ackResponse = await sendSignalingRequest(requestId, offerRequest);

    console.log('[Signaling] Offer sent, ack received:', ackResponse);

    // Note: In a real implementation, the answer would come via notification
    // For now, we return success and expect answer via separate polling
    return NextResponse.json({
      success: true,
      message: 'Offer sent successfully. Poll for answer using GET request.',
    });
  } catch (error) {
    console.error('Error handling offer:', error);
    throw error;
  }
}

/**
 * Handle SDP answer from browser
 */
async function handleAnswer(body: any): Promise<NextResponse> {
  try {
    console.log('[Signaling] Handling WebRTC answer');

    const requestId = uuidv4();
    const toPeerId = body.to_peer_id || 'remotemedia-server';

    const answerRequest = {
      request_id: requestId,
      answer: {
        to_peer_id: toPeerId,
        sdp: body.sdp,
        type: 'answer',
      },
    };

    // Send answer to gRPC signaling service
    await sendSignalingRequest(requestId, answerRequest);

    console.log('[Signaling] Answer sent successfully');

    return NextResponse.json({ success: true });
  } catch (error) {
    console.error('Error handling answer:', error);
    throw error;
  }
}

/**
 * Handle ICE candidate from browser
 */
async function handleIceCandidate(body: any): Promise<NextResponse> {
  try {
    console.log('[Signaling] Handling ICE candidate');

    const requestId = uuidv4();
    const toPeerId = body.to_peer_id || 'remotemedia-server';

    const iceCandidateRequest = {
      request_id: requestId,
      ice_candidate: {
        to_peer_id: toPeerId,
        candidate: body.candidate,
        sdp_mid: body.sdpMid,
        sdp_mline_index: body.sdpMLineIndex,
      },
    };

    // Send ICE candidate to gRPC signaling service
    await sendSignalingRequest(requestId, iceCandidateRequest);

    console.log('[Signaling] ICE candidate sent successfully');

    return NextResponse.json({ success: true });
  } catch (error) {
    console.error('Error handling ICE candidate:', error);
    throw error;
  }
}

/**
 * Get list of connected peers
 */
async function handleGetPeers(): Promise<NextResponse> {
  try {
    console.log('[Signaling] Getting peer list');

    const requestId = uuidv4();

    const listPeersRequest = {
      request_id: requestId,
      list_peers: {},
    };

    // Send list peers request
    const peerListResponse = await sendSignalingRequest(
      requestId,
      listPeersRequest
    );

    console.log('[Signaling] Peer list retrieved:', peerListResponse);

    return NextResponse.json({
      success: true,
      peers: peerListResponse.peers || [],
    });
  } catch (error) {
    console.error('Error getting peer list:', error);
    throw error;
  }
}

/**
 * GET request - return service status
 */
export async function GET() {
  try {
    await initializeGrpcClient();

    return NextResponse.json({
      status: 'ok',
      signaling_url: GRPC_URL,
      peer_id: SERVER_PEER_ID,
      connected: signalingStream !== null,
    });
  } catch (error) {
    return NextResponse.json(
      {
        status: 'error',
        message: error instanceof Error ? error.message : 'Unknown error',
      },
      { status: 500 }
    );
  }
}

/**
 * IMPORTANT NOTE FOR PRODUCTION:
 *
 * This implementation uses a singleton gRPC stream which is shared across
 * all HTTP requests. For production use, you should:
 *
 * 1. Use WebSocket or Server-Sent Events (SSE) for real-time notifications
 * 2. Store pending answers/candidates in Redis or similar
 * 3. Implement proper session management per browser client
 * 4. Add authentication and authorization
 * 5. Handle stream reconnection and recovery
 *
 * Current limitations:
 * - Answers must be polled for (no push notifications to browser)
 * - Single shared gRPC stream for all clients
 * - No persistent storage of signaling state
 *
 * For WebSocket integration example, see:
 * examples/nextjs-tts-app/server.ts (Socket.IO implementation)
 */
