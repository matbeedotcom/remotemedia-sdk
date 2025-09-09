#!/usr/bin/env node

/**
 * Test direct pipeline registration and listing via gRPC
 */

import { PipelineClient } from './dist/src/pipeline-client.js';

async function testPipelineRegistration() {
    console.log('🧪 Testing direct pipeline registration...\n');
    
    const client = new PipelineClient('localhost', 50052);
    
    try {
        console.log('🔌 Connecting to gRPC service...');
        await client.connect();
        console.log('✅ Connected successfully\n');
        
        console.log('📋 Before registration - listing pipelines:');
        const pipelinesBefore = await client.listPipelines();
        console.log(`   Found ${pipelinesBefore.length} pipelines`);
        pipelinesBefore.forEach(p => console.log(`   - ${p.name} (${p.pipelineId})`));
        
        console.log('\n🔧 Attempting to register a test pipeline...');
        try {
            const testDefinition = {
                name: "test-pipeline",
                nodes: [
                    {
                        node_id: "test-node-1",
                        node_type: "PassThroughNode",
                        config: {},
                        is_source: true,
                        is_sink: false,
                        is_streaming: false,
                        is_remote: false,
                        remote_endpoint: ""
                    }
                ],
                connections: [],
                config: {},
                metadata: {
                    description: "Test pipeline from JavaScript",
                    category: "test"
                }
            };
            
            const pipelineId = await client.registerPipeline(
                "test-pipeline-js", 
                testDefinition,
                {
                    metadata: { category: "test", source: "javascript" }
                }
            );
            
            console.log(`✅ Successfully registered pipeline: ${pipelineId}`);
            
            console.log('\n📋 After registration - listing pipelines:');
            const pipelinesAfter = await client.listPipelines();
            console.log(`   Found ${pipelinesAfter.length} pipelines`);
            pipelinesAfter.forEach(p => console.log(`   - ${p.name} (${p.pipelineId})`));
            
        } catch (registerError) {
            console.log(`❌ Registration failed: ${registerError.message}`);
            console.log(`   Error code: ${registerError.code || 'unknown'}`);
            console.log(`   Error details: ${registerError.details || 'none'}`);
        }
        
    } catch (error) {
        console.error('❌ Test failed:', error.message);
        if (error.code) {
            console.error(`   Error code: ${error.code}`);
        }
        if (error.details) {
            console.error(`   Error details: ${error.details}`);
        }
    }
}

testPipelineRegistration().catch(console.error);