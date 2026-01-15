
const grpc = require('@grpc/grpc-js');
const protoLoader = require('@grpc/proto-loader');
const fs = require('fs');
const path = require('path');
const wav = require('wav');

async function testDirectAudioStreaming() {
    console.log('🧪 Testing Direct Node Audio Streaming from JavaScript\n');
    
    // 1. Connect to gRPC service
    console.log('1️⃣ Connecting to gRPC service...');
    const protoPath = path.join(__dirname, '../../remote_service/protos/execution.proto');
    const packageDefinition = protoLoader.loadSync(protoPath, {
        keepCase: true,
        longs: String,
        enums: String,
        defaults: true,
        oneofs: true
    });
    const proto = grpc.loadPackageDefinition(packageDefinition);
    const client = new proto.remotemedia.execution.RemoteExecutionService(
        'localhost:50052',
        grpc.credentials.createInsecure()
    );
    console.log('   ✅ Connected successfully!\n');

    // 2. Start node stream
    console.log('2️⃣ Starting direct node stream for AudioBuffer...');
    const stream = client.StreamNode();

    const streamingPromise = new Promise((resolve, reject) => {
        let receivedDataCount = 0;
        stream.on('data', (response) => {
            if (response.data) {
                receivedDataCount++;
                const data = JSON.parse(response.data.toString());
                console.log(`   ✅ Received data chunk ${receivedDataCount} from pipeline. Samples: ${data[0].length}`);
            }
            if (response.error) {
                const error = new Error(response.error);
                console.error('   ❌ Stream error:', error);
                reject(error);
            }
        });
        stream.on('error', (error) => {
            console.error('   ❌ GRPC Stream error:', error);
            reject(error);
        });
        stream.on('end', () => {
            console.log(`\n   ✅ Stream ended. Received ${receivedDataCount} buffered audio chunks.`);
            resolve();
        });

        // 3. Send initialization message for the node
        const initMessage = {
            init: {
                node_type: 'AudioBuffer',
                config: { buffer_size_samples: 8192 }, // Buffer ~0.5s of audio at 16kHz
                serialization_format: 'json',
            }
        };
        stream.write(initMessage);
        console.log('   ✅ Sent initialization message.\n');
        
        // 4. Read and stream audio file
        console.log('3️⃣ Reading and streaming audio file...');
        const audioFilePath = path.join(__dirname, '../examples/transcribe_demo.wav');

        if (!fs.existsSync(audioFilePath)) {
            const error = new Error(`Audio file not found at: ${audioFilePath}`);
            console.error(`   ❌ ${error.message}`);
            stream.end();
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
                
                // The AudioBuffer node expects a tuple of (audio_data, sample_rate)
                const audioTuple = [Array.from(floatSamples), format.sampleRate];
                
                const dataMessage = {
                    data: Buffer.from(JSON.stringify(audioTuple))
                };
                stream.write(dataMessage);
            });
        });

        reader.on('end', () => {
            console.log('   ✅ Finished reading audio file. Closing stream.');
            setTimeout(() => stream.end(), 1000); // Allow time for final chunks
        });

        reader.on('error', (err) => {
            console.error('   ❌ Error reading wav file:', err);
            stream.end();
            reject(err);
        });

        file.pipe(reader);
    });

    try {
        await streamingPromise;
        console.log('\n✅ Direct audio streaming test completed successfully!');
    } catch (error) {
        console.error('\n❌ Test failed:', error.message);
    } finally {
        client.close();
    }
}

// Run the test
testDirectAudioStreaming();
