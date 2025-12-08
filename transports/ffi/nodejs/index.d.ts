/**
 * RemoteMedia Native Node.js Bindings
 *
 * Zero-copy IPC between Node.js, Python, and Rust via iceoryx2.
 */

// =============================================================================
// Data Types
// =============================================================================

export enum DataType {
  Audio = 1,
  Video = 2,
  Text = 3,
  Tensor = 4,
  ControlMessage = 5,
  Numpy = 6,
}

export enum PixelFormat {
  RGB24 = 0,
  RGBA32 = 1,
  YUV420P = 2,
  GRAY8 = 3,
}

export enum VideoCodec {
  Raw = 0,
  H264 = 1,
  VP8 = 2,
  VP9 = 3,
  AV1 = 4,
}

export enum TensorDtype {
  F32 = 0,
  F16 = 1,
  I32 = 2,
  I8 = 3,
  U8 = 4,
}

// =============================================================================
// Data Structures
// =============================================================================

export interface AudioBuffer {
  sampleRate: number;
  channels: number;
  numSamples: bigint;
  samples: Float32Array;
}

export interface VideoFrame {
  width: number;
  height: number;
  format: PixelFormat;
  codec: VideoCodec;
  frameNum: bigint;
  isKeyframe: boolean;
  pixelData: Uint8Array;
}

export interface TextBuffer {
  text: string;
  language?: string;
}

export interface TensorBuffer {
  shape: number[];
  dtype: TensorDtype;
  data: Uint8Array;
}

export interface ControlMessage {
  type: string;
  payload: Record<string, unknown>;
  cancelRange?: { start: number; end: number };
}

export type RuntimeData =
  | { type: DataType.Audio; data: AudioBuffer; sessionId: string; timestampNs: bigint }
  | { type: DataType.Video; data: VideoFrame; sessionId: string; timestampNs: bigint }
  | { type: DataType.Text; data: TextBuffer; sessionId: string; timestampNs: bigint }
  | { type: DataType.Tensor; data: TensorBuffer; sessionId: string; timestampNs: bigint }
  | { type: DataType.ControlMessage; data: ControlMessage; sessionId: string; timestampNs: bigint };

// =============================================================================
// Type Guards
// =============================================================================

export function isAudio(data: RuntimeData): data is RuntimeData & { type: DataType.Audio };
export function isVideo(data: RuntimeData): data is RuntimeData & { type: DataType.Video };
export function isText(data: RuntimeData): data is RuntimeData & { type: DataType.Text };
export function isTensor(data: RuntimeData): data is RuntimeData & { type: DataType.Tensor };
export function isControlMessage(data: RuntimeData): data is RuntimeData & { type: DataType.ControlMessage };

// =============================================================================
// Sample Types
// =============================================================================

/**
 * A received sample with zero-copy buffer access.
 */
export interface ReceivedSample {
  readonly buffer: Buffer;
  readonly size: number;
  readonly isReleased: boolean;
  readonly timestampNs: bigint;
  release(): void;
  toRuntimeData(): RuntimeData;
}

/**
 * A loaned sample buffer for zero-copy publishing.
 */
export interface LoanedSample {
  readonly buffer: Buffer;
  readonly size: number;
  readonly isConsumed: boolean;
  send(): void;
  release(): void;
}

// =============================================================================
// Subscriber Interface
// =============================================================================

export interface Subscriber {
  readonly channelName: string;
  readonly isValid: boolean;
  readonly pendingCount: number;
  readonly bufferSize: number;
  receive(): ReceivedSample | null;
  receiveTimeout(timeoutMs: number): Promise<ReceivedSample | null>;
  receiveAsync(): Promise<ReceivedSample>;
  onData(callback: (sample: ReceivedSample) => void): () => void;
  [Symbol.asyncIterator](): AsyncIterableIterator<ReceivedSample>;
  close(): void;
}

export interface SubscriberStats {
  samplesReceived: number;
  bytesReceived: bigint;
  samplesDropped: number;
  publisherCount: number;
  averageLatencyUs: number;
}

export interface SubscriberWithStats extends Subscriber {
  getStats(): SubscriberStats;
  resetStats(): void;
}

// =============================================================================
// Publisher Interface
// =============================================================================

