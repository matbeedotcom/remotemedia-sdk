/**
 * Protobuf DataBuffer encoder/decoder for WebRTC data channel.
 *
 * Hand-rolled protobuf encoding matching proto/common.proto DataBuffer.
 * This avoids a full protobuf codegen dependency for the lightweight UI.
 *
 * DataBuffer oneof data_type:
 *   1=AudioBuffer, 2=VideoFrame, 3=TensorBuffer, 4=JsonData, 5=TextBuffer,
 *   6=BinaryBuffer, 7=ControlMessage, 8=NumpyBuffer, 9=FileBuffer
 */

// ---------------------------------------------------------------------------
// DataType enum — matches DataBuffer oneof field numbers
// ---------------------------------------------------------------------------

export enum DataType {
  Audio = 1,
  Video = 2,
  Tensor = 3,
  Json = 4,
  Text = 5,
  Binary = 6,
  ControlMessage = 7,
  Numpy = 8,
  File = 9,
}

// ---------------------------------------------------------------------------
// Decoded message types
// ---------------------------------------------------------------------------

export interface DecodedMessage {
  dataType: DataType;
  /** Raw inner message bytes (e.g. TextBuffer, AudioBuffer protobuf) */
  payload: Uint8Array;
}

export interface AudioHeader {
  sampleRate: number;
  channels: number;
  numSamples: number;
}

// ---------------------------------------------------------------------------
// Low-level protobuf helpers (varint + tag-length-value)
// ---------------------------------------------------------------------------

/** Encode a uint32/uint64 as a protobuf varint, return the bytes used. */
function encodeVarint(value: number, buf: Uint8Array, offset: number): number {
  let v = value >>> 0; // force unsigned 32-bit
  while (v > 0x7f) {
    buf[offset++] = (v & 0x7f) | 0x80;
    v >>>= 7;
  }
  buf[offset++] = v;
  return offset;
}

/** Decode a varint at the given offset, return [value, newOffset]. */
function decodeVarint(buf: Uint8Array, offset: number): [number, number] {
  let result = 0;
  let shift = 0;
  while (offset < buf.length) {
    const b = buf[offset++];
    result |= (b & 0x7f) << shift;
    if ((b & 0x80) === 0) break;
    shift += 7;
  }
  return [result >>> 0, offset];
}

/** Compute the varint-encoded length of a value. */
function varintSize(value: number): number {
  let v = value >>> 0;
  let size = 1;
  while (v > 0x7f) {
    size++;
    v >>>= 7;
  }
  return size;
}

/** Encode a protobuf tag (field number + wire type). */
function encodeTag(fieldNumber: number, wireType: number): number {
  return (fieldNumber << 3) | wireType;
}

// Wire type 2 = length-delimited (messages, strings, bytes)
const WIRE_TYPE_LEN = 2;
// Wire type 0 = varint
const WIRE_TYPE_VARINT = 0;

// ---------------------------------------------------------------------------
// TextBuffer encoding (proto field layout)
// ---------------------------------------------------------------------------
// message TextBuffer {
//   bytes text_data = 1;
//   string encoding = 2;
//   string language = 3;
// }

function encodeTextBuffer(text: string): Uint8Array {
  const textBytes = new TextEncoder().encode(text);
  // field 1 (bytes text_data): tag + varint length + data
  const tag = encodeTag(1, WIRE_TYPE_LEN);
  const tagSize = varintSize(tag);
  const lenSize = varintSize(textBytes.length);
  const totalSize = tagSize + lenSize + textBytes.length;

  const buf = new Uint8Array(totalSize);
  let offset = encodeVarint(tag, buf, 0);
  offset = encodeVarint(textBytes.length, buf, offset);
  buf.set(textBytes, offset);
  return buf;
}

