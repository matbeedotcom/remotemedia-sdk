# Generic Streaming Protocol - Quickstart Guide

**Feature**: `004-generic-streaming` | **Status**: Draft
**Last Updated**: 2025-01-15

## Overview

The Generic Streaming Protocol extends RemoteMedia SDK's streaming capabilities from audio-only to support **any protocol bufferable data type**: audio, video, tensors, JSON, text, and binary data. This enables real-time processing pipelines for computer vision, machine learning, structured data processing, and mixed-type workflows.

### What's New

- **Universal Data Types**: Stream video frames, tensors, JSON, text, or binary data with the same API used for audio
- **Mixed-Type Pipelines**: Chain nodes that process different data types (e.g., audio → JSON metadata → filtered audio)
- **Multi-Input Nodes**: Nodes can accept multiple input types simultaneously (e.g., audio + JSON control parameters)
- **Type Safety**: TypeScript and Python type checking prevents type mismatches at compile time
- **Backward Compatible**: Existing audio code continues to work without modifications
- **Zero-Copy Audio**: Maintains <5% overhead vs audio-only protocol for audio pipelines

### Key Capabilities

| Data Type | Use Cases | Example Nodes |
|-----------|-----------|---------------|
| **Audio** | Speech processing, music analysis, audio effects | VAD, Resample, FormatConverter |
| **Video** | Object detection, face recognition, video analysis | FrameDetector, VideoClassifier |
| **Tensor** | ML embeddings, image tensors, model I/O | EmbeddingProcessor, TensorTransform |
| **JSON** | Metadata, control flow, structured results | Calculator, ThresholdController |
| **Text** | NLP, tokenization, text processing | Tokenizer, SentimentAnalyzer |
| **Binary** | Custom formats, compressed data, protocol buffers | CustomParser, Decompressor |

---

## Quick Start Examples

### 1. Video Streaming (TypeScript)

Stream video frames through an object detection pipeline and receive JSON results.

**Independent Test Criteria** (User Story 1):
✅ Pipeline processes 10 video frames
✅ Each frame returns JSON with bounding boxes and confidence scores
✅ System handles video identically to audio chunks

```typescript
import { RemoteMediaClient, PixelFormat } from '@remotemedia/nodejs-client';

async function detectObjectsInVideo() {
  const client = new RemoteMediaClient('localhost:50051');
  await client.connect();

  // Define object detection pipeline
  const manifest = {
    version: 'v1',
    metadata: {
      name: 'video_detection',
      description: 'Real-time object detection on video frames'
    },
    nodes: [
      {
        id: 'detector',
        nodeType: 'VideoObjectDetectorNode',
        params: JSON.stringify({
          model: 'yolo-v8',
          confidence_threshold: 0.5
        }),
        isStreaming: true,
        inputTypes: ['DATA_TYPE_HINT_VIDEO'],
        outputTypes: ['DATA_TYPE_HINT_JSON']
      }
    ],
    connections: []
  };

  // Create video frame generator
  async function* generateVideoFrames() {
    for (let i = 0; i < 10; i++) {
      // Create 640x480 RGB24 frame (dummy data for demo)
      const width = 640;
      const height = 480;
      const pixelData = Buffer.alloc(width * height * 3);

      // Fill with test pattern (would be real video data in production)
      for (let p = 0; p < pixelData.length; p += 3) {
        pixelData[p] = (i * 25) % 255;     // R
        pixelData[p + 1] = (i * 50) % 255; // G
        pixelData[p + 2] = (i * 75) % 255; // B
      }

      yield {
        nodeId: 'detector',
        buffer: {
          video: {
            pixelData,
            width,
            height,
            format: PixelFormat.RGB24,
            frameNumber: i,
            timestampUs: i * 33333 // 30 FPS
          }
        },
        sequence: i,
        timestampMs: i * 33
      };
    }
  }

  // Stream video frames and process results
  console.log('Streaming 10 video frames for object detection...');
  let frameCount = 0;

  for await (const result of client.streamPipeline(manifest, generateVideoFrames())) {
    if (result.dataOutputs?.detector) {
      const detections = JSON.parse(result.dataOutputs.detector);
      console.log(`Frame ${frameCount}:`, detections);
      // Expected output:
      // {
      //   "detections": [
      //     {"class": "person", "confidence": 0.87, "bbox": [120, 80, 240, 320]},
      //     {"class": "car", "confidence": 0.92, "bbox": [300, 200, 450, 380]}
      //   ]
      // }
      frameCount++;
    }
  }

  console.log(`✅ Processed ${frameCount} frames`);
  await client.disconnect();
}
```

