# RemoteMedia Node.js Client

Official Node.js/TypeScript client for the RemoteMedia Processing SDK. Execute remote nodes and complete pipelines with a simple, intuitive API.

## Features

- 🚀 **Simple API** - Execute remote nodes and pipelines with just a few lines of code
- 📊 **Pipeline Management** - Discover, register, execute, and stream data through pipelines
- 🔄 **Streaming Support** - Real-time bidirectional streaming for compatible nodes and pipelines
- 🎯 **Type Safety** - Full TypeScript support with comprehensive type definitions
- 🔁 **Automatic Retry** - Built-in retry logic with exponential backoff
- 🐍 **Python-like API** - Familiar patterns for Python developers
- 📦 **Zero Configuration** - Works out of the box with sensible defaults
- 🔧 **Pipeline Builder** - Create pipeline definitions directly in JavaScript

## Installation

```bash
npm install @remotemedia/nodejs-client
```

Or with yarn:

```bash
yarn add @remotemedia/nodejs-client
```

## Quick Start

### Execute Individual Nodes

```typescript
import { RemoteProxyClient } from '@remotemedia/nodejs-client';

// Connect to the server
const client = new RemoteProxyClient({
  host: 'localhost',
  port: 50052
});

// Create a node proxy
const sentimentAnalyzer = await client.createNodeProxy(
  'TransformersPipelineNode',
  {
    task: 'sentiment-analysis',
    model: 'distilbert-base-uncased-finetuned-sst-2-english'
  }
);

// Process data
const result = await sentimentAnalyzer.process("I love this library!");
console.log(result);
// Output: [{ label: 'POSITIVE', score: 0.9998 }]
```

### Execute Registered Pipelines

```typescript
import { PipelineClient } from '@remotemedia/nodejs-client';

// Connect to the pipeline service
const client = new PipelineClient({
  host: 'localhost',
  port: 50052
});

// List available pipelines
const pipelines = await client.listPipelines();
console.log('Available:', pipelines.map(p => p.name));

// Execute a pipeline
const result = await client.executePipeline('webrtc_pipeline', {
  audio_data: audioBuffer,
  sample_rate: 16000
});
console.log('Processed:', result);
```

## TypeScript Type Generation

Generate up-to-date TypeScript definitions for all registered server nodes:

### Quick Generation

```bash
# Using npm script (recommended)
npm run generate-types

# Or using the shell script
./generate-types.sh
```

### What Gets Generated

The type generator creates TypeScript definitions for:
- All registered node types with proper parameter interfaces
- Type-safe client interfaces with generics
- Complete documentation from Python docstrings

Example generated interface:
```typescript
export interface AudioTransformConfig {
  /** The target sample rate for the audio. (default: 44100) */
  output_sample_rate?: number;
  /** The target number of channels for the audio. (default: 2) */
  output_channels?: number;
}
```

### Using Generated Types

```typescript
import { NodeType, AudioTransformConfig } from './generated-types';

// Type-safe node configuration
const config: AudioTransformConfig = {
  output_sample_rate: 16000,
  output_channels: 1
};

// The client now provides full type checking
const result = await client.executeNode(NodeType.AudioTransform, config, audioData);
```

### Configuration

Customize generation with environment variables:
```bash
GRPC_HOST=localhost GRPC_PORT=50052 OUTPUT_DIR=./my-types npm run generate-types
```

## Usage Patterns

### Pipeline Management

#### Create and Register Pipelines from JavaScript

```typescript
import { PipelineClient, PipelineBuilder } from '@remotemedia/nodejs-client';

const client = new PipelineClient({ host: 'localhost', port: 50052 });

// Build a pipeline in JavaScript
const builder = new PipelineBuilder('audio_processing');
builder
  .addNode('DataSourceNode', { 
    buffer_size: 100,
    name: 'audio_input'
  })
  .addNode('AudioTransform', {
    output_sample_rate: 16000,
    output_channels: 1
  })
  .addNode('TransformersPipelineNode', {
    task: 'automatic-speech-recognition',
    model: 'openai/whisper-base'
  })
  .addNode('DataSinkNode', {
    result_key: 'transcription'
  })
  .connect(0, 1)  // source -> audio transform
  .connect(1, 2)  // audio transform -> ASR
  .connect(2, 3); // ASR -> sink

// Register the pipeline on the server
const pipelineId = await client.registerPipeline(
  'audio_processing',
  builder.build(),
  {
    metadata: {
      description: 'Audio transcription pipeline',
      author: 'nodejs-client',
      version: '1.0.0'
    }
  }
);

// Execute the registered pipeline
const result = await client.executePipeline(pipelineId, audioData);
console.log('Transcription:', result.transcription);
```

