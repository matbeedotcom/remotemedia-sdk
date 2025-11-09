#!/usr/bin/env node

/**
 * End-to-End TTS Test Script
 *
 * Tests the RemoteMedia gRPC service with KokoroTTSNode
 *
 * Usage:
 *   node test-tts.mjs
 *   node test-tts.mjs "Custom text to speak"
 */

import grpc from '@grpc/grpc-js';
import protoLoader from '@grpc/proto-loader';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import fs from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Configuration
const GRPC_SERVER = 'localhost:50051';
const PROTO_PATH = join(__dirname, '../../runtime/protos/streaming.proto');
const COMMON_PROTO_PATH = join(__dirname, '../../runtime/protos/common.proto');

// Test texts
const SHORT_TEXT = "Hello, this is a short test.";
const LONG_TEXT = `
The quick brown fox jumps over the lazy dog. This is a test of the Kokoro text-to-speech system.
We are testing streaming audio synthesis with multiple chunks. The system should start playing
audio within two seconds of starting synthesis. This demonstrates real-time streaming capabilities
for natural-sounding speech generation. The RemoteMedia SDK provides high-performance pipeline
execution for audio processing workloads. By leveraging Rust's performance and Python's flexibility,
we can achieve low-latency streaming synthesis suitable for interactive applications. This paragraph
contains enough text to generate multiple audio chunks, allowing us to verify that the streaming
pipeline works correctly end-to-end.
`.trim();

// Colors for console output
const colors = {
  reset: '\x1b[0m',
  green: '\x1b[32m',
  red: '\x1b[31m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
};

function log(color, ...args) {
  console.log(color + args.join(' ') + colors.reset);
}

function logSuccess(msg) { log(colors.green, '✓', msg); }
function logError(msg) { log(colors.red, '✗', msg); }
function logInfo(msg) { log(colors.blue, 'ℹ', msg); }
function logWarning(msg) { log(colors.yellow, '⚠', msg); }

// Load proto files
function loadProto() {
  logInfo('Loading protocol buffers...');

  const packageDefinition = protoLoader.loadSync(
    [PROTO_PATH, COMMON_PROTO_PATH],
    {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
      includeDirs: [join(__dirname, '../../runtime/protos')]
    }
  );

  const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
  return protoDescriptor.remotemedia.v1;
}

// Test streaming synthesis
async function testStreamingSynthesis(client, text, testName) {
  return new Promise((resolve, reject) => {
    logInfo(`\n${testName}`);
    logInfo(`Text length: ${text.length} characters`);

    const startTime = Date.now();
    let firstChunkTime = null;
    let chunkCount = 0;
    let totalSamples = 0;
    let errors = [];

    // Create pipeline manifest
    const manifest = {
      nodes: [
        {
          id: 'tts',
          type: 'KokoroTTSNode',
          params: {},
        },
      ],
      connections: [],
    };

    // Start streaming
    const call = client.StreamPipeline();

    // Handle responses
    call.on('data', (response) => {
      if (response.error) {
        logError(`Error: ${response.error.message}`);
        errors.push(response.error.message);
        return;
      }

      if (response.ready) {
        logInfo(`Stream ready, session: ${response.ready.session_id}`);
        return;
      }

      if (response.result) {
        chunkCount++;

        if (!firstChunkTime) {
          firstChunkTime = Date.now();
          const latency = firstChunkTime - startTime;

          if (latency < 2000) {
            logSuccess(`First chunk received in ${latency}ms`);
          } else {
            logWarning(`First chunk took ${latency}ms (target: <2000ms)`);
          }
        }

        // Count samples from audio output
        if (response.result.data_outputs) {
          for (const [nodeId, dataBuffer] of Object.entries(response.result.data_outputs)) {
            if (dataBuffer.audio) {
              const numSamples = dataBuffer.audio.num_samples || 0;
              totalSamples += numSamples;

              if (chunkCount % 5 === 0) {
                logInfo(`Received ${chunkCount} chunks, ${totalSamples} samples`);
              }
            }
          }
        }
      }

      if (response.closed) {
        const totalTime = Date.now() - startTime;

        logSuccess(`\nStream completed:`);
        logSuccess(`  Total chunks: ${chunkCount}`);
        logSuccess(`  Total samples: ${totalSamples}`);
        logSuccess(`  Total time: ${totalTime}ms`);

        if (totalSamples > 0) {
          const sampleRate = 24000; // Kokoro outputs 24kHz
          const durationSec = totalSamples / sampleRate;
          logSuccess(`  Audio duration: ${durationSec.toFixed(2)}s`);
        }

        resolve({
          success: errors.length === 0,
          chunkCount,
          totalSamples,
          firstChunkLatency: firstChunkTime ? firstChunkTime - startTime : null,
          totalTime,
          errors,
        });
      }
    });

    call.on('error', (err) => {
      logError(`gRPC error: ${err.message}`);
      reject(err);
    });

    call.on('end', () => {
      if (chunkCount === 0) {
        logWarning('Stream ended without receiving any chunks');
        resolve({
          success: false,
          chunkCount: 0,
          totalSamples: 0,
          firstChunkLatency: null,
          totalTime: Date.now() - startTime,
          errors: ['No chunks received'],
        });
      }
    });

    // Send init message
    call.write({
      init: {
        manifest: {
          version: manifest.version || 'v1',
          metadata: {
            name: manifest.metadata?.name || 'tts_test',
            description: manifest.metadata?.description || '',
            created_at: new Date().toISOString(),
          },
          nodes: manifest.nodes.map(node => ({
            id: node.id,
            node_type: node.type,
            params: JSON.stringify(node.params),
            is_streaming: true,
          })),
          connections: manifest.connections || [],
        },
        client_version: 'v1',
      },
    });

    // Send text as data chunk
    call.write({
      data_chunk: {
        node_id: 'tts',
        buffer: {
          text: {
            text_data: Buffer.from(text, 'utf-8'),
            encoding: 'utf-8',
          },
        },
        sequence: 0,
        timestamp_ms: Date.now(),
      },
    });

    // Send close control
    call.write({
      control: {
        command: 1, // COMMAND_CLOSE
      },
    });

    call.end();
  });
}