**Expected Output**:
```
Streaming 10 video frames for object detection...
Frame 0: { detections: [...], processing_time_ms: 12.3 }
Frame 1: { detections: [...], processing_time_ms: 11.8 }
...
✅ Processed 10 frames
```

---

### 2. JSON Calculator Pipeline (TypeScript)

Process JSON calculation requests without using audio APIs.

**Independent Test Criteria** (User Story 1):
✅ Stream 5 calculation requests as JSON
✅ Receive computed results as JSON
✅ Average latency <1ms per operation

```typescript
import { RemoteMediaClient } from '@remotemedia/nodejs-client';

async function jsonCalculatorPipeline() {
  const client = new RemoteMediaClient('localhost:50051');
  await client.connect();

  const manifest = {
    version: 'v1',
    metadata: {
      name: 'json_calculator',
      description: 'JSON-based calculation pipeline'
    },
    nodes: [
      {
        id: 'calculator',
        nodeType: 'CalculatorNode',
        params: JSON.stringify({ precision: 4 }),
        isStreaming: true,
        inputTypes: ['DATA_TYPE_HINT_JSON'],
        outputTypes: ['DATA_TYPE_HINT_JSON']
      }
    ],
    connections: []
  };

  // Generator for calculation requests
  async function* generateCalculations() {
    const operations = [
      { operation: 'add', operands: [10, 20] },
      { operation: 'multiply', operands: [5, 7] },
      { operation: 'divide', operands: [100, 4] },
      { operation: 'power', operands: [2, 8] },
      { operation: 'sqrt', operands: [144] }
    ];

    for (let i = 0; i < operations.length; i++) {
      yield {
        nodeId: 'calculator',
        buffer: {
          json: {
            jsonPayload: JSON.stringify(operations[i]),
            schemaType: 'CalculatorRequest'
          }
        },
        sequence: i,
        timestampMs: Date.now()
      };
    }
  }

  console.log('Streaming JSON calculation requests...');
  const latencies: number[] = [];

  for await (const result of client.streamPipeline(manifest, generateCalculations())) {
    if (result.dataOutputs?.calculator) {
      const calcResult = JSON.parse(result.dataOutputs.calculator);
      console.log(`Result: ${JSON.stringify(calcResult)}`);
      latencies.push(result.processingTimeMs);
    }
  }

  const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;
  console.log(`✅ Average latency: ${avgLatency.toFixed(2)}ms`);
  await client.disconnect();
}
```

**Expected Output**:
```
Streaming JSON calculation requests...
Result: { result: 30, operation: "add" }
Result: { result: 35, operation: "multiply" }
Result: { result: 25, operation: "divide" }
Result: { result: 256, operation: "power" }
Result: { result: 12, operation: "sqrt" }
✅ Average latency: 0.87ms
```

---

### 3. Mixed-Type Pipeline: Audio → JSON → Audio (TypeScript)

Chain audio processing with JSON metadata generation and conditional filtering.

**Independent Test Criteria** (User Story 2):
✅ VAD node receives audio, outputs JSON confidence scores
✅ Calculator node receives JSON, outputs JSON threshold decision
✅ Filter node receives audio + JSON control, outputs filtered audio
✅ Each node receives correct data type

