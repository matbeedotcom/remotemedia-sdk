
const { PipelineClient, PipelineBuilder } = require('./dist/src/pipeline-client');
const fs = require('fs');
const path = require('path');
const wav = require('wav');

async function testAudioStreaming() {
    console.log('🧪 Testing Audio Streaming from JavaScript\n');
    
    const client = new PipelineClient('localhost', 50052);
    let pipelineId;

    try {
        // 1. Connect
        console.log('1️⃣ Connecting to gRPC service...');
        await client.connect();
        console.log('   ✅ Connected successfully!\n');

        // 2. Create and register a new audio pipeline
        console.log('2️⃣ Creating and registering a new audio pipeline...');
        
        const audioPipeline = new PipelineBuilder('JavaScriptAudioStreamPipeline')
            .addNode({
                nodeId: 'audio_buffer',
                nodeType: 'AudioBuffer',
                config: {
                    buffer_size_samples: 8192
                }
            })
            .addNode({
                nodeId: 'data_sink',
                nodeType: 'DataSinkNode',
                config: {}
            })
            .setMetadata({
                category: 'javascript-created',
                description: 'Pipeline for testing audio streaming from JS',
                created_by: 'javascript_client'
            })
            .build();

        pipelineId = await client.registerPipeline(
            'js_audio_stream_test',
            audioPipeline,
            {
                metadata: {
                    source: 'javascript_client',
                    timestamp: new Date().toISOString()
                },
                autoExport: true
            }
        );
        console.log(`   ✅ Audio pipeline registered: ${pipelineId}\n`);

        // 3. Start pipeline stream
        console.log('3️⃣ Starting pipeline stream...');
        const stream = client.streamPipeline(pipelineId);

        const transcriptionPromise = new Promise((resolve, reject) => {
            stream.on('data', (data) => {
                console.log('   Received data from pipeline:', data);
            });
            stream.on('error', (error) => {
                console.error('   ❌ Stream error:', error);
                reject(error);
            });
            stream.on('end', () => {
                console.log('   ✅ Stream ended.');
                resolve();
            });
            stream.on('ready', (sessionId) => {
                console.log(`   ✅ Stream ready with session ID: ${sessionId}`);
                
                // 4. Read and stream audio file
                console.log('4️⃣ Reading and streaming audio file...');
                const audioFilePath = path.join(__dirname, '../examples/transcribe_demo.wav');

                if (!fs.existsSync(audioFilePath)) {
                    console.error(`   ❌ Audio file not found at: ${audioFilePath}`);
                    stream.close();
                    reject(new Error('Audio file not found.'));
                    return;
                }
                
                const file = fs.createReadStream(audioFilePath);
                const reader = new wav.Reader();

                reader.on('format', (format) => {
                    reader.on('data', (chunk) => {
                        const audioData = {
                            samples: Array.from(new Float32Array(new Uint8Array(chunk).buffer)),
                            sample_rate: format.sampleRate,
                            channels: format.channels
                        };
                        stream.send(audioData);
                    });
                });

                reader.on('end', () => {
                    console.log('   ✅ Finished reading audio file.');
                    stream.close();
                });

                reader.on('error', (err) => {
                    console.error('   ❌ Error reading wav file:', err);
                    stream.close();
                    reject(err);
                });

                file.pipe(reader);
            });
        });

        await transcriptionPromise;
        console.log('\n✅ Audio streaming test completed successfully!');

    } catch (error) {
        console.error('❌ Test failed:', error.message);
        if (error.stack) {
            console.error(error.stack);
        }
    } finally {
        if (pipelineId) {
            try {
                await client.unregisterPipeline(pipelineId);
                console.log('   🗑️ Pipeline unregistered\n');
            } catch (unregisterError) {
                console.error('   ❌ Failed to unregister pipeline:', unregisterError.message);
            }
        }
        client.close();
    }
}

// Run the test
testAudioStreaming();
