/**
 * Persistent gRPC Client Pool
 *
 * Maintains long-lived gRPC connections to enable node caching on the server.
 * Instead of connecting/disconnecting for each request, we keep connections alive.
 */

import { RemoteMediaClient } from '../../../../nodejs-client/dist/src/grpc-client.js';

class GRPCClientPool {
  private client: RemoteMediaClient | null = null;
  private connectionPromise: Promise<void> | null = null;
  private lastUsedAt: number = Date.now();
  private readonly IDLE_TIMEOUT = 10 * 60 * 1000; // 10 minutes
  private readonly host: string;
  private cleanupTimer: NodeJS.Timeout | null = null;

  constructor(host: string = 'localhost:50051') {
    this.host = host;
  }

  /**
   * Get a connected client instance
   */
  async getClient(): Promise<RemoteMediaClient> {
    this.lastUsedAt = Date.now();

    // If client exists and is connected, return it
    if (this.client) {
      return this.client;
    }

    // If connection is in progress, wait for it
    if (this.connectionPromise) {
      await this.connectionPromise;
      return this.client!;
    }

    // Create new connection
    this.connectionPromise = this.connect();
    await this.connectionPromise;
    this.connectionPromise = null;

    return this.client!;
  }

  /**
   * Connect to gRPC server
   */
  private async connect(): Promise<void> {
    console.log('[GRPCClientPool] Connecting to gRPC server:', this.host);

    this.client = new RemoteMediaClient(this.host);
    await this.client.connect();

    console.log('[GRPCClientPool] Connected successfully');

    // Start cleanup timer
    this.startCleanupTimer();
  }

  /**
   * Start idle timeout cleanup timer
   */
  private startCleanupTimer(): void {
    if (this.cleanupTimer) {
      clearTimeout(this.cleanupTimer);
    }

    this.cleanupTimer = setTimeout(() => {
      const idleTime = Date.now() - this.lastUsedAt;
      if (idleTime >= this.IDLE_TIMEOUT) {
        console.log('[GRPCClientPool] Client idle for 10 minutes, disconnecting...');
        this.disconnect();
      } else {
        // Check again later
        this.startCleanupTimer();
      }
    }, this.IDLE_TIMEOUT);

    // Don't block Node.js exit
    this.cleanupTimer.unref();
  }

  /**
   * Disconnect and cleanup
   */
  async disconnect(): Promise<void> {
    if (this.cleanupTimer) {
      clearTimeout(this.cleanupTimer);
      this.cleanupTimer = null;
    }

    if (this.client) {
      console.log('[GRPCClientPool] Disconnecting from gRPC server');
      await this.client.disconnect();
      this.client = null;
    }
  }

  /**
   * Force reconnect (useful for error recovery)
   */
  async reconnect(): Promise<void> {
    await this.disconnect();
    await this.getClient();
  }
}

// Export singleton instance
const clientPool = new GRPCClientPool(process.env.GRPC_HOST || 'localhost:50051');

export default clientPool;

// Cleanup on process exit
if (typeof process !== 'undefined') {
  process.on('beforeExit', async () => {
    await clientPool.disconnect();
  });
}