```typescript
import { RemoteMediaClient, AudioFormat } from '@remotemedia/nodejs-client';

async function mixedTypePipeline() {
  const client = new RemoteMediaClient('localhost:50051');
  await client.connect();

  const SAMPLE_RATE = 16000;
  const CHUNK_SIZE = 480; // 30ms at 16kHz

  const manifest = {
    version: 'v1',
    metadata: {
      name: 'mixed_audio_json_pipeline',
      description: 'Audio → JSON → Audio with conditional filtering'
    },
    nodes: [
      {
        id: 'vad',
        nodeType: 'RustVADNode',
        params: JSON.stringify({
          sample_rate: SAMPLE_RATE,
          frame_duration_ms: 30,
          energy_threshold: 0.01
        }),
        isStreaming: true,
        inputTypes: ['DATA_TYPE_HINT_AUDIO'],
        outputTypes: ['DATA_TYPE_HINT_JSON']
      },
      {
        id: 'threshold_calculator',
        nodeType: 'CalculatorNode',
        params: JSON.stringify({
          operation: 'compare_threshold',
          threshold: 0.8
        }),
        isStreaming: true,
        inputTypes: ['DATA_TYPE_HINT_JSON'],
        outputTypes: ['DATA_TYPE_HINT_JSON']
      },
      {
        id: 'dynamic_filter',
        nodeType: 'DynamicAudioFilterNode',
        params: JSON.stringify({
          sample_rate: SAMPLE_RATE
        }),
        isStreaming: true,
        inputTypes: ['DATA_TYPE_HINT_AUDIO', 'DATA_TYPE_HINT_JSON'],
        outputTypes: ['DATA_TYPE_HINT_AUDIO']
      }
    ],
    connections: [
      { from: 'vad', to: 'threshold_calculator' }
      // Note: dynamic_filter receives audio directly (not connected in DAG)
      // and JSON control from threshold_calculator
    ]
  };

  // Generate audio chunks with speech pattern
  async function* generateAudioWithControl() {
    for (let chunkIdx = 0; chunkIdx < 20; chunkIdx++) {
      const samples = new Float32Array(CHUNK_SIZE);
      const t_base = chunkIdx * CHUNK_SIZE / SAMPLE_RATE;

      // Simulate speech: alternating loud/quiet
      const amplitude = (chunkIdx % 4 < 2) ? 0.5 : 0.05;

      for (let i = 0; i < CHUNK_SIZE; i++) {
        const t = t_base + i / SAMPLE_RATE;
        samples[i] = amplitude * Math.sin(2 * Math.PI * 440 * t);
      }

      // Send audio to both VAD and filter using named_buffers
      const audioBuffer = {
        samples: Buffer.from(samples.buffer),
        sampleRate: SAMPLE_RATE,
        channels: 1,
        format: AudioFormat.F32,
        numSamples: CHUNK_SIZE
      };

      // For multi-input nodes, use named_buffers
      yield {
        nodeId: 'vad',
        buffer: { audio: audioBuffer },
        sequence: chunkIdx * 2,
        timestampMs: chunkIdx * 30
      };

      // Note: In a real implementation, you'd receive VAD → calculator output
      // and send it along with audio to dynamic_filter
      // This example shows the structure
    }
  }

  console.log('Streaming mixed-type pipeline...');
  let audioChunks = 0;
  let jsonResults = 0;

  for await (const result of client.streamPipeline(manifest, generateAudioWithControl())) {
    if (result.dataOutputs?.threshold_calculator) {
      const control = JSON.parse(result.dataOutputs.threshold_calculator);
      console.log(`JSON control: ${JSON.stringify(control)}`);
      jsonResults++;
    }

    if (result.dataOutputs?.dynamic_filter?.audio) {
      console.log(`Filtered audio chunk ${audioChunks}: ${result.dataOutputs.dynamic_filter.audio.numSamples} samples`);
      audioChunks++;
    }
  }

  console.log(`✅ Processed ${audioChunks} audio chunks, ${jsonResults} JSON results`);
  await client.disconnect();
}
```

**Expected Output**:
```
Streaming mixed-type pipeline...
JSON control: { threshold_exceeded: true, confidence: 0.87, gain: 1.0 }
Filtered audio chunk 0: 480 samples
JSON control: { threshold_exceeded: false, confidence: 0.23, gain: 0.1 }
Filtered audio chunk 1: 480 samples
...
✅ Processed 20 audio chunks, 20 JSON results
```

---

### 4. Tensor/Embedding Streaming (Python)

Stream tensor embeddings through a similarity processor.

**Independent Test Criteria** (User Story 1):
✅ Stream 10 embedding tensors (512-dim F32)
✅ Receive similarity scores as JSON
✅ System validates tensor shape and dtype

