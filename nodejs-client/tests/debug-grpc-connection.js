#!/usr/bin/env node

/**
 * Debug script to test gRPC connection and see what methods are available
 */

import { PipelineClient } from './dist/src/pipeline-client.js';

async function debugConnection() {
    console.log('🔧 Debug: Testing gRPC connection...\n');
    
    const client = new PipelineClient('localhost', 50052);
    
    try {
        console.log('🔌 Connecting to gRPC service...');
        await client.connect();
        console.log('✅ Connected successfully\n');
        
        console.log('📋 Testing ListPipelines method...');
        try {
            const pipelines = await client.listPipelines();
            console.log(`✅ ListPipelines returned: ${pipelines.length} pipelines`);
            
            if (pipelines.length > 0) {
                pipelines.forEach((pipeline, i) => {
                    console.log(`   ${i + 1}. ${pipeline.name} (${pipeline.pipelineId})`);
                });
            } else {
                console.log('   No pipelines returned from remote service');
            }
        } catch (listError) {
            console.log(`❌ ListPipelines failed: ${listError.message}`);
            console.log(`   Error code: ${listError.code}`);
            console.log(`   Error details: ${listError.details}`);
        }
        
        console.log('\n🔍 Testing GetPipelineInfo method with a test ID...');
        try {
            const info = await client.getPipelineInfo('test-id');
            console.log('✅ GetPipelineInfo works (unexpected)');
        } catch (infoError) {
            console.log(`❌ GetPipelineInfo failed as expected: ${infoError.message}`);
        }
        
    } catch (error) {
        console.error('❌ Connection failed:', error.message);
        if (error.code) {
            console.error(`   Error code: ${error.code}`);
        }
        if (error.details) {
            console.error(`   Error details: ${error.details}`);
        }
    }
}

debugConnection().catch(console.error);