export interface Publisher {
  readonly channelName: string;
  readonly isValid: boolean;
  readonly loanedCount: number;
  readonly maxLoans: number;
  loan(size: number): LoanedSample;
  tryLoan(size: number): LoanedSample | null;
  publish(data: RuntimeData): void;
  close(): void;
}

export interface PublisherStats {
  samplesSent: number;
  bytesSent: bigint;
  samplesDropped: number;
  subscriberCount: number;
}

export interface PublisherWithStats extends Publisher {
  getStats(): PublisherStats;
  resetStats(): void;
}

// =============================================================================
// Channel Interface
// =============================================================================

export interface ChannelConfig {
  capacity?: number;
  maxPayloadSize?: number;
  backpressure?: boolean;
  historySize?: number;
}

export interface Channel {
  readonly name: string;
  readonly config: Readonly<ChannelConfig>;
  readonly isOpen: boolean;
  createPublisher(): Publisher;
  createSubscriber(bufferSize?: number): Subscriber;
  close(): void;
}

// =============================================================================
// Session Interface
// =============================================================================

export interface SessionConfig {
  id: string;
  defaultChannelConfig?: ChannelConfig;
}

export interface Session {
  readonly id: string;
  readonly isActive: boolean;
  channel(name: string, config?: ChannelConfig): Channel;
  listChannels(): string[];
  close(): void;
}

// =============================================================================
// Factory Functions
// =============================================================================

export function createSession(config: SessionConfig): Session;
export function getSession(sessionId: string): Session | null;
export function listSessions(): string[];

// =============================================================================
// IPC Node Management
// =============================================================================

export interface IpcNode {
  readonly name: string;
  readonly isValid: boolean;
  close(): void;
}

export function createIpcNode(name?: string): IpcNode;

// =============================================================================
// WebRTC Types (requires napi-webrtc feature)
// =============================================================================

/** TURN server configuration */
export interface TurnServer {
  url: string;
  username: string;
  credential: string;
}

/** WebRTC server configuration */
export interface WebRtcServerConfig {
  /** Port for embedded signaling server (mutually exclusive with signalingUrl) */
  port?: number;
  /** URL for external signaling server (mutually exclusive with port) */
  signalingUrl?: string;
  /** Pipeline manifest as JSON string */
  manifest: string;
  /** STUN server URLs (at least one required) */
  stunServers: string[];
  /** TURN server configurations (optional) */
  turnServers?: TurnServer[];
  /** Maximum concurrent peers (1-10, default 10) */
  maxPeers?: number;
  /** Audio codec (only "opus" supported) */
  audioCodec?: 'opus';
  /** Video codec */
  videoCodec?: 'vp8' | 'vp9' | 'h264';
}

/** Peer media capabilities */
export interface PeerCapabilities {
  audio: boolean;
  video: boolean;
  data: boolean;
}

/** Connected peer information */
export interface PeerInfo {
  peerId: string;
  capabilities: PeerCapabilities;
  metadata: Record<string, string>;
  state: 'connecting' | 'connected' | 'disconnecting' | 'disconnected';
  connectedAt: number;
}

/** Session (room) information */
export interface SessionInfo {
  sessionId: string;
  peerIds: string[];
  createdAt: number;
  metadata: Record<string, string>;
}

// WebRTC Event Data Types

export interface PeerConnectedData {
  peerId: string;
  capabilities: PeerCapabilities;
  metadata: Record<string, string>;
}

export interface PeerDisconnectedData {
  peerId: string;
  reason?: string;
}

export interface PipelineOutputData {
  peerId: string;
  data: Buffer;
  timestamp: number;
}

export interface DataReceivedData {
  peerId: string;
  data: Buffer;
  timestamp: number;
}

export interface ErrorData {
  code: string;
  message: string;
  peerId?: string;
}

export interface SessionEventData {
  sessionId: string;
  eventType: 'peer_joined' | 'peer_left';
  peerId: string;
}

/** WebRTC Session for room/group management */
export interface WebRtcSession {
  /** Session identifier */
  readonly sessionId: string;
  /** Peer IDs in this session */
  readonly peers: Promise<string[]>;
  /** Session creation timestamp */
  readonly createdAt: Promise<number>;
  /** Session metadata */
  readonly metadata: Promise<Record<string, string>>;

