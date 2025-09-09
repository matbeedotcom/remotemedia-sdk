const { PipelineClient } = require('./dist/src/pipeline-client');

async function testConnection() {
    console.log('🔌 Testing connection to gRPC service...');
    
    const client = new PipelineClient('localhost', 50052);
    
    try {
        await client.connect();
        console.log('✅ Connected to gRPC service successfully!');
        
        // Test listing pipelines
        console.log('\n📋 Testing pipeline listing...');
        const pipelines = await client.listPipelines();
        console.log(`Found ${pipelines.length} registered pipelines`);
        
        if (pipelines.length > 0) {
            pipelines.forEach(p => {
                console.log(`  - ${p.name} (${p.category}): ${p.description}`);
            });
        }
        
        client.close();
        console.log('✅ Connection test completed successfully!');
        
    } catch (error) {
        console.error('❌ Connection test failed:', error.message);
        process.exit(1);
    }
}

testConnection();