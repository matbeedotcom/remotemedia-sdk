/**
 * Resource Loader - Load files and convert to RuntimeData format
 *
 * Supports require(file) syntax for embedding resources in pipeline manifests.
 * Currently supports: WAV audio files
 */

import fs from 'fs/promises';
import path from 'path';

// RuntimeData protobuf types (matching proto/common.proto)
export interface AudioBufferProto {
  samples: Buffer;
  sampleRate: number;
  channels: number;
  format: number; // 0=I16, 1=F32
  numSamples: number;
}

export interface RuntimeDataProto {
  audio?: AudioBufferProto;
  text?: string;
  binary?: Buffer;
  // Can extend with video, tensor, json as needed
}

/**
 * Simple WAV file decoder
 * Supports PCM 16-bit and 32-bit float WAV files
 */
function decodeWav(buffer: Buffer): AudioBufferProto {
  const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);

  // Check RIFF header
  const riff = String.fromCharCode(view.getUint8(0), view.getUint8(1), view.getUint8(2), view.getUint8(3));
  if (riff !== 'RIFF') {
    throw new Error('Invalid WAV file: missing RIFF header');
  }

  const wave = String.fromCharCode(view.getUint8(8), view.getUint8(9), view.getUint8(10), view.getUint8(11));
  if (wave !== 'WAVE') {
    throw new Error('Invalid WAV file: missing WAVE header');
  }

  // Find fmt chunk
  let offset = 12;
  let fmtOffset = 0;
  let dataOffset = 0;
  let dataSize = 0;

  while (offset < buffer.length) {
    const chunkId = String.fromCharCode(
      view.getUint8(offset),
      view.getUint8(offset + 1),
      view.getUint8(offset + 2),
      view.getUint8(offset + 3)
    );
    const chunkSize = view.getUint32(offset + 4, true);

    if (chunkId === 'fmt ') {
      fmtOffset = offset + 8;
    } else if (chunkId === 'data') {
      dataOffset = offset + 8;
      dataSize = chunkSize;
      break;
    }

    offset += 8 + chunkSize;
  }

  if (!fmtOffset || !dataOffset) {
    throw new Error('Invalid WAV file: missing fmt or data chunk');
  }

  // Parse fmt chunk
  const audioFormat = view.getUint16(fmtOffset, true);
  const channels = view.getUint16(fmtOffset + 2, true);
  const sampleRate = view.getUint32(fmtOffset + 4, true);
  const bitsPerSample = view.getUint16(fmtOffset + 14, true);

  // Extract audio data
  const audioData = buffer.slice(dataOffset, dataOffset + dataSize);

  // Convert to F32 format (what VibeVoice expects)
  let samples: Buffer;
  let format: number;
  let numSamples: number;

  if (audioFormat === 1 && bitsPerSample === 16) {
    // PCM 16-bit -> F32
    const int16Array = new Int16Array(audioData.buffer, audioData.byteOffset, audioData.byteLength / 2);
    const float32Array = new Float32Array(int16Array.length);

    for (let i = 0; i < int16Array.length; i++) {
      float32Array[i] = int16Array[i] / 32768.0; // Normalize to [-1.0, 1.0]
    }

    samples = Buffer.from(float32Array.buffer);
    format = 1; // F32
    numSamples = float32Array.length;

  } else if (audioFormat === 3 && bitsPerSample === 32) {
    // IEEE Float 32-bit - already in F32 format
    samples = audioData;
    format = 1; // F32
    numSamples = audioData.byteLength / 4;

  } else {
    throw new Error(`Unsupported WAV format: audioFormat=${audioFormat}, bitsPerSample=${bitsPerSample}`);
  }

  console.log(`[ResourceLoader] Decoded WAV: ${numSamples} samples, ${channels} channels, ${sampleRate} Hz`);

  return {
    samples,
    sampleRate,
    channels,
    format,
    numSamples,
  };
}

/**
 * Load a file and convert to RuntimeData format
 *
 * @param filePath - Absolute path to the file
 * @returns RuntimeData proto object ready to embed in pipeline manifest
 */
export async function requireFile(filePath: string): Promise<RuntimeDataProto> {
  const ext = path.extname(filePath).toLowerCase();
  const buffer = await fs.readFile(filePath);

  console.log(`[ResourceLoader] Loading file: ${filePath} (${buffer.length} bytes, type: ${ext})`);

  // Dispatch by extension
  switch (ext) {
    case '.wav':
      return { audio: decodeWav(buffer) };

    case '.txt':
    case '.md':
      return { text: buffer.toString('utf-8') };

    default:
      // Fallback: binary
      return { binary: buffer };
  }
}

/**
 * Helper to check if a value is a require() call
 */
export function isRequireCall(value: any): boolean {
  return typeof value === 'string' && value.startsWith('require:');
}

/**
 * Process a value and resolve require() calls
 */
export async function processValue(value: any): Promise<any> {
  if (isRequireCall(value)) {
    const filePath = value.slice(8); // Remove 'require:' prefix
    return await requireFile(filePath);
  }

  if (Array.isArray(value)) {
    return await Promise.all(value.map(processValue));
  }

  if (typeof value === 'object' && value !== null) {
    const processed: any = {};
    for (const [key, val] of Object.entries(value)) {
      processed[key] = await processValue(val);
    }
    return processed;
  }

  return value;
}
