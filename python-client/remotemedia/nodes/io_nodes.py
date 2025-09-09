"""
Input/Output nodes for external data integration.

These nodes provide source and sink capabilities for pipelines,
allowing JavaScript clients and other external systems to feed data
into and receive data from pipelines.
"""

import asyncio
import logging
from typing import Any, Optional, AsyncGenerator, Callable, Dict
from collections import deque
import time

from ..core.node import Node
from ..core.types import _SENTINEL

logger = logging.getLogger(__name__)


class DataSourceNode(Node):
    """
    Source node that receives data from external systems.
    
    This node acts as an entry point for pipelines, receiving data
    from JavaScript clients or other external sources via gRPC.
    """
    
    def __init__(
        self,
        buffer_size: int = 100,
        timeout_seconds: float = 30.0,
        name: Optional[str] = None
    ):
        """
        Initialize the data source node.
        
        Args:
            buffer_size: Maximum number of items to buffer
            timeout_seconds: Timeout for waiting for data
            name: Optional node name
        """
        super().__init__(name=name or "DataSourceNode")
        self.buffer_size = buffer_size
        self.timeout_seconds = timeout_seconds
        self.data_queue: asyncio.Queue = asyncio.Queue(maxsize=buffer_size)
        self.is_streaming = True
        self.is_source = True
        self._closed = False
        self._last_data_time = time.time()
    
    async def push_data(self, data: Any) -> bool:
        """
        Push data into the source node from external system.
        
        Args:
            data: Data to push into the pipeline
            
        Returns:
            True if data was accepted, False if buffer is full
        """
        if self._closed:
            return False
        
        try:
            self.data_queue.put_nowait(data)
            self._last_data_time = time.time()
            return True
        except asyncio.QueueFull:
            logger.warning(f"Source node {self.name} buffer is full")
            return False
    
    async def process(self, input_data: Any = None) -> AsyncGenerator[Any, None]:
        """
        Process method that yields data from the queue.
        
        This method is called by the pipeline to get data from the source.
        It yields data as it becomes available from external sources.
        
        Args:
            input_data: Ignored for source nodes
            
        Yields:
            Data items from the queue
        """
        while not self._closed:
            try:
                # Wait for data with timeout
                data = await asyncio.wait_for(
                    self.data_queue.get(),
                    timeout=self.timeout_seconds
                )
                
                # Check for sentinel
                if data is _SENTINEL:
                    break
                
                yield data
                
            except asyncio.TimeoutError:
                # Check if we should close due to inactivity
                if time.time() - self._last_data_time > self.timeout_seconds:
                    logger.info(f"Source node {self.name} closing due to inactivity")
                    break
                continue
    
    async def close(self):
        """Close the source node and signal end of stream."""
        self._closed = True
        await self.data_queue.put(_SENTINEL)
    
    def get_config(self) -> Dict[str, Any]:
        """Get node configuration."""
        config = super().get_config()
        config.update({
            "buffer_size": self.buffer_size,
            "timeout_seconds": self.timeout_seconds,
            "is_source": True,
            "is_streaming": True
        })
        return config


class DataSinkNode(Node):
    """
    Sink node that sends processed data to external systems.
    
    This node acts as an exit point for pipelines, sending processed
    data to JavaScript clients or other external systems via gRPC.
    """
    
    def __init__(
        self,
        callback: Optional[Callable[[Any], None]] = None,
        buffer_output: bool = False,
        buffer_size: int = 100,
        name: Optional[str] = None
    ):
        """
        Initialize the data sink node.
        
        Args:
            callback: Optional callback to invoke with output data
            buffer_output: Whether to buffer output for retrieval
            buffer_size: Size of output buffer if buffering
            name: Optional node name
        """
        super().__init__(name=name or "DataSinkNode")
        self.callback = callback
        self.buffer_output = buffer_output
        self.buffer_size = buffer_size
        self.is_sink = True
        
        if buffer_output:
            self.output_buffer = deque(maxlen=buffer_size)
        else:
            self.output_buffer = None
        
        self._output_queue: Optional[asyncio.Queue] = None
        self._total_processed = 0
    
    def set_output_queue(self, queue: asyncio.Queue):
        """
        Set an output queue for streaming results.
        
        Args:
            queue: Async queue to send results to
        """
        self._output_queue = queue
    
    def set_callback(self, callback: Callable[[Any], None]):
        """
        Set or update the callback function.
        
        Args:
            callback: Function to call with output data
        """
        self.callback = callback
    
    async def process(self, data: Any) -> Any:
        """
        Process data and send to external system.
        
        Args:
            data: Data to send to external system
            
        Returns:
            The data (passthrough)
        """
        # Increment counter
        self._total_processed += 1
        
        # Buffer if enabled
        if self.buffer_output and self.output_buffer is not None:
            self.output_buffer.append(data)
        
        # Send to output queue if set
        if self._output_queue:
            try:
                await self._output_queue.put(data)
            except Exception as e:
                logger.error(f"Failed to send data to output queue: {e}")
        
        # Call callback if set
        if self.callback:
            try:
                if asyncio.iscoroutinefunction(self.callback):
                    await self.callback(data)
                else:
                    self.callback(data)
            except Exception as e:
                logger.error(f"Sink node callback error: {e}")
        
        return data
    
    def get_buffered_output(self) -> list:
        """
        Get all buffered output data.
        
        Returns:
            List of buffered output items
        """
        if self.output_buffer is None:
            return []
        return list(self.output_buffer)
    
    def clear_buffer(self):
        """Clear the output buffer."""
        if self.output_buffer is not None:
            self.output_buffer.clear()
    
    def get_config(self) -> Dict[str, Any]:
        """Get node configuration."""
        config = super().get_config()
        config.update({
            "buffer_output": self.buffer_output,
            "buffer_size": self.buffer_size,
            "is_sink": True,
            "total_processed": self._total_processed
        })
        return config


