#!/usr/bin/env python3
"""
Generate TypeScript interface definitions for RemoteMedia Processing SDK.

This script connects to the remote service and exports the TypeScript
definitions to a file.
"""

import asyncio
import argparse
import grpc
import sys
import os

# Add the remote service src directory to the path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "service", "src"))

import execution_pb2
import execution_pb2_grpc
import types_pb2


async def generate_typescript_definitions(host: str, port: int, output_file: str):
    """Generate TypeScript definitions from the remote service."""
    
    channel = grpc.aio.insecure_channel(f"{host}:{port}")
    stub = execution_pb2_grpc.RemoteExecutionServiceStub(channel)
    
    try:
        # Request TypeScript definitions with all features
        request = execution_pb2.ExportTypeScriptRequest(
            include_node_configs=True,
            include_examples=True
        )
        
        response = await stub.ExportTypeScriptDefinitions(request)
        
        if response.status == types_pb2.EXECUTION_STATUS_SUCCESS:
            # Write to file
            with open(output_file, 'w') as f:
                f.write(response.typescript_definitions)
            
            print(f"✅ TypeScript definitions generated successfully!")
            print(f"   Output file: {output_file}")
            print(f"   Service version: {response.version}")
        else:
            print(f"❌ Failed to generate TypeScript definitions: {response.error_message}")
            return 1
            
    except grpc.RpcError as e:
        print(f"❌ gRPC error: {e.code()} - {e.details()}")
        return 1
    except Exception as e:
        print(f"❌ Error: {e}")
        return 1
    finally:
        await channel.close()
    
    return 0


def main():
    parser = argparse.ArgumentParser(
        description="Generate TypeScript interface definitions for RemoteMedia SDK"
    )
    parser.add_argument(
        "--host",
        default="localhost",
        help="Remote service host (default: localhost)"
    )
    parser.add_argument(
        "--port",
        type=int,
        default=50052,
        help="Remote service port (default: 50052)"
    )
    parser.add_argument(
        "--output",
        "-o",
        default="remotemedia-types.d.ts",
        help="Output TypeScript file (default: remotemedia-types.d.ts)"
    )
    
    args = parser.parse_args()
    
    # Run the async function
    exit_code = asyncio.run(generate_typescript_definitions(args.host, args.port, args.output))
    sys.exit(exit_code)


if __name__ == "__main__":
    main()