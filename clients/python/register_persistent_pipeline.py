#!/usr/bin/env python3
"""
Register a persistent pipeline for JavaScript client testing.
"""

import asyncio
import grpc
import sys
import json
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
sys.path.insert(0, str(Path(__file__).parent / 'remote_service' / 'src'))

import execution_pb2
import execution_pb2_grpc
import types_pb2

from remotemedia import Pipeline
from remotemedia.nodes import PassThroughNode, CalculatorNode


async def register_persistent_pipeline():
    """Register a pipeline that stays registered for JavaScript testing."""
    print("📝 Registering persistent pipeline for JavaScript client testing...")
    
    # Create gRPC client
    channel = grpc.aio.insecure_channel('localhost:50052')
    client = execution_pb2_grpc.RemoteExecutionServiceStub(channel)
    
    try:
        # Create a simple calculator pipeline
        pipeline = Pipeline(name="JSTestCalculatorPipeline")
        pipeline.add_node(PassThroughNode(name="input"))
        pipeline.add_node(CalculatorNode(name="calculator", verbose=True))
        pipeline.add_node(PassThroughNode(name="output"))
        
        definition = pipeline.export_definition()
        
        # Convert to proto format
        proto_definition = execution_pb2.PipelineDefinition(
            name=definition['name'],
            config={k: str(v) for k, v in definition.get('config', {}).items()},
            metadata={k: str(v) for k, v in definition.get('metadata', {}).items()}
        )
        
        # Add nodes
        for node in definition['nodes']:
            node_def = proto_definition.nodes.add()
            node_def.node_id = node['node_id']
            node_def.node_type = node['node_type'] 
            for k, v in node.get('config', {}).items():
                node_def.config[k] = str(v)
            node_def.is_remote = node.get('is_remote', False)
            node_def.remote_endpoint = node.get('remote_endpoint') or ''
            node_def.is_streaming = node.get('is_streaming', False)
            node_def.is_source = node.get('is_source', False)
            node_def.is_sink = node.get('is_sink', False)
        
        # Add connections
        for conn in definition.get('connections', []):
            conn_def = proto_definition.connections.add()
            conn_def.from_node = conn['from_node']
            conn_def.to_node = conn['to_node']
            conn_def.output_port = conn.get('output_port', 'default')
            conn_def.input_port = conn.get('input_port', 'default')
        
        # Register request
        register_request = execution_pb2.RegisterPipelineRequest(
            pipeline_name="js_test_calculator",
            definition=proto_definition,
            auto_export=True
        )
        
        register_request.metadata["category"] = "javascript"
        register_request.metadata["description"] = "Calculator pipeline for JavaScript client testing"
        register_request.metadata["input_format"] = "calculation_request"
        register_request.metadata["output_format"] = "calculation_result"
        register_request.dependencies.append("remotemedia")
        
        response = await client.RegisterPipeline(register_request)
        
        if response.status == types_pb2.EXECUTION_STATUS_SUCCESS:
            pipeline_id = response.pipeline_id
            print(f"✅ Pipeline registered: {pipeline_id}")
            print(f"📋 Pipeline name: js_test_calculator")
            print(f"🏷️  Category: javascript")
            print(f"📝 Description: Calculator pipeline for JavaScript client testing")
            print(f"🔗 Available at: localhost:50052")
            print()
            print("JavaScript clients can now:")
            print("  1. List pipelines: client.listPipelines('javascript')")
            print("  2. Execute pipeline: client.executePipeline(pipelineId, inputData)")
            print("  3. Use input format: {operation: 'add', args: [10, 20, 5]}")
            print()
            print("⚠️  Pipeline will remain registered until server restart")
            return pipeline_id
        else:
            print(f"❌ Registration failed: {response.error_message}")
            return None
            
    except Exception as e:
        print(f"❌ Error: {e}")
        import traceback
        traceback.print_exc()
        return None
    
    finally:
        await channel.close()


if __name__ == "__main__":
    asyncio.run(register_persistent_pipeline())