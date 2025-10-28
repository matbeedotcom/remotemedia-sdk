/**
 * Multi-node pipeline example
 * 
 * Demonstrates:
 * - Chaining multiple nodes together
 * - Using connections to pass data between nodes
 * - Processing audio through multiple stages
 */

import { RemoteMediaClient, RemoteMediaError, AudioFormat } from '../../../nodejs-client/src/grpc-client';

async function main() {
  const client = new RemoteMediaClient('localhost:50051');
  
  try {
    console.log('Connecting to service...');
    await client.connect();
    
    const version = await client.getVersion();
    console.log(`Connected to service v${version.protocolVersion}\n`);
    
    console.log('=== Multi-Node Pipeline: PassThrough -> Echo ===');
    
    const manifest = {
      version: 'v1',
      metadata: {
        name: 'multi_node_test',
        description: 'Chain PassThrough and Echo nodes',
        createdAt: '2025-10-28T00:00:00Z'
      },
      nodes: [
        {
          id: 'passthrough',
          nodeType: 'PassThrough',
          params: '{}',
          isStreaming: false
        },
        {
          id: 'echo',
          nodeType: 'Echo',
          params: '{}',
          isStreaming: false
        }
      ],
      connections: [
        {
          fromNode: 'passthrough',
          fromOutput: 'audio',
          toNode: 'echo',
          toInput: 'audio'
        }
      ]
    };
    
    // Generate 1 second of sine wave
    const SAMPLE_RATE = 16000;
    const NUM_SAMPLES = SAMPLE_RATE;
    const FREQUENCY = 440.0;
    
    const samples = new Float32Array(NUM_SAMPLES);
    for (let i = 0; i < NUM_SAMPLES; i++) {
      const t = i / SAMPLE_RATE;
      samples[i] = 0.5 * Math.sin(2 * Math.PI * FREQUENCY * t);
    }
    
    const audioBuffer = {
      samples: Buffer.from(samples.buffer),
      sampleRate: SAMPLE_RATE,
      channels: 1,
      format: AudioFormat.F32,
      numSamples: NUM_SAMPLES
    };
    
    console.log(`Input audio: ${NUM_SAMPLES} samples @ ${SAMPLE_RATE}Hz`);
    console.log('Pipeline: passthrough -> echo');
    
    const result = await client.executePipeline(
      manifest,
      { passthrough: audioBuffer },
      {}
    );
    
    console.log('\n✅ Execution successful');
    console.log(`   Wall time: ${result.metrics.wallTimeMs.toFixed(2)}ms`);
    console.log(`   Nodes executed: ${Object.keys(result.metrics.nodeMetrics).length}`);
    
    for (const [nodeId, metrics] of Object.entries(result.metrics.nodeMetrics)) {
      console.log(`   - ${nodeId}: ${metrics.executionTimeMs.toFixed(2)}ms`);
    }
    
    if (result.audioOutputs.echo) {
      const output = result.audioOutputs.echo;
      console.log(`\n   Output audio: ${output.numSamples} samples @ ${output.sampleRate}Hz`);
      console.log(`   Format: ${output.format}`);
      console.log(`   Channels: ${output.channels}`);
    }
    
    console.log('\n✅ Multi-node pipeline completed successfully!');
    
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