// Main test function
async function runTests() {
  console.log('\n' + '='.repeat(60));
  log(colors.cyan, 'RemoteMedia TTS End-to-End Test');
  console.log('='.repeat(60) + '\n');

  // Check proto files exist
  if (!fs.existsSync(PROTO_PATH)) {
    logError(`Proto file not found: ${PROTO_PATH}`);
    process.exit(1);
  }

  // Load proto
  let remotemediaProto;
  try {
    remotemediaProto = loadProto();
    logSuccess('Protocol buffers loaded');
  } catch (err) {
    logError(`Failed to load proto: ${err.message}`);
    process.exit(1);
  }

  // Create client
  logInfo(`Connecting to gRPC server at ${GRPC_SERVER}...`);
  const client = new remotemediaProto.StreamingPipelineService(
    GRPC_SERVER,
    grpc.credentials.createInsecure()
  );

  // Wait for connection
  await new Promise((resolve) => setTimeout(resolve, 500));
  logSuccess('Connected to gRPC server\n');

  // Get test text from command line or use default
  const customText = process.argv[2];
  const tests = [];

  if (customText) {
    tests.push({ name: 'Custom Text Test', text: customText });
  } else {
    tests.push(
      { name: 'Test 1: Short Text (Quick Response)', text: SHORT_TEXT },
      { name: 'Test 2: Long Text (Streaming)', text: LONG_TEXT }
    );
  }

  // Run tests
  const results = [];
  for (const test of tests) {
    try {
      const result = await testStreamingSynthesis(client, test.text, test.name);
      results.push({ ...result, name: test.name });
    } catch (err) {
      logError(`Test failed: ${err.message}`);
      results.push({
        name: test.name,
        success: false,
        errors: [err.message],
      });
    }
  }

  // Summary
  console.log('\n' + '='.repeat(60));
  log(colors.cyan, 'Test Summary');
  console.log('='.repeat(60) + '\n');

  let allPassed = true;
  results.forEach((result, i) => {
    const status = result.success ? colors.green + '✓ PASS' : colors.red + '✗ FAIL';
    console.log(`${status}${colors.reset} ${result.name}`);

    if (result.success) {
      console.log(`     First chunk: ${result.firstChunkLatency}ms`);
      console.log(`     Total chunks: ${result.chunkCount}`);
      console.log(`     Total samples: ${result.totalSamples}`);
    } else {
      console.log(`     Errors: ${result.errors.join(', ')}`);
      allPassed = false;
    }
    console.log();
  });

  // Validation checks
  console.log('='.repeat(60));
  log(colors.cyan, 'Validation');
  console.log('='.repeat(60) + '\n');

  results.forEach((result) => {
    if (result.success) {
      // Check latency requirement (<2s for first chunk)
      if (result.firstChunkLatency !== null && result.firstChunkLatency < 2000) {
        logSuccess(`${result.name}: Latency requirement met (${result.firstChunkLatency}ms < 2000ms)`);
      } else if (result.firstChunkLatency !== null) {
        logWarning(`${result.name}: Latency higher than target (${result.firstChunkLatency}ms)`);
      }

      // Check we got audio data
      if (result.totalSamples > 0) {
        logSuccess(`${result.name}: Audio data received (${result.totalSamples} samples)`);
      } else {
        logWarning(`${result.name}: No audio samples received`);
      }
    }
  });

  console.log('\n' + '='.repeat(60) + '\n');

  if (allPassed) {
    logSuccess('All tests passed!');
    process.exit(0);
  } else {
    logError('Some tests failed');
    process.exit(1);
  }
}

// Run tests
runTests().catch((err) => {
  logError(`Unhandled error: ${err.message}`);
  console.error(err);
  process.exit(1);
});
