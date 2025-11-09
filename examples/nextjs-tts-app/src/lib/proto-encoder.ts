/**
 * Minimal Protobuf encoder for DataBuffer messages
 *
 * This implements just enough of the Protobuf encoding spec to send
 * Text data via WebRTC data channels to the Rust backend.
 *
 * Based on transports/remotemedia-grpc/protos/common.proto
 */

/**
 * Encode a text string as a Protobuf DataBuffer with TextBuffer variant
 *
 * Wire format (Protobuf binary):
 * - DataBuffer.text (field 5, message type): tag=0x2a (field 5, wire type 2=length-delimited)
 * - TextBuffer.text_data (field 1, bytes): tag=0x0a (field 1, wire type 2)
 * - TextBuffer.encoding (field 2, string): tag=0x12 (field 2, wire type 2)
 *
 * @param text The text string to encode
 * @returns ArrayBuffer containing the Protobuf-encoded DataBuffer
 */
export function encodeTextData(text: string): ArrayBuffer {
  const textBytes = new TextEncoder().encode(text);
  const encoding = 'utf-8';
  const encodingBytes = new TextEncoder().encode(encoding);

  // Calculate sizes
  const textDataSize = 1 + varintLength(textBytes.length) + textBytes.length; // tag + length + data
  const encodingSize = 1 + varintLength(encodingBytes.length) + encodingBytes.length; // tag + length + data
  const textBufferSize = textDataSize + encodingSize;

  // DataBuffer.text field (field 5) contains the TextBuffer message
  const dataBufferSize = 1 + varintLength(textBufferSize) + textBufferSize;

  const buffer = new Uint8Array(new ArrayBuffer(dataBufferSize));
  let offset = 0;

  // Write DataBuffer.text field (field 5, wire type 2 = length-delimited)
  buffer[offset++] = (5 << 3) | 2; // field number 5, wire type 2
  offset = writeVarint(buffer, offset, textBufferSize);

  // Write TextBuffer.text_data field (field 1, wire type 2 = length-delimited)
  buffer[offset++] = (1 << 3) | 2; // field number 1, wire type 2
  offset = writeVarint(buffer, offset, textBytes.length);
  buffer.set(textBytes, offset);
  offset += textBytes.length;

  // Write TextBuffer.encoding field (field 2, wire type 2 = length-delimited)
  buffer[offset++] = (2 << 3) | 2; // field number 2, wire type 2
  offset = writeVarint(buffer, offset, encodingBytes.length);
  buffer.set(encodingBytes, offset);
  offset += encodingBytes.length;

  return buffer.buffer;
}

/**
 * Calculate the length of a varint encoding
 */
function varintLength(value: number): number {
  if (value < 0x80) return 1;
  if (value < 0x4000) return 2;
  if (value < 0x200000) return 3;
  if (value < 0x10000000) return 4;
  return 5;
}

/**
 * Write a varint (variable-length integer) to a buffer
 * Returns the new offset after writing
 */
function writeVarint(buffer: Uint8Array, offset: number, value: number): number {
  while (value >= 0x80) {
    buffer[offset++] = (value & 0x7f) | 0x80;
    value >>>= 7;
  }
  buffer[offset++] = value;
  return offset;
}
