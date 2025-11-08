/**
 * gRPC Session Manager
 *
 * Maintains persistent gRPC streaming sessions across multiple HTTP requests.
 * This allows conversation history to persist in the Rust runtime's StreamSession.
 */

import { RemoteMediaClient, PersistentStreamSession } from '../../../../nodejs-client/dist/src/grpc-client.js';
import type { PipelineManifest } from '../../../../nodejs-client/dist/src/grpc-client.js';

interface ActiveSession {
  sessionId: string;
  stream: PersistentStreamSession;
  lastUsedAt: number;
  manifest: PipelineManifest;
  resultListenerActive?: boolean;
}

class GRPCSessionManager {
  private sessions: Map<string, ActiveSession> = new Map();
  private readonly SESSION_TIMEOUT = 30 * 60 * 1000; // 30 minutes
  private cleanupTimer: NodeJS.Timeout | null = null;

  constructor() {
    this.startCleanupTimer();
  }

  /**
   * Get or create a streaming session
   */
  async getOrCreateSession(
    sessionId: string,
    client: RemoteMediaClient,
    manifest: PipelineManifest
  ): Promise<ActiveSession> {
    // Check if session exists and is still valid
    const existing = this.sessions.get(sessionId);
    if (existing) {
      existing.lastUsedAt = Date.now();
      console.log(`[SessionManager] ⚠️  REUSING EXISTING SESSION: ${sessionId}`);
      console.log(`[SessionManager] Existing pipeline nodes:`);
      existing.manifest.nodes.forEach((node: any, idx: number) => {
        console.log(`[SessionManager]   ${idx + 1}. ${node.id} (${node.nodeType})`);
      });
      console.log(`[SessionManager] This session will NOT be recreated with the new manifest!`);
      return existing;
    }

    // Create new streaming session
    console.log(`[SessionManager] Creating new session: ${sessionId}`);
    console.log(`[SessionManager] ========================================`);
    console.log(`[SessionManager] MANIFEST RECEIVED FOR SESSION ${sessionId}:`);
    console.log(`[SessionManager] ========================================`);
    console.log(JSON.stringify(manifest, null, 2));
    console.log(`[SessionManager] ========================================`);
    console.log(`[SessionManager] Pipeline nodes for ${sessionId}:`);
    manifest.nodes.forEach((node: any, idx: number) => {
      console.log(`[SessionManager]   ${idx + 1}. ${node.id} (${node.nodeType})`);
    });
    console.log(`[SessionManager] ========================================`);

    // Create persistent streaming session
    const stream = client.createPersistentStreamSession(manifest);

    // Wait a moment for StreamReady message
    await new Promise((resolve) => setTimeout(resolve, 100));

    // Get the actual gRPC session ID
    const grpcSessionId = stream.getSessionId();
    console.log(`[SessionManager] gRPC session ID: ${grpcSessionId}`);

    // Create session object
    const session: ActiveSession = {
      sessionId,
      stream,
      lastUsedAt: Date.now(),
      manifest,
    };

    this.sessions.set(sessionId, session);
    return session;
  }

  /**
   * Close a specific session
   */
  async closeSession(sessionId: string): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (session) {
      await session.stream.close();
      this.sessions.delete(sessionId);
    }
  }

  /**
   * Get session if it exists
   */
  getSession(sessionId: string): ActiveSession | undefined {
    return this.sessions.get(sessionId);
  }

  /**
   * List all sessions (for debugging)
   */
  listSessions(): string[] {
    return Array.from(this.sessions.keys());
  }

  /**
   * Start a background result listener for a session
   */
  async startResultListener(
    sessionId: string,
    callback: (result: any) => void,
    onError?: (error: Error) => void
  ): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Session ${sessionId} not found`);
    }

    if (session.resultListenerActive) {
      console.log(`[SessionManager] Result listener already active for session: ${sessionId}`);
      return;
    }

    session.resultListenerActive = true;
    console.log(`[SessionManager] Starting result listener for session: ${sessionId}`);

    // Run listener in background
    (async () => {
      try {
        for await (const chunk of session.stream.getResults()) {
          // Update last used time
          session.lastUsedAt = Date.now();

          // Forward result to callback
          callback(chunk);
        }
      } catch (error) {
        console.error(`[SessionManager] Result listener error for session ${sessionId}:`, error);
        if (onError) {
          onError(error instanceof Error ? error : new Error(String(error)));
        }
      } finally {
        session.resultListenerActive = false;
        console.log(`[SessionManager] Result listener stopped for session: ${sessionId}`);
      }
    })();
  }

  /**
   * Start cleanup timer for idle sessions
   */
  private startCleanupTimer(): void {
    if (this.cleanupTimer) {
      clearTimeout(this.cleanupTimer);
    }

    this.cleanupTimer = setTimeout(async () => {
      const now = Date.now();
      const toDelete: string[] = [];

      // Find expired sessions
      for (const [sessionId, session] of this.sessions.entries()) {
        if (now - session.lastUsedAt >= this.SESSION_TIMEOUT) {
          console.log(`[SessionManager] Session expired: ${sessionId}`);
          toDelete.push(sessionId);
        }
      }

      // Close and remove expired sessions
      for (const sessionId of toDelete) {
        await this.closeSession(sessionId);
      }

      // Schedule next cleanup
      this.startCleanupTimer();
    }, 60 * 1000); // Check every minute

    // Don't block Node.js exit
    this.cleanupTimer.unref();
  }

  /**
   * Close all sessions
   */
  async closeAll(): Promise<void> {
    console.log('[SessionManager] Closing all sessions');
    const promises = Array.from(this.sessions.keys()).map((id) =>
      this.closeSession(id)
    );
    await Promise.all(promises);

    if (this.cleanupTimer) {
      clearTimeout(this.cleanupTimer);
      this.cleanupTimer = null;
    }
  }
}

// Export singleton instance
const sessionManager = new GRPCSessionManager();

export default sessionManager;

// Cleanup on process exit
if (typeof process !== 'undefined') {
  process.on('beforeExit', async () => {
    await sessionManager.closeAll();
  });
}
