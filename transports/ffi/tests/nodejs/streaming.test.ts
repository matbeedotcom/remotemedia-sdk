/**
 * Streaming Pipeline Tests
 *
 * Tests streaming input/output pipelines where:
 * - Inputs stream continuously into the pipeline
 * - Outputs stream back to the client
 * - Nodes with is_streaming: true process data in real-time
 */

interface NapiRuntimeData {
  dataType: number;
  getAudioSamples(): Buffer;
  getAudioSampleRate(): number;
  getAudioChannels(): number;
  getAudioNumSamples(): number;
  getVideoPixels(): Buffer;
  getVideoWidth(): number;
  getVideoHeight(): number;
  getText(): string;
  getBinary(): Buffer;
}

interface PipelineOutput {
  size: number;
  getNodeIds(): string[];
  get(nodeId: string): NapiRuntimeData | null;
  has(nodeId: string): boolean;
}

interface StreamSession {
  readonly sessionId: string;
  readonly isActive: boolean;
  sendInput(data: NapiRuntimeData): Promise<void>;
  recvOutput(): Promise<NapiRuntimeData | null>;
  close(): Promise<void>;
}

interface NativeModule {
  NapiRuntimeData: {
    audio(samplesBuffer: Buffer, sampleRate: number, channels: number): NapiRuntimeData;
    video(
      pixelData: Buffer,
      width: number,
      height: number,
      format: number,
      codec: number | undefined,
      frameNumber: number,
      isKeyframe: boolean
    ): NapiRuntimeData;
    text(text: string): NapiRuntimeData;
    binary(data: Buffer): NapiRuntimeData;
  };

  executePipeline(
    manifestJson: string,
    inputs: Record<string, NapiRuntimeData>
  ): Promise<PipelineOutput>;

  executePipelineWithSession(
    manifestJson: string,
    inputs: Record<string, NapiRuntimeData>,
    sessionId: string
  ): Promise<PipelineOutput>;

  // Streaming API (may not exist yet)
  createStreamSession?(manifestJson: string): Promise<StreamSession>;

  isNativeLoaded(): boolean;
  getLoadError(): Error | null;
}

let native: NativeModule | null = null;
let loadError: Error | null = null;

try {
  native = require('../../nodejs') as NativeModule;
} catch (e) {
  loadError = e as Error;
}

function createAudioSamplesBuffer(numSamples: number, frequency: number = 440): Buffer {
  const buffer = Buffer.alloc(numSamples * 4);
  for (let i = 0; i < numSamples; i++) {
    const sample = Math.sin((2 * Math.PI * frequency * i) / 48000);
    buffer.writeFloatLE(sample, i * 4);
  }
  return buffer;
}

function readSamplesBuffer(buffer: Buffer): number[] {
  const samples: number[] = [];
  for (let i = 0; i < buffer.length; i += 4) {
    samples.push(buffer.readFloatLE(i));
  }
  return samples;
}

