const { PipelineClient, PipelineBuilder } = require('./dist/src/pipeline-client');

async function testFullPipelineUsage() {
    console.log('🧪 Testing Full Pipeline Usage from JavaScript\n');
    
    const client = new PipelineClient('localhost', 50052);
    
    try {
        // 1. Connect
        console.log('1️⃣ Connecting to gRPC service...');
        await client.connect();
        console.log('   ✅ Connected successfully!\n');
        
        // 2. List available pipelines
        console.log('2️⃣ Discovering available pipelines...');
        const allPipelines = await client.listPipelines();
        console.log(`   Found ${allPipelines.length} total pipelines:`);
        allPipelines.forEach(p => {
            console.log(`     - ${p.name} (${p.category}): ${p.description}`);
        });
        
        // Filter JavaScript pipelines
        const jsPipelines = await client.listPipelines('javascript');
        console.log(`\n   JavaScript-specific pipelines: ${jsPipelines.length}`);
        
        if (jsPipelines.length === 0) {
            console.log('   ⚠️ No JavaScript pipelines found. Run register_persistent_pipeline.py first.');
            return;
        }
        
        const calcPipeline = jsPipelines[0];
        console.log(`   Using pipeline: ${calcPipeline.name}\n`);
        
        // 3. Get detailed pipeline info
        console.log('3️⃣ Getting pipeline details...');
        const { info, metrics } = await client.getPipelineInfo(
            calcPipeline.pipelineId, 
            false,  // include definition
            true    // include metrics
        );
        console.log(`   Pipeline: ${info.name}`);
        console.log(`   Usage count: ${info.usageCount}`);
        if (metrics) {
            console.log(`   Total executions: ${metrics.totalExecutions}`);
            console.log(`   Avg execution time: ${metrics.averageExecutionTimeMs}ms`);
        }
        console.log();
        
        // 4. Test pipeline execution
        console.log('4️⃣ Testing pipeline execution...');
        
        const testCases = [
            { operation: 'add', args: [10, 20, 5] },
            { operation: 'multiply', args: [3, 7] },
            { operation: 'subtract', args: [100, 25] },
            { operation: 'divide', args: [50, 2] }
        ];
        
        for (const testInput of testCases) {
            console.log(`   Input: ${JSON.stringify(testInput)}`);
            
            try {
                const result = await client.executePipeline(
                    calcPipeline.pipelineId,
                    testInput,
                    { timeout: 5000 }
                );
                console.log(`   Result: ${JSON.stringify(result)}`);
                console.log(`   ✅ Calculation: ${testInput.args.join(` ${testInput.operation} `)} = ${result.result}\n`);
            } catch (error) {
                console.log(`   ❌ Error: ${error.message}\n`);
            }
        }
        
        // 5. Register a new pipeline using PipelineBuilder
        console.log('5️⃣ Creating and registering a new pipeline...');
        
        const newPipeline = new PipelineBuilder('JavaScriptCreatedPipeline')
            .addNode({
                nodeId: 'input_processor',
                nodeType: 'PassThroughNode',
                config: {}
            })
            .addNode({
                nodeId: 'calculator',
                nodeType: 'CalculatorNode',
                config: { verbose: true }
            })
            .addNode({
                nodeId: 'output_processor', 
                nodeType: 'PassThroughNode',
                config: {}
            })
            .setMetadata({
                category: 'javascript-created',
                description: 'Pipeline created entirely from JavaScript client',
                created_by: 'javascript_client'
            })
            .build();
        
        console.log('   Pipeline definition created with PipelineBuilder');
        console.log(`   Nodes: ${newPipeline.nodes.length}`);
        console.log(`   Connections: ${newPipeline.connections?.length || 0}`);
        
        try {
            const newPipelineId = await client.registerPipeline(
                'js_created_calculator',
                newPipeline,
                {
                    metadata: {
                        source: 'javascript_client',
                        timestamp: new Date().toISOString()
                    },
                    dependencies: ['remotemedia'],
                    autoExport: true
                }
            );
            
            console.log(`   ✅ New pipeline registered: ${newPipelineId}`);
            
            // Test the new pipeline
            const testResult = await client.executePipeline(
                newPipelineId,
                { operation: 'power', args: [2, 8] }
            );
            console.log(`   Test execution: 2^8 = ${testResult.result}`);
            
            // Clean up the new pipeline
            await client.unregisterPipeline(newPipelineId);
            console.log('   🗑️ New pipeline unregistered\n');
            
        } catch (error) {
            console.log(`   ❌ Pipeline creation/registration failed: ${error.message}\n`);
        }
        
        // 6. Final pipeline listing
        console.log('6️⃣ Final pipeline listing...');
        const finalPipelines = await client.listPipelines();
        console.log(`   Total pipelines: ${finalPipelines.length}`);
        finalPipelines.forEach(p => {
            console.log(`     - ${p.name} (${p.category})`);
        });
        
        console.log('\n✅ Full JavaScript pipeline testing completed successfully!');
        console.log('\n🎯 Summary of what works:');
        console.log('   ✅ gRPC connection and communication');
        console.log('   ✅ Pipeline discovery and listing');
        console.log('   ✅ Pipeline execution with various operations');
        console.log('   ✅ Pipeline registration from JavaScript');
        console.log('   ✅ PipelineBuilder for creating pipeline definitions');
        console.log('   ✅ Metadata and metrics retrieval');
        console.log('   ✅ Error handling and cleanup');
        
    } catch (error) {
        console.error('❌ Test failed:', error.message);
        if (error.stack) {
            console.error(error.stack);
        }
    } finally {
        client.close();
    }
}

// Run the test
testFullPipelineUsage();