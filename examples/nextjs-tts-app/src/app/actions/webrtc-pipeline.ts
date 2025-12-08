'use server';

/**
 * WebRTC Pipeline Server Actions
 *
 * These Server Actions manage WebRTC pipeline server instances using the
 * RemoteMedia Node.js FFI bindings. They provide session provisioning and
 * signaling URL management for client-side WebRTC connections.
 *
 * Architecture:
 *   Client (Browser)                Server (Next.js)
 *   ─────────────────              ────────────────────
 *   1. Call createSession() ───────> Start WebRTC server with pipeline
 *   2. Receive signalingUrl <─────── Return WebSocket signaling URL
 *   3. Connect via WebRTC  <───────> WebSocket signaling + ICE exchange
 *   4. Send/receive via data channel ─> Pipeline processes data ─> Response via data channel
 */

import {
  getOrCreateServer,
  shutdownServer,
  PipelineManifests,
  isWebRtcAvailable,
  getLoadError,
  type WebRtcServerConfig,
} from '@/lib/remotemedia-ffi';
import { v4 as uuidv4 } from 'uuid';

// Default configuration
const DEFAULT_SIGNALING_PORT = 51000;
const DEFAULT_STUN_SERVERS = [
  'stun:stun.l.google.com:19302',
  'stun:stun1.l.google.com:19302',
];

// Track session state
interface SessionState {
  sessionId: string;
  serverId: string;
  signalingUrl: string;
  pipelineType: string;
  createdAt: number;
}

const sessions = new Map<string, SessionState>();

// Port allocation for signaling servers
let nextPort = DEFAULT_SIGNALING_PORT;
function allocatePort(): number {
  return nextPort++;
}

/**
 * Result of session creation
 */
export interface CreateSessionResult {
  success: boolean;
  sessionId?: string;
  signalingUrl?: string;
  pipelineType?: string;
  error?: string;
  available?: boolean;
}

/**
 * Create a new WebRTC pipeline session
 *
 * This starts a WebRTC server with the specified pipeline and returns
 * the WebSocket signaling URL for the client to connect to.
 *
 * @param pipelineType - Type of pipeline to create ('calculator' | 'echo' | 'tts' | 'audio')
 * @returns Session info including signaling URL
 */
export async function createPipelineSession(
  pipelineType: 'calculator' | 'echo' | 'tts' | 'audio' = 'calculator'
): Promise<CreateSessionResult> {
  try {
    // Check if WebRTC is available
    if (!isWebRtcAvailable()) {
      const loadErr = getLoadError();
      return {
        success: false,
        available: false,
        error: loadErr
          ? `WebRTC FFI not available: ${loadErr.message}`
          : 'WebRTC FFI module not loaded',
      };
    }

    const sessionId = uuidv4();
    const serverId = `webrtc-${sessionId}`;
    const port = allocatePort();

    // Select pipeline manifest based on type
    let manifest: string;
    switch (pipelineType) {
      case 'calculator':
        manifest = PipelineManifests.calculator(`calc-session-${sessionId}`);
        break;
      case 'echo':
        manifest = PipelineManifests.echo(`echo-session-${sessionId}`);
        break;
      case 'tts':
        manifest = PipelineManifests.tts(`tts-session-${sessionId}`);
        break;
      case 'audio':
        manifest = PipelineManifests.audioPassthrough(`audio-session-${sessionId}`);
        break;
      default:
        manifest = PipelineManifests.calculator(`calc-session-${sessionId}`);
    }

    const config: WebRtcServerConfig = {
      port,
      manifest,
      stunServers: DEFAULT_STUN_SERVERS,
      audioCodec: 'opus',
      maxPeers: 10,
    };

    // Create and start the server
    const server = await getOrCreateServer(serverId, config);
    await server.startSignalingServer(port);

    // Determine the signaling URL
    // In production, this would be the public URL of your server
    const host = process.env.WEBRTC_HOST || 'localhost';
    const signalingUrl = `ws://${host}:${port}`;

    // Store session state
    const sessionState: SessionState = {
      sessionId,
      serverId,
      signalingUrl,
      pipelineType,
      createdAt: Date.now(),
    };
    sessions.set(sessionId, sessionState);

    console.log(`[WebRTC] Created session ${sessionId} with ${pipelineType} pipeline on port ${port}`);

    return {
      success: true,
      sessionId,
      signalingUrl,
      pipelineType,
      available: true,
    };
  } catch (error) {
    console.error('[WebRTC] Failed to create session:', error);
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}

/**
 * Get information about an existing session
 */
export async function getSessionInfo(
  sessionId: string
): Promise<SessionState | null> {
  return sessions.get(sessionId) || null;
}

/**
 * Terminate a WebRTC pipeline session
 */
export async function terminateSession(
  sessionId: string
): Promise<{ success: boolean; error?: string }> {
  try {
    const session = sessions.get(sessionId);
    if (!session) {
      return { success: false, error: 'Session not found' };
    }

    await shutdownServer(session.serverId);
    sessions.delete(sessionId);

    console.log(`[WebRTC] Terminated session ${sessionId}`);

    return { success: true };
  } catch (error) {
    console.error('[WebRTC] Failed to terminate session:', error);
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}

/**
 * List all active sessions
 */
export async function listSessions(): Promise<SessionState[]> {
  return Array.from(sessions.values());
}

/**
 * Check if WebRTC FFI is available
 */
export async function checkWebRtcAvailability(): Promise<{
  available: boolean;
  error?: string;
}> {
  const available = isWebRtcAvailable();
  const error = !available ? getLoadError()?.message : undefined;
  return { available, error };
}
