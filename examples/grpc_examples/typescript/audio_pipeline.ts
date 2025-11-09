/**
 * Audio pipeline example with Rust audio processing nodes
 * 
 * Demonstrates:
 * - Using RustResampleNode to change sample rate
 * - Using RustVADNode for voice activity detection
 * - Using RustFormatConverterNode for format conversion
 * - Chaining audio processing nodes together
 * - Processing real audio data through the pipeline
 */

import { RemoteMediaClient, RemoteMediaError, AudioFormat } from '@remotemedia/nodejs-client';

async function main() {
  const client = new RemoteMediaClient('localhost:50051');
  
  try {
    console.log('Connecting to gRPC service...');
    await client.connect();
    
    const version = await client.getVersion();
    console.log(`✅ Connected to service v${version.protocolVersion}`);
    console.log(`   Runtime version: ${version.runtimeVersion}`);
    console.log(`   All nodes: ${version.supportedNodeTypes.join(', ')}\n`);
    
    // Example 1: Simple resample pipeline
    console.log('=== Example 1: Audio Resampling (44.1kHz → 16kHz) ===');
    await testResamplePipeline(client);
    
    // Example 2: Voice Activity Detection
    console.log('\n=== Example 2: Voice Activity Detection ===');
    await testVADPipeline(client);
    
    // Example 3: Multi-stage audio processing
    console.log('\n=== Example 3: Multi-Stage Pipeline (Resample → VAD) ===');
    await testMultiStagePipeline(client);
    
    console.log('\n✅ All audio pipeline tests completed successfully!');
    
  } catch (error) {
    if (error instanceof RemoteMediaError) {
      console.error(`\n❌ Error: ${error.message}`);
      if (error.errorType) console.error(`   Type: ${error.errorType}`);
      if (error.failingNodeId) console.error(`   Node: ${error.failingNodeId}`);
    } else {
      console.error(`\n❌ Error: ${error}`);
    }
    process.exit(1);
  } finally {
    await client.disconnect();
  }
}

/**
 * Test audio resampling: 44.1kHz → 16kHz
 */
async function testResamplePipeline(client: RemoteMediaClient) {
  const SOURCE_RATE = 44100;
  const TARGET_RATE = 16000;
  const DURATION_SEC = 1.0;
  const FREQUENCY = 440.0; // A4 note
  
  // Generate sine wave at 44.1kHz
  const numSamples = Math.floor(SOURCE_RATE * DURATION_SEC);
  const samples = new Float32Array(numSamples);
  
  for (let i = 0; i < numSamples; i++) {
    const t = i / SOURCE_RATE;
    samples[i] = 0.3 * Math.sin(2 * Math.PI * FREQUENCY * t);
  }
  
  const manifest = {
    version: 'v1',
    metadata: {
      name: 'audio_resample',
      description: 'Resample 44.1kHz audio to 16kHz',
      createdAt: new Date().toISOString()
    },
    nodes: [
      {
        id: 'resample',
        nodeType: 'RustResampleNode',
        params: JSON.stringify({
          source_rate: SOURCE_RATE,
          target_rate: TARGET_RATE,
          quality: 'High',
          channels: 1
        }),
        isStreaming: false
      }
    ],
    connections: []
  };
  
  const audioBuffer = {
    samples: Buffer.from(samples.buffer),
    sampleRate: SOURCE_RATE,
    channels: 1,
    format: AudioFormat.F32,
    numSamples: numSamples
  };
  
  console.log(`Input: ${numSamples} samples @ ${SOURCE_RATE}Hz (${DURATION_SEC}s)`);
  console.log(`Target: ${TARGET_RATE}Hz`);
  
  const result = await client.executePipeline(
    manifest,
    { resample: audioBuffer },
    {}
  );
  
  console.log('✅ Resampling successful');
  console.log(`   Wall time: ${result.metrics.wallTimeMs.toFixed(2)}ms`);
  
  if (result.audioOutputs.resample) {
    const output = result.audioOutputs.resample;
    const expectedSamples = Math.floor(TARGET_RATE * DURATION_SEC);
    console.log(`   Output: ${output.numSamples} samples @ ${output.sampleRate}Hz`);
    console.log(`   Expected: ~${expectedSamples} samples`);
    console.log(`   Sample rate match: ${output.sampleRate === TARGET_RATE ? '✅' : '❌'}`);
  }
}

/**
 * Test Voice Activity Detection on sine wave with silence
 */
