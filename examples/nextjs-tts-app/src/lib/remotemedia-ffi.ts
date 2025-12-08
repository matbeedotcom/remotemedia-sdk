/**
 * RemoteMedia Node.js FFI Server Wrapper
 *
 * This module provides a server-side wrapper for the RemoteMedia Node.js FFI bindings.
 * It manages WebRTC server instances with pipeline manifests for real-time media processing.
 *
 * Usage:
 *   import { createWebRtcServer, getOrCreateServer } from '@/lib/remotemedia-ffi';
 *
 * IMPORTANT: This module should only be imported on the server side (Server Components,
 * Server Actions, API Routes). It will not work in the browser.
 */

import 'server-only';

// Type definitions for the native module
interface TurnServer {
  url: string;
  username: string;
  credential: string;
}

interface WebRtcServerConfig {
  port?: number;
  signalingUrl?: string;
  manifest: string;
  stunServers: string[];
  turnServers?: TurnServer[];
  maxPeers?: number;
  audioCodec?: 'opus';
  videoCodec?: 'vp8' | 'vp9' | 'h264';
}

interface PeerCapabilities {
  audio: boolean;
  video: boolean;
  data: boolean;
}

interface PeerInfo {
  peerId: string;
  capabilities: PeerCapabilities;
  metadata: Record<string, string>;
  state: string;
  connectedAt: number;
}

interface SessionInfo {
  sessionId: string;
  metadata: Record<string, string>;
  peerIds: string[];
  createdAt: number;
}

interface NapiWebRtcServer {
  readonly id: string;
  readonly state: Promise<string>;
  getPeers(): Promise<PeerInfo[]>;
  getSessions(): Promise<SessionInfo[]>;
  on(event: string, callback: (data: unknown) => void): void;
  start(): Promise<void>;
  startSignalingServer(port: number): Promise<void>;
  shutdown(): Promise<void>;
  sendToPeer(peerId: string, data: Buffer): Promise<void>;
  broadcast(data: Buffer): Promise<void>;
  disconnectPeer(peerId: string, reason?: string): Promise<void>;
  createSession(sessionId: string, metadata?: object): Promise<SessionInfo>;
  getSession(sessionId: string): Promise<SessionInfo | null>;
  deleteSession(sessionId: string): Promise<void>;
}

interface WebRtcModule {
  create(config: WebRtcServerConfig): Promise<NapiWebRtcServer>;
  connect(config: WebRtcServerConfig): Promise<NapiWebRtcServer>;
}

interface NativeModule {
  WebRtcServer?: WebRtcModule;
  isNativeLoaded(): boolean;
  getLoadError(): Error | null;
}

// Global server instances (server-side singleton pattern)
const serverInstances = new Map<string, NapiWebRtcServer>();
let nativeModule: NativeModule | null = null;
let loadError: Error | null = null;

/**
 * Load the native FFI module
 */
function loadNativeModule(): NativeModule | null {
  if (nativeModule !== null) {
    return nativeModule;
  }

  try {
    // Try to load the native module from the transports/ffi/nodejs package
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    nativeModule = require('@remotemedia/nodejs-ffi') as NativeModule;
    return nativeModule;
  } catch (e) {
    loadError = e as Error;
    console.warn('[RemoteMedia FFI] Failed to load native module:', (e as Error).message);
    return null;
  }
}

/**
 * Check if the WebRTC native module is available
 */
export function isWebRtcAvailable(): boolean {
  const native = loadNativeModule();
  return !!(native?.isNativeLoaded() && native.WebRtcServer);
}

/**
 * Get the load error if the native module failed to load
 */
export function getLoadError(): Error | null {
  loadNativeModule();
  return loadError;
}

/**
 * Pipeline manifest builders for common use cases
 */
export const PipelineManifests = {
  /**
   * Echo pipeline - returns input unchanged
   */
  echo(name: string = 'echo-pipeline'): string {
    return JSON.stringify({
      version: '1.0',
      metadata: { name },
      nodes: [
        {
          id: 'echo',
          node_type: 'Echo',
          config: {},
        },
      ],
      connections: [],
    });
  },

  /**
   * Calculator pipeline - processes JSON calculator requests
   * Input: { operation: 'add'|'subtract'|'multiply'|'divide', operands: number[] }
   * Output: { operation, operands, result: number }
   */
  calculator(name: string = 'calculator-pipeline'): string {
    return JSON.stringify({
      version: '1.0',
      metadata: { name },
      nodes: [
        {
          id: 'calculator',
          node_type: 'CalculatorNode',
          config: {},
        },
      ],
      connections: [],
    });
  },

  /**
   * Audio passthrough pipeline
   */
  audioPassthrough(name: string = 'audio-pipeline'): string {
    return JSON.stringify({
      version: '1.0',
      metadata: { name },
      nodes: [
        {
          id: 'audio_input',
          node_type: 'AudioInput',
          config: { sample_rate: 48000, channels: 1 },
        },
        {
          id: 'audio_passthrough',
          node_type: 'Echo',
          config: {},
        },
      ],
      connections: [
        { source: 'audio_input', destination: 'audio_passthrough' },
      ],
    });
  },

  /**
   * TTS pipeline - text-to-speech synthesis
   */
  tts(name: string = 'tts-pipeline', voice: string = 'af_bella'): string {
    return JSON.stringify({
      version: '1.0',
      metadata: { name },
      nodes: [
        {
          id: 'tts',
          node_type: 'KokoroTTS',
          config: { voice, language: 'en-us' },
        },
      ],
      connections: [],
    });
  },
};

/**
 * Create a new WebRTC server with the specified configuration
 */
export async function createWebRtcServer(
  config: WebRtcServerConfig
): Promise<NapiWebRtcServer> {
  const native = loadNativeModule();
  if (!native?.WebRtcServer) {
    throw new Error('WebRTC native module not available. Ensure @remotemedia/nodejs-ffi is installed.');
  }

  const server = await native.WebRtcServer.create(config);
  return server;
}

/**
 * Get or create a singleton WebRTC server instance
 *
 * @param serverId - Unique identifier for the server instance
 * @param config - Server configuration (only used on first creation)
 * @returns The WebRTC server instance
 */
export async function getOrCreateServer(
  serverId: string,
  config: WebRtcServerConfig
): Promise<NapiWebRtcServer> {
  let server = serverInstances.get(serverId);

  if (!server) {
    server = await createWebRtcServer(config);
    serverInstances.set(serverId, server);
  }

  return server;
}

/**
 * Get an existing server instance by ID
 */
export function getServer(serverId: string): NapiWebRtcServer | undefined {
  return serverInstances.get(serverId);
}

/**
 * Shutdown and remove a server instance
 */
export async function shutdownServer(serverId: string): Promise<void> {
  const server = serverInstances.get(serverId);
  if (server) {
    await server.shutdown();
    serverInstances.delete(serverId);
  }
}

/**
 * Shutdown all server instances
 */
export async function shutdownAllServers(): Promise<void> {
  const shutdownPromises = Array.from(serverInstances.values()).map((server) =>
    server.shutdown()
  );
  await Promise.all(shutdownPromises);
  serverInstances.clear();
}

/**
 * List all active server IDs
 */
export function listServers(): string[] {
  return Array.from(serverInstances.keys());
}

export type {
  WebRtcServerConfig,
  PeerCapabilities,
  PeerInfo,
  SessionInfo,
  NapiWebRtcServer,
  TurnServer,
};