function decodeTextBuffer(buf: Uint8Array): string {
  let offset = 0;
  while (offset < buf.length) {
    const [tagValue, newOffset] = decodeVarint(buf, offset);
    offset = newOffset;
    const fieldNumber = tagValue >>> 3;
    const wireType = tagValue & 0x7;

    if (wireType === WIRE_TYPE_LEN) {
      const [len, dataOffset] = decodeVarint(buf, offset);
      offset = dataOffset;
      if (fieldNumber === 1) {
        // text_data
        return new TextDecoder().decode(buf.slice(offset, offset + len));
      }
      offset += len;
    } else if (wireType === WIRE_TYPE_VARINT) {
      const [, nextOffset] = decodeVarint(buf, offset);
      offset = nextOffset;
    } else {
      break; // unsupported wire type
    }
  }
  return '';
}

// ---------------------------------------------------------------------------
// AudioBuffer encoding (proto field layout)
// ---------------------------------------------------------------------------
// message AudioBuffer {
//   bytes samples = 1;       // raw PCM
//   uint32 sample_rate = 2;
//   uint32 channels = 3;
//   AudioFormat format = 4;  // enum (1 = F32)
//   uint64 num_samples = 5;
// }

function encodeAudioBuffer(
  samples: Float32Array,
  sampleRate: number,
  channels: number,
): Uint8Array {
  const sampleBytes = new Uint8Array(samples.buffer, samples.byteOffset, samples.byteLength);

  // Pre-calculate sizes
  const parts: Uint8Array[] = [];

  // field 1: bytes samples
  parts.push(encodeLenField(1, sampleBytes));

  // field 2: uint32 sample_rate
  parts.push(encodeVarintField(2, sampleRate));

  // field 3: uint32 channels
  parts.push(encodeVarintField(3, channels));

  // field 4: AudioFormat = F32 (1)
  parts.push(encodeVarintField(4, 1));

  // field 5: uint64 num_samples
  parts.push(encodeVarintField(5, samples.length));

  return concatUint8Arrays(parts);
}

function decodeAudioBuffer(buf: Uint8Array): AudioHeader {
  let offset = 0;
  let sampleRate = 0;
  let channels = 0;
  let numSamples = 0;

  while (offset < buf.length) {
    const [tagValue, newOffset] = decodeVarint(buf, offset);
    offset = newOffset;
    const fieldNumber = tagValue >>> 3;
    const wireType = tagValue & 0x7;

    if (wireType === WIRE_TYPE_VARINT) {
      const [val, nextOffset] = decodeVarint(buf, offset);
      offset = nextOffset;
      if (fieldNumber === 2) sampleRate = val;
      else if (fieldNumber === 3) channels = val;
      else if (fieldNumber === 5) numSamples = val;
    } else if (wireType === WIRE_TYPE_LEN) {
      const [len, dataOffset] = decodeVarint(buf, offset);
      offset = dataOffset + len; // skip bytes field
    } else {
      break;
    }
  }

  return { sampleRate, channels, numSamples };
}

// ---------------------------------------------------------------------------
// BinaryBuffer encoding
// ---------------------------------------------------------------------------
// message BinaryBuffer {
//   bytes data = 1;
//   string mime_type = 2;
// }

function encodeBinaryBuffer(data: Uint8Array, mimeType?: string): Uint8Array {
  const parts: Uint8Array[] = [];
  parts.push(encodeLenField(1, data));
  if (mimeType) {
    parts.push(encodeLenField(2, new TextEncoder().encode(mimeType)));
  }
  return concatUint8Arrays(parts);
}

// ---------------------------------------------------------------------------
// JsonData encoding
// ---------------------------------------------------------------------------
// message JsonData {
//   string json_payload = 1;
//   string schema_type = 2;
// }

function encodeJsonData(jsonPayload: string): Uint8Array {
  return encodeLenField(1, new TextEncoder().encode(jsonPayload));
}

// ---------------------------------------------------------------------------
// DataBuffer encoding (wraps inner message in oneof)
// ---------------------------------------------------------------------------

/**
 * Encode an inner message as a DataBuffer protobuf.
 *
 * DataBuffer { oneof data_type { ... field N = message } }
 * Wire format: [tag(N, LEN)] [varint length] [inner message bytes]
 */
function encodeDataBuffer(fieldNumber: DataType, innerMessage: Uint8Array): Uint8Array {
  return encodeLenField(fieldNumber, innerMessage);
}

/**
 * Decode a DataBuffer protobuf into its data type and inner message bytes.
 */
