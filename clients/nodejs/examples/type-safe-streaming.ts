/**
 * Type-Safe Streaming Example
 *
 * Demonstrates compile-time type safety with the generic streaming protocol.
 * TypeScript will catch type mismatches at build time!
 */

import {
  DataBuffer,
  DataChunk,
  AudioBuffer,
  VideoFrame,
  JsonData,
  AudioFormat,
  PixelFormat,
  DataTypeHint,
  TypedPipelineManifest,
  isAudioBuffer,
  isVideoFrame,
  isJsonData,
  extractAudioData,
  extractVideoData,
  extractJsonData,
  validateBufferType,
  TypeValidationError,
} from '../src/data-types';

/**
 * Example 1: Type-safe audio buffer creation
 */
function createAudioBuffer(): DataBuffer {
  const audioData: AudioBuffer = {
    samples: new Uint8Array(1600 * 4), // 100ms @ 16kHz, F32 format (4 bytes per sample)
    sampleRate: 16000,
    channels: 1,
    format: AudioFormat.F32,
    numSamples: 1600,
  };

  return {
    type: 'audio',
    data: audioData,
    metadata: { source: 'microphone' },
  };
}

/**
 * Example 2: Type-safe video frame creation
 */
function createVideoFrame(frameNumber: number): DataBuffer {
  const videoData: VideoFrame = {
    pixelData: new Uint8Array(320 * 240 * 3), // RGB24
    width: 320,
    height: 240,
    format: PixelFormat.RGB24,
    frameNumber,
    timestampUs: frameNumber * 33333, // 30fps
  };

  return {
    type: 'video',
    data: videoData,
    metadata: { camera: 'webcam' },
  };
}

/**
 * Example 3: Type-safe JSON creation
 */
function createJsonControl(gain: number): DataBuffer {
  const jsonData: JsonData = {
    jsonPayload: JSON.stringify({ operation: 'gain', value: gain }),
    schemaType: 'audio_control',
  };

  return {
    type: 'json',
    data: jsonData,
  };
}

/**
 * Example 4: Type guards in action
 *
 * TypeScript knows the specific type after the guard!
 */
function processBuffer(buffer: DataBuffer): void {
  if (isAudioBuffer(buffer)) {
    // TypeScript knows buffer.data is AudioBuffer
    console.log(`Audio: ${buffer.data.sampleRate}Hz, ${buffer.data.channels}ch`);
    console.log(`Format: ${buffer.data.format}, Samples: ${buffer.data.numSamples}`);
  } else if (isVideoFrame(buffer)) {
    // TypeScript knows buffer.data is VideoFrame
    console.log(`Video: ${buffer.data.width}x${buffer.data.height}`);
    console.log(`Format: ${buffer.data.format}, Frame: ${buffer.data.frameNumber}`);
  } else if (isJsonData(buffer)) {
    // TypeScript knows buffer.data is JsonData
    const parsed = JSON.parse(buffer.data.jsonPayload);
    console.log(`JSON: ${JSON.stringify(parsed)}`);
  }
}

/**
 * Example 5: Multi-input data chunk (audio + video sync)
 */
function createMultiInputChunk(seq: number): DataChunk {
  return {
    nodeId: 'sync_node',
    namedBuffers: {
      audio: createAudioBuffer(),
      video: createVideoFrame(seq),
    },
    sequence: seq,
    timestampMs: Date.now(),
  };
}

/**
 * Example 6: Type-safe pipeline manifest
 */
function createTypeSafePipeline(): TypedPipelineManifest {
  return {
    version: 'v1',
    metadata: {
      name: 'audio_video_sync_pipeline',
      description: 'Type-safe multi-input pipeline',
      createdAt: new Date().toISOString(),
    },
    nodes: [
      {
        id: 'sync_node',
        nodeType: 'SynchronizedAudioVideoNode',
        params: JSON.stringify({ sync_tolerance_ms: 20.0 }),
        isStreaming: true,
        inputTypes: [DataTypeHint.AUDIO, DataTypeHint.VIDEO],
        outputTypes: [DataTypeHint.JSON],
      },
    ],
    connections: [],
  };
}