#### Stream Data Through Pipelines

```typescript
import { PipelineClient } from '@remotemedia/nodejs-client';

const client = new PipelineClient({ host: 'localhost', port: 50052 });

// Create a streaming pipeline connection
const stream = client.streamPipeline('realtime_processing_pipeline', {
  bidirectional: true,
  bufferSize: 10
});

// Handle incoming data
stream.on('data', (chunk) => {
  console.log('Received processed chunk:', chunk);
  
  // Send more data based on results
  if (chunk.needsMoreData) {
    stream.send({ additionalData: getMoreData() });
  }
});

stream.on('error', (error) => {
  console.error('Stream error:', error);
});

stream.on('end', () => {
  console.log('Stream completed');
});

// Send initial data
await stream.send({ startData: initialData });

// Send more data as needed
for (const chunk of dataChunks) {
  await stream.send(chunk);
}

// Close the stream when done
await stream.end();
```

#### Pipeline Discovery and Metrics

```typescript
// Discover pipelines with filtering
const pipelines = await client.listPipelines({
  filter: {
    tags: ['audio', 'realtime'],
    namePattern: 'webrtc_*'
  }
});

// Get detailed pipeline information
const info = await client.getPipelineInfo('webrtc_pipeline');
console.log('Pipeline nodes:', info.definition.nodes);
console.log('Dependencies:', info.definition.dependencies);

// Session management
const sessions = await client.getActiveSessions('webrtc_pipeline');
console.log(`Active sessions: ${sessions.length}`);
```

### Python-style Context Manager

Use the `withRemoteProxy` helper for automatic connection management:

```typescript
import { withRemoteProxy } from '@remotemedia/nodejs-client';

await withRemoteProxy({ host: 'localhost', port: 50052 }, async (client) => {
  const node = await client.createNodeProxy('CalculatorNode');
  const result = await node.process({
    operation: 'multiply',
    args: [21, 2]
  });
  console.log(result); // { result: 42 }
});
```

### Using Helper Classes

The `RemoteNodes` class provides convenient methods for common node types:

```typescript
import { withRemoteProxy, RemoteNodes } from '@remotemedia/nodejs-client';

await withRemoteProxy({ host: 'localhost', port: 50052 }, async (client) => {
  const nodes = new RemoteNodes(client);
  
  // Text generation
  const textGen = await nodes.transformersPipeline({
    task: 'text-generation',
    model: 'gpt2',
    model_kwargs: {
      max_length: 50,
      temperature: 0.8
    }
  });
  
  const generated = await textGen.process("The future of AI is");
  console.log(generated);
});
```

### Pipeline Processing

Chain multiple nodes together:

```typescript
import { withRemoteProxy, NodePipeline } from '@remotemedia/nodejs-client';

await withRemoteProxy({ host: 'localhost', port: 50052 }, async (client) => {
  // Create nodes
  const textProcessor = await client.createNodeProxy('TextProcessorNode');
  const sentimentAnalyzer = await client.createNodeProxy(
    'TransformersPipelineNode',
    { task: 'sentiment-analysis' }
  );
  
  // Build pipeline
  const pipeline = new NodePipeline()
    .add(textProcessor)
    .add(sentimentAnalyzer);
  
  // Process through pipeline
  const result = await pipeline.process({
    text: "this is amazing",
    operations: ["uppercase"]
  });
});
```

### Batch Processing

Process multiple items efficiently:

```typescript
import { batchProcess } from '@remotemedia/nodejs-client';

const items = [
  "I love this!",
  "This is terrible.",
  "It's okay."
];

const results = await batchProcess(sentimentAnalyzer, items, {
  batchSize: 10,
  parallel: true,
  onProgress: (completed, total) => {
    console.log(`Progress: ${completed}/${total}`);
  }
});
```

### Streaming

For nodes that support streaming:

```typescript
const audioProcessor = await client.createNodeProxy('AudioTransform');

const stream = audioProcessor.processStream(
  (data) => {
    console.log('Received:', data);
  },
  (error) => {
    console.error('Stream error:', error);
  }
);

// Send data
await stream.send({ samples: audioData });
await stream.close();
```

