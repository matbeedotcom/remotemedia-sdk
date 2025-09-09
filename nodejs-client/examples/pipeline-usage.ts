/**
 * Example: Pipeline Usage with RemoteMedia Processing SDK
 * 
 * This example demonstrates how to:
 * 1. Register pipelines with the remote service
 * 2. Execute pipelines with data
 * 3. Stream data through pipelines
 * 4. Use source and sink nodes for JavaScript integration
 */

import { PipelineClient, PipelineBuilder } from '../src/pipeline-client';

// Example 1: Register and execute a simple pipeline
async function simpleProcessingPipeline() {
  console.log('=== Simple Processing Pipeline Example ===\n');

  const client = new PipelineClient('localhost', 50052);
  await client.connect();

  try {
    // Build a simple audio processing pipeline
    const pipeline = new PipelineBuilder('AudioProcessingPipeline')
      .addNode({
        nodeId: 'audio_transform_1',
        nodeType: 'AudioTransform',
        config: {
          output_sample_rate: 16000,
          output_channels: 1
        }
      })
      .addNode({
        nodeId: 'vad_1',
        nodeType: 'VoiceActivityDetector',
        config: {
          frame_duration_ms: 30,
          energy_threshold: 0.02
        }
      })
      .setMetadata({
        category: 'audio',
        description: 'Simple audio processing with VAD'
      })
      .build();

    // Register the pipeline
    const pipelineId = await client.registerPipeline(
      'audio_processor',
      pipeline,
      {
        dependencies: ['numpy', 'scipy'],
        autoExport: true
      }
    );

    console.log(`✅ Pipeline registered with ID: ${pipelineId}`);

    // Execute the pipeline with audio data
    const audioData = new Float32Array(16000); // 1 second of audio
    const result = await client.executePipeline(pipelineId, audioData);
    
    console.log('📊 Pipeline execution result:', result);

    // Clean up
    await client.unregisterPipeline(pipelineId);
    console.log('🗑️ Pipeline unregistered\n');

  } finally {
    client.close();
  }
}

// Example 2: Stream data through a pipeline with source and sink nodes
async function streamingPipeline() {
  console.log('=== Streaming Pipeline Example ===\n');

  const client = new PipelineClient('localhost', 50052);
  await client.connect();

  try {
    // Build a streaming pipeline with source and sink
    const pipeline = new PipelineBuilder('StreamingDataPipeline')
      .addSource('data_source', {
        buffer_size: 100,
        timeout_seconds: 30
      })
      .addNode({
        nodeId: 'text_transform_1',
        nodeType: 'TextTransformNode',
        config: {
          transform_type: 'uppercase'
        }
      })
      .addSink('data_sink', {
        buffer_output: true,
        buffer_size: 100
      })
      .setMetadata({
        category: 'streaming',
        description: 'Streaming text transformation pipeline'
      })
      .build();

    // Register the pipeline
    const pipelineId = await client.registerPipeline(
      'streaming_transformer',
      pipeline
    );

    console.log(`✅ Streaming pipeline registered: ${pipelineId}`);

    // Create a streaming session
    const stream = client.streamPipeline(pipelineId);

    // Handle stream events
    stream.on('ready', (sessionId) => {
      console.log(`🚀 Stream ready with session: ${sessionId}`);
    });

    stream.on('data', (data) => {
      console.log('📦 Received data:', data);
    });

    stream.on('error', (error) => {
      console.error('❌ Stream error:', error);
    });

    stream.on('status', (status) => {
      console.log('📊 Stream status:', status);
    });

    // Wait for stream to be ready
    await new Promise(resolve => stream.once('ready', resolve));

    // Send data through the stream
    const testData = ['hello', 'world', 'from', 'javascript'];
    for (const item of testData) {
      console.log(`➡️ Sending: ${item}`);
      await stream.send(item);
      await new Promise(resolve => setTimeout(resolve, 100)); // Small delay
    }

    // Flush and close the stream
    await stream.control('FLUSH');
    await stream.close();

    // Clean up
    await client.unregisterPipeline(pipelineId);
    console.log('🗑️ Streaming pipeline unregistered\n');

  } finally {
    client.close();
  }
}

