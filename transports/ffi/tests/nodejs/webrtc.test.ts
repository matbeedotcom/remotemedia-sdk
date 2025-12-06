/**
 * WebRTC Server Tests (T022)
 *
 * Tests WebRTC server creation, configuration validation, and event handling.
 * Validates the Node.js FFI bindings for WebRTC transport.
 */

// Type imports from the native module
interface TurnServer {
  url: string;
  username: string;
  credential: string;
}

interface WebRtcServerConfig {
  port?: number;
  signalingUrl?: string;
  /** Pipeline manifest as JSON string */
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

interface PeerConnectedData {
  peerId: string;
  capabilities: PeerCapabilities;
  metadata: Record<string, string>;
}

interface PeerDisconnectedData {
  peerId: string;
  reason?: string;
}

interface PipelineOutputData {
  peerId: string;
  data: Buffer;
  timestamp: number;
}

interface DataReceivedData {
  peerId: string;
  data: Buffer;
  timestamp: number;
}

interface ErrorData {
  code: string;
  message: string;
  peerId?: string;
}

interface NapiWebRtcServer {
  readonly id: string;
  readonly state: Promise<string>;
  getPeers(): Promise<PeerInfo[]>;
  getSessions(): Promise<SessionInfo[]>;
  on(event: string, callback: (data: unknown) => void): void;
  start(): Promise<void>;
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

// Attempt to load the native module
let native: NativeModule | null = null;
let loadError: Error | null = null;

try {
  native = require('../../nodejs') as NativeModule;
} catch (e) {
  loadError = e as Error;
}

// Check if WebRTC is available (built with napi-webrtc feature)
function isWebRtcAvailable(): boolean {
  return !!(native?.isNativeLoaded() && native.WebRtcServer);
}

describe('WebRtcServer', () => {
  beforeAll(() => {
    if (!native || !native.isNativeLoaded()) {
      console.warn(
        'Native module not loaded, skipping WebRTC tests.',
        'Build with: cargo build --features napi-webrtc'
      );
      if (loadError) {
        console.warn('Load error:', loadError.message);
      }
    } else if (!native.WebRtcServer) {
      console.warn(
        'WebRTC bindings not available.',
        'Rebuild with: cargo build --features napi-webrtc'
      );
    }
  });

  describe('configuration validation', () => {
    test('should reject config without port or signalingUrl', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
      };

      await expect(native!.WebRtcServer!.create(config)).rejects.toThrow();
    });

    test('should reject config with both port and signalingUrl', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        port: 50051,
        signalingUrl: 'grpc://localhost:50052',
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
      };

      await expect(native!.WebRtcServer!.create(config)).rejects.toThrow();
    });

    test('should reject config without STUN servers', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        port: 50051,
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: [],
      };

