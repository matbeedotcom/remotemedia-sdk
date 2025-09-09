"""
Generator proxy classes for streaming remote generators.
"""

import asyncio
from typing import TypeVar, Generic, Optional, Any
import logging

from remotemedia.remote.client import RemoteExecutionClient
from remotemedia.serialization import PickleSerializer, JSONSerializer

# Import from the remote service src directory where proto files are generated
import sys
import os
proto_path = os.path.join(os.path.dirname(__file__), '../../remote_service/src')
if proto_path not in sys.path:
    sys.path.insert(0, proto_path)
import execution_pb2
import types_pb2

logger = logging.getLogger(__name__)

T = TypeVar('T')


class RemoteGeneratorProxy(Generic[T]):
    """Proxy for remote generators that preserves streaming behavior."""
    
    def __init__(self, client: RemoteExecutionClient, generator_id: str, is_async: bool = False):
        self._client = client
        self._generator_id = generator_id
        self._is_async = is_async
        self._exhausted = False
        self._closed = False
    
    def __aiter__(self):
        # Always return self for async iteration, regardless of original generator type
        return self
    
    def __iter__(self):
        if self._is_async:
            raise TypeError("Use __aiter__ for async generators")
        # For now, sync iteration is not supported
        raise NotImplementedError(
            "Sync iteration of remote generators not yet supported. "
            "Use async iteration instead: 'async for item in generator'"
        )
    
    async def __anext__(self) -> T:
        """Async iteration for async generators."""
        if self._exhausted or self._closed:
            raise StopAsyncIteration
        
        # Fetch next item (batch size 1 for true streaming)
        response = await self._client.stub.GetNextBatch(
            execution_pb2.GetNextBatchRequest(
                generator_id=self._generator_id,
                batch_size=1,
                serialization_format='pickle'
            )
        )
        
        if response.status != types_pb2.EXECUTION_STATUS_SUCCESS:
            raise RuntimeError(f"Generator error: {response.error_message}")
        
        if not response.items:
            self._exhausted = True
            # Cleanup
            await self.aclose()
            raise StopAsyncIteration
        
        # Deserialize and return item
        serializer = PickleSerializer()
        return serializer.deserialize(response.items[0])
    
    async def aclose(self):
        """Close the generator."""
        if not self._closed:
            self._closed = True
            try:
                await self._client.stub.CloseGenerator(
                    execution_pb2.CloseGeneratorRequest(generator_id=self._generator_id)
                )
            except Exception as e:
                logger.warning(f"Error closing generator: {e}")
    
    async def __aenter__(self):
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        await self.aclose()


class BatchedRemoteGeneratorProxy(Generic[T]):
    """Generator proxy that fetches items in batches for efficiency."""
    
    def __init__(self, client: RemoteExecutionClient, generator_id: str, 
                 batch_size: int = 10, is_async: bool = False):
        self._client = client
        self._generator_id = generator_id
        self._batch_size = batch_size
        self._is_async = is_async
        self._buffer = []
        self._exhausted = False
        self._closed = False
    
    def __aiter__(self):
        # Always return self for async iteration, regardless of original generator type
        return self
    
    def __iter__(self):
        if self._is_async:
            raise TypeError("Use __aiter__ for async generators")
        # For now, sync iteration is not supported
        raise NotImplementedError(
            "Sync iteration of remote generators not yet supported. "
            "Use async iteration instead: 'async for item in generator'"
        )
    
    async def __anext__(self) -> T:
        """Async iteration with batched fetching."""
        if not self._buffer and not self._exhausted and not self._closed:
            # Fetch next batch
            response = await self._client.stub.GetNextBatch(
                execution_pb2.GetNextBatchRequest(
                    generator_id=self._generator_id,
                    batch_size=self._batch_size,
                    serialization_format='pickle'
                )
            )
            
            if response.status != types_pb2.EXECUTION_STATUS_SUCCESS:
                raise RuntimeError(f"Generator error: {response.error_message}")
            
            if response.items:
                # Use pickle serializer by default
                serializer = PickleSerializer()
                self._buffer = [
                    serializer.deserialize(item) 
                    for item in response.items
                ]
            
            if not response.has_more:
                self._exhausted = True
        
        if self._buffer:
            return self._buffer.pop(0)
        else:
            # No more items
            await self.aclose()
            raise StopAsyncIteration
    
    async def aclose(self):
        """Close the generator."""
        if not self._closed:
            self._closed = True
            try:
                await self._client.stub.CloseGenerator(
                    execution_pb2.CloseGeneratorRequest(generator_id=self._generator_id)
                )
            except Exception as e:
                logger.warning(f"Error closing generator: {e}")
    
    async def __aenter__(self):
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        await self.aclose()


async def create_generator_proxy(client: RemoteExecutionClient, generator_id: str, 
                               is_async: bool = False, batch_size: Optional[int] = None) -> RemoteGeneratorProxy:
    """
    Create a generator proxy with appropriate batching strategy.
    
    Args:
        client: The remote execution client
        generator_id: The ID of the generator session
        is_async: Whether the generator is async
        batch_size: Batch size for fetching (None for single-item streaming)
    
    Returns:
        A generator proxy instance
    """
    if batch_size is None or batch_size == 1:
        return RemoteGeneratorProxy(client, generator_id, is_async)
    else:
        return BatchedRemoteGeneratorProxy(client, generator_id, batch_size, is_async)