### Error Handling and Retry

Use the built-in retry helper:

```typescript
import { retryOperation } from '@remotemedia/nodejs-client';

const result = await retryOperation(
  async () => {
    const node = await client.createNodeProxy('TransformersPipelineNode', config);
    return await node.process(data);
  },
  {
    maxAttempts: 3,
    initialDelay: 1000,
    shouldRetry: (error) => error.code === 'UNAVAILABLE'
  }
);
```

## Available Node Types

### NLP/ML Nodes

- **TransformersPipelineNode** - Hugging Face transformers pipelines
  - Tasks: sentiment-analysis, text-generation, question-answering, etc.
  - Supports model_kwargs for fine-tuning generation

### Audio Processing

- **AudioTransform** - Resample and convert audio formats
- **AudioBuffer** - Buffer audio data for batch processing

### Text Processing

- **TextProcessorNode** - Basic text operations (uppercase, word count, etc.)

### I/O Nodes (For JavaScript Integration)

- **DataSourceNode** - Inject data from JavaScript into pipelines
- **DataSinkNode** - Extract results from pipelines to JavaScript
- **JavaScriptBridgeNode** - Bidirectional communication with JavaScript
- **BidirectionalNode** - Full-duplex streaming between JavaScript and Python

### Utility Nodes

- **CalculatorNode** - Mathematical operations
- **FormatConverter** - Convert between data formats
- **PassThroughNode** - Pass data unchanged (for testing/debugging)

### Advanced Nodes

- **CodeExecutorNode** - Execute Python code (⚠️ Security risk!)
- **SerializedClassExecutorNode** - Execute serialized Python objects

## Configuration Options

```typescript
const client = new RemoteProxyClient({
  host: 'localhost',
  port: 50052,
  
  // Optional settings
  timeout: 30,                    // Request timeout in seconds
  sslEnabled: false,              // Enable SSL/TLS
  maxMessageSize: 4 * 1024 * 1024, // Max message size (4MB)
  
  // Retry configuration
  retry: {
    maxAttempts: 3,
    initialBackoff: 1000,
    maxBackoff: 5000,
    backoffMultiplier: 1.5
  }
});
```

## Server Discovery

List available nodes and check server status:

```typescript
// List all available nodes
const nodes = await client.listNodes();
nodes.forEach(node => {
  console.log(`${node.node_type}: ${node.description}`);
});

// Get server status
const status = await client.getStatus();
console.log(`Server version: ${status.version}`);
console.log(`Uptime: ${status.uptime_seconds}s`);
```

## TypeScript Support

Full TypeScript support with comprehensive type definitions:

```typescript
import { 
  RemoteProxyClient,
  RemoteExecutorConfig,
  NodeConfig,
  ExecutionOptions,
  RemoteNodeProxy,
  ServerStatus
} from '@remotemedia/nodejs-client';
```

## Examples

See the [examples](./examples) directory for complete working examples:

- `basic-usage.ts` - Simple getting started example
- `sentiment-analysis.ts` - NLP sentiment analysis
- `pipeline-processing.ts` - Multi-step pipeline processing
- `streaming-audio.ts` - Real-time audio streaming
- `batch-processing.ts` - Efficient batch operations
- `test-pipeline.js` - Complete pipeline registration and execution
- `test-full-pipeline.js` - End-to-end pipeline integration test
- `calculator-pipeline.js` - Simple calculator pipeline example
- `discover-webrtc-pipeline.js` - **NEW:** Discover and use WebRTC speech-to-speech pipeline

## Error Handling

The client provides detailed error information:

```typescript
try {
  const result = await node.process(data);
} catch (error) {
  if (error.code === 'DEADLINE_EXCEEDED') {
    console.error('Request timed out');
  } else if (error.code === 'UNAVAILABLE') {
    console.error('Server is unavailable');
  } else {
    console.error('Error:', error.message);
  }
}
```

## Development

```bash
# Install dependencies
npm install

# Build the project
npm run build

# Run tests
npm test

# Run examples
npm run example:sentiment
```

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) for details.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Support

- 📚 [Documentation](https://docs.remotemedia.io)
- 💬 [Discord Community](https://discord.gg/remotemedia)
- 🐛 [Issue Tracker](https://github.com/remotemedia/nodejs-client/issues)
- 📧 [Email Support](mailto:support@remotemedia.io)