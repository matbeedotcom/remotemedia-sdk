/**
 * Streaming Audio Pipeline with PassThrough
 *
 * NOTE: Currently, streaming works with PassThrough nodes. The Rust audio processing
 * nodes (RustResampleNode, RustVADNode, RustFormatConverterNode) are designed for
 * unary ExecutePipeline RPC and don't yet support streaming.
 *
 * For audio processing with Rust nodes, use unary RPC (see audio_pipeline.ts).
 * This example demonstrates streaming architecture with PassThrough as a foundation.
 *
 * Demonstrates:
 * - Real-time audio chunk streaming
 * - Per-chunk latency measurement
 * - Session lifecycle management
 * - Foundation for future streaming audio processing nodes
 */

import { RemoteMediaClient, RemoteMediaError, AudioFormat } from '@remotemedia/nodejs-client';

/**
 * Generate audio chunks with a pattern:
 * - 0-0.5s: silence
 * - 0.5-1.5s: 440Hz tone (voice)
 * - 1.5-2s: silence
 */
async function* generateVADChunks() {
  const SAMPLE_RATE = 16000;
  const CHUNK_DURATION_MS = 100; // 100ms chunks
  const CHUNK_SIZE = Math.floor(SAMPLE_RATE * CHUNK_DURATION_MS / 1000);
  const TOTAL_DURATION_SEC = 2.0;
  const NUM_CHUNKS = Math.floor(TOTAL_DURATION_SEC * 1000 / CHUNK_DURATION_MS);
  const FREQUENCY = 440.0; // A4 note

  console.log(`Generating ${NUM_CHUNKS} chunks of ${CHUNK_DURATION_MS}ms each (${TOTAL_DURATION_SEC}s total)`);
  console.log(`Pattern: 0.5s silence | 1.0s tone (${FREQUENCY}Hz) | 0.5s silence\n`);

  for (let seq = 0; seq < NUM_CHUNKS; seq++) {
    const samples = new Float32Array(CHUNK_SIZE);

    for (let i = 0; i < CHUNK_SIZE; i++) {
      const sampleIdx = seq * CHUNK_SIZE + i;
      const t = sampleIdx / SAMPLE_RATE;

      // Add tone from 0.5s to 1.5s, silence elsewhere
      if (t >= 0.5 && t <= 1.5) {
        samples[i] = 0.5 * Math.sin(2 * Math.PI * FREQUENCY * t);
      } else {
        samples[i] = 0.0; // Silence
      }
    }

    const buffer = {
      samples: Buffer.from(samples.buffer),
      sampleRate: SAMPLE_RATE,
      channels: 1,
      format: AudioFormat.F32,
      numSamples: CHUNK_SIZE
    };

    yield ['vad' as string, buffer, seq] as [string, any, number];

    // Simulate real-time streaming (100ms per chunk)
    await new Promise(resolve => setTimeout(resolve, 100));
  }
}

/**
 * Generate audio chunks for resampling
 */
async function* generateResampleChunks() {
  const SOURCE_RATE = 48000;
  const CHUNK_DURATION_MS = 100; // 100ms chunks
  const CHUNK_SIZE = Math.floor(SOURCE_RATE * CHUNK_DURATION_MS / 1000);
  const NUM_CHUNKS = 10;
  const FREQUENCY = 440.0;

  console.log(`Generating ${NUM_CHUNKS} chunks @ ${SOURCE_RATE}Hz`);
  console.log(`Chunk size: ${CHUNK_SIZE} samples (${CHUNK_DURATION_MS}ms)\n`);

  for (let seq = 0; seq < NUM_CHUNKS; seq++) {
    const samples = new Float32Array(CHUNK_SIZE);

    for (let i = 0; i < CHUNK_SIZE; i++) {
      const sampleIdx = seq * CHUNK_SIZE + i;
      const t = sampleIdx / SOURCE_RATE;
      samples[i] = 0.4 * Math.sin(2 * Math.PI * FREQUENCY * t);
    }

    const buffer = {
      samples: Buffer.from(samples.buffer),
      sampleRate: SOURCE_RATE,
      channels: 1,
      format: AudioFormat.F32,
      numSamples: CHUNK_SIZE
    };

    yield ['resample' as string, buffer, seq] as [string, any, number];

    // Simulate real-time streaming
    await new Promise(resolve => setTimeout(resolve, 100));
  }
}

/**
 * Test streaming VAD
 */
