"""
Basic utility nodes for the RemoteMedia SDK.
"""

from typing import Any, List, Union, TypedDict, Optional
import logging

from ..core.node import Node

logger = logging.getLogger(__name__)


# Type definitions for PassThroughNode
PassThroughInput = Any
PassThroughOutput = Any


class PassThroughError(TypedDict):
    """Error output structure for PassThroughNode."""
    error: str
    input: Any
    processed_by: str


# Type definitions for BufferNode
BufferInput = Any


class BufferOutput(TypedDict):
    """Output data structure for BufferNode."""
    buffer: List[Any]
    count: int
    processed_by: str


class BufferError(TypedDict):
    """Error output structure for BufferNode."""
    error: str
    input: Any
    processed_by: str


class PassThroughNode(Node):
    """A node that passes data through without modification."""
    
    def process(self, data: Any) -> Union[Any, PassThroughError]:
        """Pass data through unchanged."""
        try:
            logger.debug(f"PassThroughNode '{self.name}': passing through data")
            return data
        except Exception as e:
            logger.error(f"PassThroughNode '{self.name}': error: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"PassThroughNode[{self.name}]"
            }


class BufferNode(Node):
    """A node that buffers data for batch processing."""
    
    def __init__(self, buffer_size: int = 10, **kwargs):
        """
        Initialize the buffer node.
        
        Args:
            buffer_size: Maximum number of items to buffer
            **kwargs: Additional node parameters
        """
        super().__init__(**kwargs)
        self.buffer_size = buffer_size
        self.buffer: List[Any] = []
    
    def process(self, data: Any) -> Union[List[Any], None, BufferError]:
        """Buffer data and return when buffer is full."""
        try:
            self.buffer.append(data)
            logger.debug(f"BufferNode '{self.name}': buffered item ({len(self.buffer)}/{self.buffer_size})")
            
            if len(self.buffer) >= self.buffer_size:
                result = self.buffer.copy()
                self.buffer.clear()
                logger.debug(f"BufferNode '{self.name}': returning buffer of {len(result)} items")
                return result
            
            return None  # No output until buffer is full
        except Exception as e:
            logger.error(f"BufferNode '{self.name}': error: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"BufferNode[{self.name}]"
            }
    
    def flush(self) -> List[Any]:
        """Flush the current buffer and return its contents."""
        result = self.buffer.copy()
        self.buffer.clear()
        logger.debug(f"BufferNode '{self.name}': flushed buffer of {len(result)} items")
        return result


__all__ = ["PassThroughNode", "BufferNode"] 