describe('Streaming Pipeline Execution', () => {
  beforeAll(() => {
    if (!native || !native.isNativeLoaded()) {
      console.warn(
        'Native module not loaded, skipping streaming tests.',
        'Build with: cargo build --features napi --no-default-features'
      );
      if (loadError) {
        console.warn('Load error:', loadError.message);
      }
    }
  });

  describe('Streaming Session API', () => {
    test('should have createStreamSession function', () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      // Check if streaming API exists
      const hasStreamingAPI = typeof native.createStreamSession === 'function';
      expect(hasStreamingAPI).toBe(true);
    });

    test('should create streaming session with is_streaming node', async () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      expect(typeof native.createStreamSession).toBe('function');

      const manifest = {
        version: 'v1',
        metadata: { name: 'streaming-test' },
        nodes: [
          {
            id: 'audio_stream',
            node_type: 'PassThrough',
            is_streaming: true,
            params: {},
          },
        ],
        connections: [],
      };

      const session = await native.createStreamSession!(JSON.stringify(manifest));

      expect(session).toBeDefined();
      expect(session.sessionId).toBeDefined();
      expect(session.isActive).toBe(true);

      await session.close();
      expect(session.isActive).toBe(false);
    });

    test('should stream audio chunks through pipeline', async () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      expect(typeof native.createStreamSession).toBe('function');

      const manifest = {
        version: 'v1',
        metadata: { name: 'audio-streaming-test' },
        nodes: [
          {
            id: 'passthrough',
            node_type: 'PassThrough',
            is_streaming: true,
            params: {},
          },
        ],
        connections: [],
      };

      const session = await native.createStreamSession!(JSON.stringify(manifest));

      // Stream 10 audio chunks
      const numChunks = 10;
      const samplesPerChunk = 480; // 10ms @ 48kHz
      const receivedOutputs: NapiRuntimeData[] = [];

      for (let i = 0; i < numChunks; i++) {
        const audioData = native.NapiRuntimeData.audio(
          createAudioSamplesBuffer(samplesPerChunk, 440 + i * 10),
          48000,
          1
        );

        await session.sendInput(audioData);

        // Receive output (may be async)
        const output = await session.recvOutput();
        if (output) {
          receivedOutputs.push(output);
        }
      }

      console.log(`Streamed ${numChunks} chunks, received ${receivedOutputs.length} outputs`);

      await session.close();

      expect(receivedOutputs.length).toBeGreaterThan(0);
    });

    test('should stream video frames through pipeline', async () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      expect(typeof native.createStreamSession).toBe('function');

      const manifest = {
        version: 'v1',
        metadata: { name: 'video-streaming-test' },
        nodes: [
          {
            id: 'video_passthrough',
            node_type: 'PassThrough',
            is_streaming: true,
            params: {},
          },
        ],
        connections: [],
      };

      const session = await native.createStreamSession!(JSON.stringify(manifest));

      // Stream 5 video frames
      const width = 320;
      const height = 240;
      const numFrames = 5;

      for (let i = 0; i < numFrames; i++) {
        const pixelData = Buffer.alloc(width * height * 3);
        for (let j = 0; j < pixelData.length; j++) {
          pixelData[j] = (j + i) % 256;
        }

        const videoData = native.NapiRuntimeData.video(
          pixelData,
          width,
          height,
          4, // RGB24
          undefined,
          i,
          i === 0 // first frame is keyframe
        );

        await session.sendInput(videoData);
      }

      await session.close();
    });

    test('should handle bidirectional streaming', async () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      expect(typeof native.createStreamSession).toBe('function');

      const manifest = {
        version: 'v1',
        metadata: { name: 'bidirectional-streaming' },
        nodes: [
          {
            id: 'input_processor',
            node_type: 'PassThrough',
            is_streaming: true,
            params: {},
          },
        ],
        connections: [],
      };

      const session = await native.createStreamSession!(JSON.stringify(manifest));

      // Concurrent send and receive
      const sendPromises: Promise<void>[] = [];
      const outputs: NapiRuntimeData[] = [];

      // Start receiving in background
      const receiveLoop = async () => {
        while (session.isActive) {
          const output = await session.recvOutput();
          if (output) {
            outputs.push(output);
          } else {
            break;
          }
        }
      };

      const receivePromise = receiveLoop();

      // Send multiple inputs
      for (let i = 0; i < 5; i++) {
        const data = native.NapiRuntimeData.text(`Message ${i}`);
        sendPromises.push(session.sendInput(data));
      }

      await Promise.all(sendPromises);
      await session.close();
      await receivePromise;

      console.log(`Bidirectional: sent 5, received ${outputs.length}`);
    });
  });

  describe('Simulated Streaming (using executePipeline)', () => {
    test('should process sequential audio chunks with same session', async () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      const manifest = {
        version: 'v1',
        metadata: { name: 'sequential-audio' },
        nodes: [
          {
            id: 'audio_node',
            node_type: 'PassThrough',
            params: {},
          },
        ],
        connections: [],
      };

      const sessionId = `stream_session_${Date.now()}`;
      const numChunks = 10;
      const samplesPerChunk = 480;
      const results: PipelineOutput[] = [];

      const startTime = Date.now();

      for (let i = 0; i < numChunks; i++) {
        const audioData = native.NapiRuntimeData.audio(
          createAudioSamplesBuffer(samplesPerChunk, 440),
          48000,
          1
        );

        const result = await native.executePipelineWithSession(
          JSON.stringify(manifest),
          { audio_node: audioData },
          sessionId
        );

        results.push(result);
      }

      const elapsedMs = Date.now() - startTime;
      const avgMs = elapsedMs / numChunks;

      console.log(`Sequential streaming simulation:`);
      console.log(`  Chunks: ${numChunks}`);
      console.log(`  Total time: ${elapsedMs}ms`);
      console.log(`  Avg per chunk: ${avgMs.toFixed(2)}ms`);

      expect(results.length).toBe(numChunks);
      results.forEach((result) => {
        expect(result.has('audio_node')).toBe(true);
      });

      // Should be reasonably fast for streaming
      expect(avgMs).toBeLessThan(100); // < 100ms per chunk
    });

    test('should maintain state across streaming chunks', async () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      const manifest = {
        version: 'v1',
        metadata: { name: 'stateful-streaming' },
        nodes: [
          {
            id: 'processor',
            node_type: 'PassThrough',
            params: {},
          },
        ],
        connections: [],
      };

      const sessionId = `stateful_${Date.now()}`;

      // Send multiple text chunks
      const messages = ['Hello', 'World', 'Streaming', 'Test'];

      for (const msg of messages) {
        const textData = native.NapiRuntimeData.text(msg);

        const result = await native.executePipelineWithSession(
          JSON.stringify(manifest),
          { processor: textData },
          sessionId
        );

        const output = result.get('processor');
        expect(output).not.toBeNull();
        expect(output!.getText()).toBe(msg);
      }
    });

    test('should handle high-frequency streaming simulation', async () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      const manifest = {
        version: 'v1',
        metadata: { name: 'high-freq-stream' },
        nodes: [
          {
            id: 'audio',
            node_type: 'PassThrough',
            params: {},
          },
        ],
        connections: [],
      };

      const sessionId = `highfreq_${Date.now()}`;
      const numChunks = 100;
      const samplesPerChunk = 480; // 10ms chunks

      const startTime = Date.now();

      const promises: Promise<PipelineOutput>[] = [];
      for (let i = 0; i < numChunks; i++) {
        const audioData = native.NapiRuntimeData.audio(
          createAudioSamplesBuffer(samplesPerChunk),
          48000,
          1
        );

        // Execute in parallel (simulating async streaming)
        promises.push(
          native.executePipelineWithSession(
            JSON.stringify(manifest),
            { audio: audioData },
            sessionId
          )
        );
      }

      const results = await Promise.all(promises);
      const elapsedMs = Date.now() - startTime;

      console.log(`High-frequency streaming simulation:`);
      console.log(`  Chunks: ${numChunks} (parallel)`);
      console.log(`  Total time: ${elapsedMs}ms`);
      console.log(`  Throughput: ${(numChunks / (elapsedMs / 1000)).toFixed(1)} chunks/sec`);

      expect(results.length).toBe(numChunks);
    });
  });

  describe('Zero-Copy Streaming Verification', () => {
    test('should verify zero-copy during streaming', async () => {
      if (!native?.isNativeLoaded()) {
        console.log('Skipping: native module not loaded');
        return;
      }

      // Create audio data
      const samplesBuffer = createAudioSamplesBuffer(100);
      const audioData = native.NapiRuntimeData.audio(samplesBuffer, 48000, 1);

      // Get buffer views
      const buffer1 = audioData.getAudioSamples();
      const buffer2 = audioData.getAudioSamples();

      const samples1 = new Float32Array(buffer1.buffer, buffer1.byteOffset, 100);
      const samples2 = new Float32Array(buffer2.buffer, buffer2.byteOffset, 100);

      // Mutate via buffer1
      const original = samples1[0];
      samples1[0] = 12345.0;

      // Check if buffer2 sees mutation
      const isZeroCopy = samples2[0] === 12345.0;

      console.log(`Zero-copy in streaming: ${isZeroCopy ? 'YES ✓' : 'NO'}`);

      // Restore
      samples1[0] = original;

      expect(isZeroCopy).toBe(true);
    });
  });
});