async function testStreamingVAD(client: RemoteMediaClient) {
  console.log('=== Example 1: Streaming Voice Activity Detection ===\n');

  const manifest = {
    version: 'v1',
    metadata: {
      name: 'streaming_vad',
      description: 'Real-time voice activity detection',
      createdAt: new Date().toISOString()
    },
    nodes: [
      {
        id: 'vad',
        nodeType: 'RustVADNode',
        params: JSON.stringify({
          sample_rate: 16000,
          frame_duration_ms: 30,
          energy_threshold: 0.01
        }),
        isStreaming: false
      }
    ],
    connections: []
  };

  console.log('Processing chunks...');
  const latencies: number[] = [];
  let voiceDetectedCount = 0;

  console.log('[DEBUG] About to call streamPipeline');
  const generator = client.streamPipeline(manifest, generateVADChunks());
  console.log('[DEBUG] Generator created, starting for await');
  for await (const result of generator) {
    console.log('[DEBUG] Got result:', result.sequence);
    latencies.push(result.processingTimeMs);

    const timeRange = `${(result.sequence * 0.1).toFixed(1)}-${((result.sequence + 1) * 0.1).toFixed(1)}s`;
    console.log(
      `Chunk ${result.sequence.toString().padStart(2)} [${timeRange}]: ` +
      `${result.processingTimeMs.toFixed(2).padStart(6)}ms`
    );

    // Check if voice was detected (would be in data outputs)
    if (result.hasAudioOutput) {
      voiceDetectedCount++;
    }
  }

  const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;
  console.log(`\n✅ VAD streaming completed`);
  console.log(`   Total chunks: ${latencies.length}`);
  console.log(`   Average latency: ${avgLatency.toFixed(2)}ms`);
  console.log(`   Min: ${Math.min(...latencies).toFixed(2)}ms | Max: ${Math.max(...latencies).toFixed(2)}ms`);
}

/**
 * Test streaming resampling
 */
async function testStreamingResample(client: RemoteMediaClient) {
  console.log('\n=== Example 2: Streaming Audio Resampling (48kHz → 16kHz) ===\n');

  const SOURCE_RATE = 48000;
  const TARGET_RATE = 16000;

  const manifest = {
    version: 'v1',
    metadata: {
      name: 'streaming_resample',
      description: 'Real-time audio resampling',
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

  console.log(`Resampling: ${SOURCE_RATE}Hz → ${TARGET_RATE}Hz`);
  console.log('Processing chunks...');

  const latencies: number[] = [];
  let totalInputSamples = 0;
  let totalOutputSamples = 0;

  for await (const result of client.streamPipeline(manifest, generateResampleChunks())) {
    latencies.push(result.processingTimeMs);
    totalInputSamples = result.totalSamplesProcessed;

    // Estimate output samples based on rate ratio
    const inputChunkSamples = SOURCE_RATE * 0.1; // 100ms chunk
    const outputChunkSamples = Math.floor(inputChunkSamples * TARGET_RATE / SOURCE_RATE);
    totalOutputSamples += outputChunkSamples;

    console.log(
      `Chunk ${result.sequence.toString().padStart(2)}: ` +
      `${result.processingTimeMs.toFixed(2).padStart(6)}ms ` +
      `(~${outputChunkSamples} samples output)`
    );
  }

  const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;
  const expectedRatio = TARGET_RATE / SOURCE_RATE;

  console.log(`\n✅ Resample streaming completed`);
  console.log(`   Total chunks: ${latencies.length}`);
  console.log(`   Average latency: ${avgLatency.toFixed(2)}ms`);
  console.log(`   Input samples: ${totalInputSamples}`);
  console.log(`   Expected ratio: ${expectedRatio.toFixed(3)} (${TARGET_RATE}/${SOURCE_RATE})`);
}

/**
 * Test multi-stage streaming pipeline
 */
async function testStreamingMultiStage(client: RemoteMediaClient) {
  console.log('\n=== Example 3: Multi-Stage Streaming (Resample → VAD) ===\n');

  const SOURCE_RATE = 48000;
  const TARGET_RATE = 16000;

  const manifest = {
    version: 'v1',
    metadata: {
      name: 'streaming_resample_vad',
      description: 'Real-time resample then VAD',
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

  console.log(`Pipeline: Resample (${SOURCE_RATE}Hz → ${TARGET_RATE}Hz) → VAD`);
  console.log('Processing chunks...');

  const latencies: number[] = [];

  for await (const result of client.streamPipeline(manifest, generateResampleChunks())) {
    latencies.push(result.processingTimeMs);

    console.log(
      `Chunk ${result.sequence.toString().padStart(2)}: ` +
      `${result.processingTimeMs.toFixed(2).padStart(6)}ms`
    );
  }

  const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;

  console.log(`\n✅ Multi-stage streaming completed`);
  console.log(`   Total chunks: ${latencies.length}`);
  console.log(`   Average latency: ${avgLatency.toFixed(2)}ms`);
  console.log(`   Processing: Resample + VAD per chunk`);
}

async function main() {
  const client = new RemoteMediaClient('localhost:50051');

  try {
    console.log('Connecting to gRPC service...');
    await client.connect();

    const version = await client.getVersion();
    console.log(`✅ Connected to service v${version.protocolVersion}`);
    console.log(`   Runtime version: ${version.runtimeVersion}`);
    console.log(`   Audio nodes: ${version.supportedNodeTypes.filter(t => t.includes('Rust')).join(', ')}\n`);

    // Run streaming examples
    await testStreamingVAD(client);
    await testStreamingResample(client);
    await testStreamingMultiStage(client);

    console.log('\n✅ All streaming audio examples completed successfully!');

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

main();
