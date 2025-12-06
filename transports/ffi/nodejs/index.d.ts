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
