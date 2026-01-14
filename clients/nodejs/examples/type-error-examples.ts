/**
 * Type Error Examples - These should NOT compile!
 *
 * This file demonstrates how TypeScript catches type errors at compile time.
 * Uncomment the examples to see the errors.
 */

import {
  DataBuffer,
  AudioBuffer,
  VideoFrame,
  isAudioBuffer,
  isVideoFrame,
} from '../src/data-types';

/**
 * ERROR 1: Wrong data type for buffer type
 * TypeScript error: Type 'VideoFrame' is not assignable to type 'AudioBuffer'
 */
/*
function errorExample1(): DataBuffer {
  const videoData: VideoFrame = {
    pixelData: new Uint8Array(100),
    width: 320,
    height: 240,
    format: 'RGB24' as any,
    frameNumber: 0,
    timestampUs: 0,
  };

  return {
    type: 'audio',  // Says audio
    data: videoData,  // But provides video! ❌ TypeScript catches this!
  };
}
*/

/**
 * ERROR 2: Accessing wrong field without type guard
 * TypeScript error: Property 'sampleRate' does not exist on type 'AudioBuffer | VideoFrame | ...'
 */
/*
function errorExample2(buffer: DataBuffer): void {
  // Can't access sampleRate without checking type first
  console.log(buffer.data.sampleRate);  // ❌ TypeScript error!
}
*/

/**
 * ERROR 3: Assigning wrong type after guard
 * TypeScript error: Type 'VideoFrame' is not assignable to type 'AudioBuffer'
 */
/*
function errorExample3(buffer: DataBuffer): void {
  if (isVideoFrame(buffer)) {
    // TypeScript knows buffer.data is VideoFrame
    const audio: AudioBuffer = buffer.data;  // ❌ Can't assign VideoFrame to AudioBuffer!
  }
}
*/

/**
 * CORRECT: This compiles fine with type guards
 */
function correctExample(buffer: DataBuffer): void {
  if (isAudioBuffer(buffer)) {
    // ✅ TypeScript knows buffer.data is AudioBuffer
    console.log(`Sample rate: ${buffer.data.sampleRate}`);
  } else if (isVideoFrame(buffer)) {
    // ✅ TypeScript knows buffer.data is VideoFrame
    console.log(`Resolution: ${buffer.data.width}x${buffer.data.height}`);
  }
}

/**
 * CORRECT: Extract with null check
 */
import { extractAudioData } from '../src/data-types';

function correctExample2(buffer: DataBuffer): void {
  const audioData = extractAudioData(buffer);
  if (audioData) {
    // ✅ TypeScript knows audioData is AudioBuffer (not null)
    console.log(`Sample rate: ${audioData.sampleRate}`);
  }
}

console.log('✅ Type-safe examples compile successfully!');
console.log('💡 Uncomment the error examples to see TypeScript catch mistakes at compile time.');