  /**
   * Register event listener for session events
   * @param event - Event name ('peer_joined', 'peer_left')
   * @param callback - Event handler
   */
  on(event: 'peer_joined', callback: (peerId: string) => void): void;
  on(event: 'peer_left', callback: (peerId: string) => void): void;

  /**
   * Broadcast data to all peers in the session
   * @param data - Data to broadcast
   */
  broadcast(data: Buffer): Promise<void>;

  /**
   * Send data to a specific peer in the session
   * @param peerId - Target peer ID
   * @param data - Data to send
   */
  sendToPeer(peerId: string, data: Buffer): Promise<void>;

  /**
   * Add a peer to this session
   * @param peerId - Peer ID to add
   */
  addPeer(peerId: string): Promise<void>;

  /**
   * Remove a peer from this session
   * @param peerId - Peer ID to remove
   */
  removePeer(peerId: string): Promise<void>;

  /**
   * Get session info
   */
  getInfo(): Promise<SessionInfo>;
}

/** WebRTC server for real-time media streaming */
export interface WebRtcServer {
  /** Server unique identifier */
  readonly id: string;
  /** Current server state */
  readonly state: Promise<'created' | 'starting' | 'running' | 'stopping' | 'stopped'>;

  /**
   * Register event listener
   * @param event - Event name
   * @param callback - Event handler
   */
  on(event: 'peer_connected', callback: (data: PeerConnectedData) => void): void;
  on(event: 'peer_disconnected', callback: (data: PeerDisconnectedData) => void): void;
  on(event: 'pipeline_output', callback: (data: PipelineOutputData) => void): void;
  on(event: 'data', callback: (data: DataReceivedData) => void): void;
  on(event: 'error', callback: (data: ErrorData) => void): void;
  on(event: 'session', callback: (data: SessionEventData) => void): void;

  /** Start the server */
  start(): Promise<void>;

  /** Stop the server gracefully */
  shutdown(): Promise<void>;

  /** Get connected peers */
  getPeers(): Promise<PeerInfo[]>;

  /** Get active sessions */
  getSessions(): Promise<SessionInfo[]>;

  /**
   * Send data to a specific peer
   * @param peerId - Target peer ID
   * @param data - Data to send
   */
  sendToPeer(peerId: string, data: Buffer): Promise<void>;

  /**
   * Broadcast data to all connected peers
   * @param data - Data to broadcast
   */
  broadcast(data: Buffer): Promise<void>;

  /**
   * Disconnect a peer
   * @param peerId - Peer to disconnect
   * @param reason - Optional disconnect reason
   */
  disconnectPeer(peerId: string, reason?: string): Promise<void>;

  /**
   * Create a new session (room)
   * @param sessionId - Unique session identifier
   * @param metadata - Optional session metadata
   * @returns WebRtcSession instance
   */
  createSession(sessionId: string, metadata?: Record<string, string>): Promise<WebRtcSession>;

  /**
   * Create a new session and return info only
   * @param sessionId - Unique session identifier
   * @param metadata - Optional session metadata
   * @returns SessionInfo
   */
  createSessionInfo(sessionId: string, metadata?: Record<string, string>): Promise<SessionInfo>;

  /**
   * Get an existing session
   * @param sessionId - Session identifier
   */
  getSession(sessionId: string): Promise<SessionInfo | null>;

  /**
   * Delete a session
   * @param sessionId - Session to delete
   */
  deleteSession(sessionId: string): Promise<void>;
}

/** WebRTC server factory */
export interface WebRtcServerFactory {
  /**
   * Create a server with embedded signaling
   * @param config - Server configuration with port set
   */
  create(config: WebRtcServerConfig): Promise<WebRtcServer>;

  /**
   * Connect to an external signaling server
   * @param config - Server configuration with signalingUrl set
   */
  connect(config: WebRtcServerConfig): Promise<WebRtcServer>;
}

/**
 * WebRTC server module (available when built with napi-webrtc feature)
 */
export const WebRtcServer: WebRtcServerFactory | undefined;

// =============================================================================
// Proto Utils (Browser/Node.js compatible)
// =============================================================================

export * from './proto-utils';

import * as protoUtilsModule from './proto-utils';
export const protoUtils: typeof protoUtilsModule;