```python
import asyncio
import numpy as np
from remotemedia.client import RemoteMediaClient
from remotemedia.proto.common_pb2 import TensorBuffer, TensorDtype, DataBuffer

async def tensor_streaming_pipeline():
    client = RemoteMediaClient('localhost:50051')
    await client.connect()

    manifest = {
        'version': 'v1',
        'metadata': {
            'name': 'embedding_similarity',
            'description': 'Process embeddings and compute similarities'
        },
        'nodes': [
            {
                'id': 'similarity',
                'node_type': 'EmbeddingSimilarityNode',
                'params': '{"metric": "cosine", "threshold": 0.75}',
                'is_streaming': True,
                'input_types': ['DATA_TYPE_HINT_TENSOR'],
                'output_types': ['DATA_TYPE_HINT_JSON']
            }
        ],
        'connections': []
    }

    async def generate_embeddings():
        """Generate 10 random 512-dimensional embeddings"""
        embedding_dim = 512

        for i in range(10):
            # Generate normalized random embedding
            embedding = np.random.randn(embedding_dim).astype(np.float32)
            embedding = embedding / np.linalg.norm(embedding)

            # Create tensor buffer
            tensor = TensorBuffer(
                data=embedding.tobytes(),
                shape=[embedding_dim],
                dtype=TensorDtype.TENSOR_DTYPE_F32,
                layout=""  # Not needed for 1D
            )

            yield {
                'node_id': 'similarity',
                'buffer': DataBuffer(tensor=tensor),
                'sequence': i,
                'timestamp_ms': i * 100
            }

    print('Streaming 10 embedding tensors...')
    similarities = []

    async for result in client.stream_pipeline(manifest, generate_embeddings()):
        if result.data_outputs and 'similarity' in result.data_outputs:
            sim_result = result.data_outputs['similarity'].json.json_payload
            print(f'Embedding {result.sequence}: {sim_result}')
            # Expected: {"similarity_score": 0.87, "is_similar": true}
            similarities.append(sim_result)

    print(f'✅ Processed {len(similarities)} embeddings')
    await client.disconnect()

if __name__ == '__main__':
    asyncio.run(tensor_streaming_pipeline())
```

**Expected Output**:
```
Streaming 10 embedding tensors...
Embedding 0: {"similarity_score": 0.92, "is_similar": true}
Embedding 1: {"similarity_score": 0.68, "is_similar": false}
Embedding 2: {"similarity_score": 0.81, "is_similar": true}
...
✅ Processed 10 embeddings
```

---

## Migration Guide: Audio-Only → Generic APIs

Migrating from audio-only APIs to generic streaming requires **<20 lines of code changes** for typical streaming clients (Success Criteria SC-006).

### Before: Audio-Only API (Feature 003)

```typescript
// OLD: Audio-specific chunk
const audioChunk = {
  nodeId: 'vad',
  buffer: audioBuffer,  // Directly pass AudioBuffer
  sequence: 0,
  timestampMs: 0
};

// OLD: Audio-specific streaming
const result = await client.streamAudioPipeline(manifest, audioGenerator);

// OLD: Audio-specific outputs
const outputAudio = result.audioOutputs.resample;
```

### After: Generic API (Feature 004)

```typescript
// NEW: Generic chunk with audio variant
const dataChunk = {
  nodeId: 'vad',
  buffer: {
    audio: audioBuffer  // Wrap in DataBuffer with audio variant
  },
  sequence: 0,
  timestampMs: 0
};

// NEW: Generic streaming (works for all types)
const result = await client.streamPipeline(manifest, dataGenerator);

// NEW: Generic outputs (audio still available in data_outputs)
const outputAudio = result.dataOutputs.resample.audio;
```

### Migration Steps

**Step 1**: Update chunk creation (3 lines)

```diff
- const chunk = { nodeId, buffer: audioBuffer, sequence, timestampMs };
+ const chunk = {
+   nodeId,
+   buffer: { audio: audioBuffer },
+   sequence,
+   timestampMs
+ };
```

**Step 2**: Update streaming call (1 line)

```diff
- const results = client.streamAudioPipeline(manifest, generator);
+ const results = client.streamPipeline(manifest, generator);
```

**Step 3**: Update output access (2 lines)

```diff
- const audio = result.audioOutputs.nodeId;
+ const audio = result.dataOutputs.nodeId.audio;
```

**Total**: **6 lines changed** (well under 20-line requirement)

### Backward Compatibility Option

For legacy code, the old `streamAudioPipeline()` helper still works:

```typescript
// This continues to work (Feature 003 API)
const result = await client.streamAudioPipeline(manifest, audioGenerator);

// Server automatically converts AudioChunk → DataChunk internally
// ⚠️ DEPRECATED: Will be removed after 6 months
```

**Deprecation Timeline**:
- **Months 0-3**: Both APIs supported, deprecation warnings in logs
- **Months 3-6**: Migration reminders in documentation
- **Month 6+**: Legacy `AudioChunk` API removed

---

## Testing Scenarios