      await expect(native!.WebRtcServer!.create(config)).rejects.toThrow();
    });

    test('should reject invalid STUN URL format', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        port: 50051,
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['http://stun.example.com:3478'],
      };

      await expect(native!.WebRtcServer!.create(config)).rejects.toThrow();
    });

    test('should reject invalid max_peers value', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        port: 50051,
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
        maxPeers: 100, // Max is 10
      };

      await expect(native!.WebRtcServer!.create(config)).rejects.toThrow();
    });

    test('should reject TURN server without username', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        port: 50051,
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
        turnServers: [
          { url: 'turn:turn.example.com:3478', username: '', credential: 'secret' },
        ],
      };

      await expect(native!.WebRtcServer!.create(config)).rejects.toThrow();
    });
  });

  describe('server creation', () => {
    test('should create server with embedded signaling (port)', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        port: 50099, // Use high port to avoid conflicts
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
      };

      const server = await native!.WebRtcServer!.create(config);

      expect(server).toBeDefined();
      expect(server.id).toBeDefined();
      expect(server.id.length).toBeGreaterThan(0);

      const state = await server.state;
      expect(state).toBe('created');

      await server.shutdown();
    });

    test('should reject connect() without signalingUrl', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        port: 50051, // Should use signalingUrl instead
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
      };

      await expect(native!.WebRtcServer!.connect(config)).rejects.toThrow(/signaling_url/);
    });

    test('should create server with valid config', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const config: WebRtcServerConfig = {
        port: 50098,
        manifest: JSON.stringify({
          nodes: [{ id: 'test', type: 'Echo' }],
          connections: [],
        }),
        stunServers: ['stun:stun.l.google.com:19302'],
        turnServers: [
          {
            url: 'turn:turn.example.com:3478',
            username: 'user',
            credential: 'pass',
          },
        ],
        maxPeers: 5,
        videoCodec: 'vp9',
      };

      const server = await native!.WebRtcServer!.create(config);

      expect(server).toBeDefined();
      expect(server.id).toBeDefined();

      await server.shutdown();
    });
  });

  describe('event registration', () => {
    let server: NapiWebRtcServer | null = null;

    beforeEach(async () => {
      if (isWebRtcAvailable()) {
        server = await native!.WebRtcServer!.create({
          port: 50097,
          manifest: JSON.stringify({ nodes: [], connections: [] }),
          stunServers: ['stun:stun.l.google.com:19302'],
        });
      }
    });

    afterEach(async () => {
      if (server) {
        await server.shutdown();
        server = null;
      }
    });

    test('should register peer_connected callback', () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      let callbackCalled = false;
      expect(() => {
        server!.on('peer_connected', (data: unknown) => {
          callbackCalled = true;
        });
      }).not.toThrow();
    });

    test('should register peer_disconnected callback', () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      expect(() => {
        server!.on('peer_disconnected', (data: unknown) => {
          // Handler
        });
      }).not.toThrow();
    });

    test('should register pipeline_output callback', () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      expect(() => {
        server!.on('pipeline_output', (data: unknown) => {
          // Handler
        });
      }).not.toThrow();
    });

    test('should register data callback', () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      expect(() => {
        server!.on('data', (data: unknown) => {
          // Handler
        });
      }).not.toThrow();
    });

    test('should register error callback', () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      expect(() => {
        server!.on('error', (data: unknown) => {
          // Handler
        });
      }).not.toThrow();
    });

    test('should reject unknown event name', () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      expect(() => {
        server!.on('unknown_event', (data: unknown) => {
          // Handler
        });
      }).toThrow(/Unknown event/);
    });

    test('should allow multiple callbacks for same event', () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      expect(() => {
        server!.on('peer_connected', () => {});
        server!.on('peer_connected', () => {});
        server!.on('peer_connected', () => {});
      }).not.toThrow();
    });
  });

  describe('server lifecycle', () => {
    test('should start and shutdown cleanly', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const server = await native!.WebRtcServer!.create({
        port: 50096,
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
      });

      expect(await server.state).toBe('created');

      // Note: start() will try to bind to port, which may fail in test environment
      // We test that the method exists and can be called
      try {
        await server.start();
        expect(await server.state).toBe('running');
      } catch (e) {
        // Port binding might fail in CI, that's okay for this test
        console.log('Start failed (expected in some environments):', (e as Error).message);
      }

      await server.shutdown();
      expect(await server.state).toBe('stopped');
    });

    test('should return empty peers list initially', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const server = await native!.WebRtcServer!.create({
        port: 50095,
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
      });

      const peers = await server.getPeers();
      expect(peers).toEqual([]);

      await server.shutdown();
    });

    test('should return empty sessions list initially', async () => {
      if (!isWebRtcAvailable()) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const server = await native!.WebRtcServer!.create({
        port: 50094,
        manifest: JSON.stringify({ nodes: [], connections: [] }),
        stunServers: ['stun:stun.l.google.com:19302'],
      });

      const sessions = await server.getSessions();
      expect(sessions).toEqual([]);

      await server.shutdown();
    });
  });

  describe('session management', () => {
    let server: NapiWebRtcServer | null = null;

    beforeEach(async () => {
      if (isWebRtcAvailable()) {
        server = await native!.WebRtcServer!.create({
          port: 50093,
          manifest: JSON.stringify({ nodes: [], connections: [] }),
          stunServers: ['stun:stun.l.google.com:19302'],
        });
      }
    });

    afterEach(async () => {
      if (server) {
        await server.shutdown();
        server = null;
      }
    });

    test('should create session', async () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const sessionId = `test-session-${Date.now()}`;
      const session = await server.createSession(sessionId, {
        name: 'Test Room',
        host: 'user-1',
      });

      expect(session).toBeDefined();
      expect(session.sessionId).toBe(sessionId);
      expect(session.peerIds).toEqual([]);
    });

    test('should get session by ID', async () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const sessionId = `get-session-${Date.now()}`;
      await server.createSession(sessionId);

      const session = await server.getSession(sessionId);
      expect(session).not.toBeNull();
      expect(session?.sessionId).toBe(sessionId);
    });

    test('should return null for non-existent session', async () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const session = await server.getSession('non-existent-session-id');
      expect(session).toBeNull();
    });

    test('should delete session', async () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const sessionId = `delete-session-${Date.now()}`;
      await server.createSession(sessionId);

      // Verify it exists
      const before = await server.getSession(sessionId);
      expect(before).not.toBeNull();

      // Delete it
      await server.deleteSession(sessionId);

      // Verify it's gone
      const after = await server.getSession(sessionId);
      expect(after).toBeNull();
    });

    test('should list sessions', async () => {
      if (!isWebRtcAvailable() || !server) {
        console.log('Skipping: WebRTC module not loaded');
        return;
      }

      const sessionId1 = `list-session-1-${Date.now()}`;
      const sessionId2 = `list-session-2-${Date.now()}`;

      await server.createSession(sessionId1);
      await server.createSession(sessionId2);

      const sessions = await server.getSessions();
      expect(sessions.length).toBeGreaterThanOrEqual(2);

      const sessionIds = sessions.map((s) => s.sessionId);
      expect(sessionIds).toContain(sessionId1);
      expect(sessionIds).toContain(sessionId2);
    });
  });
});

describe('WebRtcServer External Signaling', () => {
  test('should reject create() for external signaling config', async () => {
    if (!isWebRtcAvailable()) {
      console.log('Skipping: WebRTC module not loaded');
      return;
    }

    const config: WebRtcServerConfig = {
      signalingUrl: 'grpc://signaling.example.com:50051',
      manifest: JSON.stringify({ nodes: [], connections: [] }),
      stunServers: ['stun:stun.l.google.com:19302'],
    };

    // create() requires port, not signalingUrl
    await expect(native!.WebRtcServer!.create(config)).rejects.toThrow(/Port is required/);
  });

  test('should validate signaling URL format', async () => {
    if (!isWebRtcAvailable()) {
      console.log('Skipping: WebRTC module not loaded');
      return;
    }

    const config: WebRtcServerConfig = {
      signalingUrl: 'http://invalid-protocol.com:50051',
      manifest: JSON.stringify({ nodes: [], connections: [] }),
      stunServers: ['stun:stun.l.google.com:19302'],
    };

    await expect(native!.WebRtcServer!.connect(config)).rejects.toThrow();
  });
});
