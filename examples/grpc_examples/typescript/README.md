# TypeScript gRPC Client Examples

Working TypeScript examples demonstrating the Rust gRPC service.

## Prerequisites

```bash
# Install Node.js client
cd nodejs-client
npm install

# Generate proto stubs
npm run generate-protos

# Build TypeScript
npm run build
```

## Running Examples

Ensure the Rust gRPC server is running:

```bash
cd runtime
cargo run --bin grpc_server --features grpc-transport
```

Then run examples (in another terminal):

```bash
cd examples/grpc_examples/typescript

# Compile TypeScript examples
npx tsc simple_execution.ts
npx tsc multi_node_pipeline.ts
npx tsc streaming_example.ts

# Run examples
node simple_execution.js
node multi_node_pipeline.js
node streaming_example.js
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