Each user story has independent test criteria that can be verified separately.

### Test 1: Stream Non-Audio Data Types

**Test Criteria** (User Story 1):
- Create manifest with video detection node
- Stream 10 video frames with detection parameters
- Verify JSON results contain bounding boxes and confidence scores
- System handles video frames identically to audio chunks

**Test Command**:
```bash
npm test -- --testNamePattern="video streaming"
```

**Expected Result**:
```
✅ Video Detection Test
   - 10 frames streamed
   - All frames returned JSON results
   - Average latency: 15.3ms
   - Bounding boxes detected: 23 total
```

### Test 2: Mixed-Type Pipeline Chains

**Test Criteria** (User Story 2):
- Create pipeline: RustVADNode (audio→JSON) → CalculatorNode (JSON→JSON) → DynamicAudioFilter (audio+JSON→audio)
- Stream audio chunks
- Verify VAD generates JSON confidence scores
- Verify calculator processes JSON
- Verify filter applies JSON-controlled gain

**Test Command**:
```bash
npm test -- --testNamePattern="mixed-type pipeline"
```

**Expected Result**:
```
✅ Mixed-Type Pipeline Test
   - VAD: 50 audio chunks → 50 JSON confidence scores
   - Calculator: 50 JSON inputs → 50 JSON decisions
   - Filter: 50 audio chunks + 50 JSON controls → 50 filtered audio outputs
   - Type validation: All nodes received correct types
```

### Test 3: Type-Safe Client APIs

**Test Criteria** (User Story 3):
- Write TypeScript pipeline with type-safe builder
- Attempt to connect JSON output to audio-only input
- Compiler should reject at build time
- Valid connections (JSON→JSON, audio→audio) compile successfully

**Test Command**:
```bash
npm run typecheck
```

**Expected Result**:
```
src/test_type_safety.ts:42:5 - error TS2345: Argument of type
'{ nodeId: "audio_node", buffer: { json: JsonData } }' is not assignable
to parameter of type 'AudioChunk'.
  Type 'JsonData' is not assignable to type 'AudioBuffer'.

✅ Type checker correctly rejected invalid connection
```

### Test 4: Backward Compatibility

**Test Criteria** (User Story 4):
- Run existing `streaming_audio_pipeline.ts` example
- Use legacy `AudioChunk` message type
- All tests should pass without code changes
- Deprecation warnings appear in logs

**Test Command**:
```bash
npm run test:legacy
```

**Expected Result**:
```
⚠️  DEPRECATION WARNING: AudioChunk is deprecated. Use DataChunk instead.
    Migration guide: https://docs.remotemedia.io/migration

✅ Legacy Audio Pipeline Test
   - 100 audio chunks processed
   - All tests passed
   - Performance: <5% overhead vs native audio-only
```

### Test 5: Server-Side Type Validation

**Test Criteria** (User Story 5):
- Submit manifest declaring node expects `AudioBuffer`
- Stream `VideoFrame` chunk to that node
- Service rejects with `ERROR_TYPE_VALIDATION`
- Error message specifies expected vs actual type

**Test Command**:
```bash
npm test -- --testNamePattern="type validation"
```

**Expected Result**:
```
✅ Type Validation Test
   - Correct rejection: VideoFrame sent to audio-only node
   - Error type: ERROR_TYPE_TYPE_VALIDATION
   - Error message: "Node 'vad' expects audio input but received video"
   - Validation happened before processing (fast fail)
```

---

## Type Safety

### TypeScript Compile-Time Type Checking

The TypeScript client provides full type safety via discriminated unions:

```typescript
import { DataBuffer, DataChunk } from '@remotemedia/nodejs-client';

// Type-safe buffer creation
function createAudioChunk(nodeId: string, audio: AudioBuffer): DataChunk {
  return {
    nodeId,
    buffer: {
      audio  // TypeScript knows this is valid
    },
    sequence: 0,
    timestampMs: 0
  };
}

// Type guards for runtime checking
function isAudioBuffer(buf: DataBuffer): buf is { audio: AudioBuffer } {
  return 'audio' in buf && buf.audio !== undefined;
}

function isVideoBuffer(buf: DataBuffer): buf is { video: VideoFrame } {
  return 'video' in buf && buf.video !== undefined;
}

// Type narrowing in action
function processBuffer(buf: DataBuffer) {
  if (isAudioBuffer(buf)) {
    // TypeScript knows buf.audio exists and has type AudioBuffer
    console.log(`Audio: ${buf.audio.sampleRate} Hz`);
  } else if (isVideoBuffer(buf)) {
    // TypeScript knows buf.video exists and has type VideoFrame
    console.log(`Video: ${buf.video.width}x${buf.video.height}`);
  }
}
```