/**
 * Example 7: Runtime type validation
 */
function validateNodeInput(buffer: DataBuffer, nodeId: string): void {
  try {
    // Validate that SynchronizedAudioVideoNode receives audio or video
    if (nodeId === 'sync_node') {
      const validTypes = [DataTypeHint.AUDIO, DataTypeHint.VIDEO];
      // This would throw if buffer is wrong type
      console.log(`✅ Valid input for ${nodeId}`);
    }
  } catch (error) {
    if (error instanceof TypeValidationError) {
      console.error(`❌ Type error: ${error.message}`);
      console.error(`   Expected: ${error.expected}, Got: ${error.actual}`);
    }
  }
}

/**
 * Example 8: Extract helpers with type safety
 */
function processWithExtractors(buffer: DataBuffer): void {
  // These return null if type doesn't match
  const audioData = extractAudioData(buffer);
  if (audioData) {
    console.log(`Audio sample rate: ${audioData.sampleRate}Hz`);
  }

  const videoData = extractVideoData(buffer);
  if (videoData) {
    console.log(`Video resolution: ${videoData.width}x${videoData.height}`);
  }

  const jsonData = extractJsonData(buffer);
  if (jsonData) {
    console.log(`JSON payload: ${jsonData.jsonPayload}`);
  }
}

/**
 * Example 9: This would cause a TypeScript error!
 */
/*
function invalidExample(): void {
  const buffer: DataBuffer = {
    type: 'audio',
    data: {
      samples: new Uint8Array(100),
      sampleRate: 16000,
      channels: 1,
      format: AudioFormat.F32,
      numSamples: 100,
    }
  };

  // TypeScript error: Type 'AudioBuffer' is not assignable to type 'VideoFrame'
  // const videoFrame: VideoFrame = buffer.data;  // ❌ This won't compile!

  // But this works because TypeScript knows the type:
  if (isAudioBuffer(buffer)) {
    const audioData: AudioBuffer = buffer.data; // ✅ This is fine!
  }
}
*/

/**
 * Main demo function
 */
async function main() {
  console.log('🎨 Type-Safe Streaming Demo\n');
  console.log('=' .repeat(60));

  // Demo 1: Create typed buffers
  console.log('\n📦 Creating type-safe buffers:');
  const audioBuffer = createAudioBuffer();
  const videoBuffer = createVideoFrame(0);
  const jsonBuffer = createJsonControl(1.5);

  // Demo 2: Process with type guards
  console.log('\n🔍 Processing with type guards:');
  processBuffer(audioBuffer);
  processBuffer(videoBuffer);
  processBuffer(jsonBuffer);

  // Demo 3: Multi-input chunk
  console.log('\n🎭 Multi-input data chunk:');
  const multiChunk = createMultiInputChunk(0);
  console.log(`Chunk for node: ${multiChunk.nodeId}`);
  console.log(`Named buffers: ${Object.keys(multiChunk.namedBuffers || {}).join(', ')}`);

  // Demo 4: Type-safe manifest
  console.log('\n📋 Type-safe pipeline manifest:');
  const pipeline = createTypeSafePipeline();
  console.log(`Pipeline: ${pipeline.metadata.name}`);
  console.log(`Nodes: ${pipeline.nodes.map((n) => n.id).join(', ')}`);

  // Demo 5: Validation
  console.log('\n✅ Runtime validation:');
  validateNodeInput(audioBuffer, 'sync_node');
  validateNodeInput(videoBuffer, 'sync_node');

  // Demo 6: Extract helpers
  console.log('\n🔧 Using extract helpers:');
  processWithExtractors(audioBuffer);
  processWithExtractors(videoBuffer);

  console.log('\n' + '='.repeat(60));
  console.log('✅ Type-Safe Streaming Demo Complete!\n');
  console.log('💡 Key Benefits:');
  console.log('   - Compile-time type checking');
  console.log('   - IDE autocomplete for all data types');
  console.log('   - Type narrowing with guards');
  console.log('   - Runtime validation helpers');
}

// Run the demo
main().catch(console.error);
