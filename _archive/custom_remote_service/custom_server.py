#!/usr/bin/env python3
"""
Custom RemoteMedia Server with Additional Nodes.

This example demonstrates how to extend the base RemoteMedia service
with custom nodes from your examples and third-party extensions.

Usage:
    python custom_remote_service/custom_server.py [--port PORT] [--host HOST]

This server includes:
- All base SDK nodes from the remotemedia package
- Custom nodes from examples/audio_examples (KokoroTTSNode, etc.)
- Custom nodes from custom_remote_service/nodes
- Custom nodes from webrtc-example/webrtc_examples
"""

import asyncio
import logging
import os
import sys
import argparse
from pathlib import Path

# Add service to path
SERVICE_DIR = Path(__file__).parent.parent / "service" / "src"
sys.path.insert(0, str(SERVICE_DIR))

# Import base service components
from config import ServiceConfig
from executor import TaskExecutor
from sandbox import SandboxManager

# Import node discovery
from node_discovery import NodeDiscovery

# Import gRPC components
import grpc
from concurrent import futures
from remotemedia.protos import execution_pb2_grpc

# Set up logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


def load_custom_nodes(config_path: str = None) -> dict:
    """
    Load custom nodes using the discovery system.

    Args:
        config_path: Path to custom_nodes.yaml (optional)

    Returns:
        Dictionary of custom node classes
    """
    import yaml

    # Default config path
    if config_path is None:
        config_path = Path(__file__).parent / "custom_nodes.yaml"

    # Load configuration
    config = {}
    if os.path.exists(config_path):
        with open(config_path, 'r') as f:
            config = yaml.safe_load(f) or {}
        logger.info(f"Loaded custom node configuration from {config_path}")

    # Check if custom node discovery is enabled
    if not config.get('enabled', True):
        logger.info("Custom node discovery is disabled in configuration")
        return {}

    # Get search paths from config
    search_paths = config.get('search_paths', [])

    # Convert relative paths to absolute
    project_root = Path(__file__).parent.parent
    absolute_paths = []
    for path in search_paths:
        if not os.path.isabs(path):
            path = project_root / path
        absolute_paths.append(str(path))

    # Initialize discovery
    discovery = NodeDiscovery(search_paths=absolute_paths)

    # Discover nodes
    discovered = discovery.discover_nodes()

    # Apply filters
    include_nodes = config.get('include_nodes', [])
    exclude_nodes = config.get('exclude_nodes', [])

    filtered_nodes = {}
    for name, node_class in discovered.items():
        # Skip if in exclude list
        if name in exclude_nodes:
            logger.debug(f"Excluding node: {name}")
            continue

        # Skip if include list is specified and node not in it
        if include_nodes and name not in include_nodes:
            logger.debug(f"Skipping node not in include list: {name}")
            continue

        filtered_nodes[name] = node_class

    logger.info(f"Loaded {len(filtered_nodes)} custom nodes")
    for name in filtered_nodes:
        logger.info(f"  - {name}")

    return filtered_nodes


async def serve(host: str = "0.0.0.0", port: int = 50052):
    """
    Start the custom RemoteMedia server with additional nodes.

    Args:
        host: Host to bind to
        port: Port to listen on
    """
    # Load configuration
    config = ServiceConfig()

    # Load custom nodes
    try:
        custom_nodes = load_custom_nodes()
    except Exception as e:
        logger.warning(f"Failed to load custom nodes: {e}")
        logger.warning("Continuing with base SDK nodes only")
        custom_nodes = {}

    # Initialize executor with custom nodes
    executor = TaskExecutor(config, custom_node_registry=custom_nodes)

    # Import and initialize the servicer with custom executor
    from server import RemoteExecutionServicer
    servicer = RemoteExecutionServicer(config, custom_executor=executor)

    # Create gRPC server
    server = grpc.aio.server(
        futures.ThreadPoolExecutor(max_workers=config.max_workers)
    )

    # Register servicer
    execution_pb2_grpc.add_RemoteExecutionServiceServicer_to_server(servicer, server)

    # Add health check
    from grpc_health.v1 import health_pb2, health_pb2_grpc, health
    health_servicer = health.HealthServicer()
    health_pb2_grpc.add_HealthServicer_to_server(health_servicer, server)
    health_servicer.set(
        "remotemedia.execution.RemoteExecutionService",
        health_pb2.HealthCheckResponse.SERVING
    )

    # Bind to port
    server.add_insecure_port(f"{host}:{port}")

    # Start server
    await server.start()
    logger.info(f"Custom RemoteMedia Server started on {host}:{port}")
    logger.info(f"Total nodes available: {len(executor.node_registry)}")
    logger.info("Server is ready to accept connections")
    logger.info("=" * 60)

    # Wait for termination with periodic health checks
    try:
        while True:
            await asyncio.sleep(60)
            # Periodic health log
            logger.debug(f"Server health check: {len(executor.node_registry)} nodes available")
    except asyncio.CancelledError:
        logger.info("Server shutdown requested")
        raise


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="RemoteMedia Custom Server with Additional Nodes"
    )
    parser.add_argument(
        '--host',
        default='0.0.0.0',
        help='Host to bind to (default: 0.0.0.0)'
    )
    parser.add_argument(
        '--port',
        type=int,
        default=50052,
        help='Port to listen on (default: 50052)'
    )
    parser.add_argument(
        '--config',
        help='Path to custom_nodes.yaml configuration file'
    )

    args = parser.parse_args()

    # Run server with comprehensive error handling
    try:
        asyncio.run(serve(host=args.host, port=args.port))
    except KeyboardInterrupt:
        logger.info("\nServer stopped by user")
    except SystemExit:
        # Allow clean exits
        raise
    except Exception as e:
        logger.error(f"Fatal server error: {e}", exc_info=True)
        logger.error("=" * 60)
        logger.error("Server crashed unexpectedly. Please check the error above.")
        logger.error("Individual node failures should not crash the server.")
        logger.error("If you see this, please report it as a bug.")
        logger.error("=" * 60)
        sys.exit(1)


if __name__ == '__main__':
    main()
