/**
 * Type-safe Data Types for Generic Streaming Protocol (Feature 004)
 *
 * Provides compile-time type safety for RemoteMedia SDK's universal streaming protocol.
 * Supports 6 data types: Audio, Video, Tensor, JSON, Text, Binary
 */

/**
 * Audio sample format enum
 */
export enum AudioFormat {
  F32 = 'F32',
  I16 = 'I16',
  I32 = 'I32',
}

/**
 * Video pixel format enum
 */
export enum PixelFormat {
  RGB24 = 'RGB24',
  RGBA32 = 'RGBA32',
  YUV420P = 'YUV420P',
  GRAY8 = 'GRAY8',
}

/**
 * Tensor data type enum
 */
export enum TensorDtype {
  F32 = 'F32',
  F16 = 'F16',
  I32 = 'I32',
  I8 = 'I8',
  U8 = 'U8',
}

/**
 * Data type hint for validation
 */
export enum DataTypeHint {
  AUDIO = 'AUDIO',
  VIDEO = 'VIDEO',
  TENSOR = 'TENSOR',
  JSON = 'JSON',
  TEXT = 'TEXT',
  BINARY = 'BINARY',
  ANY = 'ANY',
}

/**
 * Audio buffer with multi-channel PCM data
 */
export interface AudioBuffer {
  samples: Uint8Array;
  sampleRate: number;
  channels: number;
  format: AudioFormat;
  numSamples: number;
}

/**
 * Video frame with pixel data
 */
export interface VideoFrame {
  pixelData: Uint8Array;
  width: number;
  height: number;
  format: PixelFormat;
  frameNumber: number;
  timestampUs: number;
}

/**
 * Multi-dimensional tensor buffer
 */
export interface TensorBuffer {
  data: Uint8Array;
  shape: number[];
  dtype: TensorDtype;
  layout?: string;
}

/**
 * JSON data with optional schema
 */
export interface JsonData {
  jsonPayload: string; // JSON as string
  schemaType?: string;
}

/**
 * Text buffer with encoding info
 */
export interface TextBuffer {
  textData: Uint8Array;
  encoding?: string;
  language?: string;
}

/**
 * Binary data with MIME type
 */
export interface BinaryBuffer {
  data: Uint8Array;
  mimeType?: string;
}

/**
 * Discriminated union for all data buffer types
 *
 * This enables TypeScript's type narrowing based on the `type` field.
 *
 * @example
 * ```typescript
 * function processData(buffer: DataBuffer) {
 *   if (buffer.type === 'audio') {
 *     // TypeScript knows buffer.data is AudioBuffer
 *     console.log(`Sample rate: ${buffer.data.sampleRate}`);
 *   } else if (buffer.type === 'video') {
 *     // TypeScript knows buffer.data is VideoFrame
 *     console.log(`Resolution: ${buffer.data.width}x${buffer.data.height}`);
 *   }
 * }
 * ```
 */
export type DataBuffer =
  | { type: 'audio'; data: AudioBuffer; metadata?: Record<string, string> }
  | { type: 'video'; data: VideoFrame; metadata?: Record<string, string> }
  | { type: 'tensor'; data: TensorBuffer; metadata?: Record<string, string> }
  | { type: 'json'; data: JsonData; metadata?: Record<string, string> }
  | { type: 'text'; data: TextBuffer; metadata?: Record<string, string> }
  | { type: 'binary'; data: BinaryBuffer; metadata?: Record<string, string> };

/**
 * Data chunk for streaming with optional named buffers for multi-input
 */
export interface DataChunk {
  nodeId: string;
  buffer?: DataBuffer;
  namedBuffers?: Record<string, DataBuffer>;
  sequence: number;
  timestampMs: number;
}

/**
 * Type guard: Check if buffer is AudioBuffer
 */
export function isAudioBuffer(buffer: DataBuffer): buffer is Extract<DataBuffer, { type: 'audio' }> {
  return buffer.type === 'audio';
}

/**
 * Type guard: Check if buffer is VideoFrame
 */
export function isVideoFrame(buffer: DataBuffer): buffer is Extract<DataBuffer, { type: 'video' }> {
  return buffer.type === 'video';
}

/**
 * Type guard: Check if buffer is TensorBuffer
 */
