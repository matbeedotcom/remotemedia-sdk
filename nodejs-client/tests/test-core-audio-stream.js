
const { PipelineClient, PipelineBuilder } = require('./dist/src/pipeline-client');
const fs = require('fs');
const path = require('path');
const wav = require('wav');

async function testCoreAudioStreaming() {
    console.log('🧪 Testing Core Audio Streaming from JavaScript\n');
    
    const client = new PipelineClient('localhost', 50052);
    let pipelineId;

    try {
        // 1. Connect
        console.log('1️⃣ Connecting to gRPC service...');
        await client.connect();
        console.log('   ✅ Connected successfully!\n');

        // 2. Create and register a minimal, core pipeline
        console.log('2️⃣ Creating and registering a core streaming pipeline...');
        
        const corePipeline = new PipelineBuilder('JavaScriptCoreStreamPipeline')
            .addSource('audio_source')
            .addNode({
                nodeId: 'audio_buffer',
                nodeType: 'AudioBuffer',
                config: {
                    buffer_size_samples: 16000 // Buffer 1 second of audio at 16kHz
                }
            })
            .addSink('results_sink')
            .setMetadata({
                category: 'javascript-created',
                description: 'A minimal pipeline to test core audio streaming from JS',
                created_by: 'javascript_client'
            })
            .build();

        pipelineId = await client.registerPipeline(
            'js_core_stream_test',
            corePipeline,
            {
                autoExport: true
            }
        );
        console.log(`   ✅ Core streaming pipeline registered: ${pipelineId}\n`);

        // 3. Start pipeline stream
        console.log('3️⃣ Starting pipeline stream...');
        const stream = client.streamPipeline(pipelineId);

        const streamingPromise = new Promise((resolve, reject) => {
            let receivedDataCount = 0;
            stream.on('data', (data) => {
                receivedDataCount++;
                console.log(`   Received data chunk ${receivedDataCount} from pipeline.`);
            });
            stream.on('error', (error) => {
                console.error('   ❌ Stream error:', error);
                reject(error);
            });
            stream.on('end', () => {
                console.log(`\n   ✅ Stream ended. Received ${receivedDataCount} data chunks.`);
                resolve();
            });
            stream.on('ready', (sessionId) => {
                console.log(`   ✅ Stream ready with session ID: ${sessionId}`);
                
                // 4. Read and stream audio file
                console.log('4️⃣ Reading and streaming audio file...');
                const audioFilePath = path.join(__dirname, '../examples/transcribe_demo.wav');

                if (!fs.existsSync(audioFilePath)) {
                    const error = new Error(`Audio file not found at: ${audioFilePath}`);
                    console.error(`   ❌ ${error.message}`);
                    stream.close();
                    return reject(error);
                }
                
                const file = fs.createReadStream(audioFilePath);
                const reader = new wav.Reader();

                reader.on('format', (format) => {
                    console.log(`   🎧 Audio format: ${format.channels} channels, ${format.sampleRate} Hz`);
                    reader.on('data', (chunk) => {
                        const samples = new Int16Array(chunk.buffer, chunk.byteOffset, chunk.length / 2);
                        const floatSamples = new Float32Array(samples.length);
                        for (let i = 0; i < samples.length; i++) {
                            floatSamples[i] = samples[i] / 32768.0;
                        }
                        
                        const audioData = {
                            audio_data: Array.from(floatSamples),
                            sample_rate: format.sampleRate
                        };
                        stream.send(audioData);
                    });
                });

                reader.on('end', () => {
                    console.log('   ✅ Finished reading audio file.');
                    setTimeout(() => stream.close(), 1000);
                });

                reader.on('error', (err) => {
                    console.error('   ❌ Error reading wav file:', err);
                    stream.close();
                    reject(err);
                });

                file.pipe(reader);
            });
        });

        await streamingPromise;
        console.log('\n✅ Core audio streaming test completed successfully!');

    } catch (error) {
        console.error('❌ Test failed:', error.message);
        if (error.stack) {
            console.error(error.stack);
        }
    } finally {
        if (pipelineId) {
            try {
                await client.unregisterPipeline(pipelineId);
                console.log('   🗑️ Pipeline unregistered.');
            } catch (unregisterError) {
                console.error('   ❌ Failed to unregister pipeline:', unregisterError.message);
            }
        }
        client.close();
    }
}

// Run the test
testCoreAudioStreaming();
