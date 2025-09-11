#!/usr/bin/env python3
"""
Base Remote Execution Server for RemoteMedia

This module provides a base server class that can be imported and extended
by developers to create their own custom remote execution services.

Usage:
```python
from remote_media_processing.remote_service import BaseRemoteServer
from my_custom_nodes import MyCustomNode
from my_custom_pipelines import create_my_pipeline

class MyRemoteServer(BaseRemoteServer):
    def register_custom_components(self):
        # Register custom nodes
        self.register_node(MyCustomNode)
        
        # Register custom pipelines
        self.register_pipeline("my_pipeline", create_my_pipeline)

if __name__ == "__main__":
    server = MyRemoteServer()
    server.run()
```
"""

import asyncio
import logging
import os
import sys
import signal
import time
from typing import Dict, Type, Any, Callable, List, Optional
from concurrent import futures
from pathlib import Path

import grpc
from grpc_health.v1 import health_pb2_grpc

from .server import RemoteExecutionServicer, HealthServicer
from .config import ServiceConfig
from remotemedia.core.node import Node
from remotemedia.core.pipeline import Pipeline
# Import from remotemedia.protos
from remotemedia.protos import execution_pb2_grpc


class CustomComponentRegistry:
    """Registry for custom nodes and pipelines."""
    
    def __init__(self):
        self.custom_nodes: Dict[str, Type[Node]] = {}
        self.custom_pipelines: Dict[str, Callable[[], Pipeline]] = {}
        self.node_metadata: Dict[str, Dict[str, Any]] = {}
        self.pipeline_metadata: Dict[str, Dict[str, Any]] = {}
        self.logger = logging.getLogger(__name__)
    
    def register_node(self, node_class: Type[Node], metadata: Optional[Dict[str, Any]] = None) -> None:
        """Register a custom node class."""
        if not issubclass(node_class, Node):
            raise ValueError(f"Node {node_class.__name__} must inherit from remotemedia.core.node.Node")
        
        node_name = node_class.__name__
        self.custom_nodes[node_name] = node_class
        
        # Extract metadata
        self.node_metadata[node_name] = metadata or {
            "name": node_name,
            "description": node_class.__doc__ or f"Custom node: {node_name}",
            "category": getattr(node_class, 'CATEGORY', 'custom'),
            "is_streaming": getattr(node_class, 'is_streaming', False),
            "input_types": getattr(node_class, 'INPUT_TYPES', ['Any']),
            "output_types": getattr(node_class, 'OUTPUT_TYPES', ['Any'])
        }
        
        self.logger.info(f"Registered custom node: {node_name}")
    
    def register_pipeline(self, name: str, factory: Callable[[], Pipeline], 
                         metadata: Optional[Dict[str, Any]] = None) -> None:
        """Register a custom pipeline factory."""
        if not callable(factory):
            raise ValueError(f"Pipeline factory for {name} must be callable")
        
        # Test the factory
        try:
            test_pipeline = factory()
            if not isinstance(test_pipeline, Pipeline):
                raise ValueError(f"Pipeline factory for {name} must return Pipeline instance")
        except Exception as e:
            raise ValueError(f"Pipeline factory for {name} failed: {e}")
        
        self.custom_pipelines[name] = factory
        self.pipeline_metadata[name] = metadata or {
            "name": name,
            "description": f"Custom pipeline: {name}",
            "category": "custom"
        }
        
        self.logger.info(f"Registered custom pipeline: {name}")
    
    def get_node(self, name: str) -> Optional[Type[Node]]:
        """Get a custom node by name."""
        return self.custom_nodes.get(name)
    
    def get_pipeline_factory(self, name: str) -> Optional[Callable[[], Pipeline]]:
        """Get a pipeline factory by name."""
        return self.custom_pipelines.get(name)
    
    def list_nodes(self) -> List[Dict[str, Any]]:
        """List all custom nodes with metadata."""
        return [
            {"class": self.custom_nodes[name], **meta}
            for name, meta in self.node_metadata.items()
        ]
    
    def list_pipelines(self) -> List[Dict[str, Any]]:
        """List all custom pipelines with metadata."""
        return [
            {"factory": self.custom_pipelines[name], **meta}
            for name, meta in self.pipeline_metadata.items()
        ]


