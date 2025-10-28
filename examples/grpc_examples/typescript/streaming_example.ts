/**
 * Bidirectional streaming example
 * 
 * Demonstrates:
 * - Streaming audio chunks to server
 * - Receiving processed results in real-time
 * - Measuring per-chunk latency
 * - Proper session management
 */

import { RemoteMediaClient } from '../../../nodejs-client/src/client';
import {
  PipelineManifest,
  ManifestMetadata,
  NodeManifest,
  AudioBuffer,
  AudioFormat
} from '../../../nodejs-client/generated-types/execution_pb';
import {
  StreamInit,
  AudioChunk,
  ChunkCommand
} from '../../../nodejs-client/generated-types/streaming_pb';

async function main() {
  const client = new RemoteMediaClient('localhost:50051');
  
  try {
    console.log('Connecting to service...');
    await client.connect();
    
    const version = await client.getVersion();
    console.log(`Connected to service v${version.getVersionInfo()?.getProtocolVersion()}\n`);
    
    // Create streaming pipeline
    console.log('=== Streaming Audio Pipeline ===');
    
    const metadata = new ManifestMetadata();
    metadata.setName('streaming_test');
    metadata.setDescription('PassThrough streaming');
    metadata.setCreatedAt('2025-10-28T00:00:00Z');
    
    const node = new NodeManifest();
    node.setId('source');
    node.setNodeType('PassThrough');
    node.setParams('{}');
    node.setIsStreaming(false);
    
    const manifest = new PipelineManifest();
    manifest.setVersion('v1');
    manifest.setMetadata(metadata);
    manifest.setNodesList([node]);
    manifest.setConnectionsList([]);
    
    // Audio parameters
    const SAMPLE_RATE = 16000;
    const CHUNK_SIZE = 1600; // 100ms chunks
    const NUM_CHUNKS = 20;
    const FREQUENCY = 440.0; // A4 note
    
    console.log(`Sample rate: ${SAMPLE_RATE} Hz`);
    console.log(`Chunk size: ${CHUNK_SIZE} samples (${CHUNK_SIZE/SAMPLE_RATE*1000}ms)`);
    console.log(`Total chunks: ${NUM_CHUNKS}`);
    console.log(`Total duration: ${NUM_CHUNKS*CHUNK_SIZE/SAMPLE_RATE}s\n`);
    
    // Create stream
    const stream = client.streamPipeline();
    
    // Send init message
    const init = new StreamInit();
    init.setManifest(manifest);
    init.setClientVersion('v1');
    stream.write({ init });
    
    // Track latencies
    const latencies: number[] = [];
    const startTime = Date.now();
    
    // Handle responses
    stream.on('data', (response: any) => {
      if (response.hasChunkResult()) {
        const result = response.getChunkResult();
        const latency = result.getProcessingTimeMs();
        latencies.push(latency);
        
        console.log(
          `Chunk ${result.getSequence().toString().padStart(2)}: ` +
          `${latency.toFixed(2).padStart(6)}ms ` +
          `(${result.getTotalSamplesProcessed().toString().padStart(6)} samples total)`
        );
      } else if (response.hasError()) {
        const error = response.getError();
        console.error(`❌ Error: ${error.getMessage()}`);
      }
    });
    
    stream.on('end', () => {
      const totalTime = (Date.now() - startTime) / 1000;
      
      // Display statistics
      console.log('\n=== Statistics ===');
      console.log(`Total chunks: ${latencies.length}`);
      console.log(`Total time: ${totalTime.toFixed(2)}s`);
      console.log(`Average latency: ${(latencies.reduce((a, b) => a + b, 0) / latencies.length).toFixed(2)}ms`);
      console.log(`Min latency: ${Math.min(...latencies).toFixed(2)}ms`);
      console.log(`Max latency: ${Math.max(...latencies).toFixed(2)}ms`);
      
      // Check target
      const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;
      const targetLatency = 50.0;
      
      if (avgLatency < targetLatency) {
        console.log(`\n✅ Target met: ${avgLatency.toFixed(2)}ms < ${targetLatency}ms`);
      } else {
        console.log(`\n⚠️  Target missed: ${avgLatency.toFixed(2)}ms >= ${targetLatency}ms`);
      }
    });
    
    stream.on('error', (error: Error) => {
      console.error(`\n❌ Stream error: ${error.message}`);
      process.exit(1);
    });
    
    console.log('=== Processing Chunks ===');
    
    // Send chunks
    for (let seq = 0; seq < NUM_CHUNKS; seq++) {
      // Generate samples
      const samples = new Float32Array(CHUNK_SIZE);
      for (let i = 0; i < CHUNK_SIZE; i++) {
        const sampleIdx = seq * CHUNK_SIZE + i;
        const t = sampleIdx / SAMPLE_RATE;
        samples[i] = 0.5 * Math.sin(2 * Math.PI * FREQUENCY * t);
      }
      
      // Create audio buffer
      const buffer = new AudioBuffer();
      buffer.setSamples(Buffer.from(samples.buffer));
      buffer.setSampleRate(SAMPLE_RATE);
      buffer.setChannels(1);
      buffer.setFormat(AudioFormat.AUDIO_FORMAT_F32);
      buffer.setNumSamples(CHUNK_SIZE);
      
      // Create chunk
      const chunk = new AudioChunk();
      chunk.setNodeId('source');
      chunk.setAudioData(buffer);
      chunk.setSequence(seq);
      
      // Send chunk
      stream.write({ chunk });
      
      // Simulate real-time streaming
      await new Promise(resolve => setTimeout(resolve, 50)); // 50ms delay
    }
    
    // Send close command
    const closeChunk = new AudioChunk();
    closeChunk.setNodeId('source');
    closeChunk.setSequence(NUM_CHUNKS);
    closeChunk.setCommand(ChunkCommand.CHUNK_COMMAND_CLOSE);
    stream.write({ chunk: closeChunk });
    
    // End stream
    stream.end();
    
  } catch (error) {
    console.error(`\n❌ Error: ${error}`);
    process.exit(1);
  }
}

// Helper function to wait
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

main();