export function isTensorBuffer(buffer: DataBuffer): buffer is Extract<DataBuffer, { type: 'tensor' }> {
  return buffer.type === 'tensor';
}

/**
 * Type guard: Check if buffer is JsonData
 */
export function isJsonData(buffer: DataBuffer): buffer is Extract<DataBuffer, { type: 'json' }> {
  return buffer.type === 'json';
}

/**
 * Type guard: Check if buffer is TextBuffer
 */
export function isTextBuffer(buffer: DataBuffer): buffer is Extract<DataBuffer, { type: 'text' }> {
  return buffer.type === 'text';
}

/**
 * Type guard: Check if buffer is BinaryBuffer
 */
export function isBinaryBuffer(buffer: DataBuffer): buffer is Extract<DataBuffer, { type: 'binary' }> {
  return buffer.type === 'binary';
}

/**
 * Helper: Extract typed data from DataBuffer
 *
 * @example
 * ```typescript
 * const audioData = extractAudioData(buffer); // AudioBuffer | null
 * if (audioData) {
 *   console.log(`Sample rate: ${audioData.sampleRate}`);
 * }
 * ```
 */
export function extractAudioData(buffer: DataBuffer): AudioBuffer | null {
  return isAudioBuffer(buffer) ? buffer.data : null;
}

export function extractVideoData(buffer: DataBuffer): VideoFrame | null {
  return isVideoFrame(buffer) ? buffer.data : null;
}

export function extractTensorData(buffer: DataBuffer): TensorBuffer | null {
  return isTensorBuffer(buffer) ? buffer.data : null;
}

export function extractJsonData(buffer: DataBuffer): JsonData | null {
  return isJsonData(buffer) ? buffer.data : null;
}

export function extractTextData(buffer: DataBuffer): TextBuffer | null {
  return isTextBuffer(buffer) ? buffer.data : null;
}

export function extractBinaryData(buffer: DataBuffer): BinaryBuffer | null {
  return isBinaryBuffer(buffer) ? buffer.data : null;
}

/**
 * Node manifest with type constraints
 */
export interface TypedNodeManifest {
  id: string;
  nodeType: string;
  params?: string;
  isStreaming: boolean;
  inputTypes: DataTypeHint[];
  outputTypes: DataTypeHint[];
  capabilities?: any;
  host?: string;
  runtimeHint?: number;
}

/**
 * Pipeline manifest with type safety
 */
export interface TypedPipelineManifest {
  version: string;
  metadata: {
    name: string;
    description: string;
    createdAt: string;
  };
  nodes: TypedNodeManifest[];
  connections: Array<{
    from: string;
    to: string;
    outputIndex?: number;
    inputIndex?: number;
  }>;
}

/**
 * Type-safe streaming result
 */
export interface StreamResult {
  sequence: number;
  dataOutputs: Record<string, DataBuffer>;
  processingTimeMs: number;
  totalItemsProcessed: number;
}

/**
 * Type validation error
 */
export class TypeValidationError extends Error {
  constructor(
    message: string,
    public expected: DataTypeHint,
    public actual: DataTypeHint,
    public nodeId: string
  ) {
    super(message);
    this.name = 'TypeValidationError';
  }
}

/**
 * Validate that data buffer matches expected type
 */
export function validateBufferType(
  buffer: DataBuffer,
  expectedType: DataTypeHint,
  nodeId: string
): void {
  const actualType = bufferToTypeHint(buffer);

  if (expectedType !== DataTypeHint.ANY && actualType !== expectedType) {
    throw new TypeValidationError(
      `Node '${nodeId}' expects ${expectedType} input but received ${actualType}`,
      expectedType,
      actualType,
      nodeId
    );
  }
}

/**
 * Convert DataBuffer to DataTypeHint
 */
export function bufferToTypeHint(buffer: DataBuffer): DataTypeHint {
  switch (buffer.type) {
    case 'audio':
      return DataTypeHint.AUDIO;
    case 'video':
      return DataTypeHint.VIDEO;
    case 'tensor':
      return DataTypeHint.TENSOR;
    case 'json':
      return DataTypeHint.JSON;
    case 'text':
      return DataTypeHint.TEXT;
    case 'binary':
      return DataTypeHint.BINARY;
  }
}
