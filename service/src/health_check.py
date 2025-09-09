#!/usr/bin/env python3
"""
Health check script for the Remote Execution Service.
"""

import sys
import grpc
import asyncio
from config import ServiceConfig

# Import generated gRPC code
try:
    import execution_pb2
    import execution_pb2_grpc
except ImportError:
    print("gRPC proto files not available")
    sys.exit(1)


async def check_health():
    """Check if the gRPC service is healthy."""
    try:
        config = ServiceConfig()
        
        # Create gRPC channel
        channel = grpc.aio.insecure_channel(f"localhost:{config.grpc_port}")
        stub = execution_pb2_grpc.RemoteExecutionServiceStub(channel)
        
        # Make a status request
        request = execution_pb2.StatusRequest(include_metrics=False)
        response = await asyncio.wait_for(stub.GetStatus(request), timeout=5.0)
        
        await channel.close()
        
        # Check if service is healthy
        if response.status == 1:  # SERVICE_STATUS_HEALTHY
            print("Service is healthy")
            return 0
        else:
            print(f"Service is not healthy: status={response.status}")
            return 1
            
    except Exception as e:
        print(f"Health check failed: {e}")
        return 1


if __name__ == "__main__":
    try:
        exit_code = asyncio.run(check_health())
        sys.exit(exit_code)
    except Exception as e:
        print(f"Health check error: {e}")
        sys.exit(1) 