class BidirectionalNode(Node):
    """
    Bidirectional node for two-way communication with external systems.
    
    This node combines source and sink functionality, allowing for
    request-response patterns and interactive communication with
    JavaScript clients.
    """
    
    def __init__(
        self,
        process_callback: Optional[Callable[[Any], Any]] = None,
        buffer_size: int = 100,
        name: Optional[str] = None
    ):
        """
        Initialize the bidirectional node.
        
        Args:
            process_callback: Callback to process incoming data
            buffer_size: Size of input/output buffers
            name: Optional node name
        """
        super().__init__(name=name or "BidirectionalNode")
        self.process_callback = process_callback
        self.buffer_size = buffer_size
        self.is_source = True
        self.is_sink = True
        self.is_streaming = True
        
        # Separate queues for input and output
        self.input_queue: asyncio.Queue = asyncio.Queue(maxsize=buffer_size)
        self.output_queue: asyncio.Queue = asyncio.Queue(maxsize=buffer_size)
        self._closed = False
    
    async def push_input(self, data: Any) -> bool:
        """
        Push input data from external system.
        
        Args:
            data: Input data
            
        Returns:
            True if accepted, False if buffer full
        """
        if self._closed:
            return False
        
        try:
            await self.input_queue.put(data)
            return True
        except asyncio.QueueFull:
            return False
    
    async def pull_output(self, timeout: Optional[float] = None) -> Optional[Any]:
        """
        Pull output data for external system.
        
        Args:
            timeout: Optional timeout in seconds
            
        Returns:
            Output data or None if timeout
        """
        try:
            if timeout:
                return await asyncio.wait_for(
                    self.output_queue.get(),
                    timeout=timeout
                )
            else:
                return await self.output_queue.get()
        except asyncio.TimeoutError:
            return None
    
    async def process(self, input_data: Any = None) -> AsyncGenerator[Any, None]:
        """
        Process bidirectional communication.
        
        This method handles both incoming and outgoing data,
        processing input through the callback and yielding results.
        
        Args:
            input_data: Initial input data (optional)
            
        Yields:
            Processed data items
        """
        # Process initial input if provided
        if input_data is not None and self.process_callback:
            result = await self._process_with_callback(input_data)
            if result is not None:
                await self.output_queue.put(result)
                yield result
        
        # Process queue items
        while not self._closed:
            try:
                # Get input from queue
                data = await self.input_queue.get()
                
                # Check for sentinel
                if data is _SENTINEL:
                    break
                
                # Process through callback
                if self.process_callback:
                    result = await self._process_with_callback(data)
                    if result is not None:
                        await self.output_queue.put(result)
                        yield result
                else:
                    # Passthrough if no callback
                    await self.output_queue.put(data)
                    yield data
                    
            except Exception as e:
                logger.error(f"Bidirectional node processing error: {e}")
                continue
    
    async def _process_with_callback(self, data: Any) -> Any:
        """Process data through callback."""
        try:
            if asyncio.iscoroutinefunction(self.process_callback):
                return await self.process_callback(data)
            else:
                return self.process_callback(data)
        except Exception as e:
            logger.error(f"Callback processing error: {e}")
            return None
    
    async def close(self):
        """Close the bidirectional node."""
        self._closed = True
        await self.input_queue.put(_SENTINEL)
    
    def get_config(self) -> Dict[str, Any]:
        """Get node configuration."""
        config = super().get_config()
        config.update({
            "buffer_size": self.buffer_size,
            "is_source": True,
            "is_sink": True,
            "is_streaming": True
        })
        return config


class JavaScriptBridgeNode(Node):
    """
    Special node for bridging JavaScript and Python execution contexts.
    
    This node provides seamless integration between JavaScript clients
    and Python pipelines, handling data serialization and type conversion.
    """
    
    def __init__(
        self,
        transform_input: Optional[Callable[[Any], Any]] = None,
        transform_output: Optional[Callable[[Any], Any]] = None,
        name: Optional[str] = None
    ):
        """
        Initialize the JavaScript bridge node.
        
        Args:
            transform_input: Function to transform JS data to Python
            transform_output: Function to transform Python data to JS
            name: Optional node name
        """
        super().__init__(name=name or "JavaScriptBridgeNode")
        self.transform_input = transform_input
        self.transform_output = transform_output
    
    async def process(self, data: Any) -> Any:
        """
        Process data through the bridge.
        
        Args:
            data: Input data from JavaScript
            
        Returns:
            Transformed data for JavaScript
        """
        # Transform input from JavaScript format
        if self.transform_input:
            try:
                if asyncio.iscoroutinefunction(self.transform_input):
                    data = await self.transform_input(data)
                else:
                    data = self.transform_input(data)
            except Exception as e:
                logger.error(f"Input transform error: {e}")
        
        # Here you would typically process the data
        # For now, it's a passthrough
        
        # Transform output for JavaScript format
        if self.transform_output:
            try:
                if asyncio.iscoroutinefunction(self.transform_output):
                    data = await self.transform_output(data)
                else:
                    data = self.transform_output(data)
            except Exception as e:
                logger.error(f"Output transform error: {e}")
        
        return data
    
    def get_config(self) -> Dict[str, Any]:
        """Get node configuration."""
        config = super().get_config()
        config.update({
            "has_input_transform": self.transform_input is not None,
            "has_output_transform": self.transform_output is not None
        })
        return config