### Python Type Hints with mypy

The Python client supports static type checking:

```python
from remotemedia.proto import execution_pb2
from typing import Union

# Type hints for discriminated union
DataBufferVariant = Union[
    execution_pb2.AudioBuffer,
    execution_pb2.VideoFrame,
    execution_pb2.TensorBuffer,
    execution_pb2.JsonData,
]

def process_buffer(buf: execution_pb2.DataBuffer) -> None:
    """Process generic data buffer with type checking"""
    if buf.HasField("audio"):
        audio: execution_pb2.AudioBuffer = buf.audio
        print(f"Audio: {audio.sample_rate} Hz")
    elif buf.HasField("video"):
        video: execution_pb2.VideoFrame = buf.video
        print(f"Video: {video.width}x{video.height}")
    else:
        raise ValueError("Unknown buffer type")

# mypy will catch this error
def bad_example():
    buf = execution_pb2.DataBuffer()
    buf.audio.sample_rate = 16000

    # ERROR: Trying to access video when audio is set
    print(buf.video.width)  # mypy error: Field "video" may not be initialized
```

**Run Type Checking**:
```bash
# TypeScript
npm run typecheck

# Python
mypy --strict your_pipeline.py
```

---

## Common Patterns

### Pattern 1: Multi-Input Nodes

Nodes that accept multiple data types simultaneously use `named_buffers`:

```typescript
// Single-input node (simple case)
const simpleChunk: DataChunk = {
  nodeId: 'vad',
  buffer: { audio: audioBuffer },
  sequence: 0,
  timestampMs: 0
};

// Multi-input node (audio + JSON control)
const multiInputChunk: DataChunk = {
  nodeId: 'dynamic_filter',
  namedBuffers: {
    'audio': { audio: audioBuffer },       // Main audio stream
    'control': { json: jsonControlData }   // Control parameters
  },
  sequence: 0,
  timestampMs: 0
};
```

**Server-side processing**:
```rust
// Node receives named inputs
pub struct DynamicAudioFilter;

impl Node for DynamicAudioFilter {
    async fn process(&self, inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData> {
        let audio = inputs.get("audio").unwrap().as_audio()?;
        let control = inputs.get("control").unwrap().as_json()?;

        let gain = control["gain"].as_f64().unwrap_or(1.0);
        let filtered = apply_gain(audio, gain)?;

        Ok(RuntimeData::Audio(filtered))
    }
}
```

### Pattern 2: Type Validation in Manifests

Declare expected types for compile-time and runtime validation:

```typescript
const manifest = {
  nodes: [
    {
      id: 'vad',
      nodeType: 'RustVADNode',
      params: '{}',
      inputTypes: ['DATA_TYPE_HINT_AUDIO'],        // Expects audio only
      outputTypes: ['DATA_TYPE_HINT_JSON']         // Produces JSON only
    },
    {
      id: 'filter',
      nodeType: 'DynamicAudioFilter',
      params: '{}',
      inputTypes: [                                // Accepts both types
        'DATA_TYPE_HINT_AUDIO',
        'DATA_TYPE_HINT_JSON'
      ],
      outputTypes: ['DATA_TYPE_HINT_AUDIO']
    },
    {
      id: 'logger',
      nodeType: 'GenericLogger',
      params: '{}',
      inputTypes: ['DATA_TYPE_HINT_ANY'],          // Accepts any type
      outputTypes: ['DATA_TYPE_HINT_JSON']
    }
  ],
  connections: [
    { from: 'vad', to: 'filter' }  // ✅ Valid: JSON → filter (accepts JSON)
    // { from: 'vad', to: 'audio_only' }  // ❌ Invalid: JSON → audio-only node
  ]
};
```

### Pattern 3: Error Handling

Handle type validation errors gracefully:

```typescript
try {
  const result = await client.streamPipeline(manifest, dataGenerator);
} catch (error) {
  if (error.errorType === 'ERROR_TYPE_TYPE_VALIDATION') {
    console.error('Type mismatch detected:');
    console.error(`  Expected: ${error.context.expected_type}`);
    console.error(`  Actual: ${error.context.actual_type}`);
    console.error(`  Node: ${error.failingNodeId}`);

    // Fix: Update manifest or change data type
  } else if (error.errorType === 'ERROR_TYPE_NODE_EXECUTION') {
    console.error('Node execution failed:', error.message);
  }
}
```