class ExtendedRemoteExecutionServicer(RemoteExecutionServicer):
    """Extended servicer that includes custom components."""
    
    def __init__(self, config: ServiceConfig, custom_registry: CustomComponentRegistry):
        super().__init__(config)
        self.custom_registry = custom_registry
        self.logger.info(f"Extended servicer initialized with {len(custom_registry.custom_nodes)} custom nodes and {len(custom_registry.custom_pipelines)} custom pipelines")
    
    async def get_available_nodes(self, category: Optional[str] = None) -> List[Dict[str, Any]]:
        """Get available nodes including custom ones."""
        # Get base nodes
        base_nodes = await super().executor.get_available_nodes(category)
        
        # Add custom nodes
        custom_nodes = []
        for node_info in self.custom_registry.list_nodes():
            if category is None or node_info.get('category') == category:
                custom_nodes.append({
                    "name": node_info["name"],
                    "description": node_info["description"],
                    "category": node_info["category"],
                    "is_streaming": node_info["is_streaming"],
                    "input_types": node_info["input_types"],
                    "output_types": node_info["output_types"],
                    "is_custom": True
                })
        
        return base_nodes + custom_nodes


class BaseRemoteServer:
    """
    Base class for creating custom remote execution servers.
    
    Developers can inherit from this class and override register_custom_components()
    to add their own nodes and pipelines.
    """
    
    def __init__(self, config: Optional[ServiceConfig] = None, custom_dir: Optional[str] = None):
        """
        Initialize the base remote server.
        
        Args:
            config: Service configuration (uses defaults if None)
            custom_dir: Directory to scan for custom components (optional)
        """
        self.config = config or ServiceConfig()
        self.custom_dir = custom_dir
        self.custom_registry = CustomComponentRegistry()
        self.server = None
        self.logger = logging.getLogger(__name__)
        
        # Set up logging
        logging.basicConfig(
            level=getattr(logging, self.config.log_level.upper()),
            format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
        )
    
    def register_node(self, node_class: Type[Node], metadata: Optional[Dict[str, Any]] = None) -> None:
        """Register a custom node class."""
        self.custom_registry.register_node(node_class, metadata)
    
    def register_pipeline(self, name: str, factory: Callable[[], Pipeline], 
                         metadata: Optional[Dict[str, Any]] = None) -> None:
        """Register a custom pipeline factory."""
        self.custom_registry.register_pipeline(name, factory, metadata)
    
    def register_custom_components(self) -> None:
        """
        Override this method to register custom nodes and pipelines.
        
        Example:
        ```python
        def register_custom_components(self):
            from my_nodes import CustomAudioNode, CustomVideoNode
            from my_pipelines import create_speech_pipeline
            
            self.register_node(CustomAudioNode)
            self.register_node(CustomVideoNode)
            self.register_pipeline("speech_pipeline", create_speech_pipeline)
        ```
        """
        pass
    
    def auto_discover_components(self) -> None:
        """Automatically discover components from custom directory if specified."""
        if not self.custom_dir:
            return
        
        custom_path = Path(self.custom_dir)
        if not custom_path.exists():
            self.logger.warning(f"Custom directory {custom_path} does not exist")
            return
        
        self.logger.info(f"Auto-discovering components from {custom_path}")
        
        # Add custom directory to Python path
        if str(custom_path) not in sys.path:
            sys.path.insert(0, str(custom_path))
        
        # Try to import and register components
        # This is a basic implementation - could be enhanced with more sophisticated discovery
        try:
            # Look for nodes.py or nodes/ directory
            nodes_file = custom_path / "nodes.py"
            nodes_dir = custom_path / "nodes"
            
            if nodes_file.exists():
                import importlib.util
                spec = importlib.util.spec_from_file_location("custom_nodes", nodes_file)
                if spec and spec.loader:
                    custom_nodes = importlib.util.module_from_spec(spec)
                    spec.loader.exec_module(custom_nodes)
                    
                    # Register all Node classes found
                    for name in dir(custom_nodes):
                        obj = getattr(custom_nodes, name)
                        if (isinstance(obj, type) and 
                            issubclass(obj, Node) and 
                            obj != Node):
                            self.register_node(obj)
            
            # Look for pipelines.py or pipelines/ directory  
            pipelines_file = custom_path / "pipelines.py"
            if pipelines_file.exists():
                spec = importlib.util.spec_from_file_location("custom_pipelines", pipelines_file)
                if spec and spec.loader:
                    custom_pipelines = importlib.util.module_from_spec(spec)
                    spec.loader.exec_module(custom_pipelines)
                    
                    # Look for pipeline factory functions
                    for name in dir(custom_pipelines):
                        obj = getattr(custom_pipelines, name)
                        if callable(obj) and name.startswith('create_') and name.endswith('_pipeline'):
                            pipeline_name = name.replace('create_', '').replace('_pipeline', '')
                            self.register_pipeline(pipeline_name, obj)
                            
        except Exception as e:
            self.logger.error(f"Error auto-discovering components: {e}")
    
    async def initialize(self) -> None:
        """Initialize the server and register components."""
        self.logger.info("Initializing BaseRemoteServer...")
        
        # Auto-discover components if custom directory is specified
        self.auto_discover_components()
        
        # Register custom components (user override)
        self.register_custom_components()
        
        self.logger.info(f"Registered {len(self.custom_registry.custom_nodes)} custom nodes")
        self.logger.info(f"Registered {len(self.custom_registry.custom_pipelines)} custom pipelines")
    
    async def serve(self) -> None:
        """Start the gRPC server."""
        await self.initialize()
        
        # Create gRPC server
        self.server = grpc.aio.server(
            futures.ThreadPoolExecutor(max_workers=self.config.max_workers),
            options=[
                ('grpc.max_receive_message_length', -1),
                ('grpc.max_send_message_length', -1)
            ]
        )
        
        # Add servicers with custom registry
        execution_pb2_grpc.add_RemoteExecutionServiceServicer_to_server(
            ExtendedRemoteExecutionServicer(self.config, self.custom_registry), 
            self.server
        )
        health_pb2_grpc.add_HealthServicer_to_server(HealthServicer(), self.server)
        
        # Configure server
        listen_addr = f'0.0.0.0:{self.config.grpc_port}'
        self.server.add_insecure_port(listen_addr)
        
        # Start server
        self.logger.info(f"Starting Custom RemoteMedia Execution Service on {listen_addr}")
        await self.server.start()
        
        # Set up graceful shutdown
        def signal_handler(signum, frame):
            self.logger.info(f"Received signal {signum}, shutting down...")
            asyncio.create_task(self.server.stop(grace=10))
        
        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)
        
        # Wait for server termination
        await self.server.wait_for_termination()
        self.logger.info("Server stopped")
    
    def run(self) -> None:
        """Run the server (blocking)."""
        try:
            asyncio.run(self.serve())
        except KeyboardInterrupt:
            self.logger.info("Server interrupted")
        except Exception as e:
            self.logger.error(f"Server error: {e}")
            sys.exit(1)


# Convenience function for simple server creation
def create_server(custom_nodes: Optional[List[Type[Node]]] = None,
                 custom_pipelines: Optional[Dict[str, Callable[[], Pipeline]]] = None,
                 config: Optional[ServiceConfig] = None) -> BaseRemoteServer:
    """
    Create a server with custom components.
    
    Args:
        custom_nodes: List of custom node classes to register
        custom_pipelines: Dict mapping pipeline names to factory functions
        config: Service configuration
    
    Returns:
        Configured BaseRemoteServer instance
    """
    server = BaseRemoteServer(config)
    
    # Register provided nodes
    if custom_nodes:
        for node_class in custom_nodes:
            server.register_node(node_class)
    
    # Register provided pipelines
    if custom_pipelines:
        for name, factory in custom_pipelines.items():
            server.register_pipeline(name, factory)
    
    return server