// Example 3: WebRTC pipeline registration
async function webrtcPipeline() {
  console.log('=== WebRTC Pipeline Registration Example ===\n');

  const client = new PipelineClient('localhost', 50052);
  await client.connect();

  try {
    // Build the WebRTC speech-to-speech pipeline
    const pipeline = new PipelineBuilder('WebRTCSpeechPipeline')
      .addSource('webrtc_audio_input', {
        buffer_size: 1000,
        timeout_seconds: 60
      })
      .addNode({
        nodeId: 'audio_transform',
        nodeType: 'AudioTransform',
        config: {
          output_sample_rate: 16000,
          output_channels: 1
        }
      })
      .addNode({
        nodeId: 'vad',
        nodeType: 'VoiceActivityDetector',
        config: {
          frame_duration_ms: 30,
          energy_threshold: 0.02,
          speech_threshold: 0.3,
          include_metadata: true
        },
        isStreaming: true
      })
      .addNode({
        nodeId: 'vad_buffer',
        nodeType: 'VADTriggeredBuffer',
        config: {
          min_speech_duration_s: 1.0,
          silence_duration_s: 0.5,
          pre_speech_buffer_s: 1.0,
          sample_rate: 16000
        },
        isStreaming: true
      })
      .addNode({
        nodeId: 'ultravox',
        nodeType: 'UltravoxNode',
        config: {
          model_id: 'fixie-ai/ultravox-v0_5-llama-3_1-8b',
          system_prompt: 'You are a helpful assistant. Respond concisely.'
        },
        isRemote: true,
        remoteEndpoint: 'localhost'
      })
      .addNode({
        nodeId: 'kokoro_tts',
        nodeType: 'KokoroTTSNode',
        config: {
          lang_code: 'a',
          voice: 'af_heart',
          speed: 1.0,
          sample_rate: 24000,
          stream_chunks: true
        },
        isStreaming: true,
        isRemote: true,
        remoteEndpoint: 'localhost'
      })
      .addSink('webrtc_audio_output', {
        buffer_output: false
      })
      .setMetadata({
        category: 'webrtc',
        description: 'Complete speech-to-speech pipeline for WebRTC',
        webrtc_enabled: 'true'
      })
      .build();

    // Register the WebRTC pipeline
    const pipelineId = await client.registerPipeline(
      'webrtc_speech_to_speech',
      pipeline,
      {
        dependencies: [
          'aiortc',
          'aiohttp',
          'transformers',
          'torch',
          'numpy',
          'scipy'
        ],
        autoExport: true
      }
    );

    console.log(`✅ WebRTC pipeline registered: ${pipelineId}`);

    // Get pipeline info
    const { info, metrics } = await client.getPipelineInfo(
      pipelineId,
      false,
      true
    );

    console.log('📋 Pipeline Info:', info);
    if (metrics) {
      console.log('📊 Pipeline Metrics:', metrics);
    }

    // List all pipelines in WebRTC category
    const webrtcPipelines = await client.listPipelines('webrtc');
    console.log(`\n📝 Found ${webrtcPipelines.length} WebRTC pipeline(s)`);

    // The pipeline is now registered and can be used by WebRTC clients
    console.log('\n✨ Pipeline is ready for WebRTC connections!');
    console.log('   Clients can now connect and stream audio through this pipeline.');

    // Keep the pipeline registered (in production, you'd keep it)
    // For this example, we'll clean up
    await new Promise(resolve => setTimeout(resolve, 2000));
    await client.unregisterPipeline(pipelineId);
    console.log('🗑️ WebRTC pipeline unregistered\n');

  } finally {
    client.close();
  }
}

// Example 4: Interactive bidirectional pipeline
async function bidirectionalPipeline() {
  console.log('=== Bidirectional Pipeline Example ===\n');

  const client = new PipelineClient('localhost', 50052);
  await client.connect();

  try {
    // Build a bidirectional Q&A pipeline
    const pipeline = new PipelineBuilder('InteractiveQAPipeline')
      .addNode({
        nodeId: 'js_bridge',
        nodeType: 'JavaScriptBridgeNode',
        config: {}
      })
      .addNode({
        nodeId: 'bidirectional_io',
        nodeType: 'BidirectionalNode',
        config: {
          buffer_size: 50
        },
        isSource: true,
        isSink: true,
        isStreaming: true
      })
      .setMetadata({
        category: 'interactive',
        description: 'Bidirectional Q&A pipeline for interactive sessions'
      })
      .build();

    // Register the pipeline
    const pipelineId = await client.registerPipeline(
      'interactive_qa',
      pipeline
    );

    console.log(`✅ Bidirectional pipeline registered: ${pipelineId}`);

    // Create a bidirectional stream
    const stream = client.streamPipeline(pipelineId, {
      bidirectional: true
    });

    // Track questions and answers
    const qa: Array<{ question: string; answer?: string }> = [];
    let currentIndex = 0;

    stream.on('ready', async () => {
      console.log('🔄 Bidirectional stream ready');

      // Send questions
      const questions = [
        'What is the capital of France?',
        'How many planets are in our solar system?',
        'What is 2 + 2?'
      ];

      for (const question of questions) {
        console.log(`\n❓ Asking: ${question}`);
        qa.push({ question });
        await stream.send({ type: 'question', text: question });
        await new Promise(resolve => setTimeout(resolve, 500));
      }
    });

    stream.on('data', (data) => {
      console.log(`💬 Answer: ${JSON.stringify(data)}`);
      if (currentIndex < qa.length) {
        qa[currentIndex].answer = data;
        currentIndex++;
      }
    });

    // Wait for responses
    await new Promise(resolve => setTimeout(resolve, 3000));

    // Display Q&A summary
    console.log('\n📝 Q&A Summary:');
    qa.forEach((item, i) => {
      console.log(`  ${i + 1}. Q: ${item.question}`);
      console.log(`     A: ${item.answer || 'No response'}`);
    });

    // Close stream and clean up
    await stream.close();
    await client.unregisterPipeline(pipelineId);
    console.log('\n🗑️ Bidirectional pipeline unregistered\n');

  } finally {
    client.close();
  }
}

// Main function to run all examples
async function main() {
  console.log('🚀 RemoteMedia Pipeline Client Examples\n');
  console.log('Make sure the remote service is running on localhost:50052\n');

  try {
    // Run examples sequentially
    await simpleProcessingPipeline();
    await streamingPipeline();
    await webrtcPipeline();
    await bidirectionalPipeline();

    console.log('✅ All examples completed successfully!');
  } catch (error) {
    console.error('❌ Example failed:', error);
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  main().catch(console.error);
}

// Export examples for testing
export {
  simpleProcessingPipeline,
  streamingPipeline,
  webrtcPipeline,
  bidirectionalPipeline
};