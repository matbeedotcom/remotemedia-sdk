/**
 * Simple ExecutePipeline example
 * 
 * Demonstrates:
 * - Connecting to the gRPC service
 * - Checking version compatibility
 * - Executing a simple calculator pipeline
 * - Handling results and errors
 */

import { RemoteMediaClient } from '../../../nodejs-client/src/client';
import { PipelineManifest, ManifestMetadata, NodeManifest, ExecuteRequest } from '../../../nodejs-client/generated-types/execution_pb';

async function main() {
  // Create client (no auth for local development)
  const client = new RemoteMediaClient('localhost:50051');
  
  try {
    // Connect
    console.log('Connecting to gRPC service...');
    await client.connect();
    
    // Check version
    const version = await client.getVersion();
    console.log(`✅ Connected to service v${version.getVersionInfo()?.getProtocolVersion()}`);
    console.log(`   Runtime version: ${version.getVersionInfo()?.getRuntimeVersion()}`);
    
    const nodeTypes = version.getVersionInfo()?.getSupportedNodeTypesList() || [];
    console.log(`   Supported nodes: ${nodeTypes.slice(0, 5).join(', ')}...`);
    
    // Create simple calculator pipeline
    console.log('\n=== Executing Calculator Pipeline ===');
    
    const metadata = new ManifestMetadata();
    metadata.setName('simple_calculator');
    metadata.setDescription('Add 5 to input value');
    metadata.setCreatedAt('2025-10-28T00:00:00Z');
    
    const calcNode = new NodeManifest();
    calcNode.setId('calc');
    calcNode.setNodeType('CalculatorNode');
    calcNode.setParams('{"operation": "add", "value": 5.0}');
    calcNode.setIsStreaming(false);
    
    const manifest = new PipelineManifest();
    manifest.setVersion('v1');
    manifest.setMetadata(metadata);
    manifest.setNodesList([calcNode]);
    manifest.setConnectionsList([]);
    
    // Execute pipeline
    const request = new ExecuteRequest();
    request.setManifest(manifest);
    request.getDataInputsMap().set('calc', '{"value": 10.0}');
    request.setClientVersion('v1');
    
    const response = await client.executePipeline(request);
    
    if (response.hasResult()) {
      const result = response.getResult()!;
      console.log('✅ Execution successful');
      console.log('   Input: 10.0');
      console.log('   Operation: add 5.0');
      console.log(`   Result: ${result.getDataOutputsMap().get('calc')}`);
      console.log(`   Wall time: ${result.getMetrics()?.getWallTimeMs().toFixed(2)}ms`);
    } else {
      const error = response.getError()!;
      console.error(`❌ Error: ${error.getMessage()}`);
      process.exit(1);
    }
    
    // Try multiplication
    console.log('\n=== Executing Multiply Pipeline ===');
    
    calcNode.setParams('{"operation": "multiply", "value": 3.0}');
    metadata.setDescription('Multiply by 3');
    
    request.getDataInputsMap().set('calc', '{"value": 7.0}');
    
    const response2 = await client.executePipeline(request);
    
    if (response2.hasResult()) {
      const result = response2.getResult()!;
      console.log('✅ Execution successful');
      console.log('   Input: 7.0');
      console.log('   Operation: multiply 3.0');
      console.log(`   Result: ${result.getDataOutputsMap().get('calc')}`);
      console.log(`   Wall time: ${result.getMetrics()?.getWallTimeMs().toFixed(2)}ms`);
    }
    
    console.log('\n✅ All tests passed!');
    
  } catch (error) {
    console.error(`\n❌ Error: ${error}`);
    process.exit(1);
  } finally {
    await client.disconnect();
  }
}

main();