function decodeDataBuffer(buf: Uint8Array): DecodedMessage {
  let offset = 0;
  const [tagValue, newOffset] = decodeVarint(buf, offset);
  offset = newOffset;

  const fieldNumber = tagValue >>> 3;
  const wireType = tagValue & 0x7;

  if (wireType !== WIRE_TYPE_LEN) {
    throw new Error(`Unexpected wire type ${wireType} for DataBuffer`);
  }

  const [len, dataOffset] = decodeVarint(buf, offset);
  offset = dataOffset;

  const payload = buf.slice(offset, offset + len);
  return { dataType: fieldNumber as DataType, payload };
}

// ---------------------------------------------------------------------------
// Protobuf field encoding helpers
// ---------------------------------------------------------------------------

function encodeLenField(fieldNumber: number, data: Uint8Array): Uint8Array {
  const tag = encodeTag(fieldNumber, WIRE_TYPE_LEN);
  const tagSize = varintSize(tag);
  const lenSize = varintSize(data.length);
  const buf = new Uint8Array(tagSize + lenSize + data.length);
  let offset = encodeVarint(tag, buf, 0);
  offset = encodeVarint(data.length, buf, offset);
  buf.set(data, offset);
  return buf;
}

function encodeVarintField(fieldNumber: number, value: number): Uint8Array {
  const tag = encodeTag(fieldNumber, WIRE_TYPE_VARINT);
  const tagSize = varintSize(tag);
  const valSize = varintSize(value);
  const buf = new Uint8Array(tagSize + valSize);
  let offset = encodeVarint(tag, buf, 0);
  encodeVarint(value, buf, offset);
  return buf;
}

function concatUint8Arrays(arrays: Uint8Array[]): Uint8Array {
  const totalLen = arrays.reduce((sum, a) => sum + a.length, 0);
  const result = new Uint8Array(totalLen);
  let offset = 0;
  for (const a of arrays) {
    result.set(a, offset);
    offset += a.length;
  }
  return result;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/** Encode a DataBuffer protobuf message. */
export function encode(dataType: DataType, innerMessage: Uint8Array): ArrayBuffer {
  const encoded = encodeDataBuffer(dataType, innerMessage);
  // Copy into a fresh ArrayBuffer to avoid SharedArrayBuffer type issues
  const result = new ArrayBuffer(encoded.byteLength);
  new Uint8Array(result).set(encoded);
  return result;
}

/** Decode a DataBuffer protobuf message. */
export function decode(buffer: ArrayBuffer): DecodedMessage {
  return decodeDataBuffer(new Uint8Array(buffer));
}

/** Encode a text string as a DataBuffer { text: TextBuffer { text_data } }. */
export function encodeText(text: string): ArrayBuffer {
  const inner = encodeTextBuffer(text);
  return encode(DataType.Text, inner);
}

/** Decode text from a TextBuffer payload. */
export function decodeText(payload: Uint8Array): string {
  return decodeTextBuffer(payload);
}

/** Encode PCM f32 audio as a DataBuffer { audio: AudioBuffer { ... } }. */
export function encodeAudio(
  samples: Float32Array,
  sampleRate: number,
  channels: number,
): ArrayBuffer {
  const inner = encodeAudioBuffer(samples, sampleRate, channels);
  return encode(DataType.Audio, inner);
}

/** Decode audio header from an AudioBuffer payload. */
export function decodeAudioHeader(payload: Uint8Array): AudioHeader {
  return decodeAudioBuffer(payload);
}

/** Encode raw binary as a DataBuffer { binary: BinaryBuffer { data, mime_type } }. */
export function encodeBinary(data: Uint8Array, mimeType?: string): ArrayBuffer {
  const inner = encodeBinaryBuffer(data, mimeType);
  return encode(DataType.Binary, inner);
}

/** Encode a JSON string as a DataBuffer { json: JsonData { json_payload } }. */
export function encodeJson(jsonPayload: string): ArrayBuffer {
  const inner = encodeJsonData(jsonPayload);
  return encode(DataType.Json, inner);
}

/** Size constant kept for backward compat — not used in protobuf path. */
export const AUDIO_HEADER_SIZE = 16;