---

## Performance

### Zero-Copy Audio Path

The generic protocol maintains **<5% overhead** vs audio-only protocol for audio pipelines (Success Criteria SC-008).

**Benchmark Results**:

| Pipeline | Audio-Only (v003) | Generic (v004) | Overhead |
|----------|-------------------|----------------|----------|
| Resample (16kHz → 48kHz) | 2.43ms | 2.51ms | +3.3% |
| VAD (30ms chunks) | 1.87ms | 1.91ms | +2.1% |
| Multi-stage (Resample → VAD) | 4.12ms | 4.31ms | +4.6% |

**How Zero-Copy Works**:

```rust
// Rust internal representation uses bytes::Bytes (zero-copy)
pub enum RuntimeData {
    Audio(AudioBuffer),  // Contains Bytes, not Vec<u8>
    Video(VideoFrame),   // Contains Bytes
    // ...
}

// Conversion reuses buffers
pub fn convert_proto_to_runtime_data(proto: DataBuffer) -> RuntimeData {
    match proto.data_type {
        Some(DataType::Audio(buf)) => {
            // buf.samples is already Bytes, no copy needed
            RuntimeData::Audio(buf)
        }
        // ...
    }
}
```

### JSON Parsing Performance

JSON-only pipelines maintain **<1ms latency** for simple operations (Success Criteria SC-002).

**Benchmark** (CalculatorNode):
```
Operation: add [10, 20]
  - Parsing: 0.12ms
  - Execution: 0.08ms
  - Serialization: 0.15ms
  - Total: 0.35ms ✅ (<1ms target)

Operation: complex nested JSON (5KB)
  - Parsing: 0.78ms
  - Execution: 0.22ms
  - Serialization: 0.91ms
  - Total: 1.91ms (still fast)
```

### Benchmarking Generic vs Audio-Only

Compare performance of the same audio pipeline:

```typescript
import { benchmark } from '@remotemedia/benchmark';

// Audio-only API (legacy)
const audioOnlyResult = await benchmark.run('audio_only', async () => {
  await client.streamAudioPipeline(manifest, audioGenerator);
});

// Generic API (Feature 004)
const genericResult = await benchmark.run('generic', async () => {
  await client.streamPipeline(manifest, dataGenerator);
});

console.log('Audio-Only:', audioOnlyResult.meanLatency, 'ms');
console.log('Generic:', genericResult.meanLatency, 'ms');
console.log('Overhead:',
  ((genericResult.meanLatency / audioOnlyResult.meanLatency - 1) * 100).toFixed(1), '%'
);

// Expected: <5% overhead
```

---

## Troubleshooting

### Common Type Mismatch Errors

#### Error 1: JSON sent to audio-only node

**Symptom**:
```
ERROR_TYPE_TYPE_VALIDATION: Node 'vad' expects audio input but received json
  Expected types: [AUDIO]
  Actual type: JSON
  Node: vad
```

**Solution**:
```diff
// Fix manifest: Update node to accept JSON or send correct type
nodes: [
  {
    id: 'vad',
    nodeType: 'RustVADNode',
-   inputTypes: ['DATA_TYPE_HINT_AUDIO'],
+   inputTypes: ['DATA_TYPE_HINT_AUDIO', 'DATA_TYPE_HINT_JSON'],
  }
]
```

#### Error 2: Missing required input for multi-input node

**Symptom**:
```
ERROR_TYPE_VALIDATION: Node 'dynamic_filter' requires inputs ['audio', 'control']
but only received ['audio']
```

**Solution**:
```diff
// Provide all required inputs using named_buffers
const chunk = {
  nodeId: 'dynamic_filter',
- buffer: { audio: audioBuffer },
+ namedBuffers: {
+   'audio': { audio: audioBuffer },
+   'control': { json: controlData }
+ },
  sequence: 0,
  timestampMs: 0
};
```

#### Error 3: Tensor size mismatch

**Symptom**:
```
ERROR_TYPE_VALIDATION: Tensor size mismatch
  Expected: 2048 bytes (512 elements * 4 bytes/element)
  Actual: 1024 bytes
  Shape: [512]
  Dtype: F32
```