async function testVADPipeline(client: RemoteMediaClient) {
  const SAMPLE_RATE = 16000;
  const DURATION_SEC = 2.0;
  const FREQUENCY = 440.0;
  
  // Generate audio: 0.5s silence, 1s tone, 0.5s silence
  const numSamples = Math.floor(SAMPLE_RATE * DURATION_SEC);
  const samples = new Float32Array(numSamples);
  
  for (let i = 0; i < numSamples; i++) {
    const t = i / SAMPLE_RATE;
    // Add tone from 0.5s to 1.5s
    if (t >= 0.5 && t <= 1.5) {
      samples[i] = 0.5 * Math.sin(2 * Math.PI * FREQUENCY * t);
    } else {
      samples[i] = 0.0; // Silence
    }
  }
  
  const manifest = {
    version: 'v1',
    metadata: {
      name: 'vad_detection',
      description: 'Detect voice activity in audio',
      createdAt: new Date().toISOString()
    },
    nodes: [
      {
        id: 'vad',
        nodeType: 'RustVADNode',
        params: JSON.stringify({
          sample_rate: SAMPLE_RATE,
          frame_duration_ms: 30,
          energy_threshold: 0.01
        }),
        isStreaming: false
      }
    ],
    connections: []
  };
  
  const audioBuffer = {
    samples: Buffer.from(samples.buffer),
    sampleRate: SAMPLE_RATE,
    channels: 1,
    format: AudioFormat.F32,
    numSamples: numSamples
  };
  
  console.log(`Input: ${numSamples} samples @ ${SAMPLE_RATE}Hz`);
  console.log(`Pattern: 0.5s silence | 1.0s tone (440Hz) | 0.5s silence`);
  
  const result = await client.executePipeline(
    manifest,
    { vad: audioBuffer },
    {}
  );
  
  console.log('✅ VAD processing successful');
  console.log(`   Wall time: ${result.metrics.wallTimeMs.toFixed(2)}ms`);
  
  if (result.dataOutputs && result.dataOutputs.vad) {
    console.log(`   VAD output: ${JSON.stringify(result.dataOutputs.vad)}`);
  }
}

/**
 * Test multi-stage pipeline: Resample → VAD
 */
async function testMultiStagePipeline(client: RemoteMediaClient) {
  const SOURCE_RATE = 48000;
  const TARGET_RATE = 16000;
  const DURATION_SEC = 1.0;
  const FREQUENCY = 440.0;
  
  // Generate sine wave at 48kHz
  const numSamples = Math.floor(SOURCE_RATE * DURATION_SEC);
  const samples = new Float32Array(numSamples);
  
  for (let i = 0; i < numSamples; i++) {
    const t = i / SOURCE_RATE;
    samples[i] = 0.4 * Math.sin(2 * Math.PI * FREQUENCY * t);
  }
  
  const manifest = {
    version: 'v1',
    metadata: {
      name: 'resample_vad_pipeline',
      description: 'Resample audio then detect voice activity',
      createdAt: new Date().toISOString()
    },
    nodes: [
      {
        id: 'resample',
        nodeType: 'RustResampleNode',
        params: JSON.stringify({
          source_rate: SOURCE_RATE,
          target_rate: TARGET_RATE,
          quality: 'High',
          channels: 1
        }),
        isStreaming: false
      },
      {
        id: 'vad',
        nodeType: 'RustVADNode',
        params: JSON.stringify({
          sample_rate: TARGET_RATE,
          frame_duration_ms: 30,
          energy_threshold: 0.01
        }),
        isStreaming: false
      }
    ],
    connections: [
      {
        from: 'resample',
        to: 'vad'
      }
    ]
  };
  
  const audioBuffer = {
    samples: Buffer.from(samples.buffer),
    sampleRate: SOURCE_RATE,
    channels: 1,
    format: AudioFormat.F32,
    numSamples: numSamples
  };
  
  console.log(`Input: ${numSamples} samples @ ${SOURCE_RATE}Hz`);
  console.log(`Pipeline: Resample (${SOURCE_RATE}Hz → ${TARGET_RATE}Hz) → VAD`);
  
  const result = await client.executePipeline(
    manifest,
    { resample: audioBuffer },
    {}
  );
  
  console.log('✅ Multi-stage pipeline successful');
  console.log(`   Wall time: ${result.metrics.wallTimeMs.toFixed(2)}ms`);
  console.log(`   Nodes executed: ${Object.keys(result.metrics.nodeMetrics).length}`);
  
  for (const [nodeId, metrics] of Object.entries(result.metrics.nodeMetrics)) {
    console.log(`   - ${nodeId}: ${metrics.executionTimeMs.toFixed(2)}ms`);
  }
}

main();
