# TypeScript Type Generator

Generate TypeScript definitions for all registered RemoteMedia nodes dynamically via gRPC.

## Features

- **Dynamic**: Fetches ALL registered nodes from the running gRPC service
- **Type-safe**: Generates proper TypeScript interfaces for each node
- **Modular**: Creates separate files for each node configuration
- **Up-to-date**: Always reflects the current state of registered nodes

## Quick Start

1. **Start the RemoteMedia service**:
   ```bash
   cd remote_service
   docker-compose up
   ```

2. **Generate TypeScript types**:
   ```bash
   cd scripts
   ./generate-types.sh
   ```

3. **Use in your TypeScript project**:
   ```typescript
   import { NodeType, RemoteExecutionClient, AudioTransformConfig } from './generated-types';
   
   const client = new RemoteExecutionClient({
     host: 'localhost',
     port: 50052
   });
   
   const config: AudioTransformConfig = {
     output_sample_rate: 16000,
     output_channels: 1
   };
   
   const result = await client.executeNode(NodeType.AudioTransform, config, audioData);
   ```

## Generated Files Structure

```
generated-types/
├── index.ts                    # Main export file
├── base.ts                     # Base interfaces
├── node-types.ts              # NodeType enum
├── config-types.ts            # Union types and mappings
├── client.ts                  # Client interface
├── audio-transform-config.ts  # AudioTransform configuration
├── voice-activity-detector-config.ts  # VAD configuration
└── ...                        # More node configurations
```

## Configuration

Set environment variables to customize generation:

```bash
export GRPC_HOST=localhost      # gRPC service host
export GRPC_PORT=50052          # gRPC service port
export OUTPUT_DIR=./my-types    # Output directory
./generate-types.sh
```

## How It Works

1. **gRPC Service**: Python server exposes registered nodes via `ExportTypeScriptDefinitions`
2. **Parameter Extraction**: Uses introspection to extract node parameters, types, and descriptions
3. **Node.js Generator**: Connects to gRPC, fetches node data, generates TypeScript files
4. **Type Safety**: Creates strongly-typed interfaces for each node configuration

## Benefits

- ✅ **No Hard-coding**: All types generated from actual registered nodes
- ✅ **Always Current**: Reflects real-time node registry
- ✅ **Type Safety**: Full TypeScript type checking
- ✅ **Developer Experience**: IDE autocomplete and documentation
- ✅ **Modular**: Separate files for easy importing

## Troubleshooting

**Connection refused**:
- Ensure RemoteMedia service is running
- Check host/port configuration

**Empty node list**:
- Verify nodes are properly registered in Python service
- Check service logs for initialization errors

**Type errors**:
- Regenerate types after updating the service
- Ensure TypeScript version compatibility