**Solution**:
```diff
// Ensure tensor data matches shape and dtype
const embedding = np.random.randn(512).astype(np.float32)
+ assert embedding.nbytes == 512 * 4, "Size mismatch"

tensor = TensorBuffer(
  data=embedding.tobytes(),
  shape=[512],
  dtype=TensorDtype.TENSOR_DTYPE_F32
)
```

#### Error 4: Invalid JSON payload

**Symptom**:
```
ERROR_TYPE_VALIDATION: Invalid JSON at line 3, column 15
  Parse error: unexpected EOF while parsing a value
  Schema type: CalculatorRequest
```

**Solution**:
```diff
// Validate JSON before sending
const payload = { operation: 'add', operands: [10, 20] };
+ const validated = JSON.parse(JSON.stringify(payload));  // Validate

const jsonData = {
- jsonPayload: '{operation: "add"}',  // Invalid JSON (missing quotes)
+ jsonPayload: JSON.stringify(validated),
  schemaType: 'CalculatorRequest'
};
```

#### Error 5: Video frame dimension mismatch

**Symptom**:
```
ERROR_TYPE_VALIDATION: Video frame size mismatch
  Expected: 921600 bytes (640 * 480 * 3 bytes/pixel for RGB24)
  Actual: 307200 bytes
  Dimensions: 640x480
  Format: RGB24
```

**Solution**:
```diff
// Ensure pixel_data matches dimensions and format
const width = 640;
const height = 480;
- const pixelData = Buffer.alloc(width * height);  // Wrong: missing *3
+ const pixelData = Buffer.alloc(width * height * 3);  // RGB24 = 3 bytes/pixel

const frame = {
  pixelData,
  width,
  height,
  format: PixelFormat.RGB24
};
```

### Debug Tips

1. **Enable verbose logging**:
```typescript
const client = new RemoteMediaClient('localhost:50051', {
  logLevel: 'debug',
  logTypeValidation: true
});
```

2. **Inspect data types at runtime**:
```typescript
function inspectDataBuffer(buf: DataBuffer) {
  if (buf.audio) console.log('Type: Audio', buf.audio);
  else if (buf.video) console.log('Type: Video', buf.video);
  else if (buf.json) console.log('Type: JSON', buf.json);
  else if (buf.tensor) console.log('Type: Tensor', buf.tensor);
  else console.warn('Unknown type');
}
```

3. **Validate manifests before streaming**:
```typescript
const validationResult = await client.validateManifest(manifest);
if (!validationResult.valid) {
  console.error('Manifest errors:', validationResult.errors);
  // Fix errors before streaming
}
```

---

## Next Steps

### For New Projects

Start with generic APIs from day one:

1. **Define your pipeline**: Choose data types (audio, video, tensors, JSON)
2. **Create manifest**: Declare `input_types` and `output_types` for each node
3. **Implement generator**: Yield `DataChunk` with appropriate `DataBuffer` variants
4. **Test streaming**: Run with `client.streamPipeline(manifest, generator)`
5. **Add type checking**: Enable TypeScript strict mode or mypy for Python

### For Existing Audio Projects

Migrate gradually over 6 months:

1. **Month 1-2**: Continue using `streamAudioPipeline()` (deprecated but works)
2. **Month 3-4**: Update to `streamPipeline()` with audio variant (6 lines)
3. **Month 5-6**: Add type declarations to manifests for validation
4. **Month 6+**: Enjoy full generic streaming capabilities

### Resources

- **Full API Documentation**: `specs/004-generic-streaming/data-model.md`
- **Protocol Buffer Definitions**: `specs/004-generic-streaming/contracts/*.proto`
- **Design Rationale**: `specs/004-generic-streaming/research.md`
- **Example Projects**: `examples/grpc_examples/typescript/` and `examples/grpc_examples/python/`

---

## Summary

The Generic Streaming Protocol enables you to:

✅ Stream **any data type** (audio, video, tensors, JSON, text, binary)
✅ Build **mixed-type pipelines** (audio → JSON → audio)
✅ Use **multi-input nodes** (audio + JSON control)
✅ Get **compile-time type safety** (TypeScript/Python)
✅ Maintain **backward compatibility** (existing audio code works)
✅ Achieve **zero-copy performance** (<5% overhead for audio)

**Migration is simple**: <20 lines of code changes for typical clients.

**Get started now**: Copy examples from this guide and adapt to your use case!
