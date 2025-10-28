/**
 * Simple ExecutePipeline example
 * 
 * Demonstrates:
 * - Connecting to the gRPC service
 * - Checking version compatibility
 * - Executing a simple calculator pipeline
 * - Handling results and errors
 */

import { RemoteMediaClient, RemoteMediaError } from '../../../nodejs-client/src/grpc-client';

async function main() {
  const client = new RemoteMediaClient('localhost:50051');
  
  try {
    console.log('Connecting to gRPC service...');
    await client.connect();
    
    const version = await client.getVersion();
    console.log(`✅ Connected to service v${version.protocolVersion}`);
    console.log(`   Runtime version: ${version.runtimeVersion}`);
    console.log(`   Supported nodes: ${version.supportedNodeTypes.slice(0, 5).join(', ')}...`);
    
    console.log('\n=== Executing Calculator Pipeline ===');
    
    const manifest = {
      version: 'v1',
      metadata: {
        name: 'simple_calculator',
        description: 'Add 5 to input value',
        createdAt: '2025-10-28T00:00:00Z'
      },
      nodes: [
        {
          id: 'calc',
          nodeType: 'CalculatorNode',
          params: '{"operation": "add", "value": 5.0}',
          isStreaming: false
        }
      ],
      connections: []
    };
    
    const result = await client.executePipeline(
      manifest,
      {},
      { calc: '{"value": 10.0}' }
    );
    
    console.log('✅ Execution successful');
    console.log('   Input: 10.0');
    console.log('   Operation: add 5.0');
    console.log(`   Result: ${result.dataOutputs.calc}`);
    console.log(`   Wall time: ${result.metrics.wallTimeMs.toFixed(2)}ms`);
    
    console.log('\n=== Executing Multiply Pipeline ===');
    
    manifest.nodes[0].params = '{"operation": "multiply", "value": 3.0}';
    manifest.metadata.description = 'Multiply by 3';
    
    const result2 = await client.executePipeline(
      manifest,
      {},
      { calc: '{"value": 7.0}' }
    );
    
    console.log('✅ Execution successful');
    console.log('   Input: 7.0');
    console.log('   Operation: multiply 3.0');
    console.log(`   Result: ${result2.dataOutputs.calc}`);
    console.log(`   Wall time: ${result2.metrics.wallTimeMs.toFixed(2)}ms`);
    
    console.log('\n✅ All tests passed!');
    
  } catch (error) {
    if (error instanceof RemoteMediaError) {
      console.error(`\n❌ Error: ${error.message}`);
      if (error.errorType) console.error(`   Type: ${error.errorType}`);
      if (error.failingNodeId) console.error(`   Node: ${error.failingNodeId}`);
    } else {
      console.error(`\n❌ Error: ${error}`);
    }
    process.exit(1);
  } finally {
    await client.disconnect();
  }
}

main();
