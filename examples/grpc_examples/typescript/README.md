# TypeScript gRPC Client Examples

Working TypeScript examples demonstrating the Rust gRPC service.

## Prerequisites

- Node.js >= 14.0.0
- RemoteMedia gRPC service running on `localhost:50051`

## Setup

Install dependencies:

```bash
npm install
```

This will automatically build the `@remotemedia/nodejs-client` package from the local workspace.

## Building

Compile all TypeScript examples to JavaScript:

```bash
npm run build
```

This creates compiled `.js` files in the `dist/` directory.

## Running Examples

### Development Mode (with tsx - no build needed)

Run examples directly from TypeScript source:

```bash
npm run dev:simple          # Simple execution example
npm run dev:multi-node      # Multi-node pipeline
npm run dev:streaming       # Streaming pipeline
npm run dev:audio           # Audio processing pipeline
```

### Production Mode (compiled)

Build first, then run compiled JavaScript:

```bash
npm run build

npm run simple              # Simple execution example
npm run multi-node          # Multi-node pipeline
npm run streaming           # Streaming pipeline
npm run audio               # Audio processing pipeline
```

Run all examples:

```bash
npm run all
```

### Starting the gRPC Server

Ensure the Rust gRPC server is running before executing examples:

```bash
cd ../../../runtime
cargo run --bin grpc_server --features grpc-transport
```

## Examples

### 1. simple_execution.ts

**What it demonstrates**:
- Connecting to the service
- Version compatibility checking
- Executing simple calculator pipelines
- Handling results and errors

**Expected output**:
```
✅ Connected to service v1
   Runtime version: 0.2.1
   
=== Executing Calculator Pipeline ===
✅ Execution successful
   Input: 10.0
   Operation: add 5.0
   Result: 15.0
   Wall time: 3.45ms
```

**Performance**: ~3-5ms per execution

### 2. multi_node_pipeline.ts

**What it demonstrates**:
- Chaining multiple nodes together
- Using connections to pass data between nodes
- Processing audio through multiple stages
- Per-node metrics collection

**Expected output**:
```
=== Multi-Node Pipeline: PassThrough -> Echo ===
✅ Execution successful
   Wall time: 4.23ms
   Nodes executed: 2
   - passthrough: 1.12ms
   - echo: 1.05ms
   
   Output audio: 16000 samples @ 16000Hz
```

**Performance**: ~4-6ms for 2-node pipeline

### 3. streaming_example.ts

**What it demonstrates**:
- Bidirectional streaming
- Chunked audio processing
- Real-time latency measurement
- Session management

**Expected output**:
```
=== Processing Chunks ===
Chunk  0:   0.04ms (  1600 samples total)
Chunk  1:   0.03ms (  3200 samples total)
...
Chunk 19:   0.04ms ( 32000 samples total)

=== Statistics ===
Total chunks: 20
Average latency: 0.04ms
✅ Target met: 0.04ms < 50ms
```

**Performance**: ~0.04ms per chunk (100ms audio)

## Error Handling

All examples include comprehensive error handling:

```typescript
try {
  const response = await client.executePipeline(request);
  if (response.hasResult()) {
    // Handle success
  } else {
    const error = response.getError()!;
    console.error(`Error: ${error.getMessage()}`);
  }
} catch (error) {
  console.error(`Connection error: ${error}`);
}
```

## Authentication

To use with authentication enabled:

```typescript
import * as grpc from '@grpc/grpc-js';

const metadata = new grpc.Metadata();
metadata.add('authorization', 'Bearer your-secret-token');

const client = new RemoteMediaClient('localhost:50051', {
  metadata: metadata
});
```

## TypeScript Configuration

Create `tsconfig.json` in examples directory:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "outDir": "./dist",
    "rootDir": ".",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "moduleResolution": "node"
  },
  "include": ["*.ts"],
  "exclude": ["node_modules"]
}
```

## Next Steps

- Try modifying pipeline parameters
- Chain more complex node sequences
- Experiment with different chunk sizes for streaming
- Add custom error handling logic
- Integrate into your Node.js application

## Reference

- **Client API**: See `nodejs-client/README.md`
- **Proto contracts**: See `specs/003-rust-grpc-service/contracts/`
- **Server docs**: See `specs/003-rust-grpc-service/QUICKSTART.md`
