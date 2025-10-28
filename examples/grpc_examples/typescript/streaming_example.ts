/**
 * Bidirectional streaming example
 * 
 * Demonstrates:
 * - Streaming audio chunks to server
 * - Receiving processed results in real-time
 * - Measuring per-chunk latency
 * - Proper session management
 */

import { RemoteMediaClient, RemoteMediaError, AudioFormat } from '../../../nodejs-client/src/grpc-client';

async function* generateChunks() {
  const SAMPLE_RATE = 16000;
  const CHUNK_SIZE = 1600;
  const NUM_CHUNKS = 20;
  const FREQUENCY = 440.0;
  
  for (let seq = 0; seq < NUM_CHUNKS; seq++) {
    const samples = new Float32Array(CHUNK_SIZE);
    for (let i = 0; i < CHUNK_SIZE; i++) {
      const sampleIdx = seq * CHUNK_SIZE + i;
      const t = sampleIdx / SAMPLE_RATE;
      samples[i] = 0.5 * Math.sin(2 * Math.PI * FREQUENCY * t);
    }
    
    const buffer = {
      samples: Buffer.from(samples.buffer),
      sampleRate: SAMPLE_RATE,
      channels: 1,
      format: AudioFormat.F32,
      numSamples: CHUNK_SIZE
    };
    
    yield ['source' as string, buffer, seq] as [string, any, number];
    await new Promise(resolve => setTimeout(resolve, 50));
  }
}

async function main() {
  const client = new RemoteMediaClient('localhost:50051');
  
  try {
    console.log('Connecting to service...');
    await client.connect();
    
    const version = await client.getVersion();
    console.log(`Connected to service v${version.protocolVersion}\n`);
    
    console.log('=== Streaming Audio Pipeline ===');
    
    const manifest = {
      version: 'v1',
      metadata: {
        name: 'streaming_test',
        description: 'PassThrough streaming',
        createdAt: '2025-10-28T00:00:00Z'
      },
      nodes: [
        {
          id: 'source',
          nodeType: 'PassThrough',
          params: '{}',
          isStreaming: false
        }
      ],
      connections: []
    };
    
    const SAMPLE_RATE = 16000;
    const CHUNK_SIZE = 1600;
    const NUM_CHUNKS = 20;
    
    console.log(`Sample rate: ${SAMPLE_RATE} Hz`);
    console.log(`Chunk size: ${CHUNK_SIZE} samples (${CHUNK_SIZE/SAMPLE_RATE*1000}ms)`);
    console.log(`Total chunks: ${NUM_CHUNKS}`);
    console.log(`Total duration: ${NUM_CHUNKS*CHUNK_SIZE/SAMPLE_RATE}s\n`);
    
    console.log('=== Processing Chunks ===');
    const latencies: number[] = [];
    const startTime = Date.now();
    
    for await (const result of client.streamPipeline(manifest, generateChunks())) {
      latencies.push(result.processingTimeMs);
      console.log(
        `Chunk ${result.sequence.toString().padStart(2)}: ` +
        `${result.processingTimeMs.toFixed(2).padStart(6)}ms ` +
        `(${result.totalSamplesProcessed.toString().padStart(6)} samples total)`
      );
    }
    
    const totalTime = (Date.now() - startTime) / 1000;
    
    console.log('\n=== Statistics ===');
    console.log(`Total chunks: ${latencies.length}`);
    console.log(`Total time: ${totalTime.toFixed(2)}s`);
    console.log(`Average latency: ${(latencies.reduce((a, b) => a + b, 0) / latencies.length).toFixed(2)}ms`);
    console.log(`Min latency: ${Math.min(...latencies).toFixed(2)}ms`);
    console.log(`Max latency: ${Math.max(...latencies).toFixed(2)}ms`);
    
    const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;
    const targetLatency = 50.0;
    
    if (avgLatency < targetLatency) {
      console.log(`\n✅ Target met: ${avgLatency.toFixed(2)}ms < ${targetLatency}ms`);
    } else {
      console.log(`\n⚠️  Target missed: ${avgLatency.toFixed(2)}ms >= ${targetLatency}ms`);
    }
    
  } catch (error) {
    if (error instanceof RemoteMediaError) {
      console.error(`\n❌ Error: ${error.message}`);
      if (error.errorType) console.error(`   Type: ${error.errorType}`);
    } else {
      console.error(`\n❌ Error: ${error}`);
    }
    process.exit(1);
  } finally {
    await client.disconnect();
  }
}

main();
