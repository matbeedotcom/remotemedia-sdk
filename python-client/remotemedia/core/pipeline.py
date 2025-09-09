"""
Pipeline class for managing sequences of processing nodes.
"""

from typing import Any, List, Optional, Dict, Iterator, AsyncGenerator
import logging
import time
from contextlib import contextmanager
from contextlib import asynccontextmanager
import asyncio
from inspect import isasyncgen
import inspect
from concurrent.futures import ThreadPoolExecutor

from .node import Node
from .exceptions import PipelineError, NodeError
from .types import _SENTINEL

logger = logging.getLogger(__name__)
logger.setLevel(logging.WARNING)

# A unique object to represent an empty item that should be ignored by nodes.
_EMPTY = object()


class Pipeline:
    """
    Manages a sequence of processing nodes and orchestrates data flow.
    
    The Pipeline class handles execution logic, checking if nodes should
    run remotely, and managing the overall processing workflow.
    """
    
    def __init__(self, nodes: Optional[List[Node]] = None, name: Optional[str] = None):
        """
        Initialize a new pipeline.
        
        Args:
            nodes: Optional list of nodes to initialize the pipeline with.
            name: Optional name for the pipeline
        """
        self.name = name or f"Pipeline_{id(self)}"
        self.nodes: List[Node] = []
        self._is_initialized = False
        self.logger = logger.getChild(self.__class__.__name__)

        if nodes:
            for node in nodes:
                self.add_node(node)
        
        self.logger.info(f"Created pipeline: {self}")
    
    def add_node(self, node: Node) -> "Pipeline":
        """
        Add a processing node to the pipeline.
        
        Args:
            node: The node to add
            
        Returns:
            Self for method chaining
            
        Raises:
            PipelineError: If the pipeline is already initialized
        """
        if self._is_initialized:
            raise PipelineError("Cannot add nodes to an initialized pipeline")
        
        if not isinstance(node, Node):
            raise PipelineError(f"Expected Node instance, got {type(node)}")
        
        self.nodes.append(node)
        self.logger.info(f"Added node '{node.name}' to pipeline '{self.name}'")
        
        return self
    
    def remove_node(self, node_name: str) -> bool:
        """
        Remove a node from the pipeline by name.
        
        Args:
            node_name: Name of the node to remove
            
        Returns:
            True if node was removed, False if not found
            
        Raises:
            PipelineError: If the pipeline is already initialized
        """
        if self._is_initialized:
            raise PipelineError("Cannot remove nodes from an initialized pipeline")
        
        for i, node in enumerate(self.nodes):
            if node.name == node_name:
                removed_node = self.nodes.pop(i)
                self.logger.info(f"Removed node '{removed_node.name}' from pipeline '{self.name}'")
                return True
        
        return False
    
    def get_node(self, node_name: str) -> Optional[Node]:
        """
        Get a node by name.
        
        Args:
            node_name: Name of the node to find
            
        Returns:
            The node if found, None otherwise
        """
        for node in self.nodes:
            if node.name == node_name:
                return node
        return None
    
    async def initialize(self) -> None:
        """
        Initialize the pipeline and all its nodes.
        
        This method must be called before processing data.
        
        Raises:
            PipelineError: If initialization fails
        """
        if self._is_initialized:
            return
        
        if not self.nodes:
            raise PipelineError("Cannot initialize empty pipeline")
        
        self.logger.info(f"Initializing pipeline '{self.name}' with {len(self.nodes)} nodes")
        
        try:
            for node in self.nodes:
                self.logger.info(f"Initializing node: {node.name}")
                await node.initialize()
            
            self._is_initialized = True
            self.logger.info(f"Pipeline '{self.name}' initialized successfully")
            
        except Exception as e:
            self.logger.error(f"Failed to initialize pipeline '{self.name}': {e}")
            # Clean up any partially initialized nodes
            await self.cleanup()
            raise PipelineError(f"Pipeline initialization failed: {e}") from e
    
    async def process(self, stream: Optional[AsyncGenerator[Any, None]] = None) -> AsyncGenerator[Any, None]:
        """
        Process a stream of data through the pipeline asynchronously and in parallel.

        If `stream` is not provided, this method assumes the first node in the
        pipeline is a source node (i.e., its `process` method returns an async
        generator) and uses its output as the stream for the rest of the pipeline.

        Args:
            stream: An optional async generator that yields data chunks for the pipeline.

        Yields:
            The final processed data item(s) from the end of the pipeline.
        """
        if not self._is_initialized:
            raise PipelineError("Pipeline must be initialized before processing")

        if not self.nodes:
            self.logger.warning("Cannot process data with an empty pipeline.")
            return

        nodes_to_process = self.nodes
        input_stream = stream

        if input_stream is None:
            # If no stream is provided, treat the first node as the source.
            source_node = self.nodes[0]
            source_output = source_node.process()

            if not inspect.isasyncgen(source_output):
                raise PipelineError(
                    f"The first node '{source_node.name}' is not a valid source. "
                    "Its process() method must return an async generator."
                )

            input_stream = source_output
            nodes_to_process = self.nodes[1:]

        # If there are no nodes left to process (e.g., pipeline had only a source),
        # then we just yield the results from the source stream.
        if not nodes_to_process:
            async for item in input_stream:
                yield item
            return
        
        loop = asyncio.get_running_loop()
        queues = [asyncio.Queue(maxsize=0) for _ in range(len(nodes_to_process) + 1)]
        executor = ThreadPoolExecutor(max_workers=len(nodes_to_process), thread_name_prefix='PipelineWorker')

        async def _worker(node: Node, in_queue: asyncio.Queue, out_queue: asyncio.Queue):
            self.logger.info(f"WORKER-START: '{node.name}'")
            try:
                while True:
                    item = await in_queue.get()
                    self.logger.info(f"WORKER-GET: '{node.name}' received item from queue.")
                    if item is _SENTINEL:
                        self.logger.info(f"WORKER-SENTINEL: '{node.name}'")

                        # Flush the node if it has a flush method
                        if hasattr(node, 'flush') and callable(getattr(node, 'flush')):
                            self.logger.info(f"WORKER-FLUSH: Flushing node '{node.name}'")
                            flush_method = getattr(node, 'flush')
                            if inspect.iscoroutinefunction(flush_method):
                                flushed_result = await flush_method()
                            else:
                                flushed_result = await loop.run_in_executor(executor, flush_method)
                            
                            if flushed_result is not None:
                                self.logger.info(f"WORKER-FLUSH-RESULT: '{node.name}' produced output.")
                                await out_queue.put(flushed_result)

                        await out_queue.put(_SENTINEL)
                        break
                    
                    # Run the node's process method
                    if inspect.isasyncgenfunction(node.process):
                        async for result in node.process(item):
                            if result is not None:
                                self.logger.info(f"WORKER-RESULT: '{node.name}' produced output.")
                                await out_queue.put(result)
                    elif inspect.iscoroutinefunction(node.process):
                        result = await node.process(item)
                        if result is not None:
                            self.logger.info(f"WORKER-RESULT: '{node.name}' produced output.")
                            await out_queue.put(result)
                    else:
                        result = await loop.run_in_executor(executor, node.process, item)
                        if result is not None:
                            self.logger.info(f"WORKER-RESULT: '{node.name}' produced output.")
                            await out_queue.put(result)

            except Exception as e:
                self.logger.error(f"WORKER-ERROR: in '{node.name}': {e}", exc_info=True)
                await out_queue.put(_SENTINEL)
            finally:
                self.logger.info(f"WORKER-FINISH: '{node.name}'")

        async def _streaming_worker(node: Node, in_queue: asyncio.Queue, out_queue: asyncio.Queue):
            self.logger.info(f"STREAMING-WORKER-START: '{node.name}'")
            
            # Create a secondary queue to act as a buffer and signaling mechanism
            internal_queue = asyncio.Queue(1)
            
            async def feeder():
                """Task to feed the internal queue from the main input queue."""
                try:
                    while True:
                        item = await in_queue.get()
                        self.logger.debug(f"STREAMING-WORKER-FEED: '{node.name}' got item from input queue")
                        await internal_queue.put(item)
                        if item is _SENTINEL:
                            break
                except Exception as e:
                    self.logger.error(f"STREAMING-WORKER-FEED-ERROR: in '{node.name}': {e}", exc_info=True)
                    await internal_queue.put(_SENTINEL)

            feeder_task = asyncio.create_task(feeder())

            try:
                # The node's process method will take the input from the internal queue
                async def in_stream():
                    try:
                        while True:
                            item = await internal_queue.get()
                            self.logger.debug(f"STREAMING-WORKER-STREAM: '{node.name}' got item from internal queue")
                            if item is _SENTINEL:
                                break
                            yield item
                    except Exception as e:
                        self.logger.error(f"STREAMING-WORKER-STREAM-ERROR: in '{node.name}': {e}", exc_info=True)

                if inspect.isasyncgenfunction(node.process):
                    self.logger.debug(f"STREAMING-WORKER-PROCESS: '{node.name}' starting process")
                    async for result in node.process(in_stream()):
                        if result is not None:
                            self.logger.debug(f"STREAMING-WORKER-RESULT: '{node.name}' produced output")
                            await out_queue.put(result)
                
                await out_queue.put(_SENTINEL)

            except Exception as e:
                self.logger.error(f"STREAMING-WORKER-ERROR: in '{node.name}': {e}", exc_info=True)
                await out_queue.put(_SENTINEL)
            finally:
                # Wait for the feeder task to complete or cancel it
                if not feeder_task.done():
                    feeder_task.cancel()
                    try:
                        await feeder_task
                    except asyncio.CancelledError:
                        pass
                self.logger.info(f"STREAMING-WORKER-FINISH: '{node.name}'")

        tasks = []
        for i, node in enumerate(nodes_to_process):
            if getattr(node, 'is_streaming', False):
                task = asyncio.create_task(
                    _streaming_worker(node, queues[i], queues[i+1])
                )
            else:
                task = asyncio.create_task(
                    _worker(node, queues[i], queues[i+1])
                )
            tasks.append(task)

        # Define feeder and consumer as separate async functions
        async def feeder():
            """Feeds data from input stream into the first queue."""
            self.logger.info("PIPELINE-FEEDER: Feeder starting.")
            try:
                async for item in input_stream:
                    self.logger.info("PIPELINE-FEEDER: Putting item into first queue.")
                    await queues[0].put(item)
            finally:
                await queues[0].put(_SENTINEL)
                self.logger.info("PIPELINE-FEEDER: Feeder finished.")

        async def consumer():
            """Consumes results from the final queue and yields them."""
            final_queue = queues[-1]
            while True:
                result = await final_queue.get()
                self.logger.info("CONSUMER: Got item from final queue.")
                if result is _SENTINEL:
                    self.logger.info("CONSUMER: Got sentinel, breaking.")
                    break
                yield result

        # Start feeder task to run concurrently
        feeder_task = asyncio.create_task(feeder())
        tasks.append(feeder_task)
        
        try:
            # Consume results as they become available
            async for result in consumer():
                yield result
        finally:
            # Ensure all worker tasks are cancelled
            for task in tasks:
                if not task.done():
                    task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)
            executor.shutdown(wait=False)
    
    async def cleanup(self) -> None:
        """
        Clean up the pipeline and all its nodes.
        
        This method should be called when the pipeline is no longer needed.
        """
        self.logger.info(f"Cleaning up pipeline '{self.name}'")
        
        for node in self.nodes:
            try:
                await node.cleanup()
            except Exception as e:
                self.logger.warning(f"Error cleaning up node '{node.name}': {e}")
        
        self._is_initialized = False
        self.logger.info(f"Pipeline '{self.name}' cleanup completed")
    
    @asynccontextmanager
    async def managed_execution(self):
        """
        Context manager for automatic pipeline initialization and cleanup.
        
        Usage:
            async with pipeline.managed_execution():
                async for result in pipeline.process(data_stream):
                    ...
        """
        try:
            await self.initialize()
            yield self
        finally:
            await self.cleanup()
    
    @property
    def is_initialized(self) -> bool:
        """Check if the pipeline is initialized."""
        return self._is_initialized
    
    @property
    def node_count(self) -> int:
        """Get the number of nodes in the pipeline."""
        return len(self.nodes)
    
    @property
    def remote_node_count(self) -> int:
        """Get the number of remote nodes in the pipeline."""
        return sum(1 for node in self.nodes if getattr(node, 'is_remote', False))
    
    def get_config(self) -> Dict[str, Any]:
        """Get the pipeline configuration."""
        return {
            "name": self.name,
            "node_count": self.node_count,
            "remote_node_count": self.remote_node_count,
            "nodes": [node.get_config() for node in self.nodes],
            "is_initialized": self.is_initialized,
        }
    
    def export_definition(self) -> Dict[str, Any]:
        """
        Export pipeline as a complete definition for remote execution.
        
        Returns:
            Dictionary containing the complete pipeline definition including:
            - nodes: List of node definitions with configurations
            - connections: Data flow connections between nodes
            - metadata: Pipeline metadata and properties
            - dependencies: Required Python packages
        """
        return {
            "name": self.name,
            "nodes": [self._export_node_definition(i, node) for i, node in enumerate(self.nodes)],
            "connections": self._extract_connections(),
            "metadata": self._get_export_metadata(),
            "dependencies": self._extract_dependencies()
        }
    
    def _export_node_definition(self, index: int, node: Node) -> Dict[str, Any]:
        """Export a single node definition."""
        node_config = node.get_config()
        is_remote = getattr(node, 'is_remote', False)
        
        return {
            "node_id": f"{node.name}_{index}",
            "node_type": node.__class__.__name__,
            "config": node_config.get("config", {}),
            "is_remote": is_remote,
            "remote_endpoint": getattr(getattr(node, 'remote_config', None), 'host', None) if is_remote else None,
            "is_streaming": getattr(node, 'is_streaming', False),
            "is_source": getattr(node, 'is_source', False),
            "is_sink": getattr(node, 'is_sink', False),
            "module": node.__class__.__module__,
            "class_name": node.__class__.__name__
        }
    
    def _extract_connections(self) -> List[Dict[str, Any]]:
        """
        Extract node connections from pipeline structure.
        
        In a linear pipeline, each node connects to the next node.
        """
        connections = []
        for i in range(len(self.nodes) - 1):
            connections.append({
                "from_node": f"{self.nodes[i].name}_{i}",
                "to_node": f"{self.nodes[i + 1].name}_{i + 1}",
                "output_port": "default",
                "input_port": "default"
            })
        return connections
    
    def _get_export_metadata(self) -> Dict[str, Any]:
        """Get export metadata for the pipeline."""
        return {
            "exported_at": time.time(),
            "node_count": str(self.node_count),
            "remote_node_count": str(self.remote_node_count),
            "has_streaming_nodes": str(any(getattr(node, 'is_streaming', False) for node in self.nodes)),
            "pipeline_type": "linear"  # Current pipelines are linear, future: DAG
        }
    
    def _extract_dependencies(self) -> List[str]:
        """
        Extract all Python package dependencies needed by pipeline nodes.
        
        Returns:
            List of package names required for pipeline execution
        """
        dependencies = set()
        
        # Add core dependencies
        dependencies.add("remotemedia")
        
        # Extract dependencies from each node
        for node in self.nodes:
            # Check if node has explicit dependencies
            if hasattr(node, 'dependencies'):
                dependencies.update(node.dependencies)
            
            # Check for remote config pip packages
            is_remote = getattr(node, 'is_remote', False)
            remote_config = getattr(node, 'remote_config', None)
            if is_remote and remote_config and hasattr(remote_config, 'pip_packages'):
                dependencies.update(remote_config.pip_packages or [])
            
            # Add module-specific dependencies
            module_name = node.__class__.__module__
            if 'remotemedia.nodes.ml' in module_name:
                dependencies.add("transformers")
                dependencies.add("torch")
            elif 'remotemedia.nodes.audio' in module_name:
                dependencies.add("numpy")
                dependencies.add("scipy")
            elif 'remotemedia.webrtc' in module_name:
                dependencies.add("aiortc")
                dependencies.add("aiohttp")
        
        return sorted(list(dependencies))
    
    @classmethod
    async def from_definition(cls, definition: Dict[str, Any]) -> 'Pipeline':
        """
        Create a pipeline from an exported definition.
        
        Args:
            definition: Pipeline definition dictionary
            
        Returns:
            Configured Pipeline instance
            
        Raises:
            PipelineError: If definition is invalid or nodes cannot be created
        """
        pipeline = cls(name=definition.get("name", "ImportedPipeline"))
        
        # Create nodes from definitions
        node_instances = {}
        for node_def in definition.get("nodes", []):
            try:
                node = await cls._create_node_from_definition(node_def)
                node_instances[node_def["node_id"]] = node
                pipeline.add_node(node)
            except Exception as e:
                raise PipelineError(f"Failed to create node {node_def['node_id']}: {e}")
        
        # Note: Connections are implicit in linear pipeline order
        # Future enhancement: Support DAG-based pipelines with explicit connections
        
        return pipeline
    
    @classmethod
    async def _create_node_from_definition(cls, node_def: Dict[str, Any]) -> Node:
        """
        Create a node instance from its definition.
        
        Args:
            node_def: Node definition dictionary
            
        Returns:
            Configured Node instance
        """
        import importlib
        
        # Import the node class
        module_name = node_def.get("module", "remotemedia.nodes")
        class_name = node_def.get("class_name", node_def["node_type"])
        
        try:
            module = importlib.import_module(module_name)
            node_class = getattr(module, class_name)
        except (ImportError, AttributeError) as e:
            raise PipelineError(f"Cannot import {class_name} from {module_name}: {e}")
        
        # Create node with configuration
        config = node_def.get("config", {})
        
        # Handle remote configuration
        if node_def.get("is_remote"):
            from .node import RemoteExecutorConfig
            remote_config = RemoteExecutorConfig(
                host=node_def.get("remote_endpoint", "localhost"),
                port=50052  # Default port
            )
            config["remote_config"] = remote_config
        
        # Create the node instance
        node = node_class(**config)
        
        # Set streaming flag if present
        if node_def.get("is_streaming"):
            node.is_streaming = True
        
        return node
    
    def __iter__(self) -> Iterator[Node]:
        """Iterate over nodes in the pipeline."""
        return iter(self.nodes)
    
    def __len__(self) -> int:
        """Get the number of nodes in the pipeline."""
        return len(self.nodes)
    
    def __repr__(self) -> str:
        """String representation of the pipeline."""
        status = "initialized" if self.is_initialized else "not initialized"
        return f"Pipeline(name='{self.name}', nodes={len(self.nodes)}, {status})" 