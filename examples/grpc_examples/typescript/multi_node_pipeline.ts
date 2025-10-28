/**
 * Multi-node pipeline example
 * 
 * Demonstrates:
 * - Chaining multiple nodes together
 * - Using connections to pass data between nodes
 * - Processing audio through multiple stages
 */

import { RemoteMediaClient } from '../../../nodejs-client/src/client';
import {
  PipelineManifest,
  ManifestMetadata,
  NodeManifest,
  Connection,
  ExecuteRequest,
  AudioBuffer,
  AudioFormat
} from '../../../nodejs-client/generated-types/execution_pb';

async function main() {
  const client = new RemoteMediaClient('localhost:50051');
  
  try {
    console.log('Connecting to service...');
    await client.connect();
    
    const version = await client.getVersion();
    console.log(`Connected to service v${version.getVersionInfo()?.getProtocolVersion()}\n`);
    
    // Create multi-node pipeline: PassThrough -> Echo
    console.log('=== Multi-Node Pipeline: PassThrough -> Echo ===');
    
    const metadata = new ManifestMetadata();
    metadata.setName('multi_node_test');
    metadata.setDescription('Chain PassThrough and Echo nodes');
    metadata.setCreatedAt('2025-10-28T00:00:00Z');
    
    const passthroughNode = new NodeManifest();
    passthroughNode.setId('passthrough');
    passthroughNode.setNodeType('PassThrough');
    passthroughNode.setParams('{}');
    passthroughNode.setIsStreaming(false);
    
    const echoNode = new NodeManifest();
    echoNode.setId('echo');
    echoNode.setNodeType('Echo');
    echoNode.setParams('{}');
    echoNode.setIsStreaming(false);
    
    const connection = new Connection();
    connection.setFromNode('passthrough');
    connection.setFromOutput('audio');
    connection.setToNode('echo');
    connection.setToInput('audio');
    
    const manifest = new PipelineManifest();
    manifest.setVersion('v1');
    manifest.setMetadata(metadata);
    manifest.setNodesList([passthroughNode, echoNode]);
    manifest.setConnectionsList([connection]);
    
    // Generate 1 second of sine wave audio
    const SAMPLE_RATE = 16000;
    const NUM_SAMPLES = SAMPLE_RATE; // 1 second
    const FREQUENCY = 440.0; // A4 note
    
    const samples = new Float32Array(NUM_SAMPLES);
    for (let i = 0; i < NUM_SAMPLES; i++) {
      const t = i / SAMPLE_RATE;
      samples[i] = 0.5 * Math.sin(2 * Math.PI * FREQUENCY * t);
    }
    
    // Create audio buffer
    const audioBuffer = new AudioBuffer();
    audioBuffer.setSamples(Buffer.from(samples.buffer));
    audioBuffer.setSampleRate(SAMPLE_RATE);
    audioBuffer.setChannels(1);
    audioBuffer.setFormat(AudioFormat.AUDIO_FORMAT_F32);
    audioBuffer.setNumSamples(NUM_SAMPLES);
    
    console.log(`Input audio: ${NUM_SAMPLES} samples @ ${SAMPLE_RATE}Hz`);
    console.log('Pipeline: passthrough -> echo');
    
    // Execute pipeline
    const request = new ExecuteRequest();
    request.setManifest(manifest);
    request.getAudioInputsMap().set('passthrough', audioBuffer);
    request.setClientVersion('v1');
    
    const response = await client.executePipeline(request);
    
    if (response.hasResult()) {
      const result = response.getResult()!;
      console.log('\n✅ Execution successful');
      console.log(`   Wall time: ${result.getMetrics()?.getWallTimeMs().toFixed(2)}ms`);
      console.log(`   Nodes executed: ${result.getNodeMetricsMap().getLength()}`);
      
      // Display per-node metrics
      result.getNodeMetricsMap().forEach((metrics, nodeId) => {
        console.log(`   - ${nodeId}: ${metrics.getExecutionTimeMs().toFixed(2)}ms`);
      });
      
      // Check output
      const outputAudio = result.getAudioOutputsMap().get('echo');
      if (outputAudio) {
        console.log(`\n   Output audio: ${outputAudio.getNumSamples()} samples @ ${outputAudio.getSampleRate()}Hz`);
        console.log(`   Format: ${AudioFormat[outputAudio.getFormat()]}`);
        console.log(`   Channels: ${outputAudio.getChannels()}`);
      }
      
      console.log('\n✅ Multi-node pipeline completed successfully!');
    } else {
      const error = response.getError()!;
      console.error(`❌ Error: ${error.getMessage()}`);
      process.exit(1);
    }
    
  } catch (error) {
    console.error(`\n❌ Error: ${error}`);
    process.exit(1);
  } finally {
    await client.disconnect();
  }
}

main();
