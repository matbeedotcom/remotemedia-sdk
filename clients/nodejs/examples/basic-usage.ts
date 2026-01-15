/**
 * Basic Usage Example
 * 
 * This example demonstrates the fundamental usage of the RemoteMedia Node.js client.
 */

import { RemoteProxyClient } from '../src';
import {
  NodeType,
  CalculatorNodeCalculatorInput,
  CalculatorNodeCalculatorOutput
} from '../generated-types';

async function main() {
  // Create a client instance
  const client = new RemoteProxyClient({
    host: 'localhost',
    port: 50052
  });

  try {
    // Connect to the server
    await client.connect();
    console.log('✅ Connected to RemoteMedia server');

    // List available nodes
    console.log('\n📋 Available nodes:');
    const nodes = await client.listNodes();
    nodes.forEach((node) => {
      console.log(`  - ${node.node_type} (${node.category})`);
    });

    // Create a calculator node
    console.log('\n🧮 Creating calculator node...');
    const calculator = await client.createNodeProxy(NodeType.CalculatorNode);

    // Perform calculations
    const operations: CalculatorNodeCalculatorInput[] = [
      { operation: 'add', args: [5, 3] },
      { operation: 'multiply', args: [4, 7] },
      { operation: 'divide', args: [20, 4] }
    ];

    for (const op of operations) {
      const result: CalculatorNodeCalculatorOutput = await calculator.process(op);
      console.log(`${op.operation}(${op.args.join(', ')}) = ${result.result}`);
    }

    // Get server status
    console.log('\n📊 Server status:');
    const status = await client.getStatus();
    console.log(`  Status: ${status.status}`);
    console.log(`  Version: ${status.version}`);
    console.log(`  Uptime: ${Math.floor(status.uptime_seconds / 60)} minutes`);

  } catch (error) {
    console.error('❌ Error:', error);
  } finally {
    // Always close the connection
    await client.close();
    console.log('\n👋 Connection closed');
  }
}

// Run the example
if (require.main === module) {
  main().catch